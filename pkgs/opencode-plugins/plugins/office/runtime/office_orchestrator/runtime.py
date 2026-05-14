from __future__ import annotations

import hashlib
import os
from dataclasses import dataclass
from pathlib import Path

SCHEMA_FILE = ".schema-v2"


@dataclass(slots=True)
class ProjectRuntime:
    directory: str
    session_id: str | None
    root: Path
    state_path: Path
    daemon_log_path: Path
    serve_log_path: Path
    diagnostic_log_dir: Path
    diagnostic_log_symlink: Path
    socket_path: Path
    pid_path: Path


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


def _cache_root(base_dir: str | None = None) -> Path:
    return Path(base_dir or os.path.join(Path.home(), ".cache", "opencode_office_py"))


def project_runtime(
    directory: str, *, session_id: str | None = None, base_dir: str | None = None
) -> ProjectRuntime:
    directory = os.path.abspath(directory)
    digest = hashlib.md5(directory.encode("utf-8")).hexdigest()
    root = _cache_root(base_dir) / "projects" / digest
    root.mkdir(parents=True, exist_ok=True)
    return ProjectRuntime(
        directory=directory,
        session_id=session_id,
        root=root,
        state_path=root / "state.json",
        daemon_log_path=root / "daemon.log",
        serve_log_path=root / "opencode-serve.log",
        diagnostic_log_dir=root / "diagnostic",
        diagnostic_log_symlink=root / "diagnostic.log",
        socket_path=root / "daemon.sock",
        pid_path=root / "daemon.pid",
    )


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


def ensure_cache_schema(*, base_dir: str | None = None) -> bool:
    """Mark the on-disk cache layout as schema-current.

    Returns ``True`` if a migration happened (caller may want to log it).
    Idempotent via a sentinel file.

    NOTE: this used to wipe the entire cache root on first boot to clear out
    the legacy per-session daemon layout. That was unsafe (it could destroy
    ``state.json`` on a future schema bump). The new policy is **never delete
    user state**: we move legacy directories aside into a timestamped
    ``legacy.<ts>/`` folder and leave ``state.json``, backups, and logs alone.
    The orchestrator's own ``load_state`` handles forward-compat JSON
    migrations; that's the only place schema changes are allowed to touch
    data.
    """
    root = _cache_root(base_dir)
    root.mkdir(parents=True, exist_ok=True)
    sentinel = root / SCHEMA_FILE
    if sentinel.exists():
        return False

    import shutil
    import time

    preserve = {
        SCHEMA_FILE,
        "state.json",
        "daemon.log",
        "daemon.pid",
        "daemon.sock",
        "daemon.lock",
        "diagnostic",
        "diagnostic.log",
    }
    legacy_root = root / f"legacy.{int(time.time())}"
    moved = False
    for entry in root.iterdir():
        if entry.name in preserve:
            continue
        if entry.name.startswith("state.json.bak.") or entry.name.startswith(
            "state.json.corrupt."
        ):
            continue
        try:
            if not moved:
                legacy_root.mkdir(parents=True, exist_ok=True)
                moved = True
            shutil.move(str(entry), str(legacy_root / entry.name))
        except OSError:
            pass

    sentinel.write_text("ok\n", encoding="utf-8")
    return True
