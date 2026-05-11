from __future__ import annotations

import argparse
import http.client
import json
import os
import signal
import secrets
import socket
import socketserver
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlsplit

from .client import OpenCodeClient
from .logging import get_logger, init_logger, log_kv
from .orchestrator import OfficeOrchestrator
from .runtime import ProjectRuntime, project_runtime

import logging as _stdlib_logging


def pick_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def wait_for_http(base_url: str, timeout: float = 15.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            parsed = urlsplit(base_url)
            conn = http.client.HTTPConnection(parsed.hostname, parsed.port, timeout=2.0)
            conn.request("GET", "/session")
            response = conn.getresponse()
            response.read()
            return
        except Exception:
            time.sleep(0.2)
    raise RuntimeError(f"Timed out waiting for OpenCode server at {base_url}")


class ThreadedUnixServer(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True
    allow_reuse_address = True


class OfficeController:
    def __init__(self, runtime: ProjectRuntime, *, base_url: str | None, password: str | None, username: str | None) -> None:
        self.runtime = runtime
        self.lock = threading.RLock()
        self.serve_process: subprocess.Popen[str] | None = None
        self.spawned_serve_pid: int | None = None
        self.base_url = base_url
        self.password = password
        self.username = username or os.environ.get("OPENCODE_SERVER_USERNAME", "opencode")
        self.client = self._build_client()
        self.orchestrator = OfficeOrchestrator(self.client, runtime)
        self.stop_event = threading.Event()
        self.watch_thread = threading.Thread(target=self._watch_loop, name="office-watch", daemon=True)
        self.server: "OfficeHTTPServer | None" = None
        self._shutdown_lock = threading.Lock()
        self._shutdown_done = False

    def attach_user_serve(self, *, base_url: str, password: str | None, username: str) -> None:
        """Re-point this controller at the user's running opencode serve.

        Required because session IDs are scoped to a specific serve's DB. The
        plugin running in the user's TUI hands us its serve URL+creds via
        /judge/on; we rebuild the OpenCodeClient against it so fork_session
        and friends operate on real, visible sessions.
        """
        with self.lock:
            self.base_url = base_url
            self.password = password
            self.username = username or "opencode"
            self.client = OpenCodeClient(
                base_url,
                directory=self.runtime.directory,
                password=password,
                username=self.username,
            )
            self.orchestrator.client = self.client
            state = self.orchestrator.load_state()
            state.opencode_base_url = base_url
            state.opencode_password = password
            state.opencode_username = self.username
            # We did NOT spawn this serve, so don't claim its pid. Clear the
            # stale value so /judge kill doesn't try to reap a process we
            # don't own.
            state.opencode_pid = None
            self.orchestrator.save_state(state)

    def _build_client(self) -> OpenCodeClient:
        state = OfficeOrchestrator(
            OpenCodeClient(self.base_url or "http://127.0.0.1:0", directory=self.runtime.directory, password=self.password, username=self.username),
            self.runtime,
        ).load_state()
        base_url = self.base_url or state.opencode_base_url
        password = self.password or state.opencode_password
        username = self.username or state.opencode_username or "opencode"
        if not self._server_alive(base_url, password, username):
            base_url, password, username, pid = self._start_opencode_server()
            state.opencode_base_url = base_url
            state.opencode_password = password
            state.opencode_username = username
            state.opencode_pid = pid
            OfficeOrchestrator(OpenCodeClient(base_url, directory=self.runtime.directory, password=password, username=username), self.runtime).save_state(state)
            self.base_url = base_url
            self.password = password
            self.username = username
        return OpenCodeClient(base_url, directory=self.runtime.directory, password=password, username=username)

    def _server_alive(self, base_url: str | None, password: str | None, username: str | None) -> bool:
        if not base_url:
            return False
        try:
            OpenCodeClient(base_url, directory=self.runtime.directory, password=password, username=username, timeout=2.0).list_sessions()
            return True
        except Exception:
            return False

    def _start_opencode_server(self) -> tuple[str, str, str, int]:
        password = self.password or secrets.token_urlsafe(18)
        username = self.username or "opencode"
        port = pick_free_port()
        log_handle = self.runtime.serve_log_path.open("a", encoding="utf-8")
        env = os.environ.copy()
        env["OPENCODE_SERVER_PASSWORD"] = password
        env["OPENCODE_SERVER_USERNAME"] = username
        process = subprocess.Popen(
            ["opencode", "serve", "--hostname", "127.0.0.1", "--port", str(port)],
            cwd=self.runtime.directory,
            env=env,
            stdout=log_handle,
            stderr=log_handle,
            stdin=subprocess.DEVNULL,
            start_new_session=True,
            text=True,
        )
        self.serve_process = process
        self.spawned_serve_pid = int(process.pid)
        base_url = f"http://127.0.0.1:{port}"
        wait_for_http(base_url)
        return base_url, password, username, int(process.pid)

    def attach_server(self, server: "OfficeHTTPServer") -> None:
        self.server = server

    def start(self) -> None:
        if self.runtime.pid_path.exists():
            self.runtime.pid_path.unlink(missing_ok=True)
        self.runtime.pid_path.write_text(str(os.getpid()) + "\n", encoding="utf-8")
        self.watch_thread.start()

    def request_shutdown_async(self) -> None:
        """Trigger shutdown without blocking the current handler thread.

        We must not call ``server.shutdown()`` from a request handler thread
        because it deadlocks against ``serve_forever``.
        """

        def _runner() -> None:
            self.shutdown()

        threading.Thread(target=_runner, name="office-shutdown", daemon=True).start()

    def _process_descendants(self, root_pid: int) -> list[int]:
        """Return root_pid and all of its descendant pids using /proc.

        Pure stdlib, no psutil dep. Self-pid is excluded so we don't kill the
        daemon while building the list.
        """
        children: dict[int, list[int]] = {}
        try:
            for entry in os.listdir("/proc"):
                if not entry.isdigit():
                    continue
                pid = int(entry)
                try:
                    with open(f"/proc/{pid}/status", "r", encoding="utf-8") as fh:
                        ppid = 0
                        for line in fh:
                            if line.startswith("PPid:"):
                                ppid = int(line.split()[1])
                                break
                except (FileNotFoundError, ProcessLookupError, PermissionError):
                    continue
                children.setdefault(ppid, []).append(pid)
        except FileNotFoundError:
            return [root_pid]

        result: list[int] = []
        stack = [root_pid]
        while stack:
            pid = stack.pop()
            if pid == os.getpid():
                continue
            result.append(pid)
            stack.extend(children.get(pid, []))
        return result

    def _kill_family(self, root_pid: int | None) -> list[int]:
        """Kill ``root_pid`` and every descendant. Returns the killed pids."""
        if root_pid is None:
            return []

        killed: list[int] = []
        # First try the process group, since we asked Popen for one.
        try:
            pgid = os.getpgid(root_pid)
        except ProcessLookupError:
            return []
        try:
            os.killpg(pgid, signal.SIGTERM)
        except (ProcessLookupError, PermissionError):
            pass

        # Give them a brief moment to clean up, but do not negotiate.
        deadline = time.time() + 1.5
        while time.time() < deadline:
            try:
                os.kill(root_pid, 0)
            except ProcessLookupError:
                break
            time.sleep(0.1)

        # Hard kill the entire descendant set, including any process that
        # broke out of the original process group (e.g. spike via setsid).
        for pid in self._process_descendants(root_pid):
            try:
                os.kill(pid, signal.SIGKILL)
                killed.append(pid)
            except ProcessLookupError:
                continue

        # Final pgkill to be sure.
        try:
            os.killpg(pgid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass

        # Reap our own Popen handle if applicable.
        proc = self.serve_process
        if proc is not None:
            try:
                proc.wait(timeout=2.0)
            except subprocess.TimeoutExpired:
                pass
        return killed

    def _kill_serve_process(self) -> list[int]:
        target_pid = self.spawned_serve_pid
        if target_pid is None:
            state = self.orchestrator.load_state()
            target_pid = state.opencode_pid
        return self._kill_family(target_pid)

    def _disable_and_abort_judge(self) -> list[str]:
        state = self.orchestrator.load_state()
        aborted: list[str] = []
        state.enabled = False
        self.orchestrator.save_state(state)
        if state.judge_session_id:
            try:
                self.client.abort_session(state.judge_session_id)
                aborted.append(state.judge_session_id)
            except Exception as exc:
                log_kv(
                    get_logger("office.daemon"),
                    _stdlib_logging.WARNING,
                    "failed to abort judge session during shutdown",
                    judge=state.judge_session_id,
                    error=str(exc),
                )
        return aborted

    def shutdown(self, *, hard: bool = False) -> dict[str, Any]:
        with self._shutdown_lock:
            if self._shutdown_done:
                return {"already_stopped": True, "killed": []}
            self._shutdown_done = True

        self.stop_event.set()
        aborted = self._disable_and_abort_judge()
        killed = self._kill_serve_process()
        self.orchestrator.event(
            "daemon_stop",
            hard=hard,
            aborted_sessions=aborted,
            killed_pids=killed,
            serve_pid=self.spawned_serve_pid,
        )

        # Tell the HTTP server to stop accepting new connections and unblock
        # serve_forever so main() exits.
        server = self.server
        if server is not None:
            try:
                server.shutdown()
            except Exception:
                pass
        self.runtime.pid_path.unlink(missing_ok=True)
        self.runtime.socket_path.unlink(missing_ok=True)
        return {"aborted_sessions": aborted, "killed": killed, "serve_pid": self.spawned_serve_pid}

    def _watch_loop(self) -> None:
        log = get_logger("office.daemon.watch")
        log.info("watch loop starting")
        while not self.stop_event.wait(5.0):
            with self.lock:
                state = self.orchestrator.load_state()
                if not state.enabled or not state.worker_session_id or not state.judge_session_id:
                    continue
                try:
                    state, changed = self.orchestrator.worker_update(state)
                    status = self.client.session_status(state.worker_session_id) or {}
                    log_kv(
                        log,
                        _stdlib_logging.DEBUG,
                        "watch tick",
                        changed=changed,
                        worker_status=status.get("type"),
                        paused=state.paused,
                    )
                    if changed:
                        self.orchestrator.nudge_judge(reason="worker completed an assistant turn", state=state)
                    elif status.get("type") == "idle":
                        self.orchestrator.nudge_judge(reason="worker idle", state=state)
                except Exception as exc:
                    log.exception("watch loop iteration failed")
                    self.orchestrator.event("watch_error", error=str(exc))
        log.info("watch loop exiting")

    def payload(self) -> dict[str, Any]:
        return self.orchestrator.status_payload()

    def handle(self, method: str, path: str, query: dict[str, str], body: dict[str, Any] | None) -> tuple[int, Any]:
        with self.lock:
            if method == "GET" and path == "/health":
                return 200, {"ok": True}
            if method == "GET" and path == "/status":
                return 200, self.payload()
            if method == "GET" and path == "/summary":
                return 200, self.orchestrator.summary_payload()
            if method == "GET" and path == "/paths":
                return 200, self.orchestrator.paths_payload()
            if method == "GET" and path == "/worker/messages":
                limit = int(query.get("limit", "5") or "5")
                return 200, {"messages": self.orchestrator.worker_messages(limit)}
            if method == "GET" and path == "/judge/messages":
                limit = int(query.get("limit", "5") or "5")
                return 200, {"messages": self.orchestrator.judge_messages(limit)}
            if method == "GET" and path == "/logs":
                lines = int(query.get("lines", "200") or "200")
                return 200, {
                    "daemon": self.orchestrator.daemon_log_tail(lines),
                    "opencode_serve": self.orchestrator.opencode_log_tail(lines),
                    "diagnostic": self.orchestrator.diagnostic_log_tail(lines),
                }
            if method == "POST" and path == "/judge/on":
                worker_session_id = (body or {}).get("worker_session_id")
                if not worker_session_id:
                    return 400, {"error": "worker_session_id is required"}
                # Plugin is running inside the user's opencode TUI process and
                # can hand us the URL/credentials of *that* serve. The session
                # ID the plugin sees only exists in that serve's database, so
                # we must talk to it directly. If a previous /judge/on already
                # configured this, repeating the call refreshes the binding.
                base_url = (body or {}).get("base_url")
                password = (body or {}).get("password")
                username = (body or {}).get("username") or "opencode"
                if base_url:
                    self.attach_user_serve(base_url=base_url, password=password, username=username)
                state = self.orchestrator.enable_judge(worker_session_id)
                return 200, self.orchestrator.status_payload(state)
            if method == "POST" and path == "/judge/off":
                state = self.orchestrator.disable_judge()
                return 200, self.orchestrator.status_payload(state)
            if method == "POST" and path == "/judge/poke":
                state = self.orchestrator.nudge_judge(reason=(body or {}).get("reason", "manual poke"), force=True)
                return 200, self.orchestrator.status_payload(state)
            if method == "POST" and path == "/judge/pause":
                state = self.orchestrator.pause()
                return 200, self.orchestrator.status_payload(state)
            if method == "POST" and path == "/judge/resume":
                state, replayed = self.orchestrator.resume()
                payload = self.orchestrator.status_payload(state)
                payload["replayed"] = replayed
                return 200, payload
            if method == "GET" and path == "/judge/queue":
                return 200, {"events": self.orchestrator.queued_events()}
            if method == "POST" and path == "/worker/prompt":
                message = (body or {}).get("message")
                if not message:
                    return 400, {"error": "message is required"}
                state = self.orchestrator.prompt_worker(message, compact_first=bool((body or {}).get("compact_first")))
                return 200, self.orchestrator.status_payload(state)
            if method == "POST" and path == "/worker/compact":
                state = self.orchestrator.compact_worker(message=(body or {}).get("message"))
                return 200, self.orchestrator.status_payload(state)
            if method == "POST" and path == "/judge/compact":
                state = self.orchestrator.compact_judge(message=(body or {}).get("message"))
                return 200, self.orchestrator.status_payload(state)
            if method == "POST" and path == "/shutdown":
                # Soft alias for /stop. Always behaves as a hard kill so the
                # human-facing `/judge stop` semantics are absolute.
                self.request_shutdown_async()
                return 200, {"ok": True}
            if method == "POST" and path == "/stop":
                # Schedule the actual kill in a thread so we can reply first.
                self.request_shutdown_async()
                return 200, {
                    "ok": True,
                    "daemon_pid": os.getpid(),
                    "serve_pid": self.spawned_serve_pid,
                }
            if method == "GET" and path == "/processes":
                serve = self.spawned_serve_pid
                tree = self._process_descendants(serve) if serve else []
                payload = []
                for pid in [os.getpid(), *tree]:
                    try:
                        with open(f"/proc/{pid}/cmdline", "rb") as fh:
                            cmdline = fh.read().replace(b"\0", b" ").decode(errors="replace").strip()
                    except (FileNotFoundError, ProcessLookupError, PermissionError):
                        continue
                    try:
                        with open(f"/proc/{pid}/status", "r", encoding="utf-8") as fh:
                            ppid = 0
                            for line in fh:
                                if line.startswith("PPid:"):
                                    ppid = int(line.split()[1])
                                    break
                    except (FileNotFoundError, ProcessLookupError, PermissionError):
                        continue
                    payload.append({"pid": pid, "ppid": ppid, "cmd": cmdline})
                return 200, {"daemon_pid": os.getpid(), "serve_pid": serve, "processes": payload}
        return 404, {"error": f"Unknown route: {method} {path}"}


class Handler(BaseHTTPRequestHandler):
    server: "OfficeHTTPServer"

    def log_message(self, format: str, *args: Any) -> None:
        # Suppress stdlib's stderr access log; we emit our own structured
        # records via the diagnostic logger in do_GET/do_POST.
        return

    def _split(self) -> tuple[str, dict[str, str]]:
        parsed = urlsplit(self.path)
        flat = {key: values[0] for key, values in parse_qs(parsed.query).items() if values}
        return parsed.path, flat

    def _read_json(self) -> dict[str, Any] | None:
        length = int(self.headers.get("Content-Length") or 0)
        if length <= 0:
            return None
        raw = self.rfile.read(length)
        if not raw:
            return None
        try:
            return json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError:
            return None

    def _reply(self, status: int, payload: Any) -> None:
        encoded = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def _dispatch(self, method: str, body: dict[str, Any] | None) -> None:
        log = get_logger("office.daemon.http")
        path, query = self._split()
        start = time.monotonic()
        try:
            status, payload = self.server.controller.handle(method, path, query, body)
        except Exception as exc:
            log.exception("handler crashed method=%s path=%s", method, path)
            status, payload = 500, {"error": f"internal error: {exc}"}
        elapsed_ms = int((time.monotonic() - start) * 1000)
        # Successful read endpoints are noisy; demote to DEBUG. Anything
        # >=400 or any POST is INFO so it stays visible at default level.
        level = _stdlib_logging.DEBUG if (method == "GET" and status < 400) else _stdlib_logging.INFO
        if status >= 500:
            level = _stdlib_logging.ERROR
        log_kv(
            log,
            level,
            "http",
            method=method,
            path=path,
            status=status,
            ms=elapsed_ms,
            query_keys=",".join(sorted(query.keys())) or "",
            body_keys=",".join(sorted((body or {}).keys())),
        )
        self._reply(status, payload)

    def do_GET(self) -> None:  # noqa: N802
        self._dispatch("GET", None)

    def do_POST(self) -> None:  # noqa: N802
        self._dispatch("POST", self._read_json())


class OfficeHTTPServer(ThreadedUnixServer):
    def __init__(self, socket_path: str, controller: OfficeController, log_path: Path) -> None:
        self.controller = controller
        self.log_path = log_path
        super().__init__(socket_path, Handler)

    def log(self, message: str) -> None:
        self.log_path.open("a", encoding="utf-8").write(message + "\n")


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="office-daemon")
    p.add_argument("--directory", required=True)
    p.add_argument("--session-id")
    p.add_argument("--socket-path", required=True)
    p.add_argument("--base-url")
    p.add_argument("--password")
    p.add_argument("--username")
    return p


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    runtime = project_runtime(args.directory, session_id=args.session_id)
    init_logger(runtime)
    log = get_logger("office.daemon")
    log_kv(
        log,
        _stdlib_logging.INFO,
        "daemon starting",
        directory=args.directory,
        session=args.session_id,
        socket=args.socket_path,
        pid=os.getpid(),
    )
    runtime.socket_path.unlink(missing_ok=True)
    controller = OfficeController(runtime, base_url=args.base_url, password=args.password, username=args.username)
    controller.start()
    server = OfficeHTTPServer(args.socket_path, controller, runtime.daemon_log_path)
    controller.attach_server(server)

    def _signal_shutdown(signum: int, _frame: Any) -> None:
        log_kv(log, _stdlib_logging.WARNING, "received signal, requesting shutdown", signal=signum)
        controller.request_shutdown_async()

    signal.signal(signal.SIGTERM, _signal_shutdown)
    signal.signal(signal.SIGINT, _signal_shutdown)

    try:
        log.info("entering serve_forever")
        server.serve_forever(poll_interval=0.5)
    except Exception:
        log.exception("serve_forever crashed")
        raise
    finally:
        log.info("serve_forever exited; running final shutdown")
        controller.shutdown()
        server.server_close()
        log.info("daemon main returning")
    return 0


if __name__ == "__main__":
    sys.exit(main())
