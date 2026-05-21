from __future__ import annotations

from dataclasses import dataclass

from .state import BUILTIN_PERSONALITIES, JudgeConfig, VoteResult


def _personality_text(j: JudgeConfig) -> str:
    if j.personality_custom:
        return j.personality_custom
    if j.personality_key and j.personality_key in BUILTIN_PERSONALITIES:
        return BUILTIN_PERSONALITIES[j.personality_key]
    return (
        "You are an impartial evaluator. Vote pass only when the objective "
        "is demonstrably complete based on evidence in the transcript and repository."
    )


@dataclass(frozen=True, slots=True)
class JudgeBootstrapCtx:
    worker_session_id: str
    judge_session_id: str
    objective: str
    personality: str
    socket_path: str


def build_judge_bootstrap(ctx: JudgeBootstrapCtx) -> str:
    sock = ctx.socket_path
    return "\n".join(
        [
            f"You are a judge for worker session {ctx.worker_session_id}.",
            f"Your session ID is {ctx.judge_session_id}.",
            "",
            "Objective the worker is pursuing:",
            f"  {ctx.objective}",
            "",
            "Your evaluation persona:",
            f"  {ctx.personality}",
            "",
            "You will be nudged periodically to evaluate the worker's latest output.",
            "Each nudge will tell you exactly what to do. Wait for nudges; do not act spontaneously.",
            "",
            "Daemon socket (for reading worker messages if needed):",
            f"  {sock}",
            f"  curl -s --unix-socket {sock} 'http://localhost/session/messages"
            f"?session_id={ctx.worker_session_id}&limit=5'",
            "",
            "When asked to vote, respond with a short structured block:",
            "  VERDICT: pass   (or)   VERDICT: fail",
            "  REASON: <one concise paragraph of specific evidence>",
            "",
            "Nothing else is expected from you right now. Wait for your first evaluation nudge.",
        ]
    )


@dataclass(frozen=True, slots=True)
class JudgeRound1Ctx:
    worker_session_id: str
    judge_session_id: str
    objective: str
    worker_message_id: str | None
    worker_latest_text: str
    socket_path: str


def build_judge_round1(ctx: JudgeRound1Ctx) -> str:
    return "\n".join(
        [
            "== EVALUATION REQUEST — ROUND 1 ==",
            "",
            "Objective:",
            f"  {ctx.objective}",
            "",
            f"Worker's latest output (message {ctx.worker_message_id or 'unknown'}):",
            "---",
            ctx.worker_latest_text[:4000],
            "---",
            "",
            "Evaluate independently. Do not speculate about what other judges think.",
            "Read the repository and transcript evidence if you need more context.",
            f"  curl -s --unix-socket {ctx.socket_path} 'http://localhost/session/messages"
            f"?session_id={ctx.worker_session_id}&limit=10'",
            "",
            "Respond ONLY with:",
            "  VERDICT: pass   (or)   VERDICT: fail",
            "  REASON: <specific evidence, one paragraph>",
        ]
    )


@dataclass(frozen=True, slots=True)
class JudgeRound2Ctx:
    worker_session_id: str
    judge_session_id: str
    objective: str
    worker_message_id: str | None
    peer_votes: list[VoteResult]


def build_judge_round2(ctx: JudgeRound2Ctx) -> str:
    peer_section = (
        "\n".join(
            [
                f"  Judge {v.judge_session_id[:8]}: {v.verdict.upper()} — {v.reason}"
                for v in ctx.peer_votes
            ]
        )
        or "  (no peer votes)"
    )

    return "\n".join(
        [
            "== EVALUATION REQUEST — ROUND 2 (cross-pollinated) ==",
            "",
            "Objective:",
            f"  {ctx.objective}",
            "",
            "Peer judges' round-1 verdicts (for context only — form your own conclusion):",
            peer_section,
            "",
            "Reconsider with this peer context. Your final verdict will go to the president.",
            "",
            "Respond ONLY with:",
            "  VERDICT: pass   (or)   VERDICT: fail",
            "  REASON: <specific evidence, one paragraph>",
        ]
    )


@dataclass(frozen=True, slots=True)
class PresidentSynthCtx:
    objective: str
    worker_session_id: str
    round2_votes: list[VoteResult]
    consecutive_blocks: int


def build_president_synth(ctx: PresidentSynthCtx) -> str:
    vote_lines = "\n".join(
        [
            f"  Judge {v.judge_session_id[:8]}: {v.verdict.upper()} — {v.reason}"
            for v in ctx.round2_votes
        ]
    )
    block_note = (
        f"\n\nNote: this goal has been blocked {ctx.consecutive_blocks} consecutive times."
        " If judges are deadlocked on the same issue, acknowledge that in your plan"
        " and instruct the worker to try a fundamentally different approach."
        if ctx.consecutive_blocks >= 3
        else ""
    )

    return "\n".join(
        [
            "== PRESIDENT SYNTHESIS REQUEST ==",
            "",
            "Objective:",
            f"  {ctx.objective}",
            "",
            "Judge verdicts (round 2):",
            vote_lines,
            "",
            "Your task: produce ONE consolidated action plan for the worker.",
            "Reconcile conflicting judge feedback into a single clear set of instructions.",
            "Do not hedge or list alternatives — give the worker one path forward.",
            "Be specific: name files, functions, test commands, exact criteria to meet.",
            block_note,
            "",
            "Format your response as a direct instruction to the worker, starting with:",
            "  CONSOLIDATED PLAN:",
            "  <your instructions>",
        ]
    )


def build_worker_continue(
    objective: str,
    consolidated_plan: str,
    consecutive_blocks: int,
) -> str:
    block_note = (
        f"\n\n⚠ This is block #{consecutive_blocks}. "
        "If you keep running into the same wall, try a different approach entirely."
        if consecutive_blocks >= 3
        else ""
    )
    return "\n".join(
        [
            "The council has reviewed your last output and requests changes.",
            "",
            f"Objective: {objective}",
            "",
            "Council's consolidated plan:",
            "---",
            consolidated_plan,
            "---",
            block_note,
            "",
            "Continue working. Do not stop until the objective is verifiably complete.",
        ]
    )


def build_stuck_report(
    objective: str,
    consecutive_blocks: int,
    last_plan: str | None,
) -> str:
    plan_section = f"\nLast council plan:\n{last_plan}\n" if last_plan else ""
    return "\n".join(
        [
            f"⚠ STAGNATION ALERT — {consecutive_blocks} consecutive blocks without progress.",
            "",
            f"Objective: {objective}",
            plan_section,
            "The council keeps rejecting your output on the same grounds.",
            "Do NOT repeat your previous approach.",
            "Stop, re-read the objective, and propose a fundamentally different strategy.",
            "If you are blocked by something external (missing dependency, ambiguous requirement),",
            "state it explicitly so the human can intervene.",
        ]
    )


def build_loop_reprompt(objective: str | None) -> str:
    if objective:
        return (
            f"Continuing toward the goal: {objective}\n\n"
            "Pick up where you left off. Take the next concrete action."
        )
    return "Continuing. Take the next concrete action."


def parse_verdict(text: str) -> tuple[str, str]:
    verdict = "fail"
    reason = text.strip()
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.upper().startswith("VERDICT:"):
            v = stripped.split(":", 1)[1].strip().lower()
            verdict = "pass" if "pass" in v else "fail"
        if stripped.upper().startswith("REASON:"):
            reason = stripped.split(":", 1)[1].strip()
    return verdict, reason


def parse_president_plan(text: str) -> str:
    marker = "CONSOLIDATED PLAN:"
    idx = text.upper().find(marker)
    if idx >= 0:
        return text[idx + len(marker) :].strip()
    return text.strip()
