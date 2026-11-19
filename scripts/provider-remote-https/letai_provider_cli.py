#!/usr/bin/env python3

import json
import os
import hashlib
import socket
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from decimal import Decimal, InvalidOperation, ROUND_DOWN
from typing import Optional


DEFAULT_BASE_URL = "https://api.letai.run/v1"
DEFAULT_TIMEOUT_MS = 15000
DEFAULT_MAX_OUTPUT_TOKENS = 256
DEFAULT_TEMPERATURE = 0.0
DEFAULT_USER_AGENT = "oasis7-letai-provider-cli/1.0"
QUOTA_UNITS_PER_USD = Decimal("500000")
DEFAULT_COMPLETION_RETRY_COUNT = 2
DEFAULT_COMPLETION_RETRY_DELAY_MS = 1000
SSE_DIAGNOSTIC_SAMPLE_LIMIT = 5
SSE_DIAGNOSTIC_TEXT_SAMPLE_LIMIT = 80


class CompletionDecodeError(RuntimeError):
    def __init__(self, message: str, diagnostics: dict):
        super().__init__(message)
        self.diagnostics = diagnostics


def env_optional(*names: str) -> str:
    for name in names:
        value = os.environ.get(name, "")
        if value.strip():
            return value.strip()
    return ""


def env_int(default: int, *names: str) -> int:
    raw = env_optional(*names)
    if not raw:
        return default
    try:
        return int(raw)
    except ValueError as exc:
        raise RuntimeError(f"invalid integer for {' or '.join(names)}: {raw}") from exc


def env_float(default: float, *names: str) -> float:
    raw = env_optional(*names)
    if not raw:
        return default
    try:
        return float(raw)
    except ValueError as exc:
        raise RuntimeError(f"invalid float for {' or '.join(names)}: {raw}") from exc


def env_bool(default: bool, *names: str) -> bool:
    raw = env_optional(*names)
    if not raw:
        return default
    return raw.lower() in {"1", "true", "yes", "on"}


def log_event(event: str, **fields) -> None:
    payload = {"event": event}
    payload.update(fields)
    print(json.dumps(payload, ensure_ascii=True, sort_keys=True), file=sys.stderr)


def safe_url_summary(url: str) -> dict:
    parsed = urllib.parse.urlparse(url)
    return {
        "scheme": parsed.scheme,
        "host": parsed.hostname or "",
        "port": parsed.port,
        "path": parsed.path,
    }


def response_header_summary(headers) -> dict:
    summary = {}
    for key in (
        "content-type",
        "x-request-id",
        "x-trace-id",
        "cf-ray",
        "server",
    ):
        value = headers.get(key) or headers.get(key.title())
        if value:
            summary[key] = str(value)[:120]
    return summary


def sample_text(value: str) -> str:
    if len(value) <= SSE_DIAGNOSTIC_TEXT_SAMPLE_LIMIT:
        return value
    return value[:SSE_DIAGNOSTIC_TEXT_SAMPLE_LIMIT] + "..."


def text_digest(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8", errors="replace")).hexdigest()[:16]


def summarize_sse_choice(choice: dict) -> dict:
    summary = {
        "keys": sorted(str(key) for key in choice.keys()),
        "finish_reason": choice.get("finish_reason"),
    }
    delta = choice.get("delta")
    if isinstance(delta, dict):
        delta_summary = {
            "keys": sorted(str(key) for key in delta.keys()),
            "content_present": isinstance(delta.get("content"), str),
            "content_len": len(delta.get("content") or "")
            if isinstance(delta.get("content"), str)
            else None,
        }
        if isinstance(delta.get("role"), str):
            delta_summary["role"] = delta["role"]
        summary["delta"] = delta_summary
    message = choice.get("message")
    if isinstance(message, dict):
        content = message.get("content")
        summary["message"] = {
            "keys": sorted(str(key) for key in message.keys()),
            "content_present": isinstance(content, str),
            "content_len": len(content) if isinstance(content, str) else None,
        }
    return summary


def format_decode_error(message: str, diagnostics: dict) -> str:
    return (
        message
        + "; diagnostics="
        + json.dumps(diagnostics, ensure_ascii=True, sort_keys=True)
    )


def normalize_base_url(raw: str) -> str:
    base = raw.strip().rstrip("/")
    for suffix in ("/chat/completions", "/responses"):
        if base.endswith(suffix):
            base = base[: -len(suffix)]
    return base


def load_route_config() -> dict:
    route_label = env_optional("OASIS7_REMOTE_LLM_ROUTE_LABEL")
    routes_path = env_optional("OASIS7_REMOTE_LLM_ROUTES_PATH")
    if not routes_path:
        if (
            route_label
            and not env_optional("OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH")
            and not env_optional("OASIS7_REMOTE_LLM_API_KEY", "LETAI_API_KEY")
        ):
            raise RuntimeError(
                "OASIS7_REMOTE_LLM_ROUTE_LABEL requires either "
                "OASIS7_REMOTE_LLM_ROUTES_PATH or OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH"
            )
        return load_newapi_bridge_state_route(route_label)
    try:
        with open(routes_path, "r", encoding="utf-8") as handle:
            payload = json.load(handle)
    except OSError as exc:
        raise RuntimeError(f"failed to read OASIS7_REMOTE_LLM_ROUTES_PATH: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise RuntimeError(
            f"OASIS7_REMOTE_LLM_ROUTES_PATH must contain valid JSON: {exc}"
        ) from exc
    if not isinstance(payload, dict):
        raise RuntimeError("OASIS7_REMOTE_LLM_ROUTES_PATH root must be a JSON object")
    lookup_label = route_label or "default"
    route = payload.get(lookup_label)
    if route is None:
        raise RuntimeError(
            f"route config `{lookup_label}` was not found in OASIS7_REMOTE_LLM_ROUTES_PATH"
        )
    if not isinstance(route, dict):
        raise RuntimeError(f"route config `{lookup_label}` must be a JSON object")
    return route


def load_newapi_bridge_state_route(route_label: str) -> dict:
    state_path = env_optional("OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH")
    if not state_path or not route_label:
        return {}
    try:
        with open(state_path, "r", encoding="utf-8") as handle:
            payload = json.load(handle)
    except OSError as exc:
        raise RuntimeError(
            f"failed to read OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH: {exc}"
        ) from exc
    except json.JSONDecodeError as exc:
        raise RuntimeError(
            "OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH must contain valid JSON: "
            f"{exc}"
        ) from exc
    if not isinstance(payload, dict):
        raise RuntimeError(
            "OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH root must be a JSON object"
        )
    bindings = payload.get("bindings")
    project_bindings = payload.get("project_bindings")
    if not isinstance(bindings, list) or not isinstance(project_bindings, list):
        raise RuntimeError(
            "newapi bridge state must contain `bindings` and `project_bindings` arrays"
        )
    binding = resolve_newapi_binding(bindings, route_label)
    if binding is None:
        raise RuntimeError(
            "no active newapi bridge binding found for "
            f"OASIS7_REMOTE_LLM_ROUTE_LABEL={route_label}"
        )
    bridge_user_id = str(binding.get("bridge_user_id") or "").strip()
    if not bridge_user_id:
        raise RuntimeError(
            f"newapi bridge binding for {route_label} is missing bridge_user_id"
        )
    token_key = resolve_newapi_token_key(project_bindings, bridge_user_id)
    if not token_key:
        raise RuntimeError(
            f"newapi bridge binding for {route_label} does not have a usable token_key"
        )
    return {"api_key": token_key}


def resolve_newapi_binding(bindings: list, route_label: str) -> Optional[dict]:
    normalized_label = route_label.strip()
    by_ref = normalized_label
    by_bridge_user_id = normalized_label
    if ":" in normalized_label:
        prefix, value = normalized_label.split(":", 1)
        value = value.strip()
        if prefix == "newapi_user_ref" and value:
            by_bridge_user_id = ""
            by_ref = value
        elif prefix == "bridge_user_id" and value:
            by_ref = ""
            by_bridge_user_id = value
        else:
            return None
    for entry in bindings:
        if not isinstance(entry, dict):
            continue
        if str(entry.get("status") or "").strip() != "active":
            continue
        if by_ref and str(entry.get("newapi_user_ref") or "").strip() == by_ref:
            return entry
        if by_bridge_user_id and str(entry.get("bridge_user_id") or "").strip() == by_bridge_user_id:
            return entry
    return None


def resolve_newapi_token_key(project_bindings: list, bridge_user_id: str) -> str:
    selected = None
    for entry in project_bindings:
        if not isinstance(entry, dict):
            continue
        if str(entry.get("bridge_user_id") or "").strip() != bridge_user_id:
            continue
        token_key = str(entry.get("token_key") or "").strip()
        if not token_key:
            continue
        selected = token_key
    return selected or ""


def route_or_env(route: dict, route_key: str, *env_names: str, default: str = "") -> str:
    value = route.get(route_key)
    if isinstance(value, str) and value.strip():
        return value.strip()
    return env_optional(*env_names) or default


def route_or_env_int(route: dict, route_key: str, default: int, *env_names: str) -> int:
    value = route.get(route_key)
    if value is not None:
        try:
            return int(value)
        except (TypeError, ValueError) as exc:
            raise RuntimeError(f"invalid integer for route field {route_key}: {value}") from exc
    return env_int(default, *env_names)


def route_or_env_float(route: dict, route_key: str, default: float, *env_names: str) -> float:
    value = route.get(route_key)
    if value is not None:
        try:
            return float(value)
        except (TypeError, ValueError) as exc:
            raise RuntimeError(f"invalid float for route field {route_key}: {value}") from exc
    return env_float(default, *env_names)


def route_or_env_bool(route: dict, route_key: str, default: bool, *env_names: str) -> bool:
    value = route.get(route_key)
    if isinstance(value, bool):
        return value
    if value is not None:
        return str(value).lower() in {"1", "true", "yes", "on"}
    return env_bool(default, *env_names)


def parse_gateway_call(argv: list[str]) -> tuple[str, int, str]:
    params = ""
    timeout_ms = DEFAULT_TIMEOUT_MS
    agent_id = "letai"
    index = 3
    while index < len(argv):
        flag = argv[index]
        if flag == "--params":
            index += 1
            if index >= len(argv):
                raise RuntimeError("--params requires a value")
            params = argv[index]
        elif flag == "--timeout":
            index += 1
            if index >= len(argv):
                raise RuntimeError("--timeout requires a value")
            # `gateway call agent --timeout` is already passed in milliseconds.
            timeout_ms = int(argv[index])
        elif flag == "--expect-final":
            # `oasis7_provider_local_bridge` includes this gateway CLI flag when
            # it asks provider-style CLIs for a final answer. LetAI chat
            # completions already returns a single final payload, so no extra
            # handling is needed here.
            pass
        elif flag == "--json":
            # The bridge requests JSON output from provider CLIs. This wrapper
            # always writes the gateway-compatible JSON envelope.
            pass
        else:
            raise RuntimeError(f"unknown gateway flag: {flag}")
        index += 1
    if not params:
        raise RuntimeError("gateway call requires --params")
    payload = json.loads(params)
    prompt = str(payload.get("message", "")).strip()
    if not prompt:
        raise RuntimeError("gateway params missing message")
    agent_id = str(payload.get("agentId", agent_id)).strip() or agent_id
    return prompt, max(timeout_ms, 1000), agent_id


def parse_local_agent(argv: list[str]) -> tuple[str, int, str]:
    prompt = ""
    timeout_ms = DEFAULT_TIMEOUT_MS
    agent_id = "letai"
    index = 1
    while index < len(argv):
        flag = argv[index]
        if flag == "--message":
            index += 1
            if index >= len(argv):
                raise RuntimeError("--message requires a value")
            prompt = argv[index]
        elif flag == "--timeout":
            index += 1
            if index >= len(argv):
                raise RuntimeError("--timeout requires a value")
            # `gateway call agent --timeout` is already passed in milliseconds.
            timeout_ms = int(argv[index])
        elif flag == "--agent":
            index += 1
            if index >= len(argv):
                raise RuntimeError("--agent requires a value")
            agent_id = argv[index].strip() or agent_id
        elif flag in {"--session-id", "--thinking"}:
            index += 1
            if index >= len(argv):
                raise RuntimeError(f"{flag} requires a value")
        elif flag in {"--local", "--json"}:
            pass
        else:
            raise RuntimeError(f"unknown agent flag: {flag}")
        index += 1
    if not prompt.strip():
        raise RuntimeError("agent invocation missing --message")
    return prompt.strip(), max(timeout_ms, 1000), agent_id


def content_from_choice(choice: dict) -> str:
    message = choice.get("message") or {}
    content = message.get("content")
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, list):
        text_parts: list[str] = []
        for item in content:
            if isinstance(item, dict):
                if isinstance(item.get("text"), str):
                    text_parts.append(item["text"])
                elif item.get("type") == "text":
                    text = item.get("content") or item.get("value")
                    if isinstance(text, str):
                        text_parts.append(text)
        return "".join(text_parts).strip()
    return ""


def make_headers(api_key: str) -> dict[str, str]:
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
        "User-Agent": env_optional("OASIS7_REMOTE_LLM_USER_AGENT") or DEFAULT_USER_AGENT,
    }
    extra_headers_json = env_optional(
        "OASIS7_REMOTE_LLM_EXTRA_HEADERS_JSON", "LETAI_EXTRA_HEADERS_JSON"
    )
    if extra_headers_json:
        try:
            extra_headers = json.loads(extra_headers_json)
        except json.JSONDecodeError as exc:
            raise RuntimeError(
                "OASIS7_REMOTE_LLM_EXTRA_HEADERS_JSON must be valid JSON"
            ) from exc
        if not isinstance(extra_headers, dict):
            raise RuntimeError("OASIS7_REMOTE_LLM_EXTRA_HEADERS_JSON must be a JSON object")
        for key, value in extra_headers.items():
            headers[str(key)] = str(value)
    return headers


def quota_from_usd(raw: str) -> int:
    try:
        amount = Decimal(raw.strip())
    except (InvalidOperation, AttributeError) as exc:
        raise RuntimeError(f"invalid auto topup USD amount: {raw}") from exc
    if amount <= 0:
        return 0
    quota = (amount * QUOTA_UNITS_PER_USD).quantize(Decimal("1"), rounding=ROUND_DOWN)
    return int(quota)


def should_auto_topup(error_detail: str) -> bool:
    lowered = error_detail.lower()
    return "insufficient_user_quota" in lowered or "余额" in error_detail


def maybe_auto_topup_user(error_detail: str) -> bool:
    topup_usd = env_optional("OASIS7_REMOTE_LLM_AUTO_TOPUP_USD", "LETAI_AUTO_TOPUP_USD")
    if not topup_usd or not should_auto_topup(error_detail):
        return False
    quota = quota_from_usd(topup_usd)
    if quota <= 0:
        return False
    platform_key = env_optional(
        "OASIS7_REMOTE_LLM_PLATFORM_KEY",
        "OASIS7_NEWAPI_BRIDGE_LETAI_PLATFORM_KEY",
        "LETAI_PLATFORM_KEY",
    )
    platform_user_id = env_optional(
        "OASIS7_REMOTE_LLM_PLATFORM_USER_ID",
        "LETAI_PLATFORM_USER_ID",
    )
    if not platform_key or not platform_user_id:
        return False
    base_url = (
        env_optional(
            "OASIS7_REMOTE_LLM_PLATFORM_BASE_URL",
            "OASIS7_NEWAPI_BRIDGE_LETAI_BASE_URL",
            "LETAI_PLATFORM_BASE_URL",
        )
        or "https://api.letai.run"
    ).strip().rstrip("/")
    external_order_id = f"oasis7-local-auto-topup-{int(time.time())}"
    payload = {
        "external_order_id": external_order_id,
        "quota": quota,
        "amount": str(Decimal(topup_usd.strip()).normalize()),
        "currency": "USD",
    }
    request = urllib.request.Request(
        url=f"{base_url}/api/platform/open/users/{platform_user_id}/topups",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {platform_key}",
            "Content-Type": "application/json",
            "User-Agent": env_optional("OASIS7_REMOTE_LLM_USER_AGENT") or DEFAULT_USER_AGENT,
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            body = response.read().decode("utf-8", errors="replace")
            status_code = response.status
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(
            f"auto topup returned HTTP {exc.code}: {redact_detail(detail)}"
        ) from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"auto topup request failed: {exc}") from exc
    if status_code < 200 or status_code >= 300:
        raise RuntimeError(f"auto topup returned unexpected HTTP {status_code}")
    try:
        decoded = json.loads(body) if body.strip() else {}
    except json.JSONDecodeError as exc:
        raise RuntimeError("auto topup returned non-JSON response") from exc
    if isinstance(decoded, dict) and decoded.get("success") is False:
        raise RuntimeError(
            "auto topup returned success=false: "
            + str(decoded.get("message") or decoded.get("error") or "unknown error")
        )
    print(
        json.dumps(
            {
                "event": "auto_topup",
                "quota": quota,
                "amount_usd": str(topup_usd),
                "external_order_id": external_order_id,
            },
            ensure_ascii=True,
        ),
        file=sys.stderr,
    )
    return True


def send_completion_request_after_topup(
    body: dict,
    base_url: str,
    api_key: str,
    timeout_ms: int,
    use_stream: bool,
) -> tuple[int, object, str, dict]:
    retry_count_raw = (
        env_optional("OASIS7_REMOTE_LLM_AUTO_TOPUP_RETRY_COUNT", "LETAI_AUTO_TOPUP_RETRY_COUNT")
        or "3"
    )
    retry_delay_raw = (
        env_optional("OASIS7_REMOTE_LLM_AUTO_TOPUP_RETRY_DELAY_MS", "LETAI_AUTO_TOPUP_RETRY_DELAY_MS")
        or "1000"
    )
    try:
        retry_count = max(1, int(retry_count_raw))
    except ValueError as exc:
        raise RuntimeError(
            f"invalid auto topup retry count: {retry_count_raw}"
        ) from exc
    try:
        retry_delay_ms = max(0, int(retry_delay_raw))
    except ValueError as exc:
        raise RuntimeError(
            f"invalid auto topup retry delay ms: {retry_delay_raw}"
        ) from exc

    last_detail = ""
    for attempt in range(1, retry_count + 1):
        if attempt > 1 and retry_delay_ms > 0:
            time.sleep(retry_delay_ms / 1000)
        retry_request = urllib.request.Request(
            url=f"{base_url}/chat/completions",
            data=json.dumps(body).encode("utf-8"),
            headers=make_headers(api_key),
            method="POST",
        )
        try:
            return send_completion_request(
                retry_request,
                timeout_ms,
                use_stream,
            )
        except urllib.error.HTTPError as exc:
            last_detail = exc.read().decode("utf-8", errors="replace")
            if not should_auto_topup(last_detail) or attempt >= retry_count:
                raise RuntimeError(
                    f"upstream chat completion returned HTTP {exc.code}: {last_detail}"
                ) from exc
            print(
                json.dumps(
                    {
                        "event": "auto_topup_retry_wait",
                        "attempt": attempt + 1,
                        "retry_count": retry_count,
                        "delay_ms": retry_delay_ms,
                    },
                    ensure_ascii=True,
                ),
                file=sys.stderr,
            )
    raise RuntimeError(f"upstream chat completion still low quota after topup: {last_detail}")


def is_retryable_completion_error(exc: BaseException) -> bool:
    detail = str(exc).lower()
    return (
        "did not contain assistant content" in detail
        or "read operation timed out" in detail
        or "operation timed out" in detail
        or "remote end closed connection without response" in detail
        or "connection reset" in detail
        or isinstance(exc, (TimeoutError, socket.timeout))
    )


def send_completion_request_with_retries(
    body: dict,
    base_url: str,
    api_key: str,
    timeout_ms: int,
    use_stream: bool,
) -> tuple[int, dict, str, dict]:
    retry_count = route_or_env_int(
        {},
        "unused",
        DEFAULT_COMPLETION_RETRY_COUNT,
        "OASIS7_REMOTE_LLM_RETRY_COUNT",
        "LETAI_RETRY_COUNT",
    )
    retry_delay_ms = route_or_env_int(
        {},
        "unused",
        DEFAULT_COMPLETION_RETRY_DELAY_MS,
        "OASIS7_REMOTE_LLM_RETRY_DELAY_MS",
        "LETAI_RETRY_DELAY_MS",
    )
    retry_count = max(1, retry_count)
    retry_delay_ms = max(0, retry_delay_ms)
    for attempt in range(1, retry_count + 1):
        request = urllib.request.Request(
            url=f"{base_url}/chat/completions",
            data=json.dumps(body).encode("utf-8"),
            headers=make_headers(api_key),
            method="POST",
        )
        try:
            return send_completion_request(request, timeout_ms, use_stream)
        except urllib.error.HTTPError:
            raise
        except Exception as exc:
            if attempt >= retry_count or not is_retryable_completion_error(exc):
                raise
            retry_payload = {
                "attempt": attempt + 1,
                "retry_count": retry_count,
                "reason": redact_detail(str(exc)),
            }
            if isinstance(exc, CompletionDecodeError):
                retry_payload["diagnostics"] = exc.diagnostics
            print(
                json.dumps({"event": "chat_completion_retry", **retry_payload}, ensure_ascii=True),
                file=sys.stderr,
            )
            if retry_delay_ms > 0:
                time.sleep(retry_delay_ms / 1000)
    raise RuntimeError("unreachable chat completion retry state")


def redact_detail(detail: str) -> str:
    stripped = detail.strip()
    if len(stripped) > 300:
        return stripped[:300] + "..."
    return stripped


def request_completion(prompt: str, timeout_ms: int, agent_id: str) -> dict:
    route = load_route_config()
    base_url = normalize_base_url(
        route_or_env(route, "base_url", "OASIS7_REMOTE_LLM_BASE_URL", "LETAI_BASE_URL", default=DEFAULT_BASE_URL)
    )
    api_key = route_or_env(route, "api_key", "OASIS7_REMOTE_LLM_API_KEY", "LETAI_API_KEY")
    if not api_key:
        raise RuntimeError("missing required remote LLM api key")
    model = route_or_env(route, "model", "OASIS7_REMOTE_LLM_MODEL", "LETAI_MODEL")
    if not model:
        raise RuntimeError("missing required remote LLM model")
    system_prompt = route_or_env(
        route,
        "system_prompt",
        "OASIS7_REMOTE_LLM_SYSTEM_PROMPT", "LETAI_SYSTEM_PROMPT"
    )
    max_output_tokens = route_or_env_int(
        route,
        "max_output_tokens",
        DEFAULT_MAX_OUTPUT_TOKENS,
        "OASIS7_REMOTE_LLM_MAX_OUTPUT_TOKENS",
        "LETAI_MAX_OUTPUT_TOKENS",
    )
    temperature = route_or_env_float(
        route,
        "temperature",
        DEFAULT_TEMPERATURE,
        "OASIS7_REMOTE_LLM_TEMPERATURE",
        "LETAI_TEMPERATURE",
    )
    use_json_object = route_or_env_bool(
        route,
        "response_format_json_object",
        False,
        "OASIS7_REMOTE_LLM_RESPONSE_FORMAT_JSON_OBJECT",
        "LETAI_RESPONSE_FORMAT_JSON_OBJECT",
    )
    use_stream = route_or_env_bool(
        route,
        "stream",
        False,
        "OASIS7_REMOTE_LLM_STREAM",
        "LETAI_STREAM",
    )

    messages = []
    if system_prompt:
        messages.append({"role": "system", "content": system_prompt})
    messages.append({"role": "user", "content": prompt})

    body = {
        "model": model,
        "messages": messages,
        "temperature": temperature,
        "stream": use_stream,
        "max_tokens": max_output_tokens,
        "user": f"oasis7-provider:{agent_id}",
    }
    if use_json_object:
        body["response_format"] = {"type": "json_object"}

    started = time.time()
    log_event(
        "chat_completion_request",
        target=safe_url_summary(f"{base_url}/chat/completions"),
        model=model,
        stream=use_stream,
        timeout_ms=timeout_ms,
        max_output_tokens=max_output_tokens,
        temperature=temperature,
        response_format_json_object=use_json_object,
        prompt_len=len(prompt),
        agent_id=agent_id,
    )
    try:
        status_code, decoded, content, usage = send_completion_request_with_retries(
            body, base_url, api_key, timeout_ms, use_stream
        )
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        if maybe_auto_topup_user(detail):
            status_code, decoded, content, usage = send_completion_request_after_topup(
                body,
                base_url,
                api_key,
                timeout_ms,
                use_stream,
            )
        else:
            raise RuntimeError(
                f"upstream chat completion returned HTTP {exc.code}: {detail}"
            ) from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"upstream chat completion request failed: {exc}") from exc

    if status_code < 200 or status_code >= 300:
        raise RuntimeError(f"upstream chat completion returned unexpected HTTP {status_code}")
    duration_ms = int((time.time() - started) * 1000)
    return {
        "payloads": [{"text": content}],
        "meta": {
            "durationMs": duration_ms,
            "agentMeta": {
                "provider": "letai",
                "model": str(decoded.get("model") or model),
                "promptTokens": usage.get("prompt_tokens"),
                "usage": {
                    "output": usage.get("completion_tokens"),
                    "total": usage.get("total_tokens"),
                },
            },
        },
    }


def send_completion_request(
    request: urllib.request.Request,
    timeout_ms: int,
    use_stream: bool,
) -> tuple[int, dict, str, dict]:
    with urllib.request.urlopen(request, timeout=max(timeout_ms, 1000) / 1000.0) as response:
        status_code = response.status
        headers = response.headers
        if use_stream:
            decoded, content, usage = decode_sse_completion_stream(
                response,
                status_code=status_code,
                headers=headers,
            )
        else:
            payload = response.read().decode("utf-8")
            decoded, content, usage = decode_completion_payload(
                payload,
                status_code=status_code,
                headers=headers,
            )
    return status_code, decoded, content, usage


def decode_completion_payload(
    payload: str,
    status_code: Optional[int] = None,
    headers=None,
) -> tuple[dict, str, dict]:
    stripped = payload.strip()
    if not stripped:
        diagnostics = {
            "status_code": status_code,
            "headers": response_header_summary(headers or {}),
            "body_len": len(payload),
        }
        raise CompletionDecodeError(
            format_decode_error("upstream response body was empty", diagnostics),
            diagnostics,
        )
    if any(
        line.strip().startswith("data:")
        for line in payload.splitlines()
        if line.strip()
    ):
        return decode_sse_completion_payload(
            stripped,
            status_code=status_code,
            headers=headers,
        )
    decoded = json.loads(payload)
    choices = decoded.get("choices")
    if not isinstance(choices, list) or not choices:
        diagnostics = {
            "status_code": status_code,
            "headers": response_header_summary(headers or {}),
            "top_level_keys": sorted(str(key) for key in decoded.keys()),
            "choices_type": type(choices).__name__,
            "choices_len": len(choices) if isinstance(choices, list) else None,
        }
        raise CompletionDecodeError(
            format_decode_error("upstream response missing choices[0]", diagnostics),
            diagnostics,
        )
    content = content_from_choice(choices[0])
    if not content:
        diagnostics = {
            "status_code": status_code,
            "headers": response_header_summary(headers or {}),
            "top_level_keys": sorted(str(key) for key in decoded.keys()),
            "choices_len": len(choices),
            "choice_sample": summarize_sse_choice(choices[0])
            if isinstance(choices[0], dict)
            else {"type": type(choices[0]).__name__},
        }
        raise CompletionDecodeError(
            format_decode_error(
                "upstream response missing choices[0].message.content",
                diagnostics,
            ),
            diagnostics,
        )
    usage = decoded.get("usage") or {}
    return decoded, content, usage


def decode_sse_completion_stream(
    response,
    status_code: Optional[int] = None,
    headers=None,
) -> tuple[dict, str, dict]:
    text_parts: list[str] = []
    usage: dict = {}
    last_chunk: dict = {}
    line_count = 0
    data_event_count = 0
    done_count = 0
    chunk_samples: list[dict] = []
    parse_errors: list[dict] = []
    for raw_line in response:
        line_count += 1
        line = raw_line.decode("utf-8", errors="replace").strip()
        if not line or not line.startswith("data:"):
            continue
        data = line[5:].strip()
        if not data:
            continue
        data_event_count += 1
        if data == "[DONE]":
            done_count += 1
            continue
        try:
            chunk = json.loads(data)
        except json.JSONDecodeError as exc:
            if len(parse_errors) < SSE_DIAGNOSTIC_SAMPLE_LIMIT:
                parse_errors.append(
                    {
                        "error": str(exc),
                        "data_len": len(data),
                        "data_sha256_16": text_digest(data),
                    }
                )
            continue
        last_chunk = chunk
        if len(chunk_samples) < SSE_DIAGNOSTIC_SAMPLE_LIMIT:
            sample = {
                "top_level_keys": sorted(str(key) for key in chunk.keys()),
                "model": chunk.get("model"),
                "object": chunk.get("object"),
                "usage_present": isinstance(chunk.get("usage"), dict),
            }
            choices = chunk.get("choices")
            if isinstance(choices, list):
                sample["choices_len"] = len(choices)
                if choices and isinstance(choices[0], dict):
                    sample["choice0"] = summarize_sse_choice(choices[0])
            else:
                sample["choices_type"] = type(choices).__name__
            if isinstance(chunk.get("error"), dict):
                error = chunk["error"]
                sample["error"] = {
                    "keys": sorted(str(key) for key in error.keys()),
                    "code": error.get("code"),
                    "type": error.get("type"),
                    "message": sample_text(str(error.get("message") or "")),
                }
            chunk_samples.append(sample)
        choices = chunk.get("choices")
        if isinstance(choices, list):
            for choice in choices:
                if not isinstance(choice, dict):
                    continue
                delta = choice.get("delta") or {}
                if isinstance(delta, dict):
                    content = delta.get("content")
                    if isinstance(content, str):
                        text_parts.append(content)
                if not text_parts:
                    message_content = content_from_choice(choice)
                    if message_content:
                        text_parts.append(message_content)
        if isinstance(chunk.get("usage"), dict):
            usage = chunk["usage"]
    content = "".join(text_parts).strip()
    if parse_errors:
        diagnostics = {
            "status_code": status_code,
            "headers": response_header_summary(headers or {}),
            "line_count": line_count,
            "data_event_count": data_event_count,
            "done_count": done_count,
            "parse_error_count": len(parse_errors),
            "parse_error_samples": parse_errors,
            "chunk_samples": chunk_samples,
            "usage_present": bool(usage),
            "content_len": len(content),
            "last_chunk_keys": sorted(str(key) for key in last_chunk.keys())
            if isinstance(last_chunk, dict)
            else [],
        }
        raise CompletionDecodeError(
            format_decode_error(
                "upstream SSE response contained malformed data events",
                diagnostics,
            ),
            diagnostics,
        )
    if not content:
        diagnostics = {
            "status_code": status_code,
            "headers": response_header_summary(headers or {}),
            "line_count": line_count,
            "data_event_count": data_event_count,
            "done_count": done_count,
            "parse_error_count": len(parse_errors),
            "parse_error_samples": parse_errors,
            "chunk_samples": chunk_samples,
            "usage_present": bool(usage),
            "last_chunk_keys": sorted(str(key) for key in last_chunk.keys())
            if isinstance(last_chunk, dict)
            else [],
        }
        raise CompletionDecodeError(
            format_decode_error(
                "upstream SSE response did not contain assistant content",
                diagnostics,
            ),
            diagnostics,
        )
    return last_chunk, content, usage


def decode_sse_completion_payload(
    payload: str,
    status_code: Optional[int] = None,
    headers=None,
) -> tuple[dict, str, dict]:
    return decode_sse_completion_stream(
        [f"{line}\n".encode("utf-8") for line in payload.splitlines()],
        status_code=status_code,
        headers=headers,
    )


def main() -> int:
    argv = sys.argv[1:]
    if not argv:
        print("usage: letai_provider_cli.py <gateway|agent> ...", file=sys.stderr)
        return 2
    try:
        if argv[:3] == ["gateway", "call", "agent"]:
            prompt, timeout_ms, agent_id = parse_gateway_call(argv)
            result = request_completion(prompt, timeout_ms, agent_id)
            sys.stdout.write(json.dumps({"result": result}, ensure_ascii=True))
            return 0
        if argv[0] == "agent":
            prompt, timeout_ms, agent_id = parse_local_agent(argv)
            result = request_completion(prompt, timeout_ms, agent_id)
            sys.stdout.write(json.dumps(result, ensure_ascii=True))
            return 0
        raise RuntimeError(f"unsupported invocation mode: {' '.join(argv[:3])}")
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
