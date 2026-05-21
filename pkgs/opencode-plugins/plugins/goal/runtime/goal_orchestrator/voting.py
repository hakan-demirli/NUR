from __future__ import annotations

import time
from dataclasses import dataclass
from enum import Enum

from .client import OpenCodeClient
from .logging import get_logger, log_kv
from .prompts import (
    JudgeRound1Ctx,
    JudgeRound2Ctx,
    PresidentSynthCtx,
    build_judge_round1,
    build_judge_round2,
    build_president_synth,
    build_stuck_report,
    build_worker_continue,
    parse_president_plan,
    parse_verdict,
)
from .state import GoalRecord, VoteResult

_log = get_logger("goal.voting")

BLOCK_CAP = 8
VOTE_TIMEOUT_S = 1800


class VoteAction(Enum):
    NONE = "none"
    START_ROUND1 = "start_round1"
    ADVANCE_ROUND2 = "advance_round2"
    ADVANCE_PRESIDENT = "advance_president"
    GOAL_COMPLETE = "goal_complete"
    SEND_TO_WORKER = "send_to_worker"
    SEND_STUCK = "send_stuck"


@dataclass
class VoteStep:
    action: VoteAction
    payload: str = ""


def _latest_assistant_text(
    client: OpenCodeClient, session_id: str
) -> tuple[str, str | None]:
    msg = client.latest_assistant_message(session_id)
    if not msg:
        return "", None
    parts = msg.get("parts") or []
    text = " ".join(
        p.get("content") or p.get("text") or ""
        for p in parts
        if p.get("type") in ("text", "tool-result")
    ).strip()
    msg_id = (msg.get("info") or {}).get("id")
    return text, msg_id


def _judge_has_responded_since_nudge(
    client: OpenCodeClient, judge_session_id: str, nudged_at: int
) -> tuple[bool, str, str | None]:
    msg = client.latest_assistant_message(judge_session_id)
    if not msg:
        return False, "", None
    info = msg.get("info") or {}
    completed = (info.get("time") or {}).get("completed")
    if not completed:
        return False, "", None
    try:
        completed_ms = int(completed)
    except (TypeError, ValueError):
        completed_ms = 0
    if completed_ms < nudged_at:
        return False, "", None
    parts = msg.get("parts") or []
    text = " ".join(
        p.get("content") or p.get("text") or ""
        for p in parts
        if p.get("type") == "text"
    ).strip()
    msg_id = info.get("id")
    return True, text, msg_id


def on_worker_idle(
    goal: GoalRecord,
    client: OpenCodeClient,
    worker_message_id: str,
) -> VoteStep | None:
    v = goal.voting
    if v.phase != "idle":
        log_kv(
            _log,
            10,
            "worker idle but voting already in progress",
            phase=v.phase,
            cwd=goal.cwd_hash,
        )
        return None

    text, _ = _latest_assistant_text(client, goal.worker_session_id)

    v.phase = "round1"
    v.trigger_message_id = worker_message_id
    v.round1_sent_at = int(time.time() * 1000)
    v.round1_results = []
    v.round2_results = []
    v.president_result = None

    prompts: list[tuple[str, str]] = []
    for j in goal.hr.judges:
        if not j.session_id:
            continue
        p = build_judge_round1(
            JudgeRound1Ctx(
                worker_session_id=goal.worker_session_id,
                judge_session_id=j.session_id,
                objective=goal.objective,
                worker_message_id=worker_message_id,
                worker_latest_text=text,
                socket_path="",
            )
        )
        prompts.append((j.session_id, p))

    log_kv(
        _log,
        20,
        "voting round1 started",
        cwd=goal.cwd_hash,
        judges=len(prompts),
        trigger=worker_message_id,
    )
    return VoteStep(
        VoteAction.START_ROUND1,
        payload="\n---SPLIT---\n".join(f"{sid}:{prompt}" for sid, prompt in prompts),
    )


def on_judge_idle(
    goal: GoalRecord,
    client: OpenCodeClient,
    judge_session_id: str,
) -> VoteStep | None:
    v = goal.voting

    if v.phase == "round1":
        return _collect_round1(goal, client, judge_session_id)
    if v.phase == "round2":
        return _collect_round2(goal, client, judge_session_id)

    return None


def on_president_idle(
    goal: GoalRecord,
    client: OpenCodeClient,
) -> VoteStep | None:
    v = goal.voting
    if v.phase != "president":
        return None

    pres = goal.hr.president
    if not pres.session_id:
        return None

    nudged_at = v.president_sent_at or 0
    responded, text, _msg_id = _judge_has_responded_since_nudge(
        client, pres.session_id, nudged_at
    )
    if not responded:
        return None

    plan = parse_president_plan(text)
    v.president_result = plan
    v.phase = "done"

    log_kv(_log, 20, "president responded", cwd=goal.cwd_hash, plan_chars=len(plan))
    return _emit_worker_action(goal)


def _collect_round1(
    goal: GoalRecord,
    client: OpenCodeClient,
    judge_session_id: str,
) -> VoteStep | None:
    v = goal.voting
    nudged_at = v.round1_sent_at or 0

    if any(r.judge_session_id == judge_session_id for r in v.round1_results):
        return None

    responded, text, msg_id = _judge_has_responded_since_nudge(
        client, judge_session_id, nudged_at
    )
    if not responded:
        return None

    verdict, reason = parse_verdict(text)
    v.round1_results.append(
        VoteResult(
            judge_session_id=judge_session_id,
            verdict=verdict,
            reason=reason,
            message_id=msg_id or "",
        )
    )
    log_kv(
        _log,
        20,
        "round1 vote collected",
        cwd=goal.cwd_hash,
        judge=judge_session_id[:8],
        verdict=verdict,
    )

    all_judge_ids = {j.session_id for j in goal.hr.judges if j.session_id}
    voted_ids = {r.judge_session_id for r in v.round1_results}
    now_ms = int(time.time() * 1000)
    timed_out = (now_ms - nudged_at) > VOTE_TIMEOUT_S * 1000

    if voted_ids >= all_judge_ids or (timed_out and voted_ids):
        return _advance_to_round2(goal, client)
    return None


def _advance_to_round2(goal: GoalRecord, client: OpenCodeClient) -> VoteStep | None:
    v = goal.voting

    if all(r.verdict == "pass" for r in v.round1_results):
        v.phase = "done"
        log_kv(_log, 20, "round1 unanimous pass → goal complete", cwd=goal.cwd_hash)
        return VoteStep(
            VoteAction.GOAL_COMPLETE, payload="All judges passed in round 1."
        )

    v.phase = "round2"
    v.round2_sent_at = int(time.time() * 1000)
    v.round2_results = []

    prompts: list[tuple[str, str]] = []
    for j in goal.hr.judges:
        if not j.session_id:
            continue
        peer_votes = [r for r in v.round1_results if r.judge_session_id != j.session_id]
        p = build_judge_round2(
            JudgeRound2Ctx(
                worker_session_id=goal.worker_session_id,
                judge_session_id=j.session_id,
                objective=goal.objective,
                worker_message_id=v.trigger_message_id,
                peer_votes=peer_votes,
            )
        )
        prompts.append((j.session_id, p))

    log_kv(_log, 20, "advancing to round2", cwd=goal.cwd_hash, judges=len(prompts))
    return VoteStep(
        VoteAction.ADVANCE_ROUND2,
        payload="\n---SPLIT---\n".join(f"{sid}:{prompt}" for sid, prompt in prompts),
    )


def _collect_round2(
    goal: GoalRecord,
    client: OpenCodeClient,
    judge_session_id: str,
) -> VoteStep | None:
    v = goal.voting
    nudged_at = v.round2_sent_at or 0

    if any(r.judge_session_id == judge_session_id for r in v.round2_results):
        return None

    responded, text, msg_id = _judge_has_responded_since_nudge(
        client, judge_session_id, nudged_at
    )
    if not responded:
        return None

    verdict, reason = parse_verdict(text)
    v.round2_results.append(
        VoteResult(
            judge_session_id=judge_session_id,
            verdict=verdict,
            reason=reason,
            message_id=msg_id or "",
        )
    )
    log_kv(
        _log,
        20,
        "round2 vote collected",
        cwd=goal.cwd_hash,
        judge=judge_session_id[:8],
        verdict=verdict,
    )

    all_judge_ids = {j.session_id for j in goal.hr.judges if j.session_id}
    voted_ids = {r.judge_session_id for r in v.round2_results}
    now_ms = int(time.time() * 1000)
    timed_out = (now_ms - nudged_at) > VOTE_TIMEOUT_S * 1000

    if voted_ids >= all_judge_ids or (timed_out and voted_ids):
        return _advance_to_president_or_worker(goal, client)
    return None


def _advance_to_president_or_worker(
    goal: GoalRecord, client: OpenCodeClient
) -> VoteStep | None:
    v = goal.voting

    if all(r.verdict == "pass" for r in v.round2_results):
        v.phase = "done"
        log_kv(_log, 20, "round2 unanimous pass → goal complete", cwd=goal.cwd_hash)
        return VoteStep(
            VoteAction.GOAL_COMPLETE, payload="All judges passed in round 2."
        )

    if not goal.hr.needs_president():
        v.phase = "done"
        only = v.round2_results[0] if v.round2_results else None
        plan = only.reason if only else "The judge requests changes."
        v.president_result = plan
        return _emit_worker_action(goal)

    v.phase = "president"
    v.president_sent_at = int(time.time() * 1000)

    pres = goal.hr.president
    if not pres.session_id:
        v.phase = "done"
        v.president_result = v.round2_results[0].reason if v.round2_results else ""
        return _emit_worker_action(goal)

    prompt = build_president_synth(
        PresidentSynthCtx(
            objective=goal.objective,
            worker_session_id=goal.worker_session_id,
            round2_votes=v.round2_results,
            consecutive_blocks=v.consecutive_blocks,
        )
    )
    log_kv(_log, 20, "advancing to president", cwd=goal.cwd_hash)
    return VoteStep(VoteAction.ADVANCE_PRESIDENT, payload=f"{pres.session_id}:{prompt}")


def _emit_worker_action(goal: GoalRecord) -> VoteStep:
    v = goal.voting
    all_pass = all(r.verdict == "pass" for r in (v.round2_results or v.round1_results))

    if all_pass:
        return VoteStep(VoteAction.GOAL_COMPLETE, payload="Council unanimous pass.")

    v.consecutive_blocks += 1
    plan = v.president_result or (
        v.round2_results[0].reason
        if v.round2_results
        else v.round1_results[0].reason
        if v.round1_results
        else "The council requests changes."
    )

    if v.consecutive_blocks >= BLOCK_CAP:
        log_kv(
            _log,
            30,
            "block cap hit, sending stuck report",
            cwd=goal.cwd_hash,
            blocks=v.consecutive_blocks,
        )
        text = build_stuck_report(goal.objective, v.consecutive_blocks, plan)
        return VoteStep(VoteAction.SEND_STUCK, payload=text)

    text = build_worker_continue(goal.objective, plan, v.consecutive_blocks)
    return VoteStep(VoteAction.SEND_TO_WORKER, payload=text)
