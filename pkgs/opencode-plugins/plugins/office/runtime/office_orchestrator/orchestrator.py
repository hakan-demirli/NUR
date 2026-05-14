from __future__ import annotations

import json
import logging
import os
import time
from dataclasses import asdict, dataclass, field
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
from .runtime import GlobalRuntime

_log = get_logger("office.orchestrator")

QUALITY_COMPACT_TOKENS = 300_000
LARGE_CONTEXT_WINDOW = 400_000
CONTEXT_RATIO_THRESHOLD = 0.6
JUDGE_NUDGE_COOLDOWN_MS = 15_000

STATE_SCHEMA_VERSION = 1
STATE_BACKUP_COUNT = 3

SLOT_HEALTH_VALUES = {
    "ok",
    "worker_missing",
    "judge_missing",
    "orphaned",
    "degraded",
    "unknown",
}


@dataclass(slots=True)
class JudgeSlot:
    directory: str
    worker_session_id: str
    judge_session_id: str | None = None
    enabled: bool = False
    paused: bool = False
    paused_at: int | None = None
    pending_events: list[dict[str, Any]] = field(default_factory=list)
    last_worker_message_id: str | None = None
    last_worker_completed_at: int | None = None
    last_judge_nudge_at: int | None = None
    last_nudged_worker_message_id: str | None = None
    last_judge_verdict: str | None = None
    health: str = "unknown"
    health_checked_at: int | None = None
    consecutive_failures: int = 0


@dataclass(slots=True)
class OfficeState:
    slots: list[JudgeSlot] = field(default_factory=list)


class SlotNotFound(LookupError):
    pass


class OfficeOrchestrator:
    def __init__(self, client: OpenCodeClient, runtime: GlobalRuntime) -> None:
        self.client = client
        self.runtime = runtime

    def event(self, kind: str, **fields: Any) -> None:
        record = {"ts": datetime.now(timezone.utc).isoformat(), "event": kind, **fields}
        try:
            with self.runtime.daemon_log_path.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps(record, sort_keys=True) + "\n")
        except OSError:
            pass

    @staticmethod
    def _migrate(data: dict[str, Any]) -> tuple[dict[str, Any], int]:
        version = data.get("schema_version", 1)
        if not isinstance(version, int):
            version = 1
        return data, version

    def load_state(self) -> OfficeState:
        path = self.runtime.state_path
        if not path.exists():
            return OfficeState()
        try:
            raw = path.read_text(encoding="utf-8")
            data = json.loads(raw)
        except (OSError, json.JSONDecodeError) as exc:
            self.event("state_load_corrupt", error=str(exc), path=str(path))
            try:
                quarantine = path.with_suffix(f".corrupt.{int(time.time())}")
                path.rename(quarantine)
                self.event("state_quarantined", path=str(quarantine))
            except OSError:
                pass
            return OfficeState()
        if not isinstance(data, dict):
            return OfficeState()
        data, _version = self._migrate(data)
        raw_slots = data.get("slots")
        if not isinstance(raw_slots, list):
            return OfficeState()
        valid_keys = set(JudgeSlot.__dataclass_fields__)
        slots: list[JudgeSlot] = []
        for entry in raw_slots:
            if not isinstance(entry, dict):
                continue
            kwargs = {k: v for k, v in entry.items() if k in valid_keys}
            try:
                slots.append(JudgeSlot(**kwargs))
            except TypeError as exc:
                self.event(
                    "state_slot_skipped",
                    error=str(exc),
                    directory=entry.get("directory"),
                    worker_session_id=entry.get("worker_session_id"),
                )
                continue
        return OfficeState(slots=slots)

    def _rotate_backups(self) -> None:
        path = self.runtime.state_path
        if not path.exists():
            return
        try:
            for i in range(STATE_BACKUP_COUNT, 1, -1):
                src = path.with_suffix(f".json.bak.{i - 1}")
                dst = path.with_suffix(f".json.bak.{i}")
                if src.exists():
                    src.replace(dst)
            tmp = path.with_suffix(".json.bak.1.tmp")
            tmp.write_bytes(path.read_bytes())
            tmp.replace(path.with_suffix(".json.bak.1"))
        except OSError:
            pass

    def save_state(self, state: OfficeState) -> None:
        path = self.runtime.state_path
        payload = {
            "schema_version": STATE_SCHEMA_VERSION,
            "saved_at": int(time.time() * 1000),
            "slots": [asdict(slot) for slot in state.slots],
        }
        body = json.dumps(payload, indent=2, sort_keys=True) + "\n"
        self._rotate_backups()
        tmp = path.with_suffix(".json.tmp")
        try:
            fd = os.open(str(tmp), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            try:
                os.write(fd, body.encode("utf-8"))
                os.fsync(fd)
            finally:
                os.close(fd)
            os.replace(str(tmp), str(path))
            try:
                dir_fd = os.open(str(path.parent), os.O_RDONLY)
                try:
                    os.fsync(dir_fd)
                finally:
                    os.close(dir_fd)
            except OSError:
                pass
        except OSError as exc:
            self.event("state_save_fsync_failed", error=str(exc))
            tmp.write_text(body, encoding="utf-8")
            tmp.replace(path)

    def find_slot(
        self,
        directory: str,
        worker_session_id: str,
        *,
        state: OfficeState | None = None,
    ) -> JudgeSlot:
        state = state or self.load_state()
        for slot in state.slots:
            if (
                slot.directory == directory
                and slot.worker_session_id == worker_session_id
            ):
                return slot
        raise SlotNotFound(
            f"no slot for directory={directory!r} worker={worker_session_id!r}"
        )

    def find_slot_optional(
        self,
        directory: str,
        worker_session_id: str,
        *,
        state: OfficeState | None = None,
    ) -> JudgeSlot | None:
        try:
            return self.find_slot(directory, worker_session_id, state=state)
        except SlotNotFound:
            return None

    def upsert_slot(
        self,
        directory: str,
        worker_session_id: str,
        *,
        state: OfficeState | None = None,
    ) -> tuple[OfficeState, JudgeSlot]:
        state = state or self.load_state()
        existing = self.find_slot_optional(directory, worker_session_id, state=state)
        if existing is not None:
            return state, existing
        slot = JudgeSlot(directory=directory, worker_session_id=worker_session_id)
        state.slots.append(slot)
        return state, slot

    def remove_slot(
        self,
        directory: str,
        worker_session_id: str,
        *,
        state: OfficeState | None = None,
    ) -> OfficeState:
        state = state or self.load_state()
        state.slots = [
            s
            for s in state.slots
            if not (
                s.directory == directory and s.worker_session_id == worker_session_id
            )
        ]
        self.save_state(state)
        return state

    def slots_for_directory(
        self, directory: str, *, state: OfficeState | None = None
    ) -> list[JudgeSlot]:
        state = state or self.load_state()
        return [s for s in state.slots if s.directory == directory]

    def reconcile_on_boot(self) -> dict[str, int]:
        state = self.load_state()
        counters = {
            "ok": 0,
            "worker_missing": 0,
            "judge_missing": 0,
            "orphaned": 0,
            "errors": 0,
            "total": len(state.slots),
        }
        now_ms = int(time.time() * 1000)
        dirty = False
        for slot in state.slots:
            slot.health_checked_at = now_ms
            try:
                client = self._client_for(slot.directory)
                worker_ok = True
                judge_ok = True
                if slot.worker_session_id:
                    try:
                        client.get_session(slot.worker_session_id)
                    except Exception:
                        worker_ok = False
                else:
                    worker_ok = False
                if slot.judge_session_id:
                    try:
                        client.get_session(slot.judge_session_id)
                    except Exception:
                        judge_ok = False
                else:
                    judge_ok = False
                new_health = "ok"
                if not worker_ok and not judge_ok:
                    new_health = "orphaned"
                elif not worker_ok:
                    new_health = "worker_missing"
                elif not judge_ok:
                    new_health = "judge_missing"
                if new_health != "ok" and slot.enabled:
                    slot.enabled = False
                    dirty = True
                if slot.health != new_health:
                    slot.health = new_health
                    dirty = True
                slot.consecutive_failures = 0
                counters[new_health] = counters.get(new_health, 0) + 1
            except Exception as exc:
                if slot.health != "degraded":
                    slot.health = "degraded"
                    dirty = True
                counters["errors"] += 1
                self.event(
                    "reconcile_error",
                    directory=slot.directory,
                    worker=slot.worker_session_id,
                    error=str(exc),
                )
        if dirty:
            self.save_state(state)
        self.event("boot_reconciliation", **counters)
        return counters

    def _client_for(self, directory: str) -> OpenCodeClient:
        return OpenCodeClient(
            self.client.base_url,
            directory=directory,
            password=self.client.password,
            username=self.client.username,
            timeout=self.client.timeout,
        )

    def should_compact(
        self, client: OpenCodeClient, session_id: str
    ) -> tuple[bool, str | None]:
        summary = client.summary(session_id)
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
        if (
            context_limit
            and context_limit >= LARGE_CONTEXT_WINDOW
            and total >= QUALITY_COMPACT_TOKENS
        ):
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
        if (
            context_limit
            and context_limit > 0
            and ratio is not None
            and ratio >= CONTEXT_RATIO_THRESHOLD
        ):
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

    def judge_bootstrap(
        self, directory: str, worker_session_id: str, judge_session_id: str
    ) -> str:
        return (
            build_judge_bootstrap(
                JudgeBootstrapContext(
                    worker_session_id=worker_session_id,
                    judge_session_id=judge_session_id,
                    socket_path=str(self.runtime.socket_path),
                )
            )
            + f"\nProject directory: {directory}\n"
        )

    def enable_judge(self, directory: str, worker_session_id: str) -> JudgeSlot:
        state = self.load_state()
        state, slot = self.upsert_slot(directory, worker_session_id, state=state)
        client = self._client_for(directory)
        log_kv(
            _log,
            logging.INFO,
            "enable_judge: forking from worker",
            worker=worker_session_id,
            directory=directory,
        )
        judge = client.fork_session(worker_session_id)
        judge_session_id = judge["id"]
        log_kv(
            _log,
            logging.INFO,
            "judge forked",
            worker=worker_session_id,
            judge=judge_session_id,
        )
        client.prompt_async(
            judge_session_id,
            self.judge_bootstrap(directory, worker_session_id, judge_session_id),
            agent="plan",
            no_reply=True,
        )
        slot.judge_session_id = judge_session_id
        slot.last_worker_message_id = None
        slot.last_worker_completed_at = None
        slot.last_judge_nudge_at = None
        slot.last_nudged_worker_message_id = None
        slot.last_judge_verdict = None
        slot.enabled = True
        slot.paused = False
        slot.paused_at = None
        slot.pending_events = []
        self.save_state(state)
        self.event(
            "judge_on",
            directory=directory,
            worker=worker_session_id,
            judge=judge_session_id,
        )
        return slot

    def disable_judge(self, directory: str, worker_session_id: str) -> JudgeSlot:
        state = self.load_state()
        slot = self.find_slot(directory, worker_session_id, state=state)
        slot.enabled = False
        self.save_state(state)
        log_kv(
            _log,
            logging.INFO,
            "judge disabled",
            worker=slot.worker_session_id,
            judge=slot.judge_session_id,
        )
        self.event(
            "judge_off",
            directory=directory,
            worker=slot.worker_session_id,
            judge=slot.judge_session_id,
        )
        return slot

    def forget_slot(self, directory: str, worker_session_id: str) -> bool:
        state = self.load_state()
        before = len(state.slots)
        state = self.remove_slot(directory, worker_session_id, state=state)
        removed = before != len(state.slots)
        if removed:
            self.event("slot_forgotten", directory=directory, worker=worker_session_id)
        return removed

    def pause(self, directory: str, worker_session_id: str) -> JudgeSlot:
        state = self.load_state()
        slot = self.find_slot(directory, worker_session_id, state=state)
        if slot.paused:
            log_kv(
                _log,
                logging.DEBUG,
                "pause: already paused; no-op",
                worker=worker_session_id,
            )
            return slot
        slot.paused = True
        slot.paused_at = int(time.time() * 1000)
        self.save_state(state)
        log_kv(
            _log,
            logging.INFO,
            "judge paused",
            worker=worker_session_id,
            queued=len(slot.pending_events),
        )
        self.event(
            "judge_paused",
            directory=directory,
            worker=slot.worker_session_id,
            judge=slot.judge_session_id,
            queued=len(slot.pending_events),
        )
        return slot

    def resume(
        self, directory: str, worker_session_id: str
    ) -> tuple[JudgeSlot, list[dict[str, Any]]]:
        state = self.load_state()
        slot = self.find_slot(directory, worker_session_id, state=state)
        if not slot.paused:
            log_kv(
                _log,
                logging.DEBUG,
                "resume: not paused; no-op",
                worker=worker_session_id,
            )
            return slot, []
        queued = list(slot.pending_events or [])
        slot.paused = False
        slot.paused_at = None
        slot.pending_events = []
        self.save_state(state)
        log_kv(
            _log,
            logging.INFO,
            "judge resumed",
            worker=worker_session_id,
            to_replay=len(queued),
        )
        self.event(
            "judge_resumed",
            directory=directory,
            worker=slot.worker_session_id,
            judge=slot.judge_session_id,
            replay=len(queued),
        )

        replayed: list[dict[str, Any]] = []
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
                slot = self.nudge_judge(
                    directory,
                    worker_session_id,
                    reason=f"resume replay ({summary})",
                    force=True,
                )
                replayed.append(entry)
        return slot, replayed

    def queue_event(self, slot: JudgeSlot, kind: str, payload: dict[str, Any]) -> None:
        record = {
            "kind": kind,
            "queued_at": int(time.time() * 1000),
            **payload,
        }
        last_msg = (slot.pending_events[-1] if slot.pending_events else {}).get(
            "worker_message_id"
        )
        if (
            record.get("worker_message_id")
            and record.get("worker_message_id") == last_msg
        ):
            slot.pending_events[-1] = record
        else:
            slot.pending_events.append(record)
        truncated = False
        if len(slot.pending_events) > 50:
            slot.pending_events = slot.pending_events[-50:]
            truncated = True
        log_kv(
            _log,
            logging.DEBUG,
            "queued event",
            worker=slot.worker_session_id,
            kind=kind,
            queue_size=len(slot.pending_events),
            worker_message=record.get("worker_message_id"),
            reason=record.get("reason"),
            truncated=truncated,
        )
        self.event(
            "judge_event_queued",
            directory=slot.directory,
            worker=slot.worker_session_id,
            event_kind=kind,
            queue_size=len(slot.pending_events),
            worker_message=record.get("worker_message_id"),
            reason=record.get("reason"),
        )

    def nudge_judge(
        self,
        directory: str,
        worker_session_id: str,
        *,
        reason: str,
        force: bool = False,
    ) -> JudgeSlot:
        state = self.load_state()
        slot = self.find_slot(directory, worker_session_id, state=state)
        if not slot.enabled or not slot.judge_session_id:
            raise RuntimeError("Judge mode is not enabled for this slot")

        client = self._client_for(directory)
        latest = client.latest_assistant_message(slot.worker_session_id)
        latest_message_id = latest["info"]["id"] if latest else None

        log_kv(
            _log,
            logging.DEBUG,
            "nudge_judge entered",
            reason=reason,
            force=force,
            paused=slot.paused,
            worker=worker_session_id,
            latest_message=latest_message_id,
            last_nudged_message=slot.last_nudged_worker_message_id,
            last_nudge_at=slot.last_judge_nudge_at,
        )

        if slot.paused and not force:
            log_kv(
                _log,
                logging.DEBUG,
                "nudge diverted to pending queue (paused)",
                reason=reason,
                worker_message=latest_message_id,
            )
            self.queue_event(
                slot,
                "judge_nudge",
                {"reason": reason, "worker_message_id": latest_message_id},
            )
            self.save_state(state)
            return slot

        now_ms = int(time.time() * 1000)
        if (
            not force
            and slot.last_judge_nudge_at
            and now_ms - slot.last_judge_nudge_at < JUDGE_NUDGE_COOLDOWN_MS
        ):
            log_kv(
                _log,
                logging.DEBUG,
                "nudge suppressed by cooldown",
                reason=reason,
                cooldown_ms=JUDGE_NUDGE_COOLDOWN_MS,
                age_ms=now_ms - slot.last_judge_nudge_at,
            )
            return slot
        if (
            not force
            and latest_message_id
            and latest_message_id == slot.last_nudged_worker_message_id
        ):
            log_kv(
                _log,
                logging.DEBUG,
                "nudge suppressed by message dedup",
                reason=reason,
                worker_message=latest_message_id,
            )
            return slot

        judge_compact, judge_reason = self.should_compact(client, slot.judge_session_id)
        worker_compact, worker_reason = self.should_compact(
            client, slot.worker_session_id
        )
        if judge_compact:
            log_kv(
                _log,
                logging.INFO,
                "summarizing judge before nudge",
                judge=slot.judge_session_id,
                reason=judge_reason,
            )
            client.summarize(slot.judge_session_id)

        prompt = build_judge_nudge(
            JudgeNudgeContext(
                worker_session_id=slot.worker_session_id,
                judge_session_id=slot.judge_session_id,
                socket_path=str(self.runtime.socket_path),
                reason=reason,
                latest_message_id=latest_message_id,
                worker_summary=client.summary(slot.worker_session_id),
                judge_summary=client.summary(slot.judge_session_id),
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
            worker=worker_session_id,
            worker_message=latest_message_id,
            prompt_chars=len(prompt),
            worker_compact_recommended=worker_compact,
            judge_compacted_first=judge_compact,
            forced=force,
        )
        client.prompt_async(slot.judge_session_id, prompt, agent="build")
        slot.last_judge_nudge_at = now_ms
        slot.last_nudged_worker_message_id = latest_message_id
        self.save_state(state)
        self.event(
            "judge_nudged",
            directory=directory,
            worker=slot.worker_session_id,
            judge=slot.judge_session_id,
            reason=reason,
            worker_message=latest_message_id,
            worker_compact_recommended=worker_compact,
            judge_compacted_first=judge_compact,
        )
        return slot

    def prompt_worker(
        self,
        directory: str,
        worker_session_id: str,
        message: str,
        *,
        compact_first: bool = False,
    ) -> JudgeSlot:
        state = self.load_state()
        slot = self.find_slot(directory, worker_session_id, state=state)
        client = self._client_for(directory)
        if compact_first:
            log_kv(
                _log,
                logging.INFO,
                "compacting worker before prompt",
                worker=slot.worker_session_id,
            )
            client.summarize(slot.worker_session_id)
            self.event(
                "worker_compacted", directory=directory, worker=slot.worker_session_id
            )
        log_kv(
            _log,
            logging.INFO,
            "prompting worker",
            worker=slot.worker_session_id,
            chars=len(message),
            compact_first=compact_first,
        )
        client.prompt_async(slot.worker_session_id, message, agent="build")
        self.event(
            "worker_prompt",
            directory=directory,
            worker=slot.worker_session_id,
            compact_first=compact_first,
            length=len(message),
        )
        return slot

    def compact_worker(
        self, directory: str, worker_session_id: str, *, message: str | None = None
    ) -> JudgeSlot:
        state = self.load_state()
        slot = self.find_slot(directory, worker_session_id, state=state)
        client = self._client_for(directory)
        log_kv(
            _log,
            logging.INFO,
            "compact_worker invoked",
            worker=slot.worker_session_id,
            has_message=bool(message),
        )
        client.summarize(slot.worker_session_id)
        self.event(
            "worker_compacted", directory=directory, worker=slot.worker_session_id
        )
        if message:
            client.prompt_async(slot.worker_session_id, message, agent="build")
            self.event(
                "worker_prompt",
                directory=directory,
                worker=slot.worker_session_id,
                compact_first=True,
                length=len(message),
            )
        return slot

    def compact_judge(
        self, directory: str, worker_session_id: str, *, message: str | None = None
    ) -> JudgeSlot:
        state = self.load_state()
        slot = self.find_slot(directory, worker_session_id, state=state)
        if not slot.judge_session_id:
            raise RuntimeError("No judge session for this slot")
        client = self._client_for(directory)
        log_kv(
            _log,
            logging.INFO,
            "compact_judge invoked",
            judge=slot.judge_session_id,
            has_message=bool(message),
        )
        client.summarize(slot.judge_session_id)
        self.event("judge_compacted", directory=directory, judge=slot.judge_session_id)
        if message:
            client.prompt_async(slot.judge_session_id, message, agent="build")
            self.event(
                "judge_prompt",
                directory=directory,
                judge=slot.judge_session_id,
                length=len(message),
            )
        return slot

    def worker_update(self, slot: JudgeSlot) -> bool:
        if not slot.worker_session_id:
            return False
        client = self._client_for(slot.directory)
        latest = client.latest_assistant_message(slot.worker_session_id)
        if not latest:
            return False
        message_id = latest["info"]["id"]
        completed = (latest.get("info", {}).get("time") or {}).get("completed")
        if not completed:
            log_kv(
                _log,
                logging.DEBUG,
                "worker turn ignored (still streaming)",
                worker=slot.worker_session_id,
                message=message_id,
            )
            return False
        if message_id == slot.last_worker_message_id:
            return False
        log_kv(
            _log,
            logging.INFO,
            "worker turn detected",
            worker=slot.worker_session_id,
            message=message_id,
            previous=slot.last_worker_message_id,
            completed_at=completed,
        )
        slot.last_worker_message_id = message_id
        slot.last_worker_completed_at = completed
        self.event(
            "worker_turn_detected",
            directory=slot.directory,
            worker=slot.worker_session_id,
            message=message_id,
        )
        return True

    def _slot_summary(self, slot: JudgeSlot) -> dict[str, Any]:
        client = self._client_for(slot.directory)
        worker = (
            client.summary(slot.worker_session_id) if slot.worker_session_id else None
        )
        judge = client.summary(slot.judge_session_id) if slot.judge_session_id else None

        def _tokens_line(summary: dict[str, Any] | None) -> str:
            if not summary or not summary.get("tokens"):
                return "tokens=unknown"
            tokens = summary["tokens"]
            ctx = summary.get("context_limit") or 0
            ratio = f" ({tokens['total'] / ctx:.1%})" if ctx else ""
            return f"tokens={tokens['total']:,} / ctx={ctx:,}{ratio}"

        return {
            "directory": slot.directory,
            "enabled": slot.enabled,
            "paused": slot.paused,
            "paused_at": slot.paused_at,
            "queued_events": len(slot.pending_events or []),
            "worker": {
                "session_id": slot.worker_session_id,
                "model": worker.get("model") if worker else None,
                "tokens": worker.get("tokens") if worker else None,
                "context_limit": worker.get("context_limit") if worker else None,
                "human": _tokens_line(worker),
                "last_message_id": slot.last_worker_message_id,
            },
            "judge": {
                "session_id": slot.judge_session_id,
                "model": judge.get("model") if judge else None,
                "tokens": judge.get("tokens") if judge else None,
                "context_limit": judge.get("context_limit") if judge else None,
                "human": _tokens_line(judge),
                "last_nudge_at": slot.last_judge_nudge_at,
            },
            "trigger": {
                "last_judge_nudge_at": slot.last_judge_nudge_at,
                "last_nudged_worker_message_id": slot.last_nudged_worker_message_id,
            },
        }

    def status_payload(
        self, *, directory: str | None = None, worker_session_id: str | None = None
    ) -> dict[str, Any]:
        state = self.load_state()
        slots = state.slots
        if directory is not None:
            slots = [s for s in slots if s.directory == directory]
        if worker_session_id is not None:
            slots = [s for s in slots if s.worker_session_id == worker_session_id]
        return {
            "slots": [asdict(slot) for slot in slots],
            "paths": self.paths_payload(),
        }

    def slot_by_session(self, session_id: str) -> dict[str, Any]:
        state = self.load_state()
        for slot in state.slots:
            if (
                slot.worker_session_id == session_id
                or slot.judge_session_id == session_id
            ):
                side = "worker" if slot.worker_session_id == session_id else "judge"
                return {
                    "slot": {
                        "directory": slot.directory,
                        "worker_session_id": slot.worker_session_id,
                        "judge_session_id": slot.judge_session_id,
                        "enabled": slot.enabled,
                        "paused": slot.paused,
                        "health": slot.health,
                        "health_checked_at": slot.health_checked_at,
                        "last_judge_verdict": slot.last_judge_verdict,
                    },
                    "side": side,
                }
        return {"slot": None, "side": None}

    def summary_payload(
        self, *, directory: str | None = None, worker_session_id: str | None = None
    ) -> dict[str, Any]:
        state = self.load_state()
        slots = state.slots
        if directory is not None:
            slots = [s for s in slots if s.directory == directory]
        if worker_session_id is not None:
            slots = [s for s in slots if s.worker_session_id == worker_session_id]
        return {
            "slots": [self._slot_summary(slot) for slot in slots],
        }

    def paths_payload(self) -> dict[str, str]:
        return {
            "runtime_dir": str(self.runtime.root),
            "socket": str(self.runtime.socket_path),
            "state_file": str(self.runtime.state_path),
            "pid_file": str(self.runtime.pid_path),
            "daemon_log": str(self.runtime.daemon_log_path),
            "diagnostic_log": str(self.runtime.diagnostic_log_symlink),
            "diagnostic_log_dir": str(self.runtime.diagnostic_log_dir),
        }

    def queued_events(
        self, directory: str, worker_session_id: str
    ) -> list[dict[str, Any]]:
        slot = self.find_slot_optional(directory, worker_session_id)
        if slot is None:
            return []
        return list(slot.pending_events or [])

    def worker_messages(
        self, directory: str, worker_session_id: str, limit: int
    ) -> list[dict[str, Any]]:
        slot = self.find_slot_optional(directory, worker_session_id)
        if slot is None or not slot.worker_session_id:
            return []
        return self._client_for(directory).last_messages(
            slot.worker_session_id, max(1, min(limit, 50))
        )

    def judge_messages(
        self, directory: str, worker_session_id: str, limit: int
    ) -> list[dict[str, Any]]:
        slot = self.find_slot_optional(directory, worker_session_id)
        if slot is None or not slot.judge_session_id:
            return []
        return self._client_for(directory).last_messages(
            slot.judge_session_id, max(1, min(limit, 50))
        )

    def daemon_log_tail(self, lines: int) -> list[str]:
        path = self.runtime.daemon_log_path
        if not path.exists():
            return []
        text = path.read_text(encoding="utf-8", errors="replace").splitlines()
        return text[-max(1, min(lines, 5000)) :]

    def diagnostic_log_tail(self, lines: int) -> list[str]:
        symlink = self.runtime.diagnostic_log_symlink
        target: Path | None = None
        if symlink.exists():
            target = symlink
        else:
            log_dir = self.runtime.diagnostic_log_dir
            if log_dir.exists():
                candidates = sorted(
                    p for p in log_dir.iterdir() if p.is_file() and p.suffix == ".log"
                )
                target = candidates[-1] if candidates else None
        if target is None or not target.exists():
            return []
        text = target.read_text(encoding="utf-8", errors="replace").splitlines()
        return text[-max(1, min(lines, 5000)) :]
