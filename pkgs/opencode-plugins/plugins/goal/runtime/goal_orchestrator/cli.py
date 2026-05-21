from __future__ import annotations

import argparse
import contextlib
import http.client
import json
import os
import signal
import socket
import subprocess
import sys
import time
from typing import Any

from .runtime import global_runtime


class _UnixHTTP(http.client.HTTPConnection):
    def __init__(self, socket_path: str, timeout: float = 30.0) -> None:
        super().__init__("localhost", timeout=timeout)
        self.socket_path = socket_path

    def connect(self) -> None:
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.socket_path)


def daemon_request(
    runtime: Any,
    method: str,
    path: str,
    *,
    body: dict[str, Any] | None = None,
    timeout: float = 30.0,
) -> Any:
    conn = _UnixHTTP(str(runtime.socket_path), timeout=timeout)
    headers = {"Content-Type": "application/json"}
    payload = None if body is None else json.dumps(body)
    conn.request(method, path, body=payload, headers=headers)
    response = conn.getresponse()
    raw = response.read().decode("utf-8")
    data = json.loads(raw) if raw else {}
    if response.status >= 400:
        raise RuntimeError(data.get("error", f"daemon error {response.status}"))
    return data


def daemon_running(runtime: Any) -> bool:
    if not runtime.socket_path.exists():
        return False
    try:
        daemon_request(runtime, "GET", "/health", timeout=2.0)
        return True
    except Exception:
        return False


def _self_invocation() -> list[str]:
    arg0 = sys.argv[0] or ""
    if arg0 and os.path.isfile(arg0) and os.access(arg0, os.X_OK):
        return [arg0]
    return [sys.executable, "-m", "goal_orchestrator.cli"]


def start_daemon() -> dict[str, Any]:
    runtime = global_runtime()
    if daemon_running(runtime):
        return daemon_request(runtime, "GET", "/status")

    runtime.socket_path.unlink(missing_ok=True)
    runtime.daemon_log_path.touch(exist_ok=True)
    env = os.environ.copy()
    invocation = _self_invocation()
    if len(invocation) > 1 and invocation[1:3] == ["-m", "goal_orchestrator.cli"]:
        pkg_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        existing = env.get("PYTHONPATH", "")
        env["PYTHONPATH"] = f"{pkg_root}:{existing}" if existing else pkg_root

    with runtime.daemon_log_path.open("a", encoding="utf-8") as log_handle:
        subprocess.Popen(
            [*invocation, "daemon-run", "--socket-path", str(runtime.socket_path)],
            cwd=str(runtime.root),
            stdout=log_handle,
            stderr=log_handle,
            stdin=subprocess.DEVNULL,
            start_new_session=True,
            env=env,
            text=True,
        )

    deadline = time.time() + 15.0
    while time.time() < deadline:
        if daemon_running(runtime):
            return daemon_request(runtime, "GET", "/status")
        time.sleep(0.2)
    raise RuntimeError(f"Timed out waiting for goal daemon at {runtime.socket_path}")


def ensure_daemon() -> None:
    start_daemon()


def _read_tail(path: Any, lines: int) -> list[str]:
    if not path.exists():
        return []
    try:
        return path.read_text(errors="replace").splitlines()[-lines:]
    except OSError:
        return []


def _kill_family(root_pid: int) -> list[int]:
    import contextlib as _ctx

    try:
        pgid = os.getpgid(root_pid)
    except ProcessLookupError:
        return []

    def _descendants(pid: int) -> list[int]:
        children: dict[int, list[int]] = {}
        try:
            for entry in os.listdir("/proc"):
                if not entry.isdigit():
                    continue
                p = int(entry)
                try:
                    with open(f"/proc/{p}/status", encoding="utf-8") as fh:
                        ppid = 0
                        for line in fh:
                            if line.startswith("PPid:"):
                                ppid = int(line.split()[1])
                                break
                except (FileNotFoundError, ProcessLookupError, PermissionError):
                    continue
                children.setdefault(ppid, []).append(p)
        except FileNotFoundError:
            return [pid]
        result: list[int] = []
        stack = [pid]
        while stack:
            cur = stack.pop()
            if cur == os.getpid():
                continue
            result.append(cur)
            stack.extend(children.get(cur, []))
        return result

    targets = _descendants(root_pid)
    killed: list[int] = []
    with _ctx.suppress(ProcessLookupError, PermissionError):
        os.killpg(pgid, signal.SIGTERM)
        killed.extend(targets)
    deadline = time.time() + 1.5
    while time.time() < deadline:
        try:
            os.kill(root_pid, 0)
        except ProcessLookupError:
            break
        time.sleep(0.1)
    for p in _descendants(root_pid):
        with _ctx.suppress(ProcessLookupError):
            os.kill(p, signal.SIGKILL)
            killed.append(p)
    with _ctx.suppress(ProcessLookupError, PermissionError):
        os.killpg(pgid, signal.SIGKILL)
    return killed


def cmd_daemon_start(args: argparse.Namespace) -> int:
    print(json.dumps(start_daemon(), indent=2, sort_keys=True))
    return 0


def cmd_daemon_run(args: argparse.Namespace) -> int:
    from .daemon import main as daemon_main

    argv: list[str] = []
    if args.socket_path:
        argv.extend(["--socket-path", args.socket_path])
    return daemon_main(argv)


def cmd_daemon_stop(args: argparse.Namespace) -> int:
    runtime = global_runtime()
    pids: list[int] = []
    if daemon_running(runtime):
        try:
            payload = daemon_request(runtime, "GET", "/processes", timeout=2.0)
            if v := payload.get("daemon_pid"):
                pids.append(int(v))
        except Exception:
            pass
    if runtime.pid_path.exists():
        with contextlib.suppress(ValueError):
            pids.append(int(runtime.pid_path.read_text(encoding="utf-8").strip()))
    pids = list(dict.fromkeys(p for p in pids if p != os.getpid()))
    killed: list[int] = []
    for pid in pids:
        killed.extend(_kill_family(pid))
    runtime.socket_path.unlink(missing_ok=True)
    runtime.pid_path.unlink(missing_ok=True)
    print(
        json.dumps(
            {
                "ok": True,
                "attempted_pids": pids,
                "killed_pids": list(dict.fromkeys(killed)),
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_status(args: argparse.Namespace) -> int:
    ensure_daemon()
    runtime = global_runtime()
    d = args.directory or os.getcwd()
    qs = f"?directory={os.path.abspath(d)}"
    print(
        json.dumps(
            daemon_request(runtime, "GET", f"/status{qs}"), indent=2, sort_keys=True
        )
    )
    return 0


def cmd_doctor(args: argparse.Namespace) -> int:
    runtime = global_runtime()
    payload: dict[str, Any] = {
        "socket": str(runtime.socket_path),
        "daemon_running": daemon_running(runtime),
        "opencode_url": os.environ.get("OPENCODE_URL"),
    }
    if payload["daemon_running"]:
        try:
            payload["status"] = daemon_request(runtime, "GET", "/status", timeout=5.0)
        except Exception as exc:
            payload["error"] = str(exc)
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0


def cmd_logs(args: argparse.Namespace) -> int:
    runtime = global_runtime()
    if daemon_running(runtime):
        print(
            json.dumps(
                daemon_request(runtime, "GET", f"/logs?lines={args.lines}"),
                indent=2,
                sort_keys=True,
            )
        )
        return 0
    print(
        json.dumps(
            {"daemon": _read_tail(runtime.daemon_log_path, args.lines)},
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_ps(args: argparse.Namespace) -> int:
    runtime = global_runtime()
    if not daemon_running(runtime):
        print(json.dumps({"daemon_running": False}, indent=2, sort_keys=True))
        return 0
    print(
        json.dumps(
            daemon_request(runtime, "GET", "/processes"), indent=2, sort_keys=True
        )
    )
    return 0


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(prog="opencode-goal")
    root.add_argument("--directory", default=None)
    root.add_argument("--timeout", type=float, default=30.0)
    sub = root.add_subparsers(dest="command", required=True)

    def add(name: str, fn: Any, **kwargs: Any) -> argparse.ArgumentParser:
        sp = sub.add_parser(name, **kwargs)
        sp.set_defaults(func=fn)
        return sp

    add("doctor", cmd_doctor)
    daemon_run = add("daemon-run", cmd_daemon_run)
    daemon_run.add_argument("--socket-path", required=False, default=None)
    add("daemon-start", cmd_daemon_start)
    add("daemon-stop", cmd_daemon_stop)
    add("status", cmd_status)
    add("ps", cmd_ps)
    logs_cmd = add("logs", cmd_logs)
    logs_cmd.add_argument("--lines", type=int, default=200)

    return root


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    return int(args.func(args))


if __name__ == "__main__":
    sys.exit(main())
