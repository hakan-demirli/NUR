from __future__ import annotations

import argparse
import http.client
import json
import os
import signal
import socket
import subprocess
import sys
import time
from typing import Any

from .client import OpenCodeClient
from .runtime import project_runtime


class UnixHTTPConnection(http.client.HTTPConnection):
    def __init__(self, socket_path: str, timeout: float = 30.0) -> None:
        super().__init__("localhost", timeout=timeout)
        self.socket_path = socket_path

    def connect(self) -> None:
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.socket_path)


def daemon_request(runtime, method: str, path: str, *, body: dict[str, Any] | None = None, timeout: float = 30.0) -> Any:
    conn = UnixHTTPConnection(str(runtime.socket_path), timeout=timeout)
    headers = {"Content-Type": "application/json"}
    payload = None if body is None else json.dumps(body)
    conn.request(method, path, body=payload, headers=headers)
    response = conn.getresponse()
    raw = response.read().decode("utf-8")
    data = json.loads(raw) if raw else {}
    if response.status >= 400:
        raise RuntimeError(data.get("error", f"daemon error {response.status}"))
    return data


def daemon_running(runtime) -> bool:
    if not runtime.socket_path.exists():
        return False
    try:
        daemon_request(runtime, "GET", "/health", timeout=2.0)
        return True
    except Exception:
        return False


def start_daemon(args: argparse.Namespace) -> dict[str, Any]:
    runtime = project_runtime(args.directory, session_id=args.session_id)
    if daemon_running(runtime):
        return daemon_request(runtime, "GET", "/status")

    runtime.socket_path.unlink(missing_ok=True)
    runtime.daemon_log_path.touch(exist_ok=True)
    with runtime.daemon_log_path.open("a", encoding="utf-8") as log_handle:
        subprocess.Popen(
            [
                sys.argv[0],
                "--directory",
                args.directory,
                *(["--session-id", args.session_id] if args.session_id else []),
                *(["--base-url", args.base_url] if args.base_url else []),
                *(["--password", args.password] if args.password else []),
                *(["--username", args.username] if args.username else []),
                "daemon-run",
                "--socket-path",
                str(runtime.socket_path),
            ],
            cwd=args.directory,
            stdout=log_handle,
            stderr=log_handle,
            stdin=subprocess.DEVNULL,
            start_new_session=True,
            env=os.environ.copy(),
            text=True,
        )

    deadline = time.time() + 15.0
    while time.time() < deadline:
        if daemon_running(runtime):
            return daemon_request(runtime, "GET", "/status")
        time.sleep(0.2)
    raise RuntimeError(f"Timed out waiting for office daemon at {runtime.socket_path}")


def ensure_daemon(args: argparse.Namespace) -> dict[str, Any]:
    return start_daemon(args)


def cmd_doctor(args: argparse.Namespace) -> int:
    runtime = project_runtime(args.directory, session_id=args.session_id)
    payload: dict[str, Any] = {
        "directory": args.directory,
        "session_id": args.session_id,
        "socket": str(runtime.socket_path),
        "daemon_running": daemon_running(runtime),
    }
    if payload["daemon_running"]:
        try:
            payload["summary"] = daemon_request(runtime, "GET", "/summary", timeout=5.0)
            payload["processes"] = daemon_request(runtime, "GET", "/processes", timeout=5.0)
        except Exception as exc:
            payload["error"] = str(exc)
    else:
        payload["hint"] = "Daemon is not running. Run `daemon-start` or `/judge on`."
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0


def cmd_paths(args: argparse.Namespace) -> int:
    runtime = project_runtime(args.directory, session_id=args.session_id)
    if daemon_running(runtime):
        print(json.dumps(daemon_request(runtime, "GET", "/paths"), indent=2, sort_keys=True))
    else:
        print(json.dumps({
            "directory": args.directory,
            "session_id": args.session_id,
            "runtime_dir": str(runtime.root),
            "socket": str(runtime.socket_path),
            "state_file": str(runtime.state_path),
            "pid_file": str(runtime.pid_path),
            "daemon_log": str(runtime.daemon_log_path),
            "opencode_serve_log": str(runtime.serve_log_path),
            "diagnostic_log": str(runtime.diagnostic_log_symlink),
            "diagnostic_log_dir": str(runtime.diagnostic_log_dir),
            "daemon_running": False,
        }, indent=2, sort_keys=True))
    return 0


def _read_tail(path, lines: int) -> list[str]:
    if not path.exists():
        return []
    try:
        return path.read_text(errors="replace").splitlines()[-lines:]
    except OSError:
        return []


def _resolve_diagnostic_log(runtime) -> Any:
    """Return the active diagnostic log (target of the symlink, or newest file)."""
    symlink = runtime.diagnostic_log_symlink
    if symlink.exists():
        return symlink.resolve() if symlink.is_symlink() else symlink
    log_dir = runtime.diagnostic_log_dir
    if not log_dir.exists():
        return None
    candidates = sorted(p for p in log_dir.iterdir() if p.is_file() and p.suffix == ".log")
    return candidates[-1] if candidates else None


def cmd_logs(args: argparse.Namespace) -> int:
    runtime = project_runtime(args.directory, session_id=args.session_id)
    diag_path = _resolve_diagnostic_log(runtime)
    diag_lines = _read_tail(diag_path, args.lines) if diag_path else []
    if daemon_running(runtime):
        payload = daemon_request(runtime, "GET", f"/logs?lines={args.lines}")
        payload["diagnostic"] = diag_lines
        payload["diagnostic_path"] = str(diag_path) if diag_path else None
        print(json.dumps(payload, indent=2, sort_keys=True))
        return 0
    # Daemon down: read straight from disk so logs/ps still work post-mortem.
    daemon_log = _read_tail(runtime.daemon_log_path, args.lines)
    serve_log = _read_tail(runtime.serve_log_path, args.lines)
    print(
        json.dumps(
            {
                "daemon": daemon_log,
                "opencode_serve": serve_log,
                "diagnostic": diag_lines,
                "diagnostic_path": str(diag_path) if diag_path else None,
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_ps(args: argparse.Namespace) -> int:
    runtime = project_runtime(args.directory, session_id=args.session_id)
    if not daemon_running(runtime):
        print(json.dumps({"daemon_running": False}, indent=2, sort_keys=True))
        return 0
    print(json.dumps(daemon_request(runtime, "GET", "/processes"), indent=2, sort_keys=True))
    return 0


def cmd_status(args: argparse.Namespace) -> int:
    ensure_daemon(args)
    runtime = project_runtime(args.directory, session_id=args.session_id)
    print(json.dumps(daemon_request(runtime, "GET", "/status"), indent=2, sort_keys=True))
    return 0


def cmd_summary(args: argparse.Namespace) -> int:
    ensure_daemon(args)
    runtime = project_runtime(args.directory, session_id=args.session_id)
    print(json.dumps(daemon_request(runtime, "GET", "/summary"), indent=2, sort_keys=True))
    return 0


def cmd_daemon_start(args: argparse.Namespace) -> int:
    print(json.dumps(start_daemon(args), indent=2, sort_keys=True))
    return 0


def cmd_daemon_run(args: argparse.Namespace) -> int:
    from .daemon import main as daemon_main

    argv = [
        "--directory",
        args.directory,
        *(["--session-id", args.session_id] if args.session_id else []),
        "--socket-path",
        args.socket_path,
    ]
    if args.base_url:
        argv.extend(["--base-url", args.base_url])
    if args.password:
        argv.extend(["--password", args.password])
    if args.username:
        argv.extend(["--username", args.username])
    return daemon_main(argv)


def process_descendants(root_pid: int) -> list[int]:
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


def kill_family(root_pid: int) -> list[int]:
    try:
        pgid = os.getpgid(root_pid)
    except ProcessLookupError:
        return []

    targets = process_descendants(root_pid)
    killed: list[int] = []
    try:
        os.killpg(pgid, signal.SIGTERM)
        killed.extend(targets)
    except (ProcessLookupError, PermissionError):
        pass

    deadline = time.time() + 1.5
    while time.time() < deadline:
        try:
            os.kill(root_pid, 0)
        except ProcessLookupError:
            break
        time.sleep(0.1)

    for pid in process_descendants(root_pid):
        try:
            os.kill(pid, signal.SIGKILL)
            killed.append(pid)
        except ProcessLookupError:
            continue

    try:
        os.killpg(pgid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass
    return killed


def abort_judge_from_state(runtime) -> list[str]:
    if not runtime.state_path.exists():
        return []
    try:
        state = json.loads(runtime.state_path.read_text(encoding="utf-8"))
    except Exception:
        return []

    state["enabled"] = False
    try:
        runtime.state_path.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    except OSError:
        pass

    judge_session_id = state.get("judge_session_id")
    base_url = state.get("opencode_base_url")
    if not judge_session_id or not base_url:
        return []

    try:
        client = OpenCodeClient(
            base_url,
            directory=runtime.directory,
            password=state.get("opencode_password"),
            username=state.get("opencode_username") or "opencode",
            timeout=5.0,
        )
        client.abort_session(judge_session_id)
        return [judge_session_id]
    except Exception:
        return []


def cmd_daemon_stop(args: argparse.Namespace) -> int:
    runtime = project_runtime(args.directory, session_id=args.session_id)
    pids: list[int] = []
    aborted_sessions = abort_judge_from_state(runtime)

    # Snapshot all known pids before killing anything. The old implementation
    # posted /stop first; the daemon removed its pid file during graceful
    # shutdown, so the hard-kill phase often reported killed_pids=[].
    if daemon_running(runtime):
        try:
            payload = daemon_request(runtime, "GET", "/processes", timeout=2.0)
            for key in ("daemon_pid", "serve_pid"):
                value = payload.get(key)
                if value:
                    pids.append(int(value))
            for process in payload.get("processes", []):
                value = process.get("pid")
                if value:
                    pids.append(int(value))
        except Exception:
            pass

    if runtime.pid_path.exists():
        try:
            pids.append(int(runtime.pid_path.read_text(encoding="utf-8").strip()))
        except ValueError:
            pass

    state_path = runtime.state_path
    serve_pid: int | None = None
    if state_path.exists():
        try:
            serve_pid = json.loads(state_path.read_text(encoding="utf-8")).get("opencode_pid")
        except Exception:
            serve_pid = None
    if serve_pid:
        pids.append(int(serve_pid))

    pids = list(dict.fromkeys(pid for pid in pids if pid != os.getpid()))
    killed: list[int] = []
    for pid in pids:
        killed.extend(kill_family(pid))

    runtime.socket_path.unlink(missing_ok=True)
    runtime.pid_path.unlink(missing_ok=True)
    print(
        json.dumps(
            {
                "ok": True,
                "aborted_sessions": aborted_sessions,
                "attempted_pids": pids,
                "killed_pids": list(dict.fromkeys(killed)),
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_judge_on(args: argparse.Namespace) -> int:
    ensure_daemon(args)
    runtime = project_runtime(args.directory, session_id=args.session_id)
    payload = daemon_request(runtime, "POST", "/judge/on", body={"worker_session_id": args.worker_session_id})
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0


def cmd_judge_off(args: argparse.Namespace) -> int:
    ensure_daemon(args)
    runtime = project_runtime(args.directory, session_id=args.session_id)
    print(json.dumps(daemon_request(runtime, "POST", "/judge/off"), indent=2, sort_keys=True))
    return 0


def cmd_judge_pause(args: argparse.Namespace) -> int:
    ensure_daemon(args)
    runtime = project_runtime(args.directory, session_id=args.session_id)
    print(json.dumps(daemon_request(runtime, "POST", "/judge/pause", body={}), indent=2, sort_keys=True))
    return 0


def cmd_judge_resume(args: argparse.Namespace) -> int:
    ensure_daemon(args)
    runtime = project_runtime(args.directory, session_id=args.session_id)
    print(json.dumps(daemon_request(runtime, "POST", "/judge/resume", body={}), indent=2, sort_keys=True))
    return 0


def cmd_judge_queue(args: argparse.Namespace) -> int:
    ensure_daemon(args)
    runtime = project_runtime(args.directory, session_id=args.session_id)
    print(json.dumps(daemon_request(runtime, "GET", "/judge/queue"), indent=2, sort_keys=True))
    return 0


def cmd_poke(args: argparse.Namespace) -> int:
    ensure_daemon(args)
    runtime = project_runtime(args.directory, session_id=args.session_id)
    print(json.dumps(daemon_request(runtime, "POST", "/judge/poke", body={"reason": args.reason}), indent=2, sort_keys=True))
    return 0


def cmd_worker_id(args: argparse.Namespace) -> int:
    runtime = project_runtime(args.directory, session_id=args.session_id)
    if daemon_running(runtime):
        state = daemon_request(runtime, "GET", "/status").get("state", {})
        print(state.get("worker_session_id") or "")
        return 0
    if runtime.state_path.exists():
        state = json.loads(runtime.state_path.read_text(encoding="utf-8"))
        print(state.get("worker_session_id") or "")
        return 0
    return 1


def cmd_judge_id(args: argparse.Namespace) -> int:
    runtime = project_runtime(args.directory, session_id=args.session_id)
    if daemon_running(runtime):
        state = daemon_request(runtime, "GET", "/status").get("state", {})
        print(state.get("judge_session_id") or "")
        return 0
    if runtime.state_path.exists():
        state = json.loads(runtime.state_path.read_text(encoding="utf-8"))
        print(state.get("judge_session_id") or "")
        return 0
    return 1


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(prog="opencode-office")
    root.add_argument("--base-url", default=os.environ.get("OPENCODE_BASE_URL"))
    root.add_argument("--directory", default=os.getcwd())
    root.add_argument("--session-id", default=os.environ.get("OPENCODE_SESSION_ID"))
    root.add_argument("--password", default=os.environ.get("OPENCODE_SERVER_PASSWORD"))
    root.add_argument("--username", default=os.environ.get("OPENCODE_SERVER_USERNAME", "opencode"))
    root.add_argument("--timeout", type=float, default=30.0)

    sub = root.add_subparsers(dest="command", required=True)

    def add(name, fn, **kwargs):
        sp = sub.add_parser(name, **kwargs)
        sp.set_defaults(func=fn)
        return sp

    add("doctor", cmd_doctor)
    add("daemon-start", cmd_daemon_start)
    daemon_run = add("daemon-run", cmd_daemon_run)
    daemon_run.add_argument("--socket-path", required=True)
    add("daemon-stop", cmd_daemon_stop)
    add("status", cmd_status)
    add("summary", cmd_summary)
    add("paths", cmd_paths)
    add("ps", cmd_ps)

    judge_on = add("judge-on", cmd_judge_on)
    judge_on.add_argument("worker_session_id")
    add("judge-off", cmd_judge_off)
    add("judge-pause", cmd_judge_pause)
    add("judge-resume", cmd_judge_resume)
    add("judge-queue", cmd_judge_queue)

    poke = add("poke", cmd_poke)
    poke.add_argument("--reason", default="manual poke")

    logs = add("logs", cmd_logs)
    logs.add_argument("--lines", type=int, default=200)

    add("worker-id", cmd_worker_id)
    add("judge-id", cmd_judge_id)

    return root


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    return int(args.func(args))


if __name__ == "__main__":
    sys.exit(main())
