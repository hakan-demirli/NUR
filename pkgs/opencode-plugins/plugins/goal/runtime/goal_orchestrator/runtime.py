from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path

CACHE_ROOT_ENV = "GOAL_CACHE_DIR"


@dataclass(slots=True)
class GlobalRuntime:
    root: Path
    socket_path: Path
    state_path: Path
    pid_path: Path
    lock_path: Path
    daemon_log_path: Path
    diagnostic_log_dir: Path
    diagnostic_log_symlink: Path

    def ledger_path(self, cwd_hash: str) -> Path:
        p = self.root / "ledgers" / cwd_hash
        p.mkdir(parents=True, exist_ok=True)
        return p / "ledger.jsonl"


def _cache_root(base_dir: str | None = None) -> Path:
    if base_dir:
        return Path(base_dir)
    env = os.environ.get(CACHE_ROOT_ENV)
    if env:
        return Path(env)
    return Path.home() / ".cache" / "opencode_goal"


def global_runtime(*, base_dir: str | None = None) -> GlobalRuntime:
    root = _cache_root(base_dir)
    root.mkdir(parents=True, exist_ok=True)
    return GlobalRuntime(
        root=root,
        socket_path=root / "daemon.sock",
        state_path=root / "state.json",
        pid_path=root / "daemon.pid",
        lock_path=root / "daemon.lock",
        daemon_log_path=root / "daemon.log",
        diagnostic_log_dir=root / "diagnostic",
        diagnostic_log_symlink=root / "diagnostic.log",
    )


def cwd_hash(directory: str) -> str:
    import hashlib

    return hashlib.md5(os.path.abspath(directory).encode()).hexdigest()
