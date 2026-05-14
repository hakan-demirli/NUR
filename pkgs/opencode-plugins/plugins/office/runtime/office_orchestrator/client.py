from __future__ import annotations

import base64
import json
import logging
import os
import time
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any

from .logging import get_logger, log_kv

_log = get_logger("office.client")


class OpenCodeError(RuntimeError):
    pass


@dataclass(slots=True)
class SessionTokens:
    input: int
    output: int
    reasoning: int
    cache_read: int
    cache_write: int

    @property
    def total(self) -> int:
        return (
            self.input
            + self.output
            + self.reasoning
            + self.cache_read
            + self.cache_write
        )


class OpenCodeClient:
    def __init__(
        self,
        base_url: str,
        *,
        directory: str,
        password: str | None = None,
        username: str | None = None,
        timeout: float = 30.0,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.directory = directory
        self.password = password or os.environ.get("OPENCODE_SERVER_PASSWORD")
        self.username = (
            username or os.environ.get("OPENCODE_SERVER_USERNAME") or "opencode"
        )
        self.timeout = timeout

    def _headers(
        self, extra: dict[str, str] | None = None, *, directory: str | None = None
    ) -> dict[str, str]:
        headers = {
            "x-opencode-directory": directory or self.directory,
        }
        if self.password:
            token = base64.b64encode(
                f"{self.username}:{self.password}".encode()
            ).decode("ascii")
            headers["Authorization"] = f"Basic {token}"
        if extra:
            headers.update(extra)
        return headers

    class _Transient(Exception):
        def __init__(self, original: Exception):
            super().__init__(str(original))
            self.original = original

    _MAX_ATTEMPTS = 3
    _BACKOFF_BASE_MS = 200

    def _request(
        self,
        method: str,
        path: str,
        *,
        query: dict[str, Any] | None = None,
        body: Any | None = None,
        directory_override: str | None = None,
        timeout: float | None = None,
    ) -> Any:
        last_exc: Exception | None = None
        for attempt in range(1, self._MAX_ATTEMPTS + 1):
            try:
                return self._request_once(
                    method,
                    path,
                    query=query,
                    body=body,
                    directory_override=directory_override,
                    timeout=timeout,
                    attempt=attempt,
                )
            except OpenCodeClient._Transient as exc:
                last_exc = exc.original
                if attempt >= self._MAX_ATTEMPTS:
                    break
                import random

                delay_ms = min(self._BACKOFF_BASE_MS * (3 ** (attempt - 1)), 2000)
                jitter_ms = random.randint(0, 50)
                log_kv(
                    _log,
                    logging.WARNING,
                    "opencode http retry",
                    method=method,
                    path=path,
                    attempt=attempt,
                    delay_ms=delay_ms + jitter_ms,
                    error=str(exc.original),
                )
                time.sleep((delay_ms + jitter_ms) / 1000.0)
        assert last_exc is not None
        raise OpenCodeError(
            f"{method} {path} failed after {self._MAX_ATTEMPTS} attempts: {last_exc}"
        ) from last_exc

    def _request_once(
        self,
        method: str,
        path: str,
        *,
        query: dict[str, Any] | None = None,
        body: Any | None = None,
        directory_override: str | None = None,
        timeout: float | None = None,
        attempt: int = 1,
    ) -> Any:
        url = self.base_url + path
        merged_query = dict(query or {})
        if directory_override:
            merged_query["directory"] = directory_override
        if merged_query:
            encoded = urllib.parse.urlencode(
                {k: v for k, v in merged_query.items() if v is not None}, doseq=True
            )
            if encoded:
                url = f"{url}?{encoded}"
        data = None
        headers = self._headers(directory=directory_override)
        if body is not None:
            headers["Content-Type"] = "application/json"
            data = json.dumps(body).encode("utf-8")
        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        body_bytes = len(data) if data is not None else 0
        start = time.monotonic()
        try:
            with urllib.request.urlopen(
                request, timeout=timeout or self.timeout
            ) as response:
                payload = response.read()
                status_code = response.status
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="ignore")
            elapsed_ms = int((time.monotonic() - start) * 1000)
            log_kv(
                _log,
                logging.WARNING,
                "opencode http error",
                method=method,
                path=path,
                status=exc.code,
                ms=elapsed_ms,
                req_bytes=body_bytes,
                detail=detail[:500],
                attempt=attempt,
            )
            err = OpenCodeError(f"HTTP {exc.code} for {method} {path}: {detail}")
            if exc.code >= 500 or exc.code == 429:
                raise OpenCodeClient._Transient(err) from exc
            raise err from exc
        except urllib.error.URLError as exc:
            elapsed_ms = int((time.monotonic() - start) * 1000)
            log_kv(
                _log,
                logging.ERROR,
                "opencode url error",
                method=method,
                path=path,
                ms=elapsed_ms,
                error=str(exc),
                attempt=attempt,
            )
            err = OpenCodeError(f"Request failed for {method} {path}: {exc}")
            raise OpenCodeClient._Transient(err) from exc

        elapsed_ms = int((time.monotonic() - start) * 1000)
        log_kv(
            _log,
            logging.DEBUG,
            "opencode http",
            method=method,
            path=path,
            status=status_code,
            ms=elapsed_ms,
            req_bytes=body_bytes,
            resp_bytes=len(payload) if payload else 0,
        )
        if not payload:
            return None
        try:
            return json.loads(payload.decode("utf-8"))
        except json.JSONDecodeError as exc:
            log_kv(
                _log,
                logging.ERROR,
                "opencode response json invalid",
                method=method,
                path=path,
                preview=payload[:200].decode("utf-8", errors="replace"),
            )
            raise OpenCodeError(f"Invalid JSON from {method} {path}") from exc

    def list_sessions(self) -> list[dict[str, Any]]:
        return self._request("GET", "/session")

    def create_session(self) -> dict[str, Any]:
        return self._request("POST", "/session")

    def get_session(self, session_id: str) -> dict[str, Any]:
        return self._request("GET", f"/session/{session_id}")

    def fork_session(
        self, session_id: str, message_id: str | None = None
    ) -> dict[str, Any]:
        body: dict[str, Any] = {}
        if message_id:
            body["messageID"] = message_id
        session_dir = self.session_directory(session_id)
        return self._request(
            "POST",
            f"/session/{session_id}/fork",
            body=body,
            directory_override=session_dir,
        )

    def session_directory(self, session_id: str) -> str:
        try:
            info = self.get_session(session_id)
            value = info.get("directory") if isinstance(info, dict) else None
            if isinstance(value, str) and value:
                return value
        except OpenCodeError:
            pass
        return self.directory

    def abort_session(self, session_id: str) -> bool:
        return bool(self._request("POST", f"/session/{session_id}/abort"))

    def session_status_map(self) -> dict[str, dict[str, Any]]:
        return self._request("GET", "/session/status")

    def session_status(self, session_id: str) -> dict[str, Any] | None:
        return self.session_status_map().get(session_id)

    def list_messages(
        self, session_id: str, *, limit: int | None = None, before: str | None = None
    ) -> list[dict[str, Any]]:
        return self._request(
            "GET",
            f"/session/{session_id}/message",
            query={"limit": limit, "before": before},
        )

    def get_message(self, session_id: str, message_id: str) -> dict[str, Any]:
        return self._request("GET", f"/session/{session_id}/message/{message_id}")

    def prompt_async(
        self,
        session_id: str,
        text: str,
        *,
        agent: str | None = None,
        model: dict[str, str] | None = None,
        system: str | None = None,
        no_reply: bool = False,
    ) -> None:
        body: dict[str, Any] = {
            "parts": [{"type": "text", "text": text}],
            "noReply": no_reply,
        }
        if agent:
            body["agent"] = agent
        if model:
            body["model"] = model
        if system:
            body["system"] = system
        self._request("POST", f"/session/{session_id}/prompt_async", body=body)

    def send_command(
        self,
        session_id: str,
        command: str,
        arguments: str = "",
        *,
        agent: str | None = None,
    ) -> dict[str, Any]:
        body: dict[str, Any] = {"command": command, "arguments": arguments}
        if agent:
            body["agent"] = agent
        return self._request("POST", f"/session/{session_id}/command", body=body)

    def summarize(
        self, session_id: str, *, auto: bool = False, timeout: float = 300.0
    ) -> bool:
        model = self.session_model(session_id)
        if not model:
            raise OpenCodeError(
                f"Cannot summarize {session_id}: no model found in session messages"
            )
        body: dict[str, Any] = {
            "providerID": model["providerID"],
            "modelID": model["modelID"],
            "auto": auto,
        }
        return bool(
            self._request(
                "POST",
                f"/session/{session_id}/summarize",
                body=body,
                directory_override=self.session_directory(session_id),
                timeout=timeout,
            )
        )

    def run_shell(
        self,
        session_id: str,
        command: str,
        *,
        agent: str = "build",
        model: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        body: dict[str, Any] = {"command": command, "agent": agent}
        if model:
            body["model"] = model
        return self._request("POST", f"/session/{session_id}/shell", body=body)

    def diff(
        self, session_id: str, message_id: str | None = None
    ) -> list[dict[str, Any]]:
        return self._request(
            "GET", f"/session/{session_id}/diff", query={"messageID": message_id}
        )

    def list_providers(self) -> dict[str, Any]:
        return self._request("GET", "/provider")

    def last_messages(self, session_id: str, count: int) -> list[dict[str, Any]]:
        return self.list_messages(session_id, limit=count)

    def last_message(self, session_id: str) -> dict[str, Any] | None:
        messages = self.last_messages(session_id, 1)
        return messages[0] if messages else None

    def latest_assistant_message(
        self, session_id: str, *, search_limit: int = 20
    ) -> dict[str, Any] | None:
        for message in reversed(self.list_messages(session_id, limit=search_limit)):
            if message.get("info", {}).get("role") == "assistant":
                return message
        return None

    def session_model(self, session_id: str) -> dict[str, str] | None:
        for message in reversed(self.list_messages(session_id, limit=40)):
            info = message.get("info") or {}
            provider_id = info.get("providerID")
            model_id = info.get("modelID")
            if provider_id and model_id:
                return {"providerID": provider_id, "modelID": model_id}
            model = info.get("model") or {}
            provider_id = model.get("providerID")
            model_id = model.get("modelID")
            if provider_id and model_id:
                return {"providerID": provider_id, "modelID": model_id}
        return None

    def latest_tokens(self, session_id: str) -> SessionTokens | None:
        message = self.latest_assistant_message(session_id)
        if not message:
            return None
        tokens = message["info"].get("tokens") or {}
        cache = tokens.get("cache") or {}
        return SessionTokens(
            input=int(tokens.get("input") or 0),
            output=int(tokens.get("output") or 0),
            reasoning=int(tokens.get("reasoning") or 0),
            cache_read=int(cache.get("read") or 0),
            cache_write=int(cache.get("write") or 0),
        )

    def context_limit(self, session_id: str) -> int | None:
        model = self.session_model(session_id)
        if not model:
            return None
        provider_list = self.list_providers()
        for provider in provider_list.get("all", []):
            if provider.get("id") != model["providerID"]:
                continue
            model_info = (provider.get("models") or {}).get(model["modelID"])
            if model_info:
                return int(model_info["limit"]["context"])
        return None

    def summary(self, session_id: str) -> dict[str, Any]:
        status = self.session_status(session_id)
        model = self.session_model(session_id)
        tokens = self.latest_tokens(session_id)
        context = self.context_limit(session_id)
        return {
            "session_id": session_id,
            "status": status,
            "model": model,
            "context_limit": context,
            "tokens": None
            if not tokens
            else {
                "input": tokens.input,
                "output": tokens.output,
                "reasoning": tokens.reasoning,
                "cache_read": tokens.cache_read,
                "cache_write": tokens.cache_write,
                "total": tokens.total,
            },
        }
