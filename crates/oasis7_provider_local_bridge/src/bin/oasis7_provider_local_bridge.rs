use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::env;
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oasis7_agent_api::{
    ContinuousAgentRequestContextV1, DecisionRequest, Digest32, FeedbackEnvelopeV1,
    ProviderAgentChatRequest, ProviderAgentChatResponse, ProviderDiagnostics,
    ProviderErrorEnvelope, ProviderTokenUsage, ProviderTraceEnvelope, ProviderTranscriptEntry,
    cognition_response_digest, h_v1, provider_agent_chat_log_key,
};
use oasis7_client_api::{ProviderHealth, ProviderInfo};
type DecisionResponse =
    oasis7_agent_api::DecisionResponse<oasis7_simulator::Action, oasis7_simulator::AgentQuery>;
type ProviderDecision =
    oasis7_agent_api::ProviderDecision<oasis7_simulator::Action, oasis7_simulator::AgentQuery>;
type ContinuousAgentResponseContextV1 = oasis7_agent_api::ContinuousAgentResponseContextV1<
    oasis7_simulator::Action,
    oasis7_simulator::AgentQuery,
>;
type FeedbackEnvelope = oasis7_agent_api::FeedbackEnvelope<oasis7_simulator::WorldEvent>;

use oasis7_observability::{emit_stderr_or_event, init_tracing, resolve_trace_session_id};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::{Level, error, info, warn};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:5841";
const DEFAULT_PROVIDER_AGENT_ID: &str = "main";
const DEFAULT_PROVIDER_THINKING: &str = "off";
const DEFAULT_PROVIDER_ID: &str = "provider_local_bridge";
const MOCK_PROVIDER_ID: &str = "provider_local_mock";
const DEFAULT_PROTOCOL_VERSION: &str = "world-simulator-provider-loopback-http-v1";
const MAX_RECENT_FEEDBACK: usize = 8;
const MAX_ACCEPTED_REQUESTS: usize = 64;
// Version 2 records the semantic response digest contract. State written by
// the old full-DecisionResponse digest contract is intentionally not migrated
// implicitly because its stored digest has no self-describing version.
const FEEDBACK_STATE_SCHEMA_VERSION: u16 = 2;
const FEEDBACK_STATE_PATH_ENV: &str = "OASIS7_PROVIDER_FEEDBACK_STATE_PATH";
const DEFAULT_PROVIDER_AGENT_PROFILE: &str = "oasis7_p0_low_freq_npc";
const TRACE_SESSION_PROCESS_LABEL: &str = "oasis7_provider_local_bridge";
const MIN_BRIDGE_AUTH_TOKEN_LEN: usize = 24;

fn default_provider_cli_bin() -> String {
    env::var("OASIS7_PROVIDER_CLI_BIN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ["open", "claw"].concat())
}

#[path = "oasis7_provider_local_bridge/auth_support.rs"]
mod auth_support;
use oasis7_letai_credit_adapter as credit_adapter;
#[path = "oasis7_provider_local_bridge/feedback_state.rs"]
mod feedback_state;
#[path = "oasis7_provider_local_bridge/http_bridge_support.rs"]
mod http_bridge_support;
#[path = "oasis7_provider_local_bridge/invocation.rs"]
mod invocation;
#[path = "oasis7_provider_local_bridge/letai_direct.rs"]
mod letai_direct;
#[path = "oasis7_provider_local_bridge/support.rs"]
mod support;
#[cfg(test)]
#[path = "oasis7_provider_local_bridge/tests.rs"]
mod tests;

use self::auth_support::{
    NewapiBridgeStateCache, load_cached_newapi_bridge_state, parse_newapi_bridge_bearer_selector,
};
#[path = "oasis7_provider_local_bridge/agent_decision.rs"]
mod agent_decision;
#[path = "oasis7_provider_local_bridge/options.rs"]
mod options;
use self::agent_decision::{
    apply_profile_guardrails, build_continuous_session_key, build_decision_prompt,
    build_gateway_agent_params, build_session_key, deterministic_mock_decision,
    legacy_model_output_error_response, parse_provider_output, provider_decision_label,
    strict_model_output_error_response, summarize_text, timeout_seconds_from_budget,
};
use self::feedback_state::load_feedback_state;
use self::http_bridge_support::handle_connection;
pub(crate) use self::invocation::{
    AgentInvocation, AgentInvocationOutput, AgentInvoker, ProviderCliInvoker,
    RustDirectLetaiInvoker, agent_chat_idempotency_key, build_agent_chat_prompt,
    provider_error_upstream_trace, short_sha256,
};
#[cfg(test)]
use self::letai_direct::{
    LetaiChatConfig, decode_letai_completion_payload, decode_letai_sse_reader,
    load_letai_chat_config, maybe_auto_topup_letai_user, quota_from_usd,
    should_auto_topup_letai_error,
};
use self::letai_direct::{error_diagnostics_json, invoke_rust_direct_letai};
use self::options::default_gateway_health_url;
use self::support::{
    agent_output_from_json, legacy_provider_invocation_key, local_session_id_from_session_key,
    should_fallback_to_local_agent,
};

#[derive(Debug, Clone)]
struct CliOptions {
    bind_addr: String,
    provider_cli_bin: String,
    provider_agent_id: String,
    provider_thinking: String,
    gateway_health_url: String,
    auth_token: Option<String>,
    auth_route_map: BTreeMap<String, String>,
    auth_route_from_bearer: bool,
    mode: ProviderMode,
    provider_backend: ProviderBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderMode {
    Real,
    Mock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderBackend {
    RustDirectLetai,
    LegacyCli,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            bind_addr: DEFAULT_BIND_ADDR.to_string(),
            provider_cli_bin: default_provider_cli_bin(),
            provider_agent_id: DEFAULT_PROVIDER_AGENT_ID.to_string(),
            provider_thinking: DEFAULT_PROVIDER_THINKING.to_string(),
            gateway_health_url: default_gateway_health_url(),
            auth_token: None,
            auth_route_map: BTreeMap::new(),
            auth_route_from_bearer: false,
            mode: ProviderMode::Real,
            provider_backend: ProviderBackend::RustDirectLetai,
        }
    }
}

#[derive(Debug, Clone)]
struct ProviderState {
    started_at: Instant,
    options: CliOptions,
    http: Client,
    active_requests: Arc<AtomicU64>,
    last_error: Arc<Mutex<Option<String>>>,
    recent_feedback: Arc<Mutex<BTreeMap<(String, String), FeedbackPartition>>>,
    accepted_requests: Arc<Mutex<BTreeMap<String, AcceptedRequestIdentity>>>,
    feedback_state_path: Option<PathBuf>,
    newapi_bridge_state_cache: Arc<Mutex<NewapiBridgeStateCache>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FeedbackPartition {
    next_seq: u64,
    digest_by_id: BTreeMap<String, Digest32>,
    digest_by_seq: BTreeMap<u64, (String, Digest32)>,
    held: BTreeMap<u64, FeedbackEnvelopeV1>,
    recent: VecDeque<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedRequestIdentity {
    agent_subject: String,
    agent_session_id: String,
    agent_turn_id: String,
    decision_request_id: String,
    request_digest: Digest32,
    response_digest: Digest32,
    #[serde(default)]
    accepted_order: u64,
}

struct ContinuousInvocationIdentity<'a> {
    provider_invocation_key: &'a str,
    agent_subject: &'a str,
    agent_session_id: &'a str,
}

impl ProviderState {
    fn new(options: CliOptions) -> Result<Self, String> {
        let feedback_state_path = env::var(FEEDBACK_STATE_PATH_ENV)
            .ok()
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty());
        Self::new_with_feedback_state_path(options, feedback_state_path)
    }

    fn new_with_feedback_state_path(
        options: CliOptions,
        feedback_state_path: Option<PathBuf>,
    ) -> Result<Self, String> {
        let http = Client::builder()
            .timeout(Duration::from_millis(1500))
            .build()
            .map_err(|err| format!("build provider http client failed: {err}"))?;
        let (accepted_requests, recent_feedback) =
            load_feedback_state(feedback_state_path.as_deref())?;
        Ok(Self {
            started_at: Instant::now(),
            options,
            http,
            active_requests: Arc::new(AtomicU64::new(0)),
            last_error: Arc::new(Mutex::new(None)),
            recent_feedback: Arc::new(Mutex::new(recent_feedback)),
            accepted_requests: Arc::new(Mutex::new(accepted_requests)),
            feedback_state_path,
            newapi_bridge_state_cache: Arc::new(Mutex::new(NewapiBridgeStateCache::default())),
        })
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            provider_id: self.provider_id().to_string(),
            name: Some(self.provider_name().to_string()),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            protocol_version: Some(DEFAULT_PROTOCOL_VERSION.to_string()),
            capabilities: self.provider_capabilities(),
            supported_action_sets: vec![
                "wait".to_string(),
                "wait_ticks".to_string(),
                "move_agent".to_string(),
                "speak_to_nearby".to_string(),
                "inspect_target".to_string(),
                "simple_interact".to_string(),
            ],
            chain_resource_manifest_schema_version: Some(
                oasis7_agent_api::world_resource::CHAIN_RESOURCE_MANIFEST_SCHEMA_V1.to_string(),
            ),
            chain_resource_delta_schema_version: Some(
                oasis7_agent_api::world_resource::CHAIN_RESOURCE_DELTA_SCHEMA_V1.to_string(),
            ),
        }
    }

    fn provider_id(&self) -> &'static str {
        match self.options.mode {
            ProviderMode::Real => DEFAULT_PROVIDER_ID,
            ProviderMode::Mock => MOCK_PROVIDER_ID,
        }
    }

    fn provider_name(&self) -> &'static str {
        match self.options.mode {
            ProviderMode::Real => "Local Provider Bridge",
            ProviderMode::Mock => "Local Mock Provider Bridge",
        }
    }

    fn provider_capabilities(&self) -> Vec<String> {
        let mut capabilities = vec![
            "decision".to_string(),
            "feedback".to_string(),
            "agent_chat".to_string(),
            "loopback_only".to_string(),
            "typed_module_command_v2".to_string(),
            "subject_bound_capability_catalog".to_string(),
            "continuous_agent_context_v1".to_string(),
            "feedback_sequence_v1".to_string(),
        ];
        match self.options.mode {
            ProviderMode::Real => {
                capabilities.push(format!("agent:{}", self.options.provider_agent_id));
            }
            ProviderMode::Mock => {
                capabilities.push("mock".to_string());
                capabilities.push("deterministic".to_string());
                capabilities.push("no_billing".to_string());
                capabilities.push("no_quota".to_string());
            }
        }
        capabilities
    }

    fn provider_health(&self) -> ProviderHealth {
        let active_requests = self.active_requests.load(Ordering::Relaxed);
        if self.options.mode == ProviderMode::Mock {
            self.set_last_error(None);
            return ProviderHealth {
                ok: true,
                status: Some("ready".to_string()),
                uptime_ms: Some(self.started_at.elapsed().as_millis() as u64),
                last_error: None,
                queue_depth: Some(active_requests),
            };
        }
        match self
            .http
            .get(self.options.gateway_health_url.as_str())
            .send()
        {
            Ok(response) => {
                let ok = response.status().is_success();
                if ok {
                    self.set_last_error(None);
                } else {
                    self.set_last_error(Some(format!(
                        "gateway health returned HTTP {}",
                        response.status().as_u16()
                    )));
                }
                ProviderHealth {
                    ok,
                    status: Some(if ok { "ok" } else { "degraded" }.to_string()),
                    uptime_ms: Some(self.started_at.elapsed().as_millis() as u64),
                    last_error: self.last_error.lock().expect("last_error lock").clone(),
                    queue_depth: Some(active_requests),
                }
            }
            Err(err) => {
                let detail = format!("provider_gateway_unreachable: {err}");
                self.set_last_error(Some(detail.clone()));
                ProviderHealth {
                    ok: false,
                    status: Some("degraded".to_string()),
                    uptime_ms: Some(self.started_at.elapsed().as_millis() as u64),
                    last_error: Some(detail),
                    queue_depth: Some(active_requests),
                }
            }
        }
    }

    fn record_feedback(&self, feedback: FeedbackEnvelope) {
        // The legacy route is deliberately fenced: it has no subject/session
        // lineage and must never contaminate another Agent's prompt.
        let _ = feedback;
    }

    fn record_continuous_feedback(&self, feedback: FeedbackEnvelopeV1) -> Result<(), String> {
        feedback_state::validate_feedback_contract(&feedback)?;
        let accepted_requests = self
            .accepted_requests
            .lock()
            .expect("accepted requests lock");
        let Some(accepted) = accepted_requests.get(&feedback.decision_request_id) else {
            return Err("unknown_feedback".to_string());
        };
        if accepted.agent_subject != feedback.agent_subject
            || accepted.agent_session_id != feedback.agent_session_id
            || accepted.agent_turn_id != feedback.agent_turn_id
        {
            return Err("feedback_correlation_mismatch".to_string());
        }
        if accepted.request_digest != feedback.request_digest {
            return Err("feedback_digest_mismatch".to_string());
        }
        if !accepted.response_digest.is_canonical_blake3() {
            return Err("feedback_contract_invalid".to_string());
        }
        let key = (
            feedback.agent_subject.clone(),
            feedback.agent_session_id.clone(),
        );
        let mut partitions = self.recent_feedback.lock().expect("recent_feedback lock");
        let previous_partitions = partitions.clone();
        if !partitions.contains_key(&key) && partitions.len() >= MAX_ACCEPTED_REQUESTS {
            Self::evict_feedback_partition(&mut partitions, &key);
        }
        let partition = partitions.entry(key).or_default();
        let expected = partition.next_seq.saturating_add(1);
        let digest = h_v1("oasis7.cognition.feedback.v1", &feedback);
        if let Some(previous) = partition.digest_by_id.get(&feedback.feedback_id) {
            if previous != &digest {
                return Err("feedback_id_conflict".to_string());
            }
            return Ok(());
        }
        if let Some(previous) = partition
            .held
            .values()
            .find(|previous| previous.feedback_id == feedback.feedback_id)
        {
            if h_v1("oasis7.cognition.feedback.v1", previous) != digest {
                return Err("feedback_id_conflict".to_string());
            }
            return Ok(());
        }
        if let Some(previous) = partition.held.get(&feedback.feedback_seq) {
            if h_v1("oasis7.cognition.feedback.v1", previous) != digest {
                return Err("feedback_identity_collision".to_string());
            }
            return Ok(());
        }
        if let Some((_, previous)) = partition.digest_by_seq.get(&feedback.feedback_seq) {
            if previous != &digest {
                return Err("feedback_identity_collision".to_string());
            }
            return Ok(());
        }
        if feedback.feedback_seq > expected {
            if partition.held.len() >= MAX_RECENT_FEEDBACK {
                return Err("feedback_sequence_overflow".to_string());
            }
            partition.held.insert(feedback.feedback_seq, feedback);
            if let Err(error) = feedback_state::persist_cognition_state(
                self.feedback_state_path.as_deref(),
                &accepted_requests,
                &partitions,
            ) {
                *partitions = previous_partitions;
                return Err(format!("feedback_state_persist_failed: {error}"));
            }
            return Err("feedback_sequence_gap".to_string());
        }
        if feedback.feedback_seq < expected {
            return Err("feedback_sequence_gap".to_string());
        }
        Self::remember_feedback(partition, &feedback, digest);
        while let Some(next) = partition.held.remove(&partition.next_seq.saturating_add(1)) {
            let next_digest = h_v1("oasis7.cognition.feedback.v1", &next);
            Self::remember_feedback(partition, &next, next_digest);
        }
        if let Err(error) = feedback_state::persist_cognition_state(
            self.feedback_state_path.as_deref(),
            &accepted_requests,
            &partitions,
        ) {
            *partitions = previous_partitions;
            return Err(format!("feedback_state_persist_failed: {error}"));
        }
        Ok(())
    }

    fn evict_feedback_partition(
        partitions: &mut BTreeMap<(String, String), FeedbackPartition>,
        preserve_key: &(String, String),
    ) {
        // BTreeMap order gives a deterministic bounded fallback when a new
        // subject/session partition arrives.  Preserve the active partition
        // so its accepted feedback is not immediately discarded.
        while partitions.len() >= MAX_ACCEPTED_REQUESTS {
            let Some(evicted_key) = partitions.keys().find(|key| *key != preserve_key).cloned()
            else {
                break;
            };
            partitions.remove(&evicted_key);
        }
    }

    fn remember_feedback(
        partition: &mut FeedbackPartition,
        feedback: &FeedbackEnvelopeV1,
        digest: Digest32,
    ) {
        partition.next_seq = feedback.feedback_seq;
        partition.digest_by_seq.insert(
            feedback.feedback_seq,
            (feedback.feedback_id.clone(), digest.clone()),
        );
        while partition.digest_by_seq.len() > MAX_RECENT_FEEDBACK {
            let Some(oldest_seq) = partition.digest_by_seq.keys().next().copied() else {
                break;
            };
            if let Some((oldest_id, _)) = partition.digest_by_seq.remove(&oldest_seq) {
                partition.digest_by_id.remove(&oldest_id);
            }
        }
        partition
            .digest_by_id
            .insert(feedback.feedback_id.clone(), digest);
        let summary = format!(
            "feedback_id={}; status={}; feedback_seq={}; action_id={}; receipt_id={}; reason={}",
            summarize_text(feedback.feedback_id.as_str(), 128),
            summarize_text(feedback.status.as_str(), 32),
            feedback.feedback_seq,
            feedback
                .candidate_action_id
                .map_or_else(|| "none".to_string(), |id| id.to_string()),
            summarize_text(
                feedback.runtime_receipt_id.as_deref().unwrap_or("none"),
                128,
            ),
            summarize_text(feedback.reject_reason.as_deref().unwrap_or("none"), 256),
        );
        if partition.recent.len() >= MAX_RECENT_FEEDBACK {
            partition.recent.pop_front();
        }
        partition.recent.push_back(summary);
    }

    fn handle_agent_chat(
        &self,
        request: ProviderAgentChatRequest,
        route_label: Option<&str>,
        invoker: &dyn AgentInvoker,
    ) -> Result<ProviderAgentChatResponse, String> {
        let agent_id = request.agent_id.trim();
        if agent_id.is_empty() {
            return Err("agent_chat requires non-empty agent_id".to_string());
        }
        let player_id = request.player_id.trim();
        if player_id.is_empty() {
            return Err("agent_chat requires non-empty player_id".to_string());
        }
        let message = request.message.trim();
        if message.is_empty() {
            return Err("agent_chat requires non-empty message".to_string());
        }
        let chat_request_key = provider_agent_chat_log_key(&request);
        let started = Instant::now();
        info!(
            target: "oasis7_provider_local_bridge",
            chat_request_key = chat_request_key.as_str(),
            agent_id,
            player_id,
            world_time = request.world_time,
            route_label_present = route_label.is_some(),
            backend = ?self.options.provider_backend,
            "provider agent chat request started"
        );
        if self.options.mode == ProviderMode::Mock {
            info!(
                target: "oasis7_provider_local_bridge",
                chat_request_key = chat_request_key.as_str(),
                agent_id,
                player_id,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "provider agent chat request succeeded"
            );
            return Ok(ProviderAgentChatResponse {
                agent_id: agent_id.to_string(),
                message: format!("[local-mock-chat] {agent_id} 收到：{message}"),
                location_id: request.location_id,
            });
        }

        self.active_requests.fetch_add(1, Ordering::SeqCst);
        let prompt = build_agent_chat_prompt(&request);
        let session_key = format!(
            "agent:{}:subagent:world-simulator-chat:{}",
            self.options.provider_agent_id, agent_id
        );
        let invoke_result = invoker.invoke(AgentInvocation {
            provider_cli_bin: self.options.provider_cli_bin.clone(),
            agent_id: self.options.provider_agent_id.clone(),
            thinking: self.options.provider_thinking.clone(),
            session_key: session_key.clone(),
            timeout_seconds: timeout_seconds_from_budget(60_000),
            prompt,
            idempotency_key: agent_chat_idempotency_key(&session_key, &request),
            provider_invocation_key: None,
            agent_session_id: None,
            chat_request_key: Some(chat_request_key.clone()),
            route_label: route_label.map(ToOwned::to_owned),
        });
        self.active_requests.fetch_sub(1, Ordering::SeqCst);

        match invoke_result {
            Ok(output) => {
                self.set_last_error(None);
                info!(
                    target: "oasis7_provider_local_bridge",
                    chat_request_key = chat_request_key.as_str(),
                    agent_id,
                    player_id,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    provider_version = output.provider_version.as_deref().unwrap_or("unknown"),
                    "provider agent chat request succeeded"
                );
                Ok(ProviderAgentChatResponse {
                    agent_id: agent_id.to_string(),
                    message: summarize_text(output.text.as_str(), 700),
                    location_id: request.location_id,
                })
            }
            Err(err) => {
                let detail = format!("provider_agent_chat_unreachable: {err}");
                self.set_last_error(Some(detail.clone()));
                warn!(
                    target: "oasis7_provider_local_bridge",
                    chat_request_key = chat_request_key.as_str(),
                    agent_id,
                    player_id,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    error = detail.as_str(),
                    "provider agent chat request failed"
                );
                Err(detail)
            }
        }
    }

    fn resolve_newapi_bridge_route_label(&self, bearer: &str) -> Option<String> {
        let normalized = bearer.trim();
        if normalized.is_empty() {
            return None;
        }
        let payload = self.load_newapi_bridge_state()?;
        let bindings = payload.get("bindings").and_then(Value::as_array)?;
        let project_bindings = payload.get("project_bindings").and_then(Value::as_array)?;
        let (by_ref, by_bridge_user_id) = parse_newapi_bridge_bearer_selector(normalized)?;
        bindings.iter().find_map(|entry| {
            let object = entry.as_object()?;
            if object.get("status").and_then(Value::as_str) != Some("active") {
                return None;
            }
            let ref_match = by_ref.is_some_and(|value| {
                object.get("newapi_user_ref").and_then(Value::as_str) == Some(value)
            });
            let user_match = by_bridge_user_id.is_some_and(|value| {
                object.get("bridge_user_id").and_then(Value::as_str) == Some(value)
            });
            let bridge_user_id = object.get("bridge_user_id").and_then(Value::as_str)?;
            let has_token = project_bindings.iter().any(|project| {
                let Some(project) = project.as_object() else {
                    return false;
                };
                project.get("bridge_user_id").and_then(Value::as_str) == Some(bridge_user_id)
                    && project
                        .get("token_key")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty())
            });
            if (ref_match || user_match) && has_token {
                Some(normalized.to_string())
            } else {
                None
            }
        })
    }

    fn load_newapi_bridge_state(&self) -> Option<Arc<Value>> {
        load_cached_newapi_bridge_state(&self.newapi_bridge_state_cache)
    }

    fn handle_decision(
        &self,
        request: DecisionRequest,
        route_label: Option<&str>,
        invoker: &dyn AgentInvoker,
    ) -> DecisionResponse {
        self.handle_decision_with_feedback(request, route_label, invoker, Vec::new(), None, false)
    }

    fn handle_continuous_decision(
        &self,
        context: ContinuousAgentRequestContextV1,
        route_label: Option<&str>,
        invoker: &dyn AgentInvoker,
    ) -> Result<ContinuousAgentResponseContextV1, String> {
        context
            .validate_production_lane()
            .map_err(|error| error.to_string())?;
        context
            .base_decision_request
            .validate_contract()
            .map_err(|error| error.to_string())?;
        if context.base_decision_request.observation.agent_id != context.agent_subject {
            return Err("request_context_identity_mismatch".to_string());
        }
        let key = (
            context.agent_subject.clone(),
            context.agent_session_id.clone(),
        );
        let recent = self
            .recent_feedback
            .lock()
            .expect("recent_feedback lock")
            .get(&key)
            .map(|partition| partition.recent.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let provider_invocation_key = context.provider_invocation_key();
        let base = self.handle_decision_with_feedback(
            context.base_decision_request.clone(),
            route_label,
            invoker,
            recent,
            Some(ContinuousInvocationIdentity {
                provider_invocation_key: provider_invocation_key.as_str(),
                agent_subject: context.agent_subject.as_str(),
                agent_session_id: context.agent_session_id.as_str(),
            }),
            true,
        );
        let response_digest = cognition_response_digest(&base);
        let response = ContinuousAgentResponseContextV1 {
            base_decision_response: base,
            context_discriminator: context.context_discriminator.clone(),
            context_version: context.context_version,
            agent_session_id: context.agent_session_id.clone(),
            agent_turn_id: context.agent_turn_id.clone(),
            decision_request_id: context.decision_request_id.clone(),
            retry_seq: context.retry_seq,
            transport_attempt: context.transport_attempt,
            request_digest: context.request_digest.clone(),
            response_digest,
        };
        feedback_state::remember_accepted_response(self, &context, &response)?;
        Ok(response)
    }

    fn handle_decision_with_feedback(
        &self,
        request: DecisionRequest,
        route_label: Option<&str>,
        invoker: &dyn AgentInvoker,
        recent_feedback: Vec<String>,
        invocation_identity: Option<ContinuousInvocationIdentity<'_>>,
        strict_target: bool,
    ) -> DecisionResponse {
        if let Err(err) = request.validate_contract() {
            self.set_last_error(Some(err.to_string()));
            return self.provider_error_response(err.code, err.message, false, None, None, None);
        }
        if let Some(err) = validate_profile(request.agent_profile.as_deref()) {
            self.set_last_error(Some(err.clone()));
            return self.provider_error_response(
                "unsupported_agent_profile",
                err,
                false,
                None,
                None,
                None,
            );
        }
        if self.options.mode == ProviderMode::Mock {
            return self.handle_mock_decision(request);
        }

        self.active_requests.fetch_add(1, Ordering::SeqCst);
        let started_at = Instant::now();
        let prompt = build_decision_prompt(&request, recent_feedback.as_slice());
        let session_key = invocation_identity
            .as_ref()
            .map(|identity| {
                build_continuous_session_key(
                    self.options.provider_agent_id.as_str(),
                    identity.agent_subject,
                    identity.agent_session_id,
                )
            })
            .unwrap_or_else(|| {
                build_session_key(&request, self.options.provider_agent_id.as_str())
            });
        let provider_invocation_key = invocation_identity
            .as_ref()
            .map(|identity| identity.provider_invocation_key.to_string());
        let agent_session_id = invocation_identity
            .as_ref()
            .map(|identity| identity.agent_session_id.to_string());
        let timeout_seconds = timeout_seconds_from_budget(request.timeout_budget_ms);
        let invoke_result = invoker.invoke(AgentInvocation {
            provider_cli_bin: self.options.provider_cli_bin.clone(),
            agent_id: self.options.provider_agent_id.clone(),
            thinking: self.options.provider_thinking.clone(),
            session_key: session_key.clone(),
            timeout_seconds,
            prompt,
            // The production outer route supplies the validated Runtime key;
            // only the explicitly fenced legacy route derives an inner key.
            idempotency_key: provider_invocation_key
                .clone()
                .unwrap_or_else(|| legacy_provider_invocation_key(&request)),
            provider_invocation_key,
            agent_session_id,
            chat_request_key: None,
            route_label: route_label.map(ToOwned::to_owned),
        });
        self.active_requests.fetch_sub(1, Ordering::SeqCst);
        let latency_ms = started_at.elapsed().as_millis() as u64;

        match invoke_result {
            Ok(output) => match parse_provider_output(
                request.observation.agent_id.as_str(),
                &request,
                output.text.as_str(),
                strict_target,
            ) {
                Ok((decision, schema_repair_count)) => {
                    let (decision, module_command) = match decision {
                        ProviderDecision::ModuleCommand { module_command } => {
                            (ProviderDecision::Wait, Some(module_command))
                        }
                        ProviderDecision::ModuleCommandResponse { response } => {
                            (ProviderDecision::ModuleCommandResponse { response }, None)
                        }
                        decision => (decision, None),
                    };
                    let (decision, guardrail_note) = apply_profile_guardrails(&request, decision);
                    self.set_last_error(None);
                    let mut tool_trace = Vec::new();
                    if let Some(route_note) = output.route_note.clone() {
                        tool_trace.push(route_note);
                    }
                    if let Some(note) = guardrail_note.clone() {
                        tool_trace.push(note);
                    }
                    DecisionResponse {
                        decision,
                        module_command,
                        provider_error: None,
                        diagnostics: ProviderDiagnostics {
                            provider_id: Some(self.provider_id().to_string()),
                            provider_version: output.provider_version.clone(),
                            latency_ms: Some(latency_ms.max(output.duration_ms.unwrap_or(0))),
                            retry_count: 0,
                        },
                        trace_payload: ProviderTraceEnvelope {
                            provider_id: Some(self.provider_id().to_string()),
                            input_summary: Some(summarize_text(output.prompt.as_str(), 512)),
                            output_summary: Some(match (&output.route_note, &guardrail_note) {
                                (Some(route_note), Some(note)) => format!(
                                    "{}; {}; model_output={}",
                                    route_note,
                                    note,
                                    summarize_text(output.text.as_str(), 512)
                                ),
                                (Some(route_note), None) => format!(
                                    "{}; model_output={}",
                                    route_note,
                                    summarize_text(output.text.as_str(), 512)
                                ),
                                (None, Some(note)) => format!(
                                    "{}; model_output={}",
                                    note,
                                    summarize_text(output.text.as_str(), 512)
                                ),
                                (None, None) => summarize_text(output.text.as_str(), 512),
                            }),
                            latency_ms: Some(latency_ms.max(output.duration_ms.unwrap_or(0))),
                            transcript: vec![
                                ProviderTranscriptEntry {
                                    role: "user".to_string(),
                                    content: summarize_text(output.prompt.as_str(), 4000),
                                },
                                ProviderTranscriptEntry {
                                    role: "assistant".to_string(),
                                    content: summarize_text(output.text.as_str(), 4000),
                                },
                            ],
                            tool_trace,
                            token_usage: Some(ProviderTokenUsage {
                                prompt_tokens: output.prompt_tokens,
                                completion_tokens: output.completion_tokens,
                                total_tokens: output.total_tokens,
                            }),
                            cost_cents: None,
                            upstream_trace: output.upstream_trace.clone(),
                            schema_repair_count,
                        },
                        memory_write_intents: Vec::new(),
                    }
                }
                Err(err) => {
                    if strict_target {
                        let detail =
                            format!("invalid_action_schema: bridge_model_output_invalid: {err}");
                        self.set_last_error(Some(detail.clone()));
                        return strict_model_output_error_response(
                            self.provider_id(),
                            &output,
                            latency_ms,
                            detail,
                        );
                    }
                    let detail = format!("bridge_model_output_invalid: {err}");
                    self.set_last_error(Some(detail.clone()));
                    legacy_model_output_error_response(
                        self.provider_id(),
                        &request,
                        &output,
                        latency_ms,
                        detail,
                    )
                }
            },
            Err(err) => {
                let detail = format!("provider_gateway_unreachable: {err}");
                self.set_last_error(Some(detail.clone()));
                self.provider_error_response(
                    "provider_gateway_unreachable",
                    detail,
                    true,
                    Some(latency_ms),
                    Some("decision invocation failed".to_string()),
                    Some(provider_error_upstream_trace(err.as_str())),
                )
            }
        }
    }

    fn handle_mock_decision(&self, request: DecisionRequest) -> DecisionResponse {
        self.set_last_error(None);
        let decision = deterministic_mock_decision(&request);
        let action_label = provider_decision_label(&decision);
        DecisionResponse {
            decision,
            module_command: None,
            provider_error: None,
            diagnostics: ProviderDiagnostics {
                provider_id: Some(self.provider_id().to_string()),
                provider_version: Some(format!("{}-mock", env!("CARGO_PKG_VERSION"))),
                latency_ms: Some(0),
                retry_count: 0,
            },
            trace_payload: ProviderTraceEnvelope {
                provider_id: Some(self.provider_id().to_string()),
                input_summary: Some(format!(
                    "deterministic_mock request agent={} world_time={}",
                    request.observation.agent_id, request.observation.world_time
                )),
                output_summary: Some(format!(
                    "deterministic_mock decision={action_label}; no_provider_cli=true; no_billing=true; no_quota=true"
                )),
                latency_ms: Some(0),
                transcript: Vec::new(),
                tool_trace: vec![
                    "deterministic_mock=no_provider_cli".to_string(),
                    "no_billing=true".to_string(),
                    "no_quota=true".to_string(),
                ],
                token_usage: None,
                cost_cents: None,
                upstream_trace: None,
                schema_repair_count: 0,
            },
            memory_write_intents: Vec::new(),
        }
    }

    fn provider_error_response(
        &self,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        latency_ms: Option<u64>,
        output_summary: Option<String>,
        upstream_trace: Option<Value>,
    ) -> DecisionResponse {
        DecisionResponse {
            decision: ProviderDecision::Wait,
            module_command: None,
            provider_error: Some(ProviderErrorEnvelope {
                code: code.into(),
                message: message.into(),
                retryable,
            }),
            diagnostics: ProviderDiagnostics {
                provider_id: Some(self.provider_id().to_string()),
                provider_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                latency_ms,
                retry_count: 0,
            },
            trace_payload: ProviderTraceEnvelope {
                provider_id: Some(self.provider_id().to_string()),
                input_summary: None,
                output_summary,
                latency_ms,
                transcript: Vec::new(),
                tool_trace: Vec::new(),
                token_usage: None,
                cost_cents: None,
                upstream_trace,
                schema_repair_count: 0,
            },
            memory_write_intents: Vec::new(),
        }
    }

    fn set_last_error(&self, detail: Option<String>) {
        *self.last_error.lock().expect("last_error lock") = detail;
    }
}

fn main() {
    init_tracing(TRACE_SESSION_PROCESS_LABEL);
    let trace_session_id = resolve_trace_session_id(TRACE_SESSION_PROCESS_LABEL);
    let args: Vec<String> = env::args().collect();
    let options = match parse_options(args.iter().skip(1).map(String::as_str)) {
        Ok(options) => options,
        Err(err) => {
            error!(trace_session_id = %trace_session_id, error = %err, "failed to parse provider local bridge options");
            print_help();
            std::process::exit(1);
        }
    };
    let state = match ProviderState::new(options.clone()) {
        Ok(state) => state,
        Err(err) => {
            error!(trace_session_id = %trace_session_id, error = %err, "failed to initialize provider local bridge state");
            std::process::exit(1);
        }
    };
    let listener = TcpListener::bind(options.bind_addr.as_str())
        .unwrap_or_else(|err| panic!("bind {} failed: {err}", options.bind_addr));
    info!(
        trace_session_id = %trace_session_id,
        bind_addr = %options.bind_addr,
        provider_agent_id = %options.provider_agent_id,
        gateway_health_url = %options.gateway_health_url,
        provider_backend = ?options.provider_backend,
        "provider local bridge listening"
    );
    println!(
        "oasis7_provider_local_bridge listening on http://{} (agent={}, gateway_health={})",
        options.bind_addr, options.provider_agent_id, options.gateway_health_url
    );
    let invoker: Arc<dyn AgentInvoker> = match options.provider_backend {
        ProviderBackend::RustDirectLetai => Arc::new(RustDirectLetaiInvoker),
        ProviderBackend::LegacyCli => Arc::new(ProviderCliInvoker),
    };
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let state = state.clone();
        let invoker = invoker.clone();
        std::thread::spawn(move || {
            if let Err(err) = handle_connection(&mut stream, &state, invoker.as_ref()) {
                let stderr_message =
                    format!("oasis7_provider_local_bridge connection error: {err}");
                emit_stderr_or_event(
                    Level::WARN,
                    stderr_message.as_str(),
                    "provider local bridge connection failed",
                );
            }
        });
    }
}

fn parse_options<'a>(args: impl Iterator<Item = &'a str>) -> Result<CliOptions, String> {
    let mut options = CliOptions::default();
    let mut iter = args.peekable();
    while let Some(arg) = iter.next() {
        match arg {
            "--bind" => {
                options.bind_addr = required_value(&mut iter, "--bind")?.to_string();
            }
            "--provider-cli-bin" => {
                options.provider_cli_bin =
                    required_value(&mut iter, "--provider-cli-bin")?.to_string();
                options.provider_backend = ProviderBackend::LegacyCli;
            }
            "--provider-backend" => {
                options.provider_backend =
                    parse_provider_backend(required_value(&mut iter, "--provider-backend")?)?;
            }
            "--provider-agent" => {
                options.provider_agent_id =
                    required_value(&mut iter, "--provider-agent")?.to_string();
            }
            "--provider-thinking" => {
                options.provider_thinking =
                    required_value(&mut iter, "--provider-thinking")?.to_string();
            }
            "--gateway-health-url" => {
                options.gateway_health_url =
                    required_value(&mut iter, "--gateway-health-url")?.to_string();
            }
            "--auth-token" => {
                options.auth_token = Some(required_value(&mut iter, "--auth-token")?.to_string());
            }
            "--auth-route-map" => {
                let path = required_value(&mut iter, "--auth-route-map")?;
                options.auth_route_map = load_auth_route_map(path)?;
            }
            "--auth-route-from-bearer" => {
                options.auth_route_from_bearer = true;
            }
            "--mode" => {
                options.mode = parse_provider_mode(required_value(&mut iter, "--mode")?)?;
            }
            "--mock" => {
                options.mode = ProviderMode::Mock;
            }
            "-h" | "--help" => return Err("help requested".to_string()),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if options.bind_addr.trim().is_empty() {
        return Err("--bind requires a non-empty value".to_string());
    }
    if let Some(token) = options.auth_token.as_deref()
        && token.trim().len() < MIN_BRIDGE_AUTH_TOKEN_LEN
    {
        return Err(format!(
            "--auth-token must be at least {MIN_BRIDGE_AUTH_TOKEN_LEN} characters"
        ));
    }
    Ok(options)
}

fn required_value<'a>(
    iter: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
    flag: &str,
) -> Result<&'a str, String> {
    iter.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn print_help() {
    eprintln!(concat!(
        "Usage: oasis7_provider_local_bridge [options]\n\n",
        "  --bind <host:port>            Loopback bind address (default: 127.0.0.1:5841)\n",
        "  --provider-backend <rust-direct-letai|legacy-cli>\n",
        "                                provider upstream backend (default: rust-direct-letai)\n",
        "  --provider-cli-bin <path>     legacy provider CLI path; implies --provider-backend legacy-cli\n",
        "  --provider-agent <id>         provider agent id (default: main)\n",
        "  --provider-thinking <level>   provider thinking level (default: off)\n",
        "  --gateway-health-url <url>    provider gateway health URL\n",
        "  --auth-token <token>          Optional single bearer token for bridge endpoints\n",
        "  --auth-route-map <path>       Optional JSON file mapping bearer token to route label\n",
        "  --auth-route-from-bearer      Treat request bearer token itself as route label\n",
        "  --mode <real|mock>            Bridge decision backend (default: real)\n",
        "  --mock                        Alias for --mode mock; deterministic no-billing local testing\n",
    ));
}

fn route_label_env(route_label: Option<&str>) -> Vec<(&'static str, String)> {
    vec![(
        "OASIS7_REMOTE_LLM_ROUTE_LABEL",
        route_label.unwrap_or_default().to_string(),
    )]
}

fn load_auth_route_map(path: &str) -> Result<BTreeMap<String, String>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("read auth route map `{path}` failed: {err}"))?;
    let parsed = serde_json::from_str::<BTreeMap<String, String>>(raw.as_str())
        .map_err(|err| format!("parse auth route map `{path}` failed: {err}"))?;
    let normalized = parsed
        .into_iter()
        .filter_map(|(token, route)| {
            let token = token.trim().to_string();
            let route = route.trim().to_string();
            if token.len() < MIN_BRIDGE_AUTH_TOKEN_LEN || route.is_empty() {
                None
            } else {
                Some((token, route))
            }
        })
        .collect::<BTreeMap<_, _>>();
    if normalized.is_empty() {
        return Err(format!(
            "auth route map `{path}` did not contain any usable entries"
        ));
    }
    Ok(normalized)
}

fn validate_profile(agent_profile: Option<&str>) -> Option<String> {
    let agent_profile = agent_profile
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    if matches!(agent_profile, DEFAULT_PROVIDER_AGENT_PROFILE) {
        None
    } else {
        Some(format!(
            "unsupported agent_profile `{agent_profile}`; expected {DEFAULT_PROVIDER_AGENT_PROFILE}"
        ))
    }
}

fn parse_provider_mode(raw: &str) -> Result<ProviderMode, String> {
    match raw.trim() {
        "real" | "provider" | "provider_cli" => Ok(ProviderMode::Real),
        "mock" | "deterministic" | "local_mock" | "local-mock" => Ok(ProviderMode::Mock),
        other => Err(format!(
            "unsupported provider bridge mode `{other}`; expected real or mock"
        )),
    }
}

fn parse_provider_backend(raw: &str) -> Result<ProviderBackend, String> {
    match raw.trim() {
        "rust-direct-letai" | "rust_direct_letai" | "direct-letai" | "direct" | "rust" => {
            Ok(ProviderBackend::RustDirectLetai)
        }
        "legacy-cli" | "legacy_cli" | "cli" | "provider-cli" => Ok(ProviderBackend::LegacyCli),
        other => Err(format!(
            "unsupported provider backend `{other}`; expected rust-direct-letai or legacy-cli"
        )),
    }
}
