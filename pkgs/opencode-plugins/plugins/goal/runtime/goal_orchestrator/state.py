from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Any

STATE_SCHEMA_VERSION = 1
STATE_BACKUP_COUNT = 3


BUILTIN_PERSONALITIES = {
    "devils_advocate": (
        "You are a devil's advocate. Assume the worker is wrong. "
        "Find every way the objective might not actually be complete. "
        "Demand concrete evidence for every claim. Vote fail unless proven otherwise."
    ),
    "compiler": (
        "You are an objective compiler. Ignore prose and opinions. "
        "Only evaluate whether the stated deliverables exist and tests pass. "
        "Vote pass if and only if all verifiable criteria are demonstrably met."
    ),
    "peer_reviewer": (
        "You are a peer reviewer. Read the diff and evaluate code quality, "
        "naming, edge-case handling, and adherence to project conventions. "
        "Vote fail if you find substantive issues, even if tests pass."
    ),
}


@dataclass(slots=True)
class JudgeConfig:
    session_id: str | None = None
    provider_id: str | None = None
    model_id: str | None = None
    variant: str | None = None
    personality_key: str | None = None
    personality_custom: str | None = None


@dataclass(slots=True)
class HRConfig:
    president: JudgeConfig = field(default_factory=JudgeConfig)
    judges: list[JudgeConfig] = field(default_factory=list)

    def is_valid(self) -> tuple[bool, str]:
        if not self.judges:
            return False, "At least one judge is required"
        if len(self.judges) > 1 and not (
            self.president.provider_id and self.president.model_id
        ):
            return (
                False,
                "President must have a model configured when multiple judges exist",
            )
        for i, j in enumerate(self.judges):
            if not (j.provider_id and j.model_id):
                return False, f"Judge {i + 1} has no model configured"
        return True, ""

    def needs_president(self) -> bool:
        return len(self.judges) > 1


@dataclass(slots=True)
class CheckpointTag:
    session_id: str
    message_id: str
    created_at: int = field(default_factory=lambda: int(time.time() * 1000))


@dataclass(slots=True)
class PinnedMessage:
    session_id: str
    message_id: str
    preview: str
    pinned_at: int = field(default_factory=lambda: int(time.time() * 1000))


VOTE_PHASES = ("idle", "round1", "round2", "president", "done")


@dataclass(slots=True)
class VoteResult:
    judge_session_id: str
    verdict: str
    reason: str
    message_id: str
    collected_at: int = field(default_factory=lambda: int(time.time() * 1000))


@dataclass(slots=True)
class VotingState:
    phase: str = "idle"
    trigger_message_id: str | None = None
    round1_sent_at: int | None = None
    round1_results: list[VoteResult] = field(default_factory=list)
    round2_sent_at: int | None = None
    round2_results: list[VoteResult] = field(default_factory=list)
    president_sent_at: int | None = None
    president_result: str | None = None
    consecutive_blocks: int = 0


@dataclass(slots=True)
class LedgerEntry:
    timestamp: int
    worker_session_id: str
    trigger_message_id: str | None
    round1_votes: list[dict[str, Any]]
    round2_votes: list[dict[str, Any]]
    president_verdict: str | None
    outcome: str
    action_taken: str


@dataclass(slots=True)
class GoalRecord:
    cwd: str
    cwd_hash: str
    worker_session_id: str
    objective: str
    status: str
    hr: HRConfig
    voting: VotingState = field(default_factory=VotingState)
    checkpoint: CheckpointTag | None = None
    pins: list[PinnedMessage] = field(default_factory=list)
    loop_interval_seconds: int | None = None
    loop_last_fire_at: int | None = None
    last_worker_message_id: str | None = None
    last_worker_completed_at: int | None = None
    consecutive_watch_failures: int = 0
    created_at: int = field(default_factory=lambda: int(time.time() * 1000))
    updated_at: int = field(default_factory=lambda: int(time.time() * 1000))


@dataclass(slots=True)
class GoalState:
    schema_version: int = STATE_SCHEMA_VERSION
    goals: list[GoalRecord] = field(default_factory=list)

    def find(self, cwd_hash: str) -> GoalRecord | None:
        for g in self.goals:
            if g.cwd_hash == cwd_hash:
                return g
        return None

    def find_by_session(self, session_id: str) -> tuple[GoalRecord | None, str | None]:
        for g in self.goals:
            if g.worker_session_id == session_id:
                return g, "worker"
            if g.hr.president.session_id == session_id:
                return g, "president"
            for j in g.hr.judges:
                if j.session_id == session_id:
                    return g, "judge"
        return None, None

    def remove(self, cwd_hash: str) -> None:
        self.goals = [g for g in self.goals if g.cwd_hash != cwd_hash]

    def upsert(self, record: GoalRecord) -> None:
        for i, g in enumerate(self.goals):
            if g.cwd_hash == record.cwd_hash:
                self.goals[i] = record
                return
        self.goals.append(record)


def _judge_to_dict(j: JudgeConfig) -> dict[str, Any]:
    return {
        "session_id": j.session_id,
        "provider_id": j.provider_id,
        "model_id": j.model_id,
        "variant": j.variant,
        "personality_key": j.personality_key,
        "personality_custom": j.personality_custom,
    }


def _judge_from_dict(d: dict[str, Any]) -> JudgeConfig:
    return JudgeConfig(
        session_id=d.get("session_id"),
        provider_id=d.get("provider_id"),
        model_id=d.get("model_id"),
        variant=d.get("variant"),
        personality_key=d.get("personality_key"),
        personality_custom=d.get("personality_custom"),
    )


def _hr_to_dict(hr: HRConfig) -> dict[str, Any]:
    return {
        "president": _judge_to_dict(hr.president),
        "judges": [_judge_to_dict(j) for j in hr.judges],
    }


def _hr_from_dict(d: dict[str, Any]) -> HRConfig:
    return HRConfig(
        president=_judge_from_dict(d.get("president") or {}),
        judges=[_judge_from_dict(j) for j in (d.get("judges") or [])],
    )


def _checkpoint_to_dict(c: CheckpointTag | None) -> dict[str, Any] | None:
    if c is None:
        return None
    return {
        "session_id": c.session_id,
        "message_id": c.message_id,
        "created_at": c.created_at,
    }


def _checkpoint_from_dict(d: dict[str, Any] | None) -> CheckpointTag | None:
    if not d:
        return None
    return CheckpointTag(
        session_id=d["session_id"],
        message_id=d["message_id"],
        created_at=d.get("created_at", 0),
    )


def _pin_to_dict(p: PinnedMessage) -> dict[str, Any]:
    return {
        "session_id": p.session_id,
        "message_id": p.message_id,
        "preview": p.preview,
        "pinned_at": p.pinned_at,
    }


def _pin_from_dict(d: dict[str, Any]) -> PinnedMessage:
    return PinnedMessage(
        session_id=d["session_id"],
        message_id=d["message_id"],
        preview=d.get("preview", ""),
        pinned_at=d.get("pinned_at", 0),
    )


def _vote_result_to_dict(v: VoteResult) -> dict[str, Any]:
    return {
        "judge_session_id": v.judge_session_id,
        "verdict": v.verdict,
        "reason": v.reason,
        "message_id": v.message_id,
        "collected_at": v.collected_at,
    }


def _vote_result_from_dict(d: dict[str, Any]) -> VoteResult:
    return VoteResult(
        judge_session_id=d["judge_session_id"],
        verdict=d.get("verdict", "fail"),
        reason=d.get("reason", ""),
        message_id=d.get("message_id", ""),
        collected_at=d.get("collected_at", 0),
    )


def _voting_to_dict(v: VotingState) -> dict[str, Any]:
    return {
        "phase": v.phase,
        "trigger_message_id": v.trigger_message_id,
        "round1_sent_at": v.round1_sent_at,
        "round1_results": [_vote_result_to_dict(r) for r in v.round1_results],
        "round2_sent_at": v.round2_sent_at,
        "round2_results": [_vote_result_to_dict(r) for r in v.round2_results],
        "president_sent_at": v.president_sent_at,
        "president_result": v.president_result,
        "consecutive_blocks": v.consecutive_blocks,
    }


def _voting_from_dict(d: dict[str, Any]) -> VotingState:
    return VotingState(
        phase=d.get("phase", "idle"),
        trigger_message_id=d.get("trigger_message_id"),
        round1_sent_at=d.get("round1_sent_at"),
        round1_results=[
            _vote_result_from_dict(r) for r in (d.get("round1_results") or [])
        ],
        round2_sent_at=d.get("round2_sent_at"),
        round2_results=[
            _vote_result_from_dict(r) for r in (d.get("round2_results") or [])
        ],
        president_sent_at=d.get("president_sent_at"),
        president_result=d.get("president_result"),
        consecutive_blocks=d.get("consecutive_blocks", 0),
    )


def goal_to_dict(g: GoalRecord) -> dict[str, Any]:
    return {
        "cwd": g.cwd,
        "cwd_hash": g.cwd_hash,
        "worker_session_id": g.worker_session_id,
        "objective": g.objective,
        "status": g.status,
        "hr": _hr_to_dict(g.hr),
        "voting": _voting_to_dict(g.voting),
        "checkpoint": _checkpoint_to_dict(g.checkpoint),
        "pins": [_pin_to_dict(p) for p in g.pins],
        "loop_interval_seconds": g.loop_interval_seconds,
        "loop_last_fire_at": g.loop_last_fire_at,
        "last_worker_message_id": g.last_worker_message_id,
        "last_worker_completed_at": g.last_worker_completed_at,
        "consecutive_watch_failures": g.consecutive_watch_failures,
        "created_at": g.created_at,
        "updated_at": g.updated_at,
    }


def goal_from_dict(d: dict[str, Any]) -> GoalRecord:
    return GoalRecord(
        cwd=d["cwd"],
        cwd_hash=d["cwd_hash"],
        worker_session_id=d["worker_session_id"],
        objective=d["objective"],
        status=d.get("status", "active"),
        hr=_hr_from_dict(d.get("hr") or {}),
        voting=_voting_from_dict(d.get("voting") or {}),
        checkpoint=_checkpoint_from_dict(d.get("checkpoint")),
        pins=[_pin_from_dict(p) for p in (d.get("pins") or [])],
        loop_interval_seconds=d.get("loop_interval_seconds"),
        loop_last_fire_at=d.get("loop_last_fire_at"),
        last_worker_message_id=d.get("last_worker_message_id"),
        last_worker_completed_at=d.get("last_worker_completed_at"),
        consecutive_watch_failures=d.get("consecutive_watch_failures", 0),
        created_at=d.get("created_at", 0),
        updated_at=d.get("updated_at", 0),
    )


def state_to_dict(s: GoalState) -> dict[str, Any]:
    return {
        "schema_version": s.schema_version,
        "goals": [goal_to_dict(g) for g in s.goals],
    }


def state_from_dict(d: dict[str, Any]) -> GoalState:
    return GoalState(
        schema_version=d.get("schema_version", STATE_SCHEMA_VERSION),
        goals=[goal_from_dict(g) for g in (d.get("goals") or [])],
    )
