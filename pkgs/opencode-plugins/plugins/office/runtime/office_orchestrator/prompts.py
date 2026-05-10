"""Prompt builders for the office orchestrator.

Pure functions only. No I/O, no logging, no `self`. The orchestrator
gathers facts (session ids, summaries, compaction decisions) and hands
them to a builder; the builder returns a string. This keeps prompt
wording in one obvious place so it can be tuned without touching the
state machine.

Conventions:
- One ``@dataclass(frozen=True, slots=True)`` per prompt whose input set
  is non-trivial. Trivial prompts can take plain kwargs.
- Builders return ``str``. They never mutate inputs.
- No template engine. Just f-strings and ``"\\n".join`` against a list.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True, slots=True)
class JudgeBootstrapContext:
    worker_session_id: str
    judge_session_id: str
    socket_path: str


def build_judge_bootstrap(ctx: JudgeBootstrapContext) -> str:
    sock = ctx.socket_path
    return "\n".join(
        [
            f"You are the judge for worker session {ctx.worker_session_id}.",
            f"You are session {ctx.judge_session_id}.",
            "The office daemon is the bridge between you, the worker, and the human. Do not invent ad hoc control paths.",
            "",
            "Daemon control plane:",
            f"  Unix socket: {sock}",
            "  Protocol:    plain HTTP/JSON over the Unix socket",
            "",
            "Use ordinary `curl --unix-socket` calls. Do not use any Python wrapper.",
            "",
            "Read endpoints:",
            f"  curl -s --unix-socket {sock} http://localhost/health",
            f"  curl -s --unix-socket {sock} http://localhost/status",
            f"  curl -s --unix-socket {sock} http://localhost/paths",
            f"  curl -s --unix-socket {sock} 'http://localhost/worker/messages?limit=5'",
            f"  curl -s --unix-socket {sock} 'http://localhost/judge/messages?limit=5'",
            f"  curl -s --unix-socket {sock} 'http://localhost/logs?lines=200'",
            "",
            "Write endpoints (all POST, JSON body):",
            f"  curl -s --unix-socket {sock} -X POST http://localhost/worker/prompt \\\n"
            "    -H 'Content-Type: application/json' \\\n"
            "    -d '{\"message\":\"<your concrete instruction>\"}'",
            f"  curl -s --unix-socket {sock} -X POST http://localhost/worker/prompt \\\n"
            "    -H 'Content-Type: application/json' \\\n"
            "    -d '{\"message\":\"<your concrete instruction>\",\"compact_first\":true}'",
            f"  curl -s --unix-socket {sock} -X POST http://localhost/worker/compact \\\n"
            "    -H 'Content-Type: application/json' -d '{}'",
            f"  curl -s --unix-socket {sock} -X POST http://localhost/judge/compact \\\n"
            "    -H 'Content-Type: application/json' -d '{}'",
            "",
            "Acceptance policy:",
            "- Reject scaffolding, placeholders, TODOs, vague next-step language, and unverified claims.",
            "- Require concrete repo evidence and command-output evidence before accepting completion.",
            "- If tests, fuzzing, coverage, or diff-testing were requested, acceptance requires evidence they were actually executed, unless a precise external blocker is proven from the repo.",
            "- If the worker is bloated, compact it before reprompting. If you are bloated, run `/compact` on yourself.",
            "- Do not ask the human for permission to keep the worker going; just call /worker/prompt.",
        ]
    )


@dataclass(frozen=True, slots=True)
class JudgeNudgeContext:
    worker_session_id: str
    judge_session_id: str
    socket_path: str
    reason: str
    latest_message_id: str | None
    worker_summary: dict[str, Any]
    judge_summary: dict[str, Any]
    worker_compact_recommended: bool
    worker_compact_reason: str | None
    judge_was_compacted: bool
    judge_compact_reason: str | None


def build_judge_nudge(ctx: JudgeNudgeContext) -> str:
    sock = ctx.socket_path
    lines = [
        f"Worker session {ctx.worker_session_id} needs review.",
        f"Trigger: {ctx.reason}",
        f"Latest worker message ID: {ctx.latest_message_id or 'unknown'}",
        f"Worker summary: {json.dumps(ctx.worker_summary, sort_keys=True)}",
        f"Judge summary: {json.dumps(ctx.judge_summary, sort_keys=True)}",
        "Inspect the worker transcript and repository now.",
        "Reject scaffolding, placeholders, TODOs, or unexecuted verification.",
        "Use the office daemon over its Unix socket; do not invent ad hoc control paths.",
        f"Daemon socket: {sock}",
        "To reprompt the worker, run:",
        f"  curl -s --unix-socket {sock} -X POST http://localhost/worker/prompt \\\n"
        "    -H 'Content-Type: application/json' \\\n"
        "    -d '{\"message\":\"<concrete next instruction>\"}'",
        "If you need to compact first, add \"compact_first\":true to the body.",
    ]
    if ctx.worker_compact_recommended and ctx.worker_compact_reason:
        lines.append(
            f"Worker compaction is recommended before reprompting: {ctx.worker_compact_reason}."
        )
    if ctx.judge_was_compacted and ctx.judge_compact_reason:
        lines.append(
            f"You were compacted before this review: {ctx.judge_compact_reason}."
        )
    return "\n".join(lines)
