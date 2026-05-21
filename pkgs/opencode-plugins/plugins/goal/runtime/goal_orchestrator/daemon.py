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

from .logging import get_logger, init_logger, log_kv
from .orchestrator import GoalNotFound, GoalOrchestrator, GoalValidationError
from .runtime import global_runtime
from .state import HRConfig, JudgeConfig, goal_to_dict

WATCH_INTERVAL_S = 4.0

_log = get_logger("goal.daemon")


class _UnixServer(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True
    allow_reuse_address = True


class GoalController:
    def __init__(self) -> None:
        self.runtime = global_runtime()
        self.orchestrator = GoalOrchestrator(self.runtime)
        self.stop_event = threading.Event()
        self.lock = threading.RLock()
        self._server: Any = None
        self._shutdown_lock = threading.Lock()
        self._shutdown_done = False
        self._watch_thread = threading.Thread(
            target=self._watch_loop, name="goal-watch", daemon=True
        )

    def attach_server(self, server: Any) -> None:
        self._server = server

    def start(self) -> None:
        self.runtime.pid_path.unlink(missing_ok=True)
        self.runtime.pid_path.write_text(str(os.getpid()) + "\n", encoding="utf-8")
        self._watch_thread.start()

    def request_shutdown_async(self) -> None:
        threading.Thread(
            target=self.shutdown, name="goal-shutdown", daemon=True
        ).start()

    def shutdown(self) -> dict[str, Any]:
        with self._shutdown_lock:
            if self._shutdown_done:
                return {"already_stopped": True}
            self._shutdown_done = True
        self.stop_event.set()
        if self._server:
            with contextlib.suppress(Exception):
                self._server.shutdown()
        self.runtime.pid_path.unlink(missing_ok=True)
        self.runtime.socket_path.unlink(missing_ok=True)
        return {"ok": True}

    def _watch_loop(self) -> None:
        log_kv(_log, _stdlib_logging.INFO, "watch loop starting")
        while not self.stop_event.wait(WATCH_INTERVAL_S):
            with self.lock:
                try:
                    state = self.orchestrator.load_state()
                    dirty = self.orchestrator.watch_tick(state)
                    if dirty:
                        self.orchestrator.save_state(state)
                except Exception:
                    _log.exception("watch loop iteration failed")
        log_kv(_log, _stdlib_logging.INFO, "watch loop exiting")

    def handle(
        self, method: str, path: str, query: dict[str, str], body: dict[str, Any] | None
    ) -> tuple[int, Any]:
        with self.lock:
            try:
                return self._dispatch(method, path, query, body)
            except GoalNotFound as exc:
                return 404, {"error": str(exc)}
            except GoalValidationError as exc:
                return 400, {"error": str(exc)}
            except RuntimeError as exc:
                return 400, {"error": str(exc)}

    def _q(
        self, query: dict[str, str], body: dict[str, Any] | None, key: str
    ) -> str | None:
        return (body or {}).get(key) or query.get(key) or None

    def _directory(self, query: dict[str, str], body: dict[str, Any] | None) -> str:
        d = self._q(query, body, "directory")
        if not d:
            raise RuntimeError("directory is required")
        return os.path.abspath(d)

    def _dispatch(
        self, method: str, path: str, query: dict[str, str], body: dict[str, Any] | None
    ) -> tuple[int, Any]:
        orch = self.orchestrator

        if method == "GET" and path == "/health":
            return 200, {"ok": True}

        if method == "GET" and path == "/status":
            d = self._q(query, None, "directory")
            if d:
                return 200, orch.goal_status(d)
            return 200, orch.all_goals_status()

        if method == "GET" and path == "/logs":
            lines = int(query.get("lines", "200") or "200")
            return 200, {"daemon": orch.daemon_log_tail(lines)}

        if method == "GET" and path == "/ledger":
            d = self._directory(query, None)
            return 200, {"entries": orch.read_ledger(d)}

        if method == "GET" and path == "/session/messages":
            sid = self._q(query, None, "session_id")
            if not sid:
                return 400, {"error": "session_id is required"}
            limit = int(query.get("limit", "20") or "20")
            return 200, {"messages": orch.get_user_messages(sid, limit)}

        if method == "POST" and path == "/goal/start":
            d = self._directory(query, body)
            worker = self._q(query, body, "worker_session_id")
            if not worker:
                return 400, {"error": "worker_session_id is required"}
            objective = (body or {}).get("objective", "").strip()
            if not objective:
                return 400, {"error": "objective is required"}
            hr = _hr_from_body((body or {}).get("hr") or {})
            rec = orch.start_goal(d, worker, objective, hr)
            return 200, {"goal": goal_to_dict(rec)}

        if method == "POST" and path == "/goal/pause":
            d = self._directory(query, body)
            return 200, {"goal": goal_to_dict(orch.pause_goal(d))}

        if method == "POST" and path == "/goal/resume":
            d = self._directory(query, body)
            return 200, {"goal": goal_to_dict(orch.resume_goal(d))}

        if method == "POST" and path == "/goal/clear":
            d = self._directory(query, body)
            removed = orch.clear_goal(d)
            return 200, {"removed": removed}

        if method == "POST" and path == "/goal/append":
            d = self._directory(query, body)
            text = (body or {}).get("text", "").strip()
            if not text:
                return 400, {"error": "text is required"}
            return 200, {"goal": goal_to_dict(orch.append_goal(d, text))}

        if method == "POST" and path == "/goal/checkpoint":
            d = self._directory(query, body)
            return 200, {"goal": goal_to_dict(orch.set_checkpoint(d))}

        if method == "POST" and path == "/goal/checkpoint/recover":
            d = self._directory(query, body)
            return 200, {"goal": goal_to_dict(orch.recover_from_checkpoint(d))}

        if method == "POST" and path == "/goal/pin":
            d = self._directory(query, body)
            b = body or {}
            return 200, {
                "goal": goal_to_dict(
                    orch.pin_message(
                        d,
                        b.get("session_id", ""),
                        b.get("message_id", ""),
                        b.get("preview", ""),
                    )
                )
            }

        if method == "POST" and path == "/goal/unpin":
            d = self._directory(query, body)
            msg_id = (body or {}).get("message_id", "")
            return 200, {"goal": goal_to_dict(orch.unpin_message(d, msg_id))}

        if method == "POST" and path == "/hr/update":
            d = self._directory(query, body)
            hr = _hr_from_body((body or {}).get("hr") or {})
            return 200, {"goal": goal_to_dict(orch.update_hr(d, hr))}

        if method == "POST" and path == "/loop/start":
            d = self._directory(query, body)
            interval = int((body or {}).get("interval_seconds", 300) or 300)
            return 200, {"goal": goal_to_dict(orch.set_loop(d, interval))}

        if method == "POST" and path == "/loop/stop":
            d = self._directory(query, body)
            return 200, {"goal": goal_to_dict(orch.clear_loop(d))}

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


def _hr_from_body(d: dict[str, Any]) -> HRConfig:
    def _judge(j: dict[str, Any]) -> JudgeConfig:
        return JudgeConfig(
            session_id=j.get("session_id"),
            provider_id=j.get("provider_id"),
            model_id=j.get("model_id"),
            variant=j.get("variant"),
            personality_key=j.get("personality_key"),
            personality_custom=j.get("personality_custom"),
        )

    return HRConfig(
        president=_judge(d.get("president") or {}),
        judges=[_judge(j) for j in (d.get("judges") or [])],
    )


class _Handler(BaseHTTPRequestHandler):
    server: Any

    def log_message(self, format: str, *args: Any) -> None:
        return

    def _split(self) -> tuple[str, dict[str, str]]:
        parsed = urlsplit(self.path)
        flat = {k: vals[0] for k, vals in parse_qs(parsed.query).items() if vals}
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
        path, query = self._split()
        start = time.monotonic()
        try:
            status, payload = self.server.controller.handle(method, path, query, body)
        except Exception as exc:
            _log.exception("handler crashed %s %s", method, path)
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
            _log, level, "http", method=method, path=path, status=status, ms=elapsed_ms
        )
        self._reply(status, payload)

    def do_GET(self) -> None:
        self._dispatch("GET", None)

    def do_POST(self) -> None:
        self._dispatch("POST", self._read_json())


class GoalHTTPServer(_UnixServer):
    def __init__(self, socket_path: str, controller: GoalController) -> None:
        self.controller = controller
        super().__init__(socket_path, _Handler)


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="goal-daemon")
    p.add_argument("--socket-path", required=False, default=None)
    return p


def _acquire_lock(runtime: Any, log: Any) -> Any:
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
            "another goal daemon owns the lock; refusing to start",
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
    runtime = global_runtime()
    init_logger(runtime)
    log = get_logger("goal.daemon")

    lock_fd = _acquire_lock(runtime, log)
    if lock_fd is None:
        return 3

    log_kv(
        log,
        _stdlib_logging.INFO,
        "daemon starting",
        socket=str(runtime.socket_path),
        pid=os.getpid(),
    )

    runtime.socket_path.unlink(missing_ok=True)
    controller = GoalController()
    controller.start()

    socket_path = args.socket_path or str(runtime.socket_path)
    server = GoalHTTPServer(socket_path, controller)
    controller.attach_server(server)

    def _on_signal(signum: int, _frame: Any) -> None:
        log_kv(
            log,
            _stdlib_logging.WARNING,
            "signal received, shutting down",
            signal=signum,
        )
        controller.request_shutdown_async()

    signal.signal(signal.SIGTERM, _on_signal)
    signal.signal(signal.SIGINT, _on_signal)

    try:
        log.info("entering serve_forever")
        server.serve_forever(poll_interval=0.5)
    except Exception:
        log.exception("serve_forever crashed")
        raise
    finally:
        log.info("serve_forever exited; finalising shutdown")
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
