"""Diagnostic logger for the office daemon.

This is a *separate* stream from the structured business events emitted
by ``OfficeOrchestrator.event``. The event stream is a stable JSONL
contract consumed by ``/judge logs`` and the daemon ``/logs`` endpoint;
this module is for debugging the daemon itself - compaction decisions,
cooldown skips, HTTP request/response tracing, watch-loop activity,
exception tracebacks.

Design (mirrors crates/repx-core/src/logging.rs):

- Levels: standard ``logging`` (DEBUG/INFO/WARNING/ERROR/CRITICAL).
- Default INFO. Override with ``OFFICE_LOG_LEVEL=DEBUG`` (or TRACE,
  which is mapped to DEBUG since stdlib has no TRACE).
- Per-process timestamped file:
  ``<runtime.root>/diagnostic/<YYYY-MM-DD_HH-MM-SS>_<pid>.log``
- Stable symlink ``<runtime.root>/diagnostic.log`` → newest file, so
  ``tail -F`` always follows the live daemon.
- Rotation: keep at most ``OFFICE_LOG_MAX_FILES`` (default 10), delete
  files older than ``OFFICE_LOG_MAX_AGE_DAYS`` (default 14).
- Format: ``[YYYY-MM-DD HH:MM:SS] [LEVEL] file:line msg key=value ...``
- Optional stderr tee: ``OFFICE_LOG_STDERR=1``.
- Structured fields: pass via ``extra={"fields": {...}}`` or use the
  helper ``log_kv(level, msg, **fields)``.

Public surface:

- ``init_logger(runtime)``           idempotent; safe to call multiple times
- ``get_logger(name="office")``      thin wrapper over ``logging.getLogger``
- ``log_kv(logger, level, msg, **)`` formats key=value into the message

Nothing here is async; the daemon is threaded but stdlib ``logging``
handlers are thread-safe by default.
"""

from __future__ import annotations

import contextlib
import logging
import os
import re
import sys
import time
from datetime import datetime, timedelta
from logging.handlers import WatchedFileHandler
from pathlib import Path
from typing import Any

from .runtime import GlobalRuntime, ProjectRuntime

_LEVEL_NAMES = {
    "TRACE": logging.DEBUG,  # python stdlib has no TRACE; map to DEBUG.
    "DEBUG": logging.DEBUG,
    "INFO": logging.INFO,
    "WARN": logging.WARNING,
    "WARNING": logging.WARNING,
    "ERROR": logging.ERROR,
    "CRITICAL": logging.CRITICAL,
    "FATAL": logging.CRITICAL,
}

_initialized = False
_active_log_path: Path | None = None


def _resolve_level() -> int:
    raw = os.environ.get("OFFICE_LOG_LEVEL", "INFO").strip().upper()
    return _LEVEL_NAMES.get(raw, logging.INFO)


def _resolve_int_env(name: str, default: int) -> int:
    raw = os.environ.get(name)
    if raw is None or not raw.strip():
        return default
    try:
        return int(raw.strip())
    except ValueError:
        return default


class _OfficeFormatter(logging.Formatter):
    def __init__(self) -> None:
        super().__init__()

    def format(self, record: logging.LogRecord) -> str:
        ts = datetime.fromtimestamp(record.created).strftime("%Y-%m-%d %H:%M:%S")
        level = record.levelname
        if level == "WARNING":
            level = "WARN"
        location = f"{record.filename}:{record.lineno}"
        msg = record.getMessage()

        fields = getattr(record, "fields", None)
        if fields:
            kv = " ".join(f"{k}={_format_value(v)}" for k, v in fields.items())
            msg = f"{msg} {kv}" if msg else kv

        line = f"[{ts}] [{level:5}] {location} {msg}"

        if record.exc_info:
            line += "\n" + self.formatException(record.exc_info)
        return line


def _format_value(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    text = str(value)
    if not text:
        return '""'
    if any(c.isspace() for c in text) or "=" in text or '"' in text:
        escaped = text.replace("\\", "\\\\").replace('"', '\\"')
        return f'"{escaped}"'
    return text


_TIMESTAMP_RE = re.compile(r"^(\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2})_(\d+)\.log$")


def _rotate(log_dir: Path, max_files: int, max_age_days: int) -> None:
    if not log_dir.exists():
        return
    try:
        entries = sorted(
            (
                p
                for p in log_dir.iterdir()
                if p.is_file() and _TIMESTAMP_RE.match(p.name)
            ),
            key=lambda p: p.name,
        )
    except OSError:
        return

    if max_files > 0 and len(entries) > max_files:
        for victim in entries[: len(entries) - max_files]:
            with contextlib.suppress(OSError):
                victim.unlink()
        entries = entries[len(entries) - max_files :]

    if max_age_days > 0:
        cutoff = datetime.now() - timedelta(days=max_age_days)
        for entry in list(entries):
            match = _TIMESTAMP_RE.match(entry.name)
            if not match:
                continue
            try:
                ts = datetime.strptime(match.group(1), "%Y-%m-%d_%H-%M-%S")
            except ValueError:
                continue
            if ts < cutoff:
                with contextlib.suppress(OSError):
                    entry.unlink()


def init_logger(runtime: ProjectRuntime | GlobalRuntime) -> Path:
    global _initialized, _active_log_path
    if _initialized and _active_log_path is not None:
        return _active_log_path

    log_dir = runtime.root / "diagnostic"
    log_dir.mkdir(parents=True, exist_ok=True)

    max_files = _resolve_int_env("OFFICE_LOG_MAX_FILES", 10)
    max_age_days = _resolve_int_env("OFFICE_LOG_MAX_AGE_DAYS", 14)
    _rotate(log_dir, max_files, max_age_days)

    timestamp = time.strftime("%Y-%m-%d_%H-%M-%S")
    pid = os.getpid()
    log_path = log_dir / f"{timestamp}_{pid}.log"

    formatter = _OfficeFormatter()
    file_handler = WatchedFileHandler(str(log_path), encoding="utf-8")
    file_handler.setFormatter(formatter)

    root = logging.getLogger("office")
    for handler in list(root.handlers):
        root.removeHandler(handler)
    root.setLevel(_resolve_level())
    root.addHandler(file_handler)
    root.propagate = False

    if os.environ.get("OFFICE_LOG_STDERR", "").strip() in {"1", "true", "yes"}:
        stderr_handler = logging.StreamHandler(sys.stderr)
        stderr_handler.setFormatter(formatter)
        root.addHandler(stderr_handler)

    symlink = runtime.root / "diagnostic.log"
    with contextlib.suppress(OSError):
        if symlink.is_symlink() or symlink.exists():
            symlink.unlink()
    with contextlib.suppress(OSError):
        symlink.symlink_to(Path("diagnostic") / log_path.name)

    _active_log_path = log_path
    _initialized = True
    root.info(
        "logger initialized",
        extra={
            "fields": {
                "log_path": str(log_path),
                "level": logging.getLevelName(root.level),
                "max_files": max_files,
                "max_age_days": max_age_days,
                "pid": pid,
            }
        },
    )
    return log_path


def get_logger(name: str = "office") -> logging.Logger:
    if name == "office" or name.startswith("office."):
        return logging.getLogger(name)
    return logging.getLogger(f"office.{name}")


def log_kv(logger: logging.Logger, level: int, msg: str, **fields: Any) -> None:
    """Log a message with structured key=value tail.

    ``log_kv(log, logging.DEBUG, "compaction decision", will_compact=True, ratio=0.62)``
    -> ``[..] [DEBUG] orchestrator.py:108 compaction decision will_compact=true ratio=0.62``
    """
    logger.log(level, msg, extra={"fields": fields}, stacklevel=2)


def active_log_path() -> Path | None:
    return _active_log_path
