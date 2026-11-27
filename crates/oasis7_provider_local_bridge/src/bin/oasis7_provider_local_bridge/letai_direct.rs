use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde_json::{Value, json};

use super::{AgentInvocation, AgentInvocationOutput, short_sha256, summarize_text};

const DEFAULT_LETAI_BASE_URL: &str = "https://api.letai.run/v1";
const DEFAULT_LETAI_MAX_OUTPUT_TOKENS: u64 = 256;
const DEFAULT_LETAI_TEMPERATURE: f64 = 0.0;
const DEFAULT_LETAI_RETRY_COUNT: u64 = 2;
const DEFAULT_LETAI_RETRY_DELAY_MS: u64 = 1000;
const DEFAULT_LETAI_USER_AGENT: &str = "oasis7-letai-provider-rust/1.0";

#[derive(Debug, Clone)]
pub(super) struct LetaiChatConfig {
    pub(super) base_url: String,
    pub(super) api_key: String,
    pub(super) model: String,
    pub(super) system_prompt: Option<String>,
    pub(super) max_output_tokens: u64,
    pub(super) temperature: f64,
    pub(super) stream: bool,
    pub(super) response_format_json_object: bool,
    pub(super) user_agent: String,
    pub(super) extra_headers: Vec<(String, String)>,
    pub(super) retry_count: u64,
    pub(super) retry_delay_ms: u64,
}

#[derive(Debug, Clone)]
pub(super) struct LetaiChatResult {
    pub(super) content: String,
    pub(super) model: String,
    pub(super) duration_ms: u64,
    pub(super) prompt_tokens: Option<u64>,
    pub(super) completion_tokens: Option<u64>,
    pub(super) total_tokens: Option<u64>,
    pub(super) upstream_trace: Value,
}

pub(super) fn invoke_rust_direct_letai(
    invocation: AgentInvocation,
) -> Result<AgentInvocationOutput, String> {
    let config = load_letai_chat_config(invocation.route_label.as_deref())?;
    let started = Instant::now();
    let result = send_letai_chat_completion_with_retries(&config, &invocation)?;
    Ok(AgentInvocationOutput {
        prompt: invocation.prompt,
        text: result.content,
        provider_version: Some(format!("letai/{}", result.model)),
        duration_ms: Some(result.duration_ms.max(started.elapsed().as_millis() as u64)),
        prompt_tokens: result.prompt_tokens,
        completion_tokens: result.completion_tokens,
        total_tokens: result.total_tokens,
        route_note: Some("invocation_backend=rust_direct_letai".to_string()),
        upstream_trace: Some(result.upstream_trace),
    })
}

pub(super) fn load_letai_chat_config(route_label: Option<&str>) -> Result<LetaiChatConfig, String> {
    let route = load_letai_routes_path_config(route_label)?;
    let base_url = normalize_base_url(
        route_or_env_optional(
            &route,
            "base_url",
            &["OASIS7_REMOTE_LLM_BASE_URL", "LETAI_BASE_URL"],
        )
        .unwrap_or_else(|| DEFAULT_LETAI_BASE_URL.to_string())
        .as_str(),
    );
    let api_key = route_string(&route, "api_key")
        .or_else(|| env_optional(&["OASIS7_REMOTE_LLM_API_KEY", "LETAI_API_KEY"]))
        .ok_or_else(|| "missing required remote LLM api key".to_string())?;
    let model = route_or_env_optional(&route, "model", &["OASIS7_REMOTE_LLM_MODEL", "LETAI_MODEL"])
        .ok_or_else(|| "missing required remote LLM model".to_string())?;
    Ok(LetaiChatConfig {
        base_url,
        api_key,
        model,
        system_prompt: route_or_env_optional(
            &route,
            "system_prompt",
            &["OASIS7_REMOTE_LLM_SYSTEM_PROMPT", "LETAI_SYSTEM_PROMPT"],
        ),
        max_output_tokens: route_or_env_u64(
            &route,
            "max_output_tokens",
            DEFAULT_LETAI_MAX_OUTPUT_TOKENS,
            &[
                "OASIS7_REMOTE_LLM_MAX_OUTPUT_TOKENS",
                "LETAI_MAX_OUTPUT_TOKENS",
            ],
        )?,
        temperature: route_or_env_f64(
            &route,
            "temperature",
            DEFAULT_LETAI_TEMPERATURE,
            &["OASIS7_REMOTE_LLM_TEMPERATURE", "LETAI_TEMPERATURE"],
        )?,
        stream: route_or_env_bool(
            &route,
            "stream",
            false,
            &["OASIS7_REMOTE_LLM_STREAM", "LETAI_STREAM"],
        ),
        response_format_json_object: route_or_env_bool(
            &route,
            "response_format_json_object",
            false,
            &[
                "OASIS7_REMOTE_LLM_RESPONSE_FORMAT_JSON_OBJECT",
                "LETAI_RESPONSE_FORMAT_JSON_OBJECT",
            ],
        ),
        user_agent: route_or_env_optional(
            &route,
            "user_agent",
            &["OASIS7_REMOTE_LLM_USER_AGENT", "LETAI_USER_AGENT"],
        )
        .unwrap_or_else(|| DEFAULT_LETAI_USER_AGENT.to_string()),
        extra_headers: load_extra_headers(&route)?,
        retry_count: route_or_env_u64(
            &route,
            "retry_count",
            DEFAULT_LETAI_RETRY_COUNT,
            &["OASIS7_REMOTE_LLM_RETRY_COUNT", "LETAI_RETRY_COUNT"],
        )?
        .max(1),
        retry_delay_ms: route_or_env_u64(
            &route,
            "retry_delay_ms",
            DEFAULT_LETAI_RETRY_DELAY_MS,
            &["OASIS7_REMOTE_LLM_RETRY_DELAY_MS", "LETAI_RETRY_DELAY_MS"],
        )?,
    })
}

fn send_letai_chat_completion_with_retries(
    config: &LetaiChatConfig,
    invocation: &AgentInvocation,
) -> Result<LetaiChatResult, String> {
    let mut last_error = String::new();
    for attempt in 1..=config.retry_count {
        match send_letai_chat_completion(config, invocation) {
            Ok(result) => return Ok(result),
            Err(err) => {
                last_error = err;
                if attempt >= config.retry_count || !is_retryable_letai_error(last_error.as_str()) {
                    break;
                }
                let diagnostics = json!({
                    "event": "rust_direct_letai_chat_retry",
                    "attempt": attempt + 1,
                    "retry_count": config.retry_count,
                    "chat_request_key": invocation.chat_request_key.as_deref(),
                    "reason": summarize_text(last_error.as_str(), 300),
                    "diagnostics": error_diagnostics_json(last_error.as_str()),
                });
                eprintln!("{}", diagnostics);
                if config.retry_delay_ms > 0 {
                    std::thread::sleep(Duration::from_millis(config.retry_delay_ms));
                }
            }
        }
    }
    Err(last_error)
}

fn send_letai_chat_completion(
    config: &LetaiChatConfig,
    invocation: &AgentInvocation,
) -> Result<LetaiChatResult, String> {
    let timeout = Duration::from_secs(invocation.timeout_seconds.max(1));
    let client = Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|err| format!("build LetAI HTTP client failed: {err}"))?;
    let mut messages = Vec::new();
    if let Some(system_prompt) = config.system_prompt.as_deref() {
        messages.push(json!({"role": "system", "content": system_prompt}));
    }
    messages.push(json!({"role": "user", "content": invocation.prompt}));
    let mut body = json!({
        "model": config.model,
        "messages": messages,
        "temperature": config.temperature,
        "stream": config.stream,
        "max_tokens": config.max_output_tokens,
        "user": format!("oasis7-provider:{}", invocation.agent_id),
        "idempotency_key": invocation.idempotency_key,
        "session_key": invocation.session_key,
    });
    if let Some(provider_invocation_key) = invocation.provider_invocation_key.as_deref() {
        body["provider_invocation_key"] = json!(provider_invocation_key);
    }
    if let Some(agent_session_id) = invocation.agent_session_id.as_deref() {
        body["agent_session_id"] = json!(agent_session_id);
    }
    if config.response_format_json_object {
        body["response_format"] = json!({"type": "json_object"});
    }
    let started = Instant::now();
    let url = format!("{}/chat/completions", config.base_url);
    eprintln!(
        "{}",
        json!({
            "event": "rust_direct_letai_chat_request",
            "target": safe_url_summary(url.as_str()),
            "model": config.model,
            "stream": config.stream,
            "timeout_ms": timeout.as_millis(),
            "max_output_tokens": config.max_output_tokens,
            "temperature": config.temperature,
            "response_format_json_object": config.response_format_json_object,
            "system_prompt_present": config.system_prompt.is_some(),
            "prompt_len": invocation.prompt.len(),
            "agent_id": invocation.agent_id,
            "chat_request_key": invocation.chat_request_key.as_deref(),
        })
    );
    let mut response = client
        .post(url.as_str())
        .bearer_auth(config.api_key.as_str())
        .header("Content-Type", "application/json")
        .header("User-Agent", config.user_agent.as_str());
    for (name, value) in &config.extra_headers {
        if is_reserved_identity_header(name.as_str()) {
            continue;
        }
        response = response.header(name.as_str(), value.as_str());
    }
    response = response.header("Idempotency-Key", invocation.idempotency_key.as_str());
    if let Some(provider_invocation_key) = invocation.provider_invocation_key.as_deref() {
        response = response.header("X-Oasis7-Provider-Invocation-Key", provider_invocation_key);
    }
    if let Some(agent_session_id) = invocation.agent_session_id.as_deref() {
        response = response.header("X-Oasis7-Agent-Session-Id", agent_session_id);
    }
    response = response.header(
        "X-Oasis7-Provider-Session-Key",
        invocation.session_key.as_str(),
    );
    let mut response = response
        .json(&body)
        .send()
        .map_err(|err| format!("upstream chat completion request failed: {err}"))?;
    let status = response.status();
    let headers = response.headers().clone();
    if !status.is_success() {
        let detail = response
            .text()
            .unwrap_or_else(|err| format!("read error body failed: {err}"));
        return Err(format!(
            "upstream chat completion returned HTTP {}: {}",
            status.as_u16(),
            summarize_text(detail.as_str(), 500)
        ));
    }
    let mut result = if config.stream {
        decode_letai_sse_completion(&mut response, Some(status.as_u16()), &headers)?
    } else {
        let payload = response
            .text()
            .map_err(|err| format!("read upstream response body failed: {err}"))?;
        decode_letai_completion_payload(payload.as_str(), Some(status.as_u16()), &headers)?
    };
    result.duration_ms = started.elapsed().as_millis() as u64;
    Ok(result)
}

fn is_reserved_identity_header(name: &str) -> bool {
    [
        "idempotency-key",
        "x-oasis7-provider-invocation-key",
        "x-oasis7-agent-session-id",
        "x-oasis7-provider-session-key",
    ]
    .iter()
    .any(|reserved| name.eq_ignore_ascii_case(reserved))
}

pub(super) fn decode_letai_completion_payload(
    payload: &str,
    status_code: Option<u16>,
    headers: &reqwest::header::HeaderMap,
) -> Result<LetaiChatResult, String> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "upstream response body was empty; diagnostics={}",
            json!({
                "status_code": status_code,
                "headers": response_header_summary(headers),
                "body_len": payload.len(),
            })
        ));
    }
    if trimmed.lines().any(|line| line.trim().starts_with("data:")) {
        let cursor = std::io::Cursor::new(trimmed.as_bytes());
        return decode_letai_sse_reader(cursor, status_code, headers);
    }
    let decoded: Value = serde_json::from_str(trimmed)
        .map_err(|err| format!("decode upstream response JSON failed: {err}"))?;
    let choices = decoded.get("choices").and_then(Value::as_array);
    let Some(first_choice) = choices.and_then(|choices| choices.first()) else {
        return Err(format!(
            "upstream response missing choices[0]; diagnostics={}",
            json!({
                "status_code": status_code,
                "headers": response_header_summary(headers),
                "top_level_keys": value_keys(&decoded),
                "choices_len": choices.map(Vec::len),
            })
        ));
    };
    let content = content_from_choice(first_choice);
    if content.trim().is_empty() {
        return Err(format!(
            "upstream response missing choices[0].message.content; diagnostics={}",
            json!({
                "status_code": status_code,
                "headers": response_header_summary(headers),
                "top_level_keys": value_keys(&decoded),
                "choices_len": choices.map(Vec::len),
                "choice_sample": summarize_choice(first_choice),
            })
        ));
    }
    let usage = decoded.get("usage").unwrap_or(&Value::Null);
    Ok(LetaiChatResult {
        content: content.trim().to_string(),
        model: decoded
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        duration_ms: 0,
        prompt_tokens: usage.get("prompt_tokens").and_then(Value::as_u64),
        completion_tokens: usage.get("completion_tokens").and_then(Value::as_u64),
        total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
        upstream_trace: json!({
            "stage": "chat_completion_decode",
            "mode": "json",
            "status_code": status_code,
            "headers": response_header_summary(headers),
            "top_level_keys": value_keys(&decoded),
            "choices_len": choices.map(Vec::len),
            "content_len": content.trim().len(),
            "usage_present": usage.is_object(),
        }),
    })
}

fn decode_letai_sse_completion(
    response: &mut reqwest::blocking::Response,
    status_code: Option<u16>,
    headers: &reqwest::header::HeaderMap,
) -> Result<LetaiChatResult, String> {
    decode_letai_sse_reader(response, status_code, headers)
}

pub(super) fn decode_letai_sse_reader<R: Read>(
    reader: R,
    status_code: Option<u16>,
    headers: &reqwest::header::HeaderMap,
) -> Result<LetaiChatResult, String> {
    let mut text_parts = Vec::new();
    let mut usage = Value::Null;
    let mut last_chunk = Value::Null;
    let mut model = "unknown".to_string();
    let mut line_count = 0_u64;
    let mut data_event_count = 0_u64;
    let mut done_count = 0_u64;
    let mut parse_errors = Vec::new();
    let mut chunk_samples = Vec::new();
    for line_result in BufReader::new(reader).lines() {
        line_count += 1;
        let line = line_result.map_err(|err| format!("read upstream SSE failed: {err}"))?;
        let line = line.trim();
        if line.is_empty() || !line.starts_with("data:") {
            continue;
        }
        let data = line[5..].trim();
        if data.is_empty() {
            continue;
        }
        data_event_count += 1;
        if data == "[DONE]" {
            done_count += 1;
            continue;
        }
        let chunk: Value = match serde_json::from_str(data) {
            Ok(chunk) => chunk,
            Err(err) => {
                if parse_errors.len() < 5 {
                    parse_errors.push(json!({
                        "error": err.to_string(),
                        "data_len": data.len(),
                        "data_sha256_16": short_sha256(data),
                    }));
                }
                continue;
            }
        };
        if let Some(chunk_model) = chunk.get("model").and_then(Value::as_str) {
            model = chunk_model.to_string();
        }
        if chunk_samples.len() < 5 {
            chunk_samples.push(summarize_sse_chunk(&chunk));
        }
        if let Some(chunk_usage) = chunk.get("usage").filter(|value| value.is_object()) {
            usage = chunk_usage.clone();
        }
        if let Some(choices) = chunk.get("choices").and_then(Value::as_array) {
            for choice in choices {
                if let Some(content) = choice
                    .get("delta")
                    .and_then(|delta| delta.get("content"))
                    .and_then(Value::as_str)
                {
                    text_parts.push(content.to_string());
                }
                if text_parts.is_empty() {
                    let fallback = content_from_choice(choice);
                    if !fallback.is_empty() {
                        text_parts.push(fallback);
                    }
                }
            }
        }
        last_chunk = chunk;
    }
    let content = text_parts.join("").trim().to_string();
    if !parse_errors.is_empty() {
        return Err(format!(
            "upstream SSE response contained malformed data events; diagnostics={}",
            json!({
                "status_code": status_code,
                "headers": response_header_summary(headers),
                "line_count": line_count,
                "data_event_count": data_event_count,
                "done_count": done_count,
                "parse_error_count": parse_errors.len(),
                "parse_error_samples": parse_errors,
                "chunk_samples": chunk_samples,
                "usage_present": usage.is_object(),
                "content_len": content.len(),
                "last_chunk_keys": value_keys(&last_chunk),
            })
        ));
    }
    if content.is_empty() {
        return Err(format!(
            "upstream SSE response did not contain assistant content; diagnostics={}",
            json!({
                "status_code": status_code,
                "headers": response_header_summary(headers),
                "line_count": line_count,
                "data_event_count": data_event_count,
                "done_count": done_count,
                "parse_error_count": parse_errors.len(),
                "parse_error_samples": parse_errors,
                "chunk_samples": chunk_samples,
                "usage_present": usage.is_object(),
                "last_chunk_keys": value_keys(&last_chunk),
            })
        ));
    }
    let content_len = content.len();
    Ok(LetaiChatResult {
        content,
        model,
        duration_ms: 0,
        prompt_tokens: usage.get("prompt_tokens").and_then(Value::as_u64),
        completion_tokens: usage.get("completion_tokens").and_then(Value::as_u64),
        total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
        upstream_trace: json!({
            "stage": "chat_completion_decode",
            "mode": "sse",
            "status_code": status_code,
            "headers": response_header_summary(headers),
            "line_count": line_count,
            "data_event_count": data_event_count,
            "done_count": done_count,
            "parse_error_count": parse_errors.len(),
            "chunk_samples": chunk_samples,
            "usage_present": usage.is_object(),
            "content_len": content_len,
            "last_chunk_keys": value_keys(&last_chunk),
        }),
    })
}

fn content_from_choice(choice: &Value) -> String {
    let Some(message) = choice.get("message") else {
        return String::new();
    };
    let Some(content) = message.get("content") else {
        return String::new();
    };
    if let Some(text) = content.as_str() {
        return text.trim().to_string();
    }
    if let Some(items) = content.as_array() {
        return items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("content").and_then(Value::as_str))
                    .or_else(|| item.get("value").and_then(Value::as_str))
            })
            .collect::<String>()
            .trim()
            .to_string();
    }
    String::new()
}

fn summarize_sse_chunk(chunk: &Value) -> Value {
    let mut sample = json!({
        "top_level_keys": value_keys(chunk),
        "model": chunk.get("model").and_then(Value::as_str),
        "object": chunk.get("object").and_then(Value::as_str),
        "usage_present": chunk.get("usage").is_some_and(Value::is_object),
    });
    if let Some(choices) = chunk.get("choices").and_then(Value::as_array) {
        sample["choices_len"] = json!(choices.len());
        if let Some(choice) = choices.first() {
            sample["choice0"] = summarize_choice(choice);
        }
    } else if let Some(choices) = chunk.get("choices") {
        sample["choices_type"] = json!(value_type_name(choices));
    }
    if let Some(error) = chunk.get("error").filter(|value| value.is_object()) {
        sample["error"] = json!({
            "keys": value_keys(error),
            "code": error.get("code").and_then(Value::as_str),
            "type": error.get("type").and_then(Value::as_str),
            "message": error.get("message").and_then(Value::as_str).map(|value| summarize_text(value, 80)),
        });
    }
    sample
}

fn summarize_choice(choice: &Value) -> Value {
    let mut summary = json!({
        "keys": value_keys(choice),
        "finish_reason": choice.get("finish_reason"),
    });
    if let Some(delta) = choice.get("delta").filter(|value| value.is_object()) {
        summary["delta"] = json!({
            "keys": value_keys(delta),
            "content_present": delta.get("content").and_then(Value::as_str).is_some(),
            "content_len": delta.get("content").and_then(Value::as_str).map(str::len),
            "role": delta.get("role").and_then(Value::as_str),
        });
    }
    if let Some(message) = choice.get("message").filter(|value| value.is_object()) {
        summary["message"] = json!({
            "keys": value_keys(message),
            "content_present": message.get("content").and_then(Value::as_str).is_some(),
            "content_len": message.get("content").and_then(Value::as_str).map(str::len),
        });
    }
    summary
}

fn normalize_base_url(raw: &str) -> String {
    let mut base = raw.trim().trim_end_matches('/').to_string();
    for suffix in ["/chat/completions", "/responses"] {
        if base.ends_with(suffix) {
            let keep = base.len() - suffix.len();
            base.truncate(keep);
        }
    }
    base
}

fn load_letai_routes_path_config(route_label: Option<&str>) -> Result<Value, String> {
    let Some(routes_path) = env_optional(&["OASIS7_REMOTE_LLM_ROUTES_PATH"]) else {
        if route_label.is_some_and(|label| !label.trim().is_empty())
            && env_optional(&["OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH"]).is_none()
            && env_optional(&["OASIS7_REMOTE_LLM_API_KEY", "LETAI_API_KEY"]).is_none()
        {
            return Err(
                "OASIS7_REMOTE_LLM_ROUTE_LABEL requires either OASIS7_REMOTE_LLM_ROUTES_PATH or OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH"
                    .to_string(),
            );
        }
        return Ok(Value::Null);
    };
    let raw = fs::read_to_string(routes_path.as_str())
        .map_err(|err| format!("failed to read OASIS7_REMOTE_LLM_ROUTES_PATH: {err}"))?;
    let payload = serde_json::from_str::<Value>(raw.as_str())
        .map_err(|err| format!("OASIS7_REMOTE_LLM_ROUTES_PATH must contain valid JSON: {err}"))?;
    let Some(routes) = payload.as_object() else {
        return Err("OASIS7_REMOTE_LLM_ROUTES_PATH root must be a JSON object".to_string());
    };
    let lookup_label = route_label
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("default");
    let Some(route) = routes.get(lookup_label) else {
        return Err(format!(
            "route config `{lookup_label}` was not found in OASIS7_REMOTE_LLM_ROUTES_PATH"
        ));
    };
    if !route.is_object() {
        return Err(format!(
            "route config `{lookup_label}` must be a JSON object"
        ));
    }
    Ok(route.clone())
}

fn route_string(route: &Value, key: &str) -> Option<String> {
    route
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn route_or_env_optional(route: &Value, route_key: &str, env_names: &[&str]) -> Option<String> {
    route_string(route, route_key).or_else(|| env_optional(env_names))
}

fn route_or_env_u64(
    route: &Value,
    route_key: &str,
    default: u64,
    env_names: &[&str],
) -> Result<u64, String> {
    if let Some(value) = route.get(route_key).filter(|value| !value.is_null()) {
        if let Some(number) = value.as_u64() {
            return Ok(number);
        }
        if let Some(raw) = value.as_str() {
            return raw.parse::<u64>().map_err(|err| {
                format!("invalid integer for route field {route_key}: {raw}: {err}")
            });
        }
        return Err(format!(
            "invalid integer for route field {route_key}: {}",
            value_type_name(value)
        ));
    }
    env_u64(default, env_names)
}

fn route_or_env_f64(
    route: &Value,
    route_key: &str,
    default: f64,
    env_names: &[&str],
) -> Result<f64, String> {
    if let Some(value) = route.get(route_key).filter(|value| !value.is_null()) {
        if let Some(number) = value.as_f64() {
            return Ok(number);
        }
        if let Some(raw) = value.as_str() {
            return raw
                .parse::<f64>()
                .map_err(|err| format!("invalid float for route field {route_key}: {raw}: {err}"));
        }
        return Err(format!(
            "invalid float for route field {route_key}: {}",
            value_type_name(value)
        ));
    }
    env_f64(default, env_names)
}

fn route_or_env_bool(route: &Value, route_key: &str, default: bool, env_names: &[&str]) -> bool {
    if let Some(value) = route.get(route_key).filter(|value| !value.is_null()) {
        if let Some(flag) = value.as_bool() {
            return flag;
        }
        if let Some(raw) = value.as_str() {
            return matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
        return false;
    }
    env_bool(default, env_names)
}

fn load_extra_headers(route: &Value) -> Result<Vec<(String, String)>, String> {
    let Some(raw) = route_string(route, "extra_headers_json").or_else(|| {
        env_optional(&[
            "OASIS7_REMOTE_LLM_EXTRA_HEADERS_JSON",
            "LETAI_EXTRA_HEADERS_JSON",
        ])
    }) else {
        return Ok(Vec::new());
    };
    let decoded = serde_json::from_str::<Value>(raw.as_str())
        .map_err(|err| format!("OASIS7_REMOTE_LLM_EXTRA_HEADERS_JSON must be valid JSON: {err}"))?;
    let Some(object) = decoded.as_object() else {
        return Err("OASIS7_REMOTE_LLM_EXTRA_HEADERS_JSON must be a JSON object".to_string());
    };
    let mut headers = Vec::new();
    for (name, value) in object {
        reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|err| format!("invalid extra header name `{name}`: {err}"))?;
        let value = value
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| value.to_string());
        reqwest::header::HeaderValue::from_str(value.as_str())
            .map_err(|err| format!("invalid extra header value for `{name}`: {err}"))?;
        headers.push((name.to_string(), value));
    }
    Ok(headers)
}

fn env_optional(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn env_u64(default: u64, names: &[&str]) -> Result<u64, String> {
    let Some(raw) = env_optional(names) else {
        return Ok(default);
    };
    raw.parse::<u64>()
        .map_err(|err| format!("invalid integer for {}: {raw}: {err}", names.join(" or ")))
}

fn env_f64(default: f64, names: &[&str]) -> Result<f64, String> {
    let Some(raw) = env_optional(names) else {
        return Ok(default);
    };
    raw.parse::<f64>()
        .map_err(|err| format!("invalid float for {}: {raw}: {err}", names.join(" or ")))
}

fn env_bool(default: bool, names: &[&str]) -> bool {
    let Some(raw) = env_optional(names) else {
        return default;
    };
    matches!(
        raw.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn response_header_summary(headers: &reqwest::header::HeaderMap) -> Value {
    let mut map = serde_json::Map::new();
    for key in [
        "content-type",
        "x-request-id",
        "x-trace-id",
        "cf-ray",
        "server",
    ] {
        if let Some(value) = headers.get(key).and_then(|value| value.to_str().ok()) {
            map.insert(key.to_string(), json!(summarize_text(value, 120)));
        }
    }
    Value::Object(map)
}

fn safe_url_summary(url: &str) -> Value {
    match reqwest::Url::parse(url) {
        Ok(parsed) => json!({
            "scheme": parsed.scheme(),
            "host": parsed.host_str().unwrap_or_default(),
            "port": parsed.port(),
            "path": parsed.path(),
        }),
        Err(_) => json!({
            "scheme": "",
            "host": "",
            "port": null,
            "path": "",
        }),
    }
}

fn value_keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub(super) fn is_retryable_letai_error(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    lowered.contains("did not contain assistant content")
        || lowered.contains("http 502")
        || lowered.contains("http 503")
        || lowered.contains("http 504")
        || lowered.contains("error code: 502")
        || lowered.contains("error code: 503")
        || lowered.contains("error code: 504")
        || lowered.contains("error sending request")
        || lowered.contains("request failed")
        || lowered.contains("read operation timed out")
        || lowered.contains("operation timed out")
        || lowered.contains("remote end closed connection without response")
        || lowered.contains("connection reset")
        || lowered.contains("timed out")
}

pub(super) fn error_diagnostics_json(error: &str) -> Value {
    let Some((_, diagnostics)) = error.split_once("diagnostics=") else {
        return Value::Null;
    };
    serde_json::from_str::<Value>(diagnostics.trim()).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentInvocation;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    fn invocation() -> AgentInvocation {
        AgentInvocation {
            provider_cli_bin: "provider-cli".to_string(),
            agent_id: "main".to_string(),
            thinking: "off".to_string(),
            session_key: "agent:main:subagent:world-simulator:continuous:agent-1:session-1"
                .to_string(),
            timeout_seconds: 5,
            prompt: "{\"decision\":\"wait\"}".to_string(),
            idempotency_key:
                "blake3:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            provider_invocation_key: Some(
                "blake3:2222222222222222222222222222222222222222222222222222222222222222"
                    .to_string(),
            ),
            agent_session_id: Some("session-1".to_string()),
            chat_request_key: None,
            route_label: None,
        }
    }

    #[test]
    fn direct_letai_retries_preserve_outer_identity_in_body_and_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock LetAI server");
        let base_url = format!("http://{}", listener.local_addr().expect("server address"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let request_sink = Arc::clone(&requests);
        let server = thread::spawn(move || {
            for status in [503, 200] {
                let (mut stream, _) = listener.accept().expect("accept LetAI request");
                let mut request = Vec::new();
                let mut buffer = [0_u8; 8192];
                let bytes = stream.read(&mut buffer).expect("read LetAI request");
                request.extend_from_slice(&buffer[..bytes]);
                request_sink
                    .lock()
                    .expect("request sink lock")
                    .push(String::from_utf8(request).expect("request utf8"));
                let body = if status == 200 {
                    r#"{"model":"test-model","choices":[{"message":{"content":"{\"decision\":\"wait\"}"}}]}"#
                } else {
                    "temporary upstream failure"
                };
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write LetAI response");
            }
        });
        let config = LetaiChatConfig {
            base_url,
            api_key: "test-key".to_string(),
            model: "test-model".to_string(),
            system_prompt: None,
            max_output_tokens: 32,
            temperature: 0.0,
            stream: false,
            response_format_json_object: false,
            user_agent: "test-agent".to_string(),
            extra_headers: vec![
                (
                    "Idempotency-Key".to_string(),
                    "wrong-idempotency".to_string(),
                ),
                (
                    "X-Oasis7-Provider-Invocation-Key".to_string(),
                    "wrong-provider-key".to_string(),
                ),
                (
                    "X-Oasis7-Agent-Session-Id".to_string(),
                    "wrong-session".to_string(),
                ),
                (
                    "X-Oasis7-Provider-Session-Key".to_string(),
                    "wrong-session-key".to_string(),
                ),
            ],
            retry_count: 2,
            retry_delay_ms: 0,
        };
        let expected = invocation();
        let result = send_letai_chat_completion_with_retries(&config, &expected)
            .expect("retry should succeed");
        server.join().expect("LetAI server thread");
        assert_eq!(result.content, r#"{"decision":"wait"}"#);
        let requests = requests.lock().expect("request sink lock");
        assert_eq!(requests.len(), 2);
        for request in requests.iter() {
            let body = request.split("\r\n\r\n").nth(1).expect("request body");
            let body: Value = serde_json::from_str(body).expect("request JSON");
            assert_eq!(
                body["provider_invocation_key"],
                expected.provider_invocation_key.as_deref().unwrap()
            );
            assert_eq!(
                body["agent_session_id"],
                expected.agent_session_id.as_deref().unwrap()
            );
            assert_eq!(body["idempotency_key"], expected.idempotency_key);
            assert_eq!(body["session_key"], expected.session_key);
            let request_lower = request.to_ascii_lowercase();
            assert!(request_lower.contains("idempotency-key: "));
            assert!(request_lower.contains("x-oasis7-provider-invocation-key: "));
            assert!(request_lower.contains("x-oasis7-agent-session-id: session-1"));
            assert!(request_lower.contains("x-oasis7-provider-session-key: agent:main:subagent"));
            assert_eq!(request_lower.matches("idempotency-key: ").count(), 1);
            assert_eq!(
                request_lower
                    .matches("x-oasis7-provider-invocation-key: ")
                    .count(),
                1
            );
            assert_eq!(
                request_lower.matches("x-oasis7-agent-session-id: ").count(),
                1
            );
            assert_eq!(
                request_lower
                    .matches("x-oasis7-provider-session-key: ")
                    .count(),
                1
            );
        }
    }
}
