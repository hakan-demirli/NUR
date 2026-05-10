from __future__ import annotations

import json
import logging
import time
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from .client import OpenCodeClient
from .logging import get_logger, log_kv
from .prompts import (
    JudgeBootstrapContext,
    JudgeNudgeContext,
    build_judge_bootstrap,
    build_judge_nudge,
)
from .runtime import ProjectRuntime

_log = get_logger("office.orchestrator")

QUALITY_COMPACT_TOKENS = 300_000
LARGE_CONTEXT_WINDOW = 400_000
CONTEXT_RATIO_THRESHOLD = 0.6
JUDGE_NUDGE_COOLDOWN_MS = 15_000


@dataclass(slots=True)
class OfficeState:
    directory: str
    worker_session_id: str | None = None
    judge_session_id: str | None = None
    last_worker_message_id: str | None = None
    last_worker_completed_at: int | None = None
    last_judge_nudge_at: int | None = None
    last_nudged_worker_message_id: str | None = None
    last_judge_verdict: str | None = None
    enabled: bool = False
    paused: bool = False
    paused_at: int | None = None
    pending_events: list[dict[str, Any]] | None = None
    opencode_base_url: str | None = None
    opencode_password: str | None = None
    opencode_username: str | None = None
    opencode_pid: int | None = None


class OfficeOrchestrator:
    def __init__(self, client: OpenCodeClient, runtime: ProjectRuntime) -> None:
        self.client = client
        self.runtime = runtime

    def event(self, kind: str, **fields: Any) -> None:
        record = {"ts": datetime.now(timezone.utc).isoformat(), "event": kind, **fields}
        try:
            with self.runtime.daemon_log_path.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps(record, sort_keys=True) + "\n")
        except OSError:
            pass

    def load_state(self) -> OfficeState:
        if not self.runtime.state_path.exists():
            return OfficeState(directory=self.runtime.directory)
        return OfficeState(**json.loads(self.runtime.state_path.read_text(encoding="utf-8")))

    def save_state(self, state: OfficeState) -> None:
        self.runtime.state_path.write_text(json.dumps(asdict(state), indent=2) + "\n", encoding="utf-8")

    def clear_state(self) -> None:
        if self.runtime.state_path.exists():
            self.runtime.state_path.unlink()

    def judge_bootstrap(self, worker_session_id: str, judge_session_id: str) -> str:
        return build_judge_bootstrap(
            JudgeBootstrapContext(
                worker_session_id=worker_session_id,
                judge_session_id=judge_session_id,
                socket_path=str(self.runtime.socket_path),
            )
        )

    def should_compact(self, session_id: str) -> tuple[bool, str | None]:
        summary = self.client.summary(session_id)
        tokens = summary.get("tokens")
        context_limit = summary.get("context_limit")
        if not tokens:
            log_kv(
                _log,
                logging.DEBUG,
                "compaction skipped: no token info",
                session=session_id,
                context_limit=context_limit,
            )
            return False, None
        total = int(tokens["total"])
        ratio = (total / context_limit) if context_limit else None
        if context_limit and context_limit >= LARGE_CONTEXT_WINDOW and total >= QUALITY_COMPACT_TOKENS:
            reason = f"usage crossed {QUALITY_COMPACT_TOKENS:,} tokens on a large-window model"
            log_kv(
                _log,
                logging.INFO,
                "compaction recommended (token threshold)",
                session=session_id,
                total=total,
                context_limit=context_limit,
                ratio=f"{ratio:.3f}" if ratio is not None else None,
                threshold_tokens=QUALITY_COMPACT_TOKENS,
                reason=reason,
            )
            return True, reason
        if context_limit and context_limit > 0 and ratio is not None and ratio >= CONTEXT_RATIO_THRESHOLD:
            reason = f"usage reached {ratio:.1%} of context"
            log_kv(
                _log,
                logging.INFO,
                "compaction recommended (ratio threshold)",
                session=session_id,
                total=total,
                context_limit=context_limit,
                ratio=f"{ratio:.3f}",
                threshold_ratio=CONTEXT_RATIO_THRESHOLD,
                reason=reason,
            )
            return True, reason
        log_kv(
            _log,
            logging.DEBUG,
            "compaction not needed",
            session=session_id,
            total=total,
            context_limit=context_limit,
            ratio=f"{ratio:.3f}" if ratio is not None else None,
        )
        return False, None

    def enable_judge(self, worker_session_id: str, state: OfficeState | None = None) -> OfficeState:
        state = state or self.load_state()
        log_kv(_log, logging.INFO, "enable_judge: forking from worker", worker=worker_session_id)
        judge = self.client.fork_session(worker_session_id)
        judge_session_id = judge["id"]
        log_kv(_log, logging.INFO, "judge forked", worker=worker_session_id, judge=judge_session_id)
        self.client.prompt_async(
            judge_session_id,
            self.judge_bootstrap(worker_session_id, judge_session_id),
            agent="plan",
            no_reply=True,
        )
        state.worker_session_id = worker_session_id
        state.judge_session_id = judge_session_id
        state.last_worker_message_id = None
        state.last_worker_completed_at = None
        state.last_judge_nudge_at = None
        state.last_nudged_worker_message_id = None
        state.last_judge_verdict = None
        state.enabled = True
        self.save_state(state)
        self.event("judge_on", worker=worker_session_id, judge=judge_session_id)
        return state

    def disable_judge(self, state: OfficeState | None = None) -> OfficeState:
        state = state or self.load_state()
        state.enabled = False
        self.save_state(state)
        log_kv(_log, logging.INFO, "judge disabled", worker=state.worker_session_id, judge=state.judge_session_id)
        self.event("judge_off", worker=state.worker_session_id, judge=state.judge_session_id)
        return state

    def pause(self, state: OfficeState | None = None) -> OfficeState:
        state = state or self.load_state()
        if state.paused:
            log_kv(_log, logging.DEBUG, "pause: already paused; no-op")
            return state
        state.paused = True
        state.paused_at = int(time.time() * 1000)
        if state.pending_events is None:
            state.pending_events = []
        self.save_state(state)
        log_kv(_log, logging.INFO, "judge paused", queued=len(state.pending_events or []))
        self.event(
            "judge_paused",
            worker=state.worker_session_id,
            judge=state.judge_session_id,
            queued=len(state.pending_events or []),
        )
        return state

    def resume(self, state: OfficeState | None = None) -> tuple[OfficeState, list[dict[str, Any]]]:
        state = state or self.load_state()
        if not state.paused:
            log_kv(_log, logging.DEBUG, "resume: not paused; no-op")
            return state, []
        queued = list(state.pending_events or [])
        state.paused = False
        state.paused_at = None
        state.pending_events = []
        self.save_state(state)
        log_kv(_log, logging.INFO, "judge resumed", to_replay=len(queued))
        self.event(
            "judge_resumed",
            worker=state.worker_session_id,
            judge=state.judge_session_id,
            replay=len(queued),
        )

        replayed: list[dict[str, Any]] = []
        # Coalesce queued events by their distinguishing key. We currently only
        # buffer judge nudges, so collapse runs that point at the same worker
        # message into a single replay - this preserves the original reasons
        # while avoiding spamming the judge.
        if queued:
            collapsed: dict[str, dict[str, Any]] = {}
            for item in queued:
                key = item.get("worker_message_id") or "_no_msg"
                if key in collapsed:
                    collapsed[key]["reasons"].append(item.get("reason"))
                else:
                    collapsed[key] = {
                        "worker_message_id": item.get("worker_message_id"),
                        "reasons": [item.get("reason")],
                        "first_queued_at": item.get("queued_at"),
                    }
            for entry in collapsed.values():
                reasons = [r for r in entry["reasons"] if r]
                summary = ", ".join(dict.fromkeys(reasons)) or "queued review"
                # Force the nudge so cooldowns don't suppress the replay; the
                # whole point is that the user explicitly asked for it.
                state = self.nudge_judge(reason=f"resume replay ({summary})", state=state, force=True)
                replayed.append(entry)
        return state, replayed

    def queue_event(self, kind: str, payload: dict[str, Any], state: OfficeState | None = None) -> OfficeState:
        state = state or self.load_state()
        record = {
            "kind": kind,
            "queued_at": int(time.time() * 1000),
            **payload,
        }
        if state.pending_events is None:
            state.pending_events = []
        # Deduplicate consecutive events that point at the same worker message
        # so the queue stays bounded if the worker fires many turns while
        # paused.
        last_msg = (state.pending_events[-1] if state.pending_events else {}).get("worker_message_id")
        if record.get("worker_message_id") and record.get("worker_message_id") == last_msg:
            state.pending_events[-1] = record
        else:
            state.pending_events.append(record)
        # Bound the queue at 50 entries so we never grow unbounded.
        truncated = False
        if len(state.pending_events) > 50:
            state.pending_events = state.pending_events[-50:]
            truncated = True
        self.save_state(state)
        log_kv(
            _log,
            logging.DEBUG,
            "queued event",
            kind=kind,
            queue_size=len(state.pending_events),
            worker_message=record.get("worker_message_id"),
            reason=record.get("reason"),
            truncated=truncated,
        )
        self.event(
            "judge_event_queued",
            event_kind=kind,
            queue_size=len(state.pending_events),
            worker_message=record.get("worker_message_id"),
            reason=record.get("reason"),
        )
        return state

    def prompt_worker(self, message: str, *, compact_first: bool = False, state: OfficeState | None = None) -> OfficeState:
        state = state or self.load_state()
        if not state.worker_session_id:
            raise RuntimeError("No worker session configured")
        if compact_first:
            log_kv(_log, logging.INFO, "compacting worker before prompt", worker=state.worker_session_id)
            self.client.send_command(state.worker_session_id, "compact", "")
            self.event("worker_compacted", worker=state.worker_session_id)
        log_kv(
            _log,
            logging.INFO,
            "prompting worker",
            worker=state.worker_session_id,
            chars=len(message),
            compact_first=compact_first,
        )
        self.client.prompt_async(state.worker_session_id, message, agent="build")
        self.event("worker_prompt", worker=state.worker_session_id, compact_first=compact_first, length=len(message))
        return state

    def compact_worker(self, *, message: str | None = None, state: OfficeState | None = None) -> OfficeState:
        state = state or self.load_state()
        if not state.worker_session_id:
            raise RuntimeError("No worker session configured")
        log_kv(_log, logging.INFO, "compact_worker invoked", worker=state.worker_session_id, has_message=bool(message))
        self.client.send_command(state.worker_session_id, "compact", "")
        self.event("worker_compacted", worker=state.worker_session_id)
        if message:
            self.client.prompt_async(state.worker_session_id, message, agent="build")
            self.event("worker_prompt", worker=state.worker_session_id, compact_first=True, length=len(message))
        return state

    def compact_judge(self, *, message: str | None = None, state: OfficeState | None = None) -> OfficeState:
        state = state or self.load_state()
        if not state.judge_session_id:
            raise RuntimeError("No judge session configured")
        log_kv(_log, logging.INFO, "compact_judge invoked", judge=state.judge_session_id, has_message=bool(message))
        self.client.send_command(state.judge_session_id, "compact", "")
        self.event("judge_compacted", judge=state.judge_session_id)
        if message:
            self.client.prompt_async(state.judge_session_id, message, agent="build")
            self.event("judge_prompt", judge=state.judge_session_id, length=len(message))
        return state

    def nudge_judge(self, *, reason: str, state: OfficeState | None = None, force: bool = False) -> OfficeState:
        state = state or self.load_state()
        if not state.enabled or not state.worker_session_id or not state.judge_session_id:
            raise RuntimeError("Judge mode is not enabled")

        latest = self.client.latest_assistant_message(state.worker_session_id)
        latest_message_id = latest["info"]["id"] if latest else None

        log_kv(
            _log,
            logging.DEBUG,
            "nudge_judge entered",
            reason=reason,
            force=force,
            paused=state.paused,
            latest_message=latest_message_id,
            last_nudged_message=state.last_nudged_worker_message_id,
            last_nudge_at=state.last_judge_nudge_at,
        )

        # When paused, capture the event for later replay instead of pinging
        # the judge. We still respect `force` from the resume path because
        # that call sets `paused=False` before forcing the replay.
        if state.paused and not force:
            log_kv(
                _log,
                logging.DEBUG,
                "nudge diverted to pending queue (paused)",
                reason=reason,
                worker_message=latest_message_id,
            )
            return self.queue_event(
                "judge_nudge",
                {"reason": reason, "worker_message_id": latest_message_id},
                state=state,
            )

        now_ms = int(time.time() * 1000)
        if not force and state.last_judge_nudge_at and now_ms - state.last_judge_nudge_at < JUDGE_NUDGE_COOLDOWN_MS:
            log_kv(
                _log,
                logging.DEBUG,
                "nudge suppressed by cooldown",
                reason=reason,
                cooldown_ms=JUDGE_NUDGE_COOLDOWN_MS,
                age_ms=now_ms - state.last_judge_nudge_at,
            )
            return state
        if not force and latest_message_id and latest_message_id == state.last_nudged_worker_message_id:
            log_kv(
                _log,
                logging.DEBUG,
                "nudge suppressed by message dedup",
                reason=reason,
                worker_message=latest_message_id,
            )
            return state

        judge_compact, judge_reason = self.should_compact(state.judge_session_id)
        worker_compact, worker_reason = self.should_compact(state.worker_session_id)
        if judge_compact:
            log_kv(
                _log,
                logging.INFO,
                "running /compact on judge before nudge",
                judge=state.judge_session_id,
                reason=judge_reason,
            )
            self.client.send_command(state.judge_session_id, "compact", "")

        prompt = build_judge_nudge(
            JudgeNudgeContext(
                worker_session_id=state.worker_session_id,
                judge_session_id=state.judge_session_id,
                socket_path=str(self.runtime.socket_path),
                reason=reason,
                latest_message_id=latest_message_id,
                worker_summary=self.client.summary(state.worker_session_id),
                judge_summary=self.client.summary(state.judge_session_id),
                worker_compact_recommended=worker_compact,
                worker_compact_reason=worker_reason,
                judge_was_compacted=judge_compact,
                judge_compact_reason=judge_reason,
            )
        )

        log_kv(
            _log,
            logging.INFO,
            "nudging judge",
            reason=reason,
            worker_message=latest_message_id,
            prompt_chars=len(prompt),
            worker_compact_recommended=worker_compact,
            judge_compacted_first=judge_compact,
            forced=force,
        )
        self.client.prompt_async(state.judge_session_id, prompt, agent="build")
        state.last_judge_nudge_at = now_ms
        state.last_nudged_worker_message_id = latest_message_id
        self.save_state(state)
        self.event(
            "judge_nudged",
            worker=state.worker_session_id,
            judge=state.judge_session_id,
            reason=reason,
            worker_message=latest_message_id,
            worker_compact_recommended=worker_compact,
            judge_compacted_first=judge_compact,
        )
        return state

    def worker_update(self, state: OfficeState | None = None) -> tuple[OfficeState, bool]:
        state = state or self.load_state()
        if not state.worker_session_id:
            return state, False
        latest = self.client.latest_assistant_message(state.worker_session_id)
        if not latest:
            return state, False
        message_id = latest["info"]["id"]
        # Only count this as a "turn" once the model has actually finished.
        # Assistant messages appear in the API the moment streaming starts;
        # `time.completed` is unset until the turn ends. Without this gate
        # the watch loop wakes the judge mid-stream every 5s and the judge
        # sees half-written content + half-applied edits, which is exactly
        # the false-poke pattern we hit in production.
        completed = (latest.get("info", {}).get("time") or {}).get("completed")
        if not completed:
            log_kv(
                _log,
                logging.DEBUG,
                "worker turn ignored (still streaming)",
                worker=state.worker_session_id,
                message=message_id,
            )
            return state, False
        if message_id == state.last_worker_message_id:
            return state, False
        log_kv(
            _log,
            logging.INFO,
            "worker turn detected",
            worker=state.worker_session_id,
            message=message_id,
            previous=state.last_worker_message_id,
            completed_at=completed,
        )
        state.last_worker_message_id = message_id
        state.last_worker_completed_at = completed
        self.save_state(state)
        self.event("worker_turn_detected", worker=state.worker_session_id, message=message_id)
        return state, True

    def status_payload(self, state: OfficeState | None = None) -> dict[str, Any]:
        state = state or self.load_state()
        payload: dict[str, Any] = {
            "state": asdict(state),
            "paths": self.paths_payload(),
        }
        if state.worker_session_id:
            payload["worker"] = self.client.summary(state.worker_session_id)
        if state.judge_session_id:
            payload["judge"] = self.client.summary(state.judge_session_id)
        return payload

    def paths_payload(self) -> dict[str, str]:
        return {
            "directory": self.runtime.directory,
            "runtime_dir": str(self.runtime.root),
            "socket": str(self.runtime.socket_path),
            "state_file": str(self.runtime.state_path),
            "pid_file": str(self.runtime.pid_path),
            "daemon_log": str(self.runtime.daemon_log_path),
            "opencode_serve_log": str(self.runtime.serve_log_path),
            "diagnostic_log": str(self.runtime.diagnostic_log_symlink),
            "diagnostic_log_dir": str(self.runtime.diagnostic_log_dir),
        }

    def summary_payload(self, state: OfficeState | None = None) -> dict[str, Any]:
        state = state or self.load_state()
        worker = self.client.summary(state.worker_session_id) if state.worker_session_id else None
        judge = self.client.summary(state.judge_session_id) if state.judge_session_id else None

        def _tokens_line(summary: dict[str, Any] | None) -> str:
            if not summary or not summary.get("tokens"):
                return "tokens=unknown"
            tokens = summary["tokens"]
            ctx = summary.get("context_limit") or 0
            ratio = f" ({tokens['total'] / ctx:.1%})" if ctx else ""
            return f"tokens={tokens['total']:,} / ctx={ctx:,}{ratio}"

        return {
            "enabled": state.enabled,
            "paused": state.paused,
            "paused_at": state.paused_at,
            "queued_events": len(state.pending_events or []),
            "worker": {
                "session_id": state.worker_session_id,
                "model": worker.get("model") if worker else None,
                "tokens": worker.get("tokens") if worker else None,
                "context_limit": worker.get("context_limit") if worker else None,
                "human": _tokens_line(worker),
                "last_message_id": state.last_worker_message_id,
            },
            "judge": {
                "session_id": state.judge_session_id,
                "model": judge.get("model") if judge else None,
                "tokens": judge.get("tokens") if judge else None,
                "context_limit": judge.get("context_limit") if judge else None,
                "human": _tokens_line(judge),
                "last_nudge_at": state.last_judge_nudge_at,
            },
            "trigger": {
                "last_judge_nudge_at": state.last_judge_nudge_at,
                "last_nudged_worker_message_id": state.last_nudged_worker_message_id,
            },
        }

    def queued_events(self, state: OfficeState | None = None) -> list[dict[str, Any]]:
        state = state or self.load_state()
        return list(state.pending_events or [])

    def worker_messages(self, limit: int) -> list[dict[str, Any]]:
        state = self.load_state()
        if not state.worker_session_id:
            return []
        return self.client.last_messages(state.worker_session_id, max(1, min(limit, 50)))

    def judge_messages(self, limit: int) -> list[dict[str, Any]]:
        state = self.load_state()
        if not state.judge_session_id:
            return []
        return self.client.last_messages(state.judge_session_id, max(1, min(limit, 50)))

    def daemon_log_tail(self, lines: int) -> list[str]:
        path = self.runtime.daemon_log_path
        if not path.exists():
            return []
        text = path.read_text(encoding="utf-8", errors="replace").splitlines()
        return text[-max(1, min(lines, 5000)):]

    def opencode_log_tail(self, lines: int) -> list[str]:
        path = self.runtime.serve_log_path
        if not path.exists():
            return []
        text = path.read_text(encoding="utf-8", errors="replace").splitlines()
        return text[-max(1, min(lines, 5000)):]

    def diagnostic_log_tail(self, lines: int) -> list[str]:
        # Prefer the symlink (which always points at the live log); fall
        # back to the newest file in the diagnostic directory if the
        # symlink is missing (e.g. older runs before the symlink existed).
        symlink = self.runtime.diagnostic_log_symlink
        target: Path | None = None
        if symlink.exists():
            target = symlink
        else:
            log_dir = self.runtime.diagnostic_log_dir
            if log_dir.exists():
                candidates = sorted(p for p in log_dir.iterdir() if p.is_file() and p.suffix == ".log")
                target = candidates[-1] if candidates else None
        if target is None or not target.exists():
            return []
        text = target.read_text(encoding="utf-8", errors="replace").splitlines()
        return text[-max(1, min(lines, 5000)):]
