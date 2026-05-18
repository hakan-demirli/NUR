from __future__ import annotations

import argparse
import contextlib
import json
import logging as _stdlib_logging
import os
import signal
import socketserver
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler
from typing import Any
from urllib.parse import parse_qs, urlsplit

from .client import OpenCodeClient
from .logging import get_logger, init_logger, log_kv
from .orchestrator import OfficeOrchestrator, SlotNotFound
from .runtime import GlobalRuntime, ensure_cache_schema, global_runtime

DEFAULT_OPENCODE_URL = "http://127.0.0.1:4096"


class ThreadedUnixServer(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True
    allow_reuse_address = True


class OfficeController:
    def __init__(self, runtime: GlobalRuntime) -> None:
        self.runtime = runtime
        self.lock = threading.RLock()
        base_url = os.environ.get("OPENCODE_URL", DEFAULT_OPENCODE_URL).rstrip("/")
        client = OpenCodeClient(base_url, directory="")
        self.orchestrator = OfficeOrchestrator(client, runtime)
        self.stop_event = threading.Event()
        self.watch_thread = threading.Thread(
            target=self._watch_loop, name="office-watch", daemon=True
        )
        self.server: OfficeHTTPServer | None = None
        self._shutdown_lock = threading.Lock()
        self._shutdown_done = False

    def attach_server(self, server: OfficeHTTPServer) -> None:
        self.server = server

    def start(self) -> None:
        self.runtime.pid_path.unlink(missing_ok=True)
        self.runtime.pid_path.write_text(str(os.getpid()) + "\n", encoding="utf-8")
        self.watch_thread.start()

    def request_shutdown_async(self) -> None:
        def _runner() -> None:
            self.shutdown()

        threading.Thread(target=_runner, name="office-shutdown", daemon=True).start()

    def shutdown(self) -> dict[str, Any]:
        with self._shutdown_lock:
            if self._shutdown_done:
                return {"already_stopped": True}
            self._shutdown_done = True
        self.stop_event.set()
        self.orchestrator.event("daemon_stop", pid=os.getpid())
        server = self.server
        if server is not None:
            with contextlib.suppress(Exception):
                server.shutdown()
        self.runtime.pid_path.unlink(missing_ok=True)
        self.runtime.socket_path.unlink(missing_ok=True)
        return {"ok": True}

    _CIRCUIT_BREAKER_THRESHOLD = 5

    def _watch_loop(self) -> None:
        log = get_logger("office.daemon.watch")
        log.info("watch loop starting")
        while not self.stop_event.wait(5.0):
            with self.lock:
                state = self.orchestrator.load_state()
                dirty = False
                for slot in list(state.slots):
                    if (
                        not slot.enabled
                        or not slot.worker_session_id
                        or not slot.judge_session_id
                    ):
                        continue
                    if slot.consecutive_failures >= self._CIRCUIT_BREAKER_THRESHOLD:
                        continue
                    try:
                        changed = self.orchestrator.worker_update(slot)
                        client = self.orchestrator._client_for(slot.directory)
                        status = client.session_status(slot.worker_session_id) or {}
                        log_kv(
                            log,
                            _stdlib_logging.DEBUG,
                            "watch tick",
                            directory=slot.directory,
                            worker=slot.worker_session_id,
                            changed=changed,
                            worker_status=status.get("type"),
                            paused=slot.paused,
                        )
                        if slot.consecutive_failures > 0 or slot.health == "degraded":
                            slot.consecutive_failures = 0
                            if slot.health == "degraded":
                                slot.health = "ok"
                            dirty = True
                        if changed:
                            dirty = True
                            self.orchestrator.save_state(state)
                            self.orchestrator.nudge_judge(
                                slot.directory,
                                slot.worker_session_id,
                                reason="worker completed an assistant turn",
                            )

                    except Exception as exc:
                        slot.consecutive_failures += 1
                        dirty = True
                        log.exception("watch loop iteration failed")
                        self.orchestrator.event(
                            "watch_error",
                            directory=slot.directory,
                            worker=slot.worker_session_id,
                            error=str(exc),
                            consecutive_failures=slot.consecutive_failures,
                        )
                        if (
                            slot.consecutive_failures >= self._CIRCUIT_BREAKER_THRESHOLD
                            and slot.health != "degraded"
                        ):
                            slot.health = "degraded"
                            self.orchestrator.event(
                                "circuit_breaker_tripped",
                                directory=slot.directory,
                                worker=slot.worker_session_id,
                            )
                if dirty:
                    self.orchestrator.save_state(state)
        log.info("watch loop exiting")

    def handle(
        self, method: str, path: str, query: dict[str, str], body: dict[str, Any] | None
    ) -> tuple[int, Any]:
        with self.lock:
            try:
                return self._dispatch(method, path, query, body)
            except SlotNotFound as exc:
                return 404, {"error": str(exc)}
            except RuntimeError as exc:
                return 400, {"error": str(exc)}

    def _select(
        self, query: dict[str, str], body: dict[str, Any] | None
    ) -> tuple[str | None, str | None]:
        directory = (body or {}).get("directory") if body else None
        directory = directory or query.get("directory")
        worker = (body or {}).get("worker_session_id") if body else None
        worker = worker or query.get("worker_session_id") or query.get("worker")
        if directory:
            directory = os.path.abspath(directory)
        return directory, worker

    def _require(self, directory: str | None, worker: str | None) -> tuple[str, str]:
        if not directory:
            raise RuntimeError("directory is required")
        if not worker:
            raise RuntimeError("worker_session_id is required")
        return directory, worker

    def _dispatch(
        self, method: str, path: str, query: dict[str, str], body: dict[str, Any] | None
    ) -> tuple[int, Any]:
        if method == "GET" and path == "/health":
            return 200, {"ok": True}
        if method == "GET" and path == "/status":
            directory, worker = self._select(query, None)
            return 200, self.orchestrator.status_payload(
                directory=directory, worker_session_id=worker
            )
        if method == "GET" and path == "/slot/by-session":
            session_id = query.get("session_id", "").strip()
            if not session_id:
                raise RuntimeError("session_id is required")
            return 200, self.orchestrator.slot_by_session(session_id)
        if method == "GET" and path == "/summary":
            directory, worker = self._select(query, None)
            return 200, self.orchestrator.summary_payload(
                directory=directory, worker_session_id=worker
            )
        if method == "GET" and path == "/paths":
            return 200, self.orchestrator.paths_payload()
        if method == "GET" and path == "/logs":
            lines = int(query.get("lines", "200") or "200")
            return 200, {
                "daemon": self.orchestrator.daemon_log_tail(lines),
                "diagnostic": self.orchestrator.diagnostic_log_tail(lines),
            }

        if method == "GET" and path == "/worker/messages":
            directory, worker = self._require(*self._select(query, None))
            limit = int(query.get("limit", "5") or "5")
            return 200, {
                "messages": self.orchestrator.worker_messages(directory, worker, limit)
            }
        if method == "GET" and path == "/judge/messages":
            directory, worker = self._require(*self._select(query, None))
            limit = int(query.get("limit", "5") or "5")
            return 200, {
                "messages": self.orchestrator.judge_messages(directory, worker, limit)
            }
        if method == "GET" and path == "/judge/queue":
            directory, worker = self._require(*self._select(query, None))
            return 200, {"events": self.orchestrator.queued_events(directory, worker)}

        if method == "POST" and path == "/judge/on":
            directory, worker = self._require(*self._select(query, body))
            slot = self.orchestrator.enable_judge(directory, worker)
            return 200, self.orchestrator.status_payload(
                directory=directory, worker_session_id=worker
            ) | {
                "state": {
                    "worker_session_id": slot.worker_session_id,
                    "judge_session_id": slot.judge_session_id,
                },
            }
        if method == "POST" and path == "/judge/off":
            directory, worker = self._require(*self._select(query, body))
            self.orchestrator.disable_judge(directory, worker)
            return 200, self.orchestrator.status_payload(
                directory=directory, worker_session_id=worker
            )
        if method == "POST" and path == "/judge/forget":
            directory, worker = self._require(*self._select(query, body))
            removed = self.orchestrator.forget_slot(directory, worker)
            return 200, {"removed": removed}
        if method == "POST" and path == "/judge/poke":
            directory, worker = self._require(*self._select(query, body))
            self.orchestrator.nudge_judge(
                directory,
                worker,
                reason=(body or {}).get("reason", "manual poke"),
                force=True,
            )
            return 200, self.orchestrator.status_payload(
                directory=directory, worker_session_id=worker
            )
        if method == "POST" and path == "/judge/pause":
            directory, worker = self._require(*self._select(query, body))
            self.orchestrator.pause(directory, worker)
            return 200, self.orchestrator.status_payload(
                directory=directory, worker_session_id=worker
            )
        if method == "POST" and path == "/judge/resume":
            directory, worker = self._require(*self._select(query, body))
            _, replayed = self.orchestrator.resume(directory, worker)
            payload = self.orchestrator.status_payload(
                directory=directory, worker_session_id=worker
            )
            payload["replayed"] = replayed
            return 200, payload

        if method == "POST" and path == "/worker/prompt":
            directory, worker = self._require(*self._select(query, body))
            message = (body or {}).get("message")
            if not message:
                return 400, {"error": "message is required"}
            self.orchestrator.prompt_worker(
                directory,
                worker,
                message,
                compact_first=bool((body or {}).get("compact_first")),
            )
            return 200, self.orchestrator.status_payload(
                directory=directory, worker_session_id=worker
            )
        if method == "POST" and path == "/worker/compact":
            directory, worker = self._require(*self._select(query, body))
            self.orchestrator.compact_worker(
                directory, worker, message=(body or {}).get("message")
            )
            return 200, self.orchestrator.status_payload(
                directory=directory, worker_session_id=worker
            )
        if method == "POST" and path == "/judge/compact":
            directory, worker = self._require(*self._select(query, body))
            self.orchestrator.compact_judge(
                directory, worker, message=(body or {}).get("message")
            )
            return 200, self.orchestrator.status_payload(
                directory=directory, worker_session_id=worker
            )

        if method == "POST" and path in ("/shutdown", "/stop"):
            self.request_shutdown_async()
            return 200, {"ok": True, "daemon_pid": os.getpid()}

        if method == "GET" and path == "/processes":
            try:
                with open(f"/proc/{os.getpid()}/cmdline", "rb") as fh:
                    cmd = (
                        fh.read().replace(b"\0", b" ").decode(errors="replace").strip()
                    )
            except OSError:
                cmd = ""
            return 200, {
                "daemon_pid": os.getpid(),
                "processes": [{"pid": os.getpid(), "ppid": os.getppid(), "cmd": cmd}],
            }

        return 404, {"error": f"Unknown route: {method} {path}"}


class Handler(BaseHTTPRequestHandler):
    server: OfficeHTTPServer

    def log_message(self, format: str, *args: Any) -> None:
        return

    def _split(self) -> tuple[str, dict[str, str]]:
        parsed = urlsplit(self.path)
        flat = {
            key: values[0] for key, values in parse_qs(parsed.query).items() if values
        }
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
        except Exception as exc:  # pragma: no cover - defensive
            log.exception("handler crashed method=%s path=%s", method, path)
            status, payload = 500, {"error": f"internal error: {exc}"}
        elapsed_ms = int((time.monotonic() - start) * 1000)
        level = (
            _stdlib_logging.DEBUG
            if (method == "GET" and status < 400)
            else _stdlib_logging.INFO
        )
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

    def do_GET(self) -> None:
        self._dispatch("GET", None)

    def do_POST(self) -> None:
        self._dispatch("POST", self._read_json())


class OfficeHTTPServer(ThreadedUnixServer):
    def __init__(self, socket_path: str, controller: OfficeController) -> None:
        self.controller = controller
        super().__init__(socket_path, Handler)


def _make_runtime_for_logger() -> Any:
    return global_runtime()


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="office-daemon")
    p.add_argument("--socket-path", required=False, default=None)
    return p


def _acquire_singleton_lock(runtime: GlobalRuntime, log: Any) -> Any:
    import fcntl

    runtime.lock_path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(str(runtime.lock_path), os.O_WRONLY | os.O_CREAT, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError as exc:
        os.close(fd)
        log_kv(
            log,
            _stdlib_logging.ERROR,
            "another office daemon owns the lock; refusing to start",
            lock_path=str(runtime.lock_path),
            error=str(exc),
        )
        return None
    try:
        os.ftruncate(fd, 0)
        os.write(fd, (str(os.getpid()) + "\n").encode("utf-8"))
        os.fsync(fd)
    except OSError:
        pass
    return fd


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    migrated = ensure_cache_schema()
    runtime = global_runtime()
    init_logger(runtime)
    log = get_logger("office.daemon")
    lock_fd = _acquire_singleton_lock(runtime, log)
    if lock_fd is None:
        return 3
    log_kv(
        log,
        _stdlib_logging.INFO,
        "daemon starting",
        socket=str(runtime.socket_path),
        pid=os.getpid(),
        opencode_url=os.environ.get("OPENCODE_URL", DEFAULT_OPENCODE_URL),
        cache_migrated=migrated,
    )
    runtime.socket_path.unlink(missing_ok=True)
    controller = OfficeController(runtime)
    try:
        counters = controller.orchestrator.reconcile_on_boot()
        log_kv(log, _stdlib_logging.INFO, "boot reconciliation done", **counters)
    except Exception:
        log.exception("boot reconciliation failed; continuing")
    controller.start()
    socket_path = args.socket_path or str(runtime.socket_path)
    server = OfficeHTTPServer(socket_path, controller)
    controller.attach_server(server)

    def _signal_shutdown(signum: int, _frame: Any) -> None:
        log_kv(
            log,
            _stdlib_logging.WARNING,
            "received signal, requesting shutdown",
            signal=signum,
        )
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
        with contextlib.suppress(OSError):
            os.close(lock_fd)
        with contextlib.suppress(OSError):
            runtime.lock_path.unlink(missing_ok=True)
        log.info("daemon main returning")
    return 0


if __name__ == "__main__":
    sys.exit(main())
