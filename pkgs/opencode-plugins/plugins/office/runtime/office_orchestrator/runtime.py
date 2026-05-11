from __future__ import annotations

import hashlib
import os
from dataclasses import dataclass
from pathlib import Path


@dataclass(slots=True)
class ProjectRuntime:
    directory: str
    session_id: str | None
    root: Path
    socket_path: Path
    state_path: Path
    pid_path: Path
    daemon_log_path: Path
    serve_log_path: Path
    diagnostic_log_dir: Path
    diagnostic_log_symlink: Path


def project_runtime(directory: str, *, session_id: str | None = None, base_dir: str | None = None) -> ProjectRuntime:
    directory = os.path.abspath(directory)
    key = directory if not session_id else f"{directory}\0{session_id}"
    digest = hashlib.md5(key.encode("utf-8")).hexdigest()
    root = Path(base_dir or os.path.join(Path.home(), ".cache", "opencode_office_py")) / digest
    root.mkdir(parents=True, exist_ok=True)
    return ProjectRuntime(
        directory=directory,
        session_id=session_id,
        root=root,
        socket_path=root / "daemon.sock",
        state_path=root / "state.json",
        pid_path=root / "daemon.pid",
        daemon_log_path=root / "daemon.log",
        serve_log_path=root / "opencode-serve.log",
        diagnostic_log_dir=root / "diagnostic",
        diagnostic_log_symlink=root / "diagnostic.log",
    )
