from __future__ import annotations

import contextlib
import json
import logging
import os
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from .client import OpenCodeClient
from .logging import get_logger, log_kv
from .prompts import (
    JudgeBootstrapCtx,
    _personality_text,
    build_judge_bootstrap,
    build_loop_reprompt,
)
from .runtime import GlobalRuntime, cwd_hash
from .state import (
    STATE_BACKUP_COUNT,
    CheckpointTag,
    GoalRecord,
    GoalState,
    HRConfig,
    LedgerEntry,
    PinnedMessage,
    VotingState,
    goal_to_dict,
    state_from_dict,
    state_to_dict,
)
from .voting import (
    VoteAction,
    VoteStep,
    on_judge_idle,
    on_president_idle,
    on_worker_idle,
)

_log = get_logger("goal.orchestrator")

DEFAULT_OPENCODE_URL = "http://127.0.0.1:4096"
WATCH_FAILURE_CIRCUIT_BREAKER = 5


class GoalNotFound(LookupError):
    pass


class GoalValidationError(ValueError):
    pass


class GoalOrchestrator:
    def __init__(self, runtime: GlobalRuntime) -> None:
        self.runtime = runtime
        base_url = os.environ.get("OPENCODE_URL", DEFAULT_OPENCODE_URL).rstrip("/")
        self._client = OpenCodeClient(base_url, directory="")

    def load_state(self) -> GoalState:
        path = self.runtime.state_path
        if not path.exists():
            return GoalState()
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            self._event("state_load_corrupt", error=str(exc))
            quarantine = path.with_suffix(f".corrupt.{int(time.time())}")
            with contextlib.suppress(OSError):
                path.rename(quarantine)
            return GoalState()
        try:
            return state_from_dict(data)
        except Exception as exc:
            self._event("state_parse_error", error=str(exc))
            return GoalState()

    def save_state(self, state: GoalState) -> None:
        path = self.runtime.state_path
        body = json.dumps(state_to_dict(state), indent=2, sort_keys=True) + "\n"
        self._rotate_backups(path)
        tmp = path.with_suffix(".json.tmp")
        try:
            fd = os.open(str(tmp), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            try:
                os.write(fd, body.encode("utf-8"))
                os.fsync(fd)
            finally:
                os.close(fd)
            os.replace(str(tmp), str(path))
        except OSError as exc:
            self._event("state_save_failed", error=str(exc))
            tmp.write_text(body, encoding="utf-8")
            tmp.replace(path)

    @staticmethod
    def _rotate_backups(path: Path) -> None:
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

    def _event(self, kind: str, **fields: Any) -> None:
        record = {"ts": datetime.now(timezone.utc).isoformat(), "event": kind, **fields}
        try:
            with self.runtime.daemon_log_path.open("a", encoding="utf-8") as fh:
                fh.write(json.dumps(record, sort_keys=True) + "\n")
        except OSError:
            pass

    def _client_for(self, directory: str) -> OpenCodeClient:
        return OpenCodeClient(
            self._client.base_url,
            directory=directory,
            password=self._client.password,
            username=self._client.username,
        )

    def start_goal(
        self,
        directory: str,
        worker_session_id: str,
        objective: str,
        hr: HRConfig,
    ) -> GoalRecord:
        ok, msg = hr.is_valid()
        if not ok:
            raise GoalValidationError(msg)

        directory = os.path.abspath(directory)
        ch = cwd_hash(directory)
        client = self._client_for(directory)
        sock = str(self.runtime.socket_path)

        for j in hr.judges:
            log_kv(
                _log, logging.INFO, "forking judge session", worker=worker_session_id
            )
            fork = client.fork_session(worker_session_id)
            j.session_id = fork["id"]
            bootstrap = build_judge_bootstrap(
                JudgeBootstrapCtx(
                    worker_session_id=worker_session_id,
                    judge_session_id=j.session_id,
                    objective=objective,
                    personality=_personality_text(j),
                    socket_path=sock,
                )
            )
            client.prompt_async(j.session_id, bootstrap, agent="plan", no_reply=True)
            log_kv(_log, logging.INFO, "judge created", judge=j.session_id)

        if hr.needs_president():
            log_kv(
                _log,
                logging.INFO,
                "forking president session",
                worker=worker_session_id,
            )
            fork = client.fork_session(worker_session_id)
            hr.president.session_id = fork["id"]
            log_kv(
                _log,
                logging.INFO,
                "president created",
                president=hr.president.session_id,
            )

        record = GoalRecord(
            cwd=directory,
            cwd_hash=ch,
            worker_session_id=worker_session_id,
            objective=objective,
            status="active",
            hr=hr,
            voting=VotingState(),
        )
        state = self.load_state()
        state.upsert(record)
        self.save_state(state)
        self._event(
            "goal_started",
            cwd=directory,
            worker=worker_session_id,
            judges=len(hr.judges),
            has_president=hr.needs_president(),
        )
        return record

    def _get(self, directory: str) -> tuple[GoalState, GoalRecord]:
        ch = cwd_hash(os.path.abspath(directory))
        state = self.load_state()
        rec = state.find(ch)
        if rec is None:
            raise GoalNotFound(f"No goal for {directory}")
        return state, rec

    def pause_goal(self, directory: str) -> GoalRecord:
        state, rec = self._get(directory)
        rec.status = "paused"
        rec.updated_at = int(time.time() * 1000)
        self.save_state(state)
        self._event("goal_paused", cwd=rec.cwd)
        return rec

    def resume_goal(self, directory: str) -> GoalRecord:
        state, rec = self._get(directory)
        rec.status = "active"
        rec.updated_at = int(time.time() * 1000)
        self.save_state(state)
        self._event("goal_resumed", cwd=rec.cwd)
        return rec

    def clear_goal(self, directory: str) -> bool:
        ch = cwd_hash(os.path.abspath(directory))
        state = self.load_state()
        rec = state.find(ch)
        if rec is None:
            return False
        state.remove(ch)
        self.save_state(state)
        self._event("goal_cleared", cwd=directory)
        return True

    def append_goal(self, directory: str, text: str) -> GoalRecord:
        state, rec = self._get(directory)
        rec.objective = rec.objective + "\n" + text
        rec.updated_at = int(time.time() * 1000)
        self.save_state(state)
        return rec

    def set_checkpoint(self, directory: str) -> GoalRecord:
        state, rec = self._get(directory)
        client = self._client_for(rec.cwd)
        latest = client.latest_assistant_message(rec.worker_session_id)
        if not latest:
            raise RuntimeError("Worker has no messages yet; cannot set checkpoint")
        msg_id = (latest.get("info") or {}).get("id", "")
        rec.checkpoint = CheckpointTag(
            session_id=rec.worker_session_id,
            message_id=msg_id,
        )
        rec.updated_at = int(time.time() * 1000)
        self.save_state(state)
        self._event(
            "checkpoint_set",
            cwd=rec.cwd,
            session_id=rec.worker_session_id,
            message_id=msg_id,
        )
        return rec

    def recover_from_checkpoint(self, directory: str) -> GoalRecord:
        state, rec = self._get(directory)
        if not rec.checkpoint:
            raise RuntimeError("No checkpoint set for this goal")
        client = self._client_for(rec.cwd)
        cp = rec.checkpoint

        log_kv(
            _log,
            logging.INFO,
            "recovering from checkpoint",
            session=cp.session_id,
            message=cp.message_id,
        )
        fork = client.fork_session(cp.session_id, cp.message_id)
        new_session_id = fork["id"]

        briefing = "\n".join(
            [
                "== CHECKPOINT RECOVERY ==",
                f"You are a fresh session forked from {cp.session_id} at message {cp.message_id}.",
                f"Objective: {rec.objective}",
                "",
                "Continue from where the previous session left off.",
                "Treat the above context as your starting state.",
            ]
        )
        client.prompt_async(new_session_id, briefing, agent="build")

        old_worker = rec.worker_session_id
        rec.worker_session_id = new_session_id
        rec.voting = VotingState()
        rec.last_worker_message_id = None
        rec.last_worker_completed_at = None
        rec.updated_at = int(time.time() * 1000)
        self.save_state(state)
        self._event(
            "checkpoint_recovered",
            cwd=rec.cwd,
            old_session=old_worker,
            new_session=new_session_id,
        )
        return rec

    def pin_message(
        self, directory: str, session_id: str, message_id: str, preview: str
    ) -> GoalRecord:
        state, rec = self._get(directory)
        if any(p.message_id == message_id for p in rec.pins):
            return rec
        rec.pins.append(
            PinnedMessage(
                session_id=session_id,
                message_id=message_id,
                preview=preview[:120],
            )
        )
        rec.updated_at = int(time.time() * 1000)
        self.save_state(state)
        return rec

    def unpin_message(self, directory: str, message_id: str) -> GoalRecord:
        state, rec = self._get(directory)
        rec.pins = [p for p in rec.pins if p.message_id != message_id]
        rec.updated_at = int(time.time() * 1000)
        self.save_state(state)
        return rec

    def update_hr(self, directory: str, hr: HRConfig) -> GoalRecord:
        state, rec = self._get(directory)
        rec.hr = hr
        rec.updated_at = int(time.time() * 1000)
        self.save_state(state)
        return rec

    def set_loop(self, directory: str, interval_seconds: int) -> GoalRecord:
        state, rec = self._get(directory)
        rec.loop_interval_seconds = interval_seconds
        rec.loop_last_fire_at = int(time.time() * 1000)
        rec.updated_at = int(time.time() * 1000)
        self.save_state(state)
        self._event("loop_set", cwd=rec.cwd, interval_s=interval_seconds)
        return rec

    def clear_loop(self, directory: str) -> GoalRecord:
        state, rec = self._get(directory)
        rec.loop_interval_seconds = None
        rec.loop_last_fire_at = None
        rec.updated_at = int(time.time() * 1000)
        self.save_state(state)
        self._event("loop_cleared", cwd=rec.cwd)
        return rec

    def _maybe_fire_loop(self, rec: GoalRecord, client: OpenCodeClient) -> bool:
        if not rec.loop_interval_seconds:
            return False
        now_ms = int(time.time() * 1000)
        last = rec.loop_last_fire_at or 0
        elapsed_s = (now_ms - last) / 1000
        if elapsed_s < rec.loop_interval_seconds:
            return False
        prompt = build_loop_reprompt(rec.objective)
        client.prompt_async(rec.worker_session_id, prompt, agent="build")
        rec.loop_last_fire_at = now_ms
        self._event("loop_fired", cwd=rec.cwd, elapsed_s=int(elapsed_s))
        log_kv(
            _log, logging.INFO, "loop tick", cwd=rec.cwd_hash, elapsed_s=int(elapsed_s)
        )
        return True

    def append_ledger(self, rec: GoalRecord, entry: LedgerEntry) -> None:
        path = self.runtime.ledger_path(rec.cwd_hash)
        row = {
            "timestamp": entry.timestamp,
            "worker_session_id": entry.worker_session_id,
            "trigger_message_id": entry.trigger_message_id,
            "round1_votes": entry.round1_votes,
            "round2_votes": entry.round2_votes,
            "president_verdict": entry.president_verdict,
            "outcome": entry.outcome,
            "action_taken": entry.action_taken,
        }
        try:
            with path.open("a", encoding="utf-8") as fh:
                fh.write(json.dumps(row, sort_keys=True) + "\n")
        except OSError as exc:
            log_kv(
                _log,
                logging.ERROR,
                "ledger write failed",
                cwd=rec.cwd_hash,
                error=str(exc),
            )

    def read_ledger(self, directory: str) -> list[dict[str, Any]]:
        ch = cwd_hash(os.path.abspath(directory))
        path = self.runtime.ledger_path(ch)
        if not path.exists():
            return []
        rows: list[dict[str, Any]] = []
        with contextlib.suppress(OSError):
            for line in path.read_text(encoding="utf-8").splitlines():
                line = line.strip()
                if line:
                    with contextlib.suppress(json.JSONDecodeError):
                        rows.append(json.loads(line))
        return rows

    def watch_tick(self, state: GoalState) -> bool:
        dirty = False
        for rec in list(state.goals):
            if rec.status != "active":
                continue
            if rec.consecutive_watch_failures >= WATCH_FAILURE_CIRCUIT_BREAKER:
                continue
            try:
                changed = self._tick_goal(rec)
                if changed:
                    dirty = True
                    rec.consecutive_watch_failures = 0
            except Exception as exc:
                rec.consecutive_watch_failures += 1
                dirty = True
                log_kv(
                    _log,
                    logging.ERROR,
                    "watch tick error",
                    cwd=rec.cwd_hash,
                    error=str(exc),
                    failures=rec.consecutive_watch_failures,
                )
                self._event("watch_error", cwd=rec.cwd, error=str(exc))
        return dirty

    def _tick_goal(self, rec: GoalRecord) -> bool:
        client = self._client_for(rec.cwd)
        dirty = False

        if self._maybe_fire_loop(rec, client):
            dirty = True

        latest = client.latest_assistant_message(rec.worker_session_id)
        if not latest:
            return dirty

        info = latest.get("info") or {}
        msg_id = info.get("id")
        completed = (info.get("time") or {}).get("completed")

        if not completed or not msg_id:
            return dirty

        worker_newly_idle = msg_id != rec.last_worker_message_id
        if worker_newly_idle:
            rec.last_worker_message_id = msg_id
            rec.last_worker_completed_at = int(time.time() * 1000)
            dirty = True

            if rec.voting.phase == "idle":
                step = on_worker_idle(rec, client, msg_id)
                if step:
                    dirty = True
                    self._execute_step(rec, client, step, state=None)

        if rec.voting.phase in ("round1", "round2"):
            for j in rec.hr.judges:
                if not j.session_id:
                    continue
                step = on_judge_idle(rec, client, j.session_id)
                if step:
                    dirty = True
                    self._execute_step(rec, client, step, state=None)
                    break

        if rec.voting.phase == "president":
            step = on_president_idle(rec, client)
            if step:
                dirty = True
                self._execute_step(rec, client, step, state=None)

        return dirty

    def _execute_step(
        self,
        rec: GoalRecord,
        client: OpenCodeClient,
        step: VoteStep,
        state: GoalState | None,
    ) -> None:
        log_kv(
            _log, logging.INFO, "vote step", cwd=rec.cwd_hash, action=step.action.value
        )

        if (
            step.action == VoteAction.START_ROUND1
            or step.action == VoteAction.ADVANCE_ROUND2
        ):
            self._send_split_prompts(client, step.payload, agent="build")

        elif step.action == VoteAction.ADVANCE_PRESIDENT:
            parts = step.payload.split(":", 1)
            if len(parts) == 2:
                client.prompt_async(parts[0], parts[1], agent="build")

        elif step.action == VoteAction.GOAL_COMPLETE:
            rec.status = "complete"
            rec.voting.phase = "idle"
            self._event("goal_complete", cwd=rec.cwd, reason=step.payload)
            self.append_ledger(
                rec,
                LedgerEntry(
                    timestamp=int(time.time() * 1000),
                    worker_session_id=rec.worker_session_id,
                    trigger_message_id=rec.voting.trigger_message_id,
                    round1_votes=[
                        {
                            "judge_session_id": r.judge_session_id,
                            "verdict": r.verdict,
                            "reason": r.reason,
                        }
                        for r in rec.voting.round1_results
                    ],
                    round2_votes=[
                        {
                            "judge_session_id": r.judge_session_id,
                            "verdict": r.verdict,
                            "reason": r.reason,
                        }
                        for r in rec.voting.round2_results
                    ],
                    president_verdict=rec.voting.president_result,
                    outcome="pass",
                    action_taken="goal_complete",
                ),
            )

        elif step.action in (VoteAction.SEND_TO_WORKER, VoteAction.SEND_STUCK):
            action_name = (
                "stuck_report"
                if step.action == VoteAction.SEND_STUCK
                else "worker_reprompted"
            )
            client.prompt_async(rec.worker_session_id, step.payload, agent="build")
            rec.voting.phase = "idle"
            self.append_ledger(
                rec,
                LedgerEntry(
                    timestamp=int(time.time() * 1000),
                    worker_session_id=rec.worker_session_id,
                    trigger_message_id=rec.voting.trigger_message_id,
                    round1_votes=[
                        {
                            "judge_session_id": r.judge_session_id,
                            "verdict": r.verdict,
                            "reason": r.reason,
                        }
                        for r in rec.voting.round1_results
                    ],
                    round2_votes=[
                        {
                            "judge_session_id": r.judge_session_id,
                            "verdict": r.verdict,
                            "reason": r.reason,
                        }
                        for r in rec.voting.round2_results
                    ],
                    president_verdict=rec.voting.president_result,
                    outcome="fail",
                    action_taken=action_name,
                ),
            )
            self._event(action_name, cwd=rec.cwd, blocks=rec.voting.consecutive_blocks)

    @staticmethod
    def _send_split_prompts(client: OpenCodeClient, payload: str, agent: str) -> None:
        for chunk in payload.split("\n---SPLIT---\n"):
            chunk = chunk.strip()
            if not chunk:
                continue
            sid, _, prompt = chunk.partition(":")
            if sid and prompt:
                client.prompt_async(sid.strip(), prompt.strip(), agent=agent)

    def goal_status(self, directory: str) -> dict[str, Any]:
        try:
            _, rec = self._get(directory)
        except GoalNotFound:
            return {"goal": None}
        return {"goal": goal_to_dict(rec)}

    def all_goals_status(self) -> dict[str, Any]:
        state = self.load_state()
        return {"goals": [goal_to_dict(g) for g in state.goals]}

    def get_user_messages(
        self, session_id: str, limit: int = 20
    ) -> list[dict[str, Any]]:
        messages = self._client.list_messages(session_id, limit=limit)
        return [
            {
                "id": (m.get("info") or {}).get("id"),
                "preview": _message_preview(m),
            }
            for m in messages
            if (m.get("info") or {}).get("role") == "user"
        ]

    def daemon_log_tail(self, lines: int) -> list[str]:
        path = self.runtime.daemon_log_path
        if not path.exists():
            return []
        return path.read_text(encoding="utf-8", errors="replace").splitlines()[-lines:]


def _message_preview(message: dict[str, Any]) -> str:
    parts = message.get("parts") or []
    for p in parts:
        text = p.get("content") or p.get("text") or ""
        if text:
            return text.strip()[:120]
    return "(no text)"
