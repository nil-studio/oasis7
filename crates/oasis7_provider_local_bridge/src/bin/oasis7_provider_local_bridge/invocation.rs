use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tracing::error;

use super::{
    ProviderAgentChatRequest, TRACE_SESSION_PROCESS_LABEL, agent_output_from_json,
    build_gateway_agent_params, local_session_id_from_session_key, resolve_trace_session_id,
    route_label_env, should_fallback_to_local_agent, summarize_text,
};

pub(super) fn error_diagnostics_json(error: &str) -> Value {
    let Some((_, diagnostics)) = error.split_once("diagnostics=") else {
        return Value::Null;
    };
    serde_json::from_str::<Value>(diagnostics.trim()).unwrap_or(Value::Null)
}

pub(super) fn build_agent_chat_prompt(request: &ProviderAgentChatRequest) -> String {
    let context = json!({
        "agent_id": request.agent_id,
        "player_id": request.player_id,
        "world_time": request.world_time,
        "location_id": request.location_id,
        "resources": request.resources,
        "recent_feedback": request.recent_feedback,
        "player_message": request.message,
    });
    format!(
        concat!(
            "You are the selected oasis7 agent. Reply directly to the player in the same language as their message. ",
            "Do not return JSON. Do not claim unavailable facts. ",
            "If asked where you are, use location_id exactly. If asked about nearby resources, use resources exactly. ",
            "Keep the reply under 80 Chinese characters or 60 English words. Context follows:\n{}"
        ),
        context
    )
}

pub(super) fn provider_error_upstream_trace(error: &str) -> Value {
    let diagnostics = error_diagnostics_json(error);
    json!({
        "stage": "decision_invocation",
        "error_summary": summarize_text(error, 500),
        "diagnostics": diagnostics,
        "diagnostics_present": !diagnostics.is_null(),
    })
}

#[derive(Debug, Clone)]
pub(super) struct AgentInvocation {
    pub(super) provider_cli_bin: String,
    pub(super) agent_id: String,
    pub(super) thinking: String,
    pub(super) session_key: String,
    pub(super) timeout_seconds: u64,
    pub(super) prompt: String,
    pub(super) idempotency_key: String,
    /// Present only on the validated continuous-agent route. The legacy
    /// inner-request route leaves both outer identity fields absent.
    pub(super) provider_invocation_key: Option<String>,
    pub(super) agent_session_id: Option<String>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) chat_request_key: Option<String>,
    pub(super) route_label: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct AgentInvocationOutput {
    pub(super) prompt: String,
    pub(super) text: String,
    pub(super) provider_version: Option<String>,
    pub(super) duration_ms: Option<u64>,
    pub(super) prompt_tokens: Option<u64>,
    pub(super) completion_tokens: Option<u64>,
    pub(super) total_tokens: Option<u64>,
    pub(super) route_note: Option<String>,
    pub(super) upstream_trace: Option<Value>,
}

pub(super) trait AgentInvoker: Send + Sync {
    fn invoke(&self, invocation: AgentInvocation) -> Result<AgentInvocationOutput, String>;
}

#[derive(Debug, Clone, Default)]
pub(super) struct ProviderCliInvoker;

impl AgentInvoker for ProviderCliInvoker {
    fn invoke(&self, invocation: AgentInvocation) -> Result<AgentInvocationOutput, String> {
        match invoke_gateway_agent(invocation.clone()) {
            Ok(output) => Ok(output),
            Err(err) if should_fallback_to_local_agent(err.as_str()) => {
                invoke_local_agent(invocation, err.as_str())
            }
            Err(err) => Err(err),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn local_cli_fallback_receives_outer_identity_environment() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let script_path = std::env::temp_dir().join(format!(
            "oasis7-provider-identity-fallback-{}-{unique}",
            std::process::id()
        ));
        fs::write(
            &script_path,
            r##"#!/bin/sh
cat <<EOF
{"payloads":[{"text":"{\"provider_invocation_key\":\"$OASIS7_PROVIDER_INVOCATION_KEY\",\"agent_session_id\":\"$OASIS7_AGENT_SESSION_ID\",\"idempotency_key\":\"$OASIS7_PROVIDER_IDEMPOTENCY_KEY\",\"session_key\":\"$OASIS7_PROVIDER_SESSION_KEY\"}"}]}
EOF
"##,
        )
        .expect("write provider fallback fixture");
        let mut permissions = fs::metadata(&script_path)
            .expect("read provider fallback fixture")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("make provider fallback executable");

        let invocation = AgentInvocation {
            provider_cli_bin: script_path.to_string_lossy().into_owned(),
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
        };
        let output = invoke_local_agent(invocation.clone(), "gateway timeout")
            .expect("local provider fallback should return output");
        let echoed: Value = serde_json::from_str(output.text.as_str()).expect("echoed JSON");
        assert_eq!(
            echoed["provider_invocation_key"],
            invocation.provider_invocation_key.as_deref().unwrap()
        );
        assert_eq!(
            echoed["agent_session_id"],
            invocation.agent_session_id.as_deref().unwrap()
        );
        assert_eq!(echoed["idempotency_key"], invocation.idempotency_key);
        assert_eq!(echoed["session_key"], invocation.session_key);
        let _ = fs::remove_file(script_path);
    }
}

fn invoke_gateway_agent(invocation: AgentInvocation) -> Result<AgentInvocationOutput, String> {
    let params = build_gateway_agent_params(&invocation)
        .map_err(|err| format!("serialize gateway call params failed: {err}"))?;
    let rpc_timeout_ms = invocation
        .timeout_seconds
        .saturating_mul(1000)
        .saturating_add(2000);
    let started = Instant::now();
    let output = Command::new(invocation.provider_cli_bin.as_str())
        .arg("gateway")
        .arg("call")
        .arg("agent")
        .arg("--expect-final")
        .arg("--json")
        .arg("--timeout")
        .arg(rpc_timeout_ms.to_string())
        .arg("--params")
        .arg(params)
        .envs(route_label_env(invocation.route_label.as_deref()))
        .env(
            oasis7_observability::TRACE_SESSION_ID_ENV,
            resolve_trace_session_id(TRACE_SESSION_PROCESS_LABEL),
        )
        .output()
        .map_err(|err| format!("spawn provider gateway call agent failed: {err}"))?;
    if !output.status.success() {
        let summary = provider_cli_failure_summary(
            "gateway_call_agent",
            &output,
            started.elapsed(),
            invocation.route_label.as_deref().is_some(),
            rpc_timeout_ms,
        );
        error!(target: "oasis7_provider_local_bridge", %summary, "provider cli invocation failed");
        return Err(format!("provider gateway call agent failed: {summary}"));
    }
    let payload = String::from_utf8(output.stdout)
        .map_err(|err| format!("provider gateway call agent stdout was not utf8: {err}"))?;
    agent_output_from_json(invocation.prompt, payload.as_str(), None)
}

fn invoke_local_agent(
    invocation: AgentInvocation,
    gateway_error: &str,
) -> Result<AgentInvocationOutput, String> {
    let session_id = local_session_id_from_session_key(invocation.session_key.as_str());
    let timeout_ms = invocation.timeout_seconds.saturating_mul(1000);
    let started = Instant::now();
    let output = Command::new(invocation.provider_cli_bin.as_str())
        .arg("agent")
        .arg("--agent")
        .arg(invocation.agent_id.as_str())
        .arg("--message")
        .arg(invocation.prompt.as_str())
        .arg("--local")
        .arg("--session-id")
        .arg(session_id.as_str())
        .arg("--thinking")
        .arg(invocation.thinking.as_str())
        .arg("--timeout")
        .arg(timeout_ms.to_string())
        .arg("--json")
        .envs(route_label_env(invocation.route_label.as_deref()))
        .envs(invocation_identity_env(&invocation))
        .env(
            oasis7_observability::TRACE_SESSION_ID_ENV,
            resolve_trace_session_id(TRACE_SESSION_PROCESS_LABEL),
        )
        .output()
        .map_err(|err| format!("spawn provider local agent failed: {err}"))?;
    if !output.status.success() {
        let summary = provider_cli_failure_summary(
            "local_agent_fallback",
            &output,
            started.elapsed(),
            invocation.route_label.as_deref().is_some(),
            timeout_ms,
        );
        error!(target: "oasis7_provider_local_bridge", %summary, "provider cli fallback failed");
        return Err(format!(
            "provider gateway fallback failed after `{}`; local agent failed: {}",
            summarize_text(gateway_error, 240),
            summary,
        ));
    }
    let payload = String::from_utf8(output.stdout)
        .map_err(|err| format!("provider local agent stdout was not utf8: {err}"))?;
    agent_output_from_json(
        invocation.prompt,
        payload.as_str(),
        Some(format!(
            "invocation_fallback=local_embedded; reason={}",
            summarize_text(gateway_error, 240)
        )),
    )
}

fn invocation_identity_env(invocation: &AgentInvocation) -> Vec<(String, String)> {
    let mut env = vec![(
        "OASIS7_PROVIDER_IDEMPOTENCY_KEY".to_string(),
        invocation.idempotency_key.clone(),
    )];
    if let Some(provider_invocation_key) = invocation.provider_invocation_key.as_ref() {
        env.push((
            "OASIS7_PROVIDER_INVOCATION_KEY".to_string(),
            provider_invocation_key.clone(),
        ));
    }
    if let Some(agent_session_id) = invocation.agent_session_id.as_ref() {
        env.push((
            "OASIS7_AGENT_SESSION_ID".to_string(),
            agent_session_id.clone(),
        ));
    }
    env.push((
        "OASIS7_PROVIDER_SESSION_KEY".to_string(),
        invocation.session_key.clone(),
    ));
    env
}

fn provider_cli_failure_summary(
    mode: &str,
    output: &std::process::Output,
    elapsed: Duration,
    route_label_present: bool,
    timeout_ms: u64,
) -> String {
    let stderr = String::from_utf8_lossy(output.stderr.as_slice());
    let stdout = String::from_utf8_lossy(output.stdout.as_slice());
    json!({
        "mode": mode,
        "status": output.status.to_string(),
        "elapsed_ms": elapsed.as_millis(),
        "timeout_ms": timeout_ms,
        "route_label_present": route_label_present,
        "stderr": text_diagnostic_summary(stderr.as_ref(), true),
        "stdout": text_diagnostic_summary(stdout.as_ref(), false),
        "stderr_events": stderr_json_events(stderr.as_ref(), 8),
    })
    .to_string()
}

fn text_diagnostic_summary(text: &str, include_tail: bool) -> Value {
    let trimmed = text.trim();
    let mut summary = json!({
        "len": trimmed.len(),
        "sha256_16": short_sha256(trimmed),
        "truncated": trimmed.len() > 320,
    });
    if include_tail {
        summary["tail"] = Value::String(summarize_text(trimmed, 320));
    }
    summary
}

fn stderr_json_events(text: &str, max_events: usize) -> Vec<Value> {
    let mut events = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if value.get("event").is_some() {
            events.push(value);
        }
    }
    if events.len() > max_events {
        events.split_off(events.len() - max_events)
    } else {
        events
    }
}

pub(super) fn short_sha256(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..8])
}

pub(super) fn agent_chat_idempotency_key(
    session_key: &str,
    request: &ProviderAgentChatRequest,
) -> String {
    let stable_chat_key = short_sha256(
        format!(
            "{}\0{}\0{}\0{}",
            request.world_time, request.agent_id, request.player_id, request.message
        )
        .as_str(),
    );
    format!("{session_key}-{}-{stable_chat_key}", request.world_time)
}
