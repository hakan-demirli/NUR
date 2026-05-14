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

from .runtime import global_runtime, project_runtime


class UnixHTTPConnection(http.client.HTTPConnection):
    def __init__(self, socket_path: str, timeout: float = 30.0) -> None:
        super().__init__("localhost", timeout=timeout)
        self.socket_path = socket_path

    def connect(self) -> None:
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.socket_path)


def daemon_request(
    runtime,
    method: str,
    path: str,
    *,
    body: dict[str, Any] | None = None,
    timeout: float = 30.0,
) -> Any:
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


def _self_invocation() -> list[str]:
    arg0 = sys.argv[0] or ""
    if arg0 and os.path.isfile(arg0) and os.access(arg0, os.X_OK):
        return [arg0]
    return [sys.executable, "-m", "office_orchestrator.cli"]


def start_daemon() -> dict[str, Any]:
    runtime = global_runtime()
    if daemon_running(runtime):
        return daemon_request(runtime, "GET", "/status")

    runtime.socket_path.unlink(missing_ok=True)
    runtime.daemon_log_path.touch(exist_ok=True)
    env = os.environ.copy()
    invocation = _self_invocation()
    if len(invocation) > 1 and invocation[1:3] == ["-m", "office_orchestrator.cli"]:
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
    raise RuntimeError(f"Timed out waiting for office daemon at {runtime.socket_path}")


def ensure_daemon() -> None:
    start_daemon()


def _slot_body(
    args: argparse.Namespace, extra: dict[str, Any] | None = None
) -> dict[str, Any]:
    directory = args.directory or os.getcwd()
    body: dict[str, Any] = {
        "directory": os.path.abspath(directory),
    }
    worker = getattr(args, "worker_session_id", None) or args.session_id
    if worker:
        body["worker_session_id"] = worker
    if extra:
        body.update(extra)
    return body


def cmd_doctor(args: argparse.Namespace) -> int:
    runtime = global_runtime()
    payload: dict[str, Any] = {
        "socket": str(runtime.socket_path),
        "daemon_running": daemon_running(runtime),
        "opencode_url": os.environ.get("OPENCODE_URL"),
    }
    if payload["daemon_running"]:
        try:
            payload["summary"] = daemon_request(runtime, "GET", "/summary", timeout=5.0)
            payload["processes"] = daemon_request(
                runtime, "GET", "/processes", timeout=5.0
            )
        except Exception as exc:
            payload["error"] = str(exc)
    else:
        payload["hint"] = "Daemon is not running. Run `daemon-start` or `/judge on`."
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0


def cmd_paths(args: argparse.Namespace) -> int:
    runtime = global_runtime()
    project = project_runtime(args.directory or os.getcwd(), session_id=args.session_id)
    if daemon_running(runtime):
        payload = daemon_request(runtime, "GET", "/paths")
    else:
        payload = {
            "runtime_dir": str(runtime.root),
            "socket": str(runtime.socket_path),
            "state_file": str(runtime.state_path),
            "pid_file": str(runtime.pid_path),
            "daemon_log": str(runtime.daemon_log_path),
            "diagnostic_log": str(runtime.diagnostic_log_symlink),
            "diagnostic_log_dir": str(runtime.diagnostic_log_dir),
            "daemon_running": False,
        }
    payload["project_dir"] = str(project.root)
    payload["project_daemon_log"] = str(project.daemon_log_path)
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0


def _read_tail(path, lines: int) -> list[str]:
    if not path.exists():
        return []
    try:
        return path.read_text(errors="replace").splitlines()[-lines:]
    except OSError:
        return []


def cmd_logs(args: argparse.Namespace) -> int:
    runtime = global_runtime()
    if daemon_running(runtime):
        payload = daemon_request(runtime, "GET", f"/logs?lines={args.lines}")
        print(json.dumps(payload, indent=2, sort_keys=True))
        return 0
    daemon_log = _read_tail(runtime.daemon_log_path, args.lines)
    print(json.dumps({"daemon": daemon_log}, indent=2, sort_keys=True))
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


def cmd_status(args: argparse.Namespace) -> int:
    ensure_daemon()
    runtime = global_runtime()
    query = ""
    parts: list[str] = []
    if args.directory:
        parts.append(f"directory={os.path.abspath(args.directory)}")
    worker = args.worker_session_id or args.session_id
    if worker:
        parts.append(f"worker_session_id={worker}")
    if parts:
        query = "?" + "&".join(parts)
    print(
        json.dumps(
            daemon_request(runtime, "GET", f"/status{query}"), indent=2, sort_keys=True
        )
    )
    return 0


def cmd_summary(args: argparse.Namespace) -> int:
    ensure_daemon()
    runtime = global_runtime()
    parts: list[str] = []
    if args.directory:
        parts.append(f"directory={os.path.abspath(args.directory)}")
    worker = args.worker_session_id or args.session_id
    if worker:
        parts.append(f"worker_session_id={worker}")
    query = ("?" + "&".join(parts)) if parts else ""
    print(
        json.dumps(
            daemon_request(runtime, "GET", f"/summary{query}"), indent=2, sort_keys=True
        )
    )
    return 0


def cmd_daemon_start(args: argparse.Namespace) -> int:
    print(json.dumps(start_daemon(), indent=2, sort_keys=True))
    return 0


def cmd_daemon_run(args: argparse.Namespace) -> int:
    from .daemon import main as daemon_main

    argv: list[str] = []
    if args.socket_path:
        argv.extend(["--socket-path", args.socket_path])
    return daemon_main(argv)


def process_descendants(root_pid: int) -> list[int]:
    children: dict[int, list[int]] = {}
    try:
        for entry in os.listdir("/proc"):
            if not entry.isdigit():
                continue
            pid = int(entry)
            try:
                with open(f"/proc/{pid}/status", encoding="utf-8") as fh:
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
    with contextlib.suppress(ProcessLookupError, PermissionError):
        os.killpg(pgid, signal.SIGKILL)
    return killed


def cmd_daemon_stop(args: argparse.Namespace) -> int:
    runtime = global_runtime()
    pids: list[int] = []
    if daemon_running(runtime):
        try:
            payload = daemon_request(runtime, "GET", "/processes", timeout=2.0)
            value = payload.get("daemon_pid")
            if value:
                pids.append(int(value))
        except Exception:
            pass
    if runtime.pid_path.exists():
        with contextlib.suppress(ValueError):
            pids.append(int(runtime.pid_path.read_text(encoding="utf-8").strip()))
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
                "attempted_pids": pids,
                "killed_pids": list(dict.fromkeys(killed)),
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_judge_on(args: argparse.Namespace) -> int:
    ensure_daemon()
    runtime = global_runtime()
    body = _slot_body(
        args, {"worker_session_id": args.worker_session_id or args.session_id}
    )
    if not body.get("worker_session_id"):
        print(json.dumps({"error": "worker_session_id is required"}), file=sys.stderr)
        return 2
    print(
        json.dumps(
            daemon_request(runtime, "POST", "/judge/on", body=body),
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_judge_off(args: argparse.Namespace) -> int:
    ensure_daemon()
    runtime = global_runtime()
    print(
        json.dumps(
            daemon_request(runtime, "POST", "/judge/off", body=_slot_body(args)),
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_judge_forget(args: argparse.Namespace) -> int:
    ensure_daemon()
    runtime = global_runtime()
    print(
        json.dumps(
            daemon_request(runtime, "POST", "/judge/forget", body=_slot_body(args)),
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_judge_pause(args: argparse.Namespace) -> int:
    ensure_daemon()
    runtime = global_runtime()
    print(
        json.dumps(
            daemon_request(runtime, "POST", "/judge/pause", body=_slot_body(args)),
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_judge_resume(args: argparse.Namespace) -> int:
    ensure_daemon()
    runtime = global_runtime()
    print(
        json.dumps(
            daemon_request(runtime, "POST", "/judge/resume", body=_slot_body(args)),
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_judge_queue(args: argparse.Namespace) -> int:
    ensure_daemon()
    runtime = global_runtime()
    body = _slot_body(args)
    qs = "?" + "&".join(f"{k}={v}" for k, v in body.items() if v is not None)
    print(
        json.dumps(
            daemon_request(runtime, "GET", f"/judge/queue{qs}"),
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_poke(args: argparse.Namespace) -> int:
    ensure_daemon()
    runtime = global_runtime()
    print(
        json.dumps(
            daemon_request(
                runtime,
                "POST",
                "/judge/poke",
                body=_slot_body(args, {"reason": args.reason}),
            ),
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def cmd_worker_id(args: argparse.Namespace) -> int:
    runtime = global_runtime()
    if not daemon_running(runtime):
        return 1
    target = os.path.abspath(args.directory or os.getcwd())
    payload = daemon_request(runtime, "GET", "/status")
    for slot in payload.get("slots", []):
        if slot.get("directory") == target:
            print(slot.get("worker_session_id") or "")
            return 0
    return 1


def cmd_judge_id(args: argparse.Namespace) -> int:
    runtime = global_runtime()
    if not daemon_running(runtime):
        return 1
    target = os.path.abspath(args.directory or os.getcwd())
    payload = daemon_request(runtime, "GET", "/status")
    for slot in payload.get("slots", []):
        if slot.get("directory") == target:
            print(slot.get("judge_session_id") or "")
            return 0
    return 1


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(prog="opencode-office")
    root.add_argument("--directory", default=None)
    root.add_argument("--session-id", default=os.environ.get("OPENCODE_SESSION_ID"))
    root.add_argument(
        "--worker-session-id",
        default=None,
        help="explicit worker session id; defaults to --session-id",
    )
    root.add_argument("--timeout", type=float, default=30.0)

    sub = root.add_subparsers(dest="command", required=True)

    def add(name, fn, **kwargs):
        sp = sub.add_parser(name, **kwargs)
        sp.set_defaults(func=fn)
        return sp

    add("doctor", cmd_doctor)
    add("daemon-start", cmd_daemon_start)
    daemon_run = add("daemon-run", cmd_daemon_run)
    daemon_run.add_argument("--socket-path", required=False, default=None)
    add("daemon-stop", cmd_daemon_stop)
    add("status", cmd_status)
    add("summary", cmd_summary)
    add("paths", cmd_paths)
    add("ps", cmd_ps)

    judge_on = add("judge-on", cmd_judge_on)
    judge_on.add_argument("worker_session_id", nargs="?", default=None)
    add("judge-off", cmd_judge_off)
    add("judge-forget", cmd_judge_forget)
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
