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

from .runtime import GlobalRuntime

_LEVEL_NAMES = {
    "TRACE": logging.DEBUG,
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
    raw = os.environ.get("GOAL_LOG_LEVEL", "INFO").strip().upper()
    return _LEVEL_NAMES.get(raw, logging.INFO)


def _resolve_int_env(name: str, default: int) -> int:
    raw = os.environ.get(name)
    if raw is None or not raw.strip():
        return default
    try:
        return int(raw.strip())
    except ValueError:
        return default


class _GoalFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        ts = datetime.fromtimestamp(record.created).strftime("%Y-%m-%d %H:%M:%S")
        level = record.levelname
        if level == "WARNING":
            level = "WARN"
        location = f"{record.filename}:{record.lineno}"
        msg = record.getMessage()
        fields = getattr(record, "fields", None)
        if fields:
            kv = " ".join(f"{k}={_fmt(v)}" for k, v in fields.items())
            msg = f"{msg} {kv}" if msg else kv
        line = f"[{ts}] [{level:5}] {location} {msg}"
        if record.exc_info:
            line += "\n" + self.formatException(record.exc_info)
        return line


def _fmt(value: Any) -> str:
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
            m = _TIMESTAMP_RE.match(entry.name)
            if not m:
                continue
            try:
                ts = datetime.strptime(m.group(1), "%Y-%m-%d_%H-%M-%S")
            except ValueError:
                continue
            if ts < cutoff:
                with contextlib.suppress(OSError):
                    entry.unlink()


def init_logger(runtime: GlobalRuntime) -> Path:
    global _initialized, _active_log_path
    if _initialized and _active_log_path is not None:
        return _active_log_path

    log_dir = runtime.root / "diagnostic"
    log_dir.mkdir(parents=True, exist_ok=True)

    max_files = _resolve_int_env("GOAL_LOG_MAX_FILES", 10)
    max_age_days = _resolve_int_env("GOAL_LOG_MAX_AGE_DAYS", 14)
    _rotate(log_dir, max_files, max_age_days)

    timestamp = time.strftime("%Y-%m-%d_%H-%M-%S")
    pid = os.getpid()
    log_path = log_dir / f"{timestamp}_{pid}.log"

    formatter = _GoalFormatter()
    file_handler = WatchedFileHandler(str(log_path), encoding="utf-8")
    file_handler.setFormatter(formatter)

    root = logging.getLogger("goal")
    for handler in list(root.handlers):
        root.removeHandler(handler)
    root.setLevel(_resolve_level())
    root.addHandler(file_handler)
    root.propagate = False

    if os.environ.get("GOAL_LOG_STDERR", "").strip() in {"1", "true", "yes"}:
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
        "logger initialized", extra={"fields": {"log_path": str(log_path), "pid": pid}}
    )
    return log_path


def get_logger(name: str = "goal") -> logging.Logger:
    if name == "goal" or name.startswith("goal."):
        return logging.getLogger(name)
    return logging.getLogger(f"goal.{name}")


def log_kv(logger: logging.Logger, level: int, msg: str, **fields: Any) -> None:
    logger.log(level, msg, extra={"fields": fields}, stacklevel=2)


def active_log_path() -> Path | None:
    return _active_log_path
