use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use oasis7_wasm_abi::{AgentCommandResponse, CapabilityCatalogSnapshot, ModuleCommandCatalogEntry};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION: &str = "oc_dual_obs_v1";
pub const DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION: &str = "oc_dual_act_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderExecutionMode {
    PlayerParity,
    #[default]
    HeadlessAgent,
}

impl ProviderExecutionMode {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "player_parity" | "player-parity" | "player" => Some(Self::PlayerParity),
            "headless_agent" | "headless-agent" | "headless" => Some(Self::HeadlessAgent),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlayerParity => "player_parity",
            Self::HeadlessAgent => "headless_agent",
        }
    }
}

fn default_provider_execution_mode() -> ProviderExecutionMode {
    ProviderExecutionMode::HeadlessAgent
}

fn default_observation_schema_version() -> String {
    DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION.to_string()
}

fn default_action_schema_version() -> String {
    DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCatalogEntry {
    pub action_ref: String,
    pub summary: String,
}

impl ActionCatalogEntry {
    pub fn new(action_ref: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            action_ref: action_ref.into(),
            summary: summary.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderSelfState {
    pub location_ref: String,
    pub pose_hint: String,
    #[serde(default)]
    pub status_flags: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_summary: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderMissionContext {
    pub goal_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderNearbyEntity {
    pub entity_ref: String,
    pub kind: String,
    pub relation: String,
    pub relative_hint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRecentEvent {
    pub event_ref: String,
    pub kind: String,
    pub summary: String,
    pub age_ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderNavigationNode {
    pub node_ref: String,
    pub relation: String,
    pub relative_hint: String,
    pub traversable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderInteractionTarget {
    pub target_ref: String,
    pub target_kind: String,
    pub interaction_hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderObservation {
    pub self_state: ProviderSelfState,
    pub mission_context: ProviderMissionContext,
    #[serde(default)]
    pub nearby_entities: Vec<ProviderNearbyEntity>,
    #[serde(default)]
    pub recent_events: Vec<ProviderRecentEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_navigation_graph: Vec<ProviderNavigationNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hazard_summary: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interaction_targets: Vec<ProviderInteractionTarget>,
}

impl ProviderObservation {
    fn validate_for_mode(
        &self,
        mode: ProviderExecutionMode,
    ) -> Result<(), DecisionRequestContractError> {
        if matches!(mode, ProviderExecutionMode::PlayerParity)
            && (!self.local_navigation_graph.is_empty()
                || !self.hazard_summary.is_empty()
                || !self.interaction_targets.is_empty())
        {
            return Err(DecisionRequestContractError::new(
                "mode_observation_mismatch",
                "player_parity observation cannot include headless-only navigation, hazard, or interaction target helpers",
            ));
        }
        Ok(())
    }
}

/// Provider-facing observation projection. `world_time` intentionally stays
/// a primitive scalar so this crate does not import simulator implementation
/// types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationEnvelope {
    pub agent_id: String,
    pub world_time: u64,
    #[serde(default = "default_provider_execution_mode")]
    pub mode: ProviderExecutionMode,
    #[serde(default = "default_observation_schema_version")]
    pub observation_schema_version: String,
    #[serde(default = "default_action_schema_version")]
    pub action_schema_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    pub observation: ProviderObservation,
    #[serde(default)]
    pub recent_event_summary: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_summary: Option<String>,
    #[serde(default)]
    pub action_catalog: Vec<ActionCatalogEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub module_command_catalog: Vec<ModuleCommandCatalogEntry>,
    pub timeout_budget_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub observation: ObservationEnvelope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_config_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_id: Option<String>,
    /// Runtime-produced capability discovery. It carries no authority by
    /// itself; the Runtime revalidates before executing a response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_catalog: Option<CapabilityCatalogSnapshot>,
    /// Host-bound invocation identity, not a bearer grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_invocation_context: Option<CapabilityInvocationContext>,
    pub timeout_budget_ms: u64,
}

impl DecisionRequest {
    pub fn validate_contract(&self) -> Result<(), DecisionRequestContractError> {
        if self.observation.observation_schema_version
            != DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION
        {
            return Err(DecisionRequestContractError::new(
                "unsupported_schema_version",
                format!(
                    "unsupported observation_schema_version `{}`; expected {}",
                    self.observation.observation_schema_version,
                    DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION
                ),
            ));
        }
        if self.observation.action_schema_version != DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION {
            return Err(DecisionRequestContractError::new(
                "unsupported_schema_version",
                format!(
                    "unsupported action_schema_version `{}`; expected {}",
                    self.observation.action_schema_version, DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION
                ),
            ));
        }
        if self.capability_catalog.is_some() != self.capability_invocation_context.is_some() {
            return Err(DecisionRequestContractError::new(
                "incomplete_capability_context",
                "capability_catalog and capability_invocation_context must be supplied together",
            ));
        }
        if let Some(catalog) = self.capability_catalog.as_ref() {
            catalog.validate().map_err(|error| {
                DecisionRequestContractError::new("invalid_capability_context", error.to_string())
            })?;
            let invocation = self
                .capability_invocation_context
                .as_ref()
                .expect("capability context pair checked above");
            if catalog.snapshot_id != invocation.catalog_snapshot_id
                || catalog.subject != invocation.subject
                || catalog.presenter != invocation.presenter
            {
                return Err(DecisionRequestContractError::new(
                    "capability_context_mismatch",
                    "capability catalog and invocation context are not bound to the same snapshot",
                ));
            }
            if invocation.grant_id.trim().is_empty()
                || invocation.catalog_snapshot_id.trim().is_empty()
                || invocation.response_nonce.trim().is_empty()
            {
                return Err(DecisionRequestContractError::new(
                    "invalid_capability_context",
                    "capability invocation grant, snapshot, and response nonce are required",
                ));
            }
        }
        self.observation
            .observation
            .validate_for_mode(self.observation.mode)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRequestContractError {
    pub code: String,
    pub message: String,
}

impl DecisionRequestContractError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for DecisionRequestContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for DecisionRequestContractError {}

/// Serialization-only invocation context; the Runtime owns grants and
/// revocation checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityInvocationContext {
    pub grant_id: String,
    pub subject: oasis7_wasm_abi::CapabilitySubject,
    pub presenter: oasis7_wasm_abi::CapabilityPresenter,
    pub audience: oasis7_wasm_abi::CapabilityAudience,
    pub catalog_snapshot_id: String,
    pub module_id: String,
    pub module_version: String,
    pub response_nonce: String,
}

/// A module command proposal is not an executable capability grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderModuleCommand {
    pub module_id: String,
    pub module_version: String,
    pub namespace: String,
    pub name: String,
    pub schema_version: u32,
    pub schema_hash: String,
    pub payload: Vec<u8>,
}

/// Provider decision uses the exact established serde tag and variant fields,
/// while making game-specific action/query payloads generic. No conversion to
/// untyped JSON is required at the API boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "Preserves direct typed command payloads on the established wire shape."
)]
pub enum ProviderDecision<A = Value, Q = Value> {
    Wait,
    WaitTicks {
        ticks: u64,
    },
    Act {
        action_ref: String,
        action: A,
    },
    Query {
        query_ref: String,
        query: Q,
    },
    ModuleCommand {
        module_command: ProviderModuleCommand,
    },
    ModuleCommandResponse {
        response: AgentCommandResponse,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderErrorEnvelope {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderTokenUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTranscriptEntry {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProviderTraceEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub transcript: Vec<ProviderTranscriptEntry>,
    #[serde(default)]
    pub tool_trace: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<ProviderTokenUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_cents: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_trace: Option<Value>,
    #[serde(default)]
    pub schema_repair_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderDiagnostics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub retry_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteIntent {
    pub scope: String,
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionResponse<A = Value, Q = Value> {
    pub decision: ProviderDecision<A, Q>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_command: Option<ProviderModuleCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_error: Option<ProviderErrorEnvelope>,
    #[serde(default)]
    pub diagnostics: ProviderDiagnostics,
    #[serde(default)]
    pub trace_payload: ProviderTraceEnvelope,
    #[serde(default)]
    pub memory_write_intents: Vec<MemoryWriteIntent>,
}

impl<A, Q> DecisionResponse<A, Q> {
    pub fn wait(provider_id: impl Into<String>) -> Self {
        let provider_id = provider_id.into();
        Self {
            decision: ProviderDecision::Wait,
            module_command: None,
            provider_error: None,
            diagnostics: ProviderDiagnostics {
                provider_id: Some(provider_id.clone()),
                ..ProviderDiagnostics::default()
            },
            trace_payload: ProviderTraceEnvelope {
                provider_id: Some(provider_id),
                output_summary: Some("decision=wait".to_string()),
                ..ProviderTraceEnvelope::default()
            },
            memory_write_intents: Vec::new(),
        }
    }
}

/// Legacy feedback transport. The host owns projection of action results and
/// world events; consumers select their event representation without importing
/// a simulator implementation into the wire contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(bound(deserialize = "E: Deserialize<'de>"))]
pub struct FeedbackEnvelope<E = Value> {
    pub action_id: u64,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
    #[serde(default)]
    pub emitted_events: Vec<E>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_delta_summary: Option<String>,
}
