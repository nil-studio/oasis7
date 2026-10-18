use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::super::location_id_for_pos;
use crate::geometry::GeoPos;
use crate::runtime::{
    Action as RuntimeAction, AgentIntentV2, DomainEvent as RuntimeDomainEvent, ModuleSourcePackage,
    SchedulerWakeV1, World as RuntimeWorld, WorldEvent as RuntimeWorldEvent,
    WorldEventBody as RuntimeWorldEventBody,
};
#[cfg(target_arch = "wasm32")]
use crate::simulator::AgentRunner;
#[cfg(not(target_arch = "wasm32"))]
use crate::simulator::AsyncAgentRunner;
use crate::simulator::{
    ActionCatalogEntry, AgentDecision, AgentDecisionTrace,
    ContinuationProposalV1 as SimulatorContinuationProposalV1, ContinuousAgentResponseContextV1,
    LlmAgentBehavior, LlmAgentConfig, MemoryWriteStore, OpenAiChatCompletionClient,
    ProviderAgentChatRequest, ProviderBackedAgentBehavior, ProviderExecutionMode,
    ProviderLoopbackAdapter, ProviderLoopbackHttpClient, RuntimeBindingV1, WorldEvent, WorldKernel,
    evaluate_provider_compatibility, provider_agent_chat_log_key,
};
use crate::viewer::live::ViewerLiveDecisionMode;
use crate::viewer::protocol::{AgentChatAck, AgentChatError};
use oasis7_simulator::{
    Action as SimulatorAction, AgentPromptProfile, ChunkRuntimeConfig, Location, ResourceOwner,
    WorldConfig, WorldEventKind, WorldModel,
};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimePendingAction {
    pub(super) agent_id: String,
    pub(super) action: SimulatorAction,
    pub(super) cognition: Option<RuntimeProviderActionContext>,
    /// Runtime may emit an accepted event before its terminal receipt. Keep
    /// the pending action correlated while ensuring legacy action feedback is
    /// delivered at most once.
    pub(super) feedback_emitted: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(in crate::viewer::runtime_live) struct RuntimeProviderActionContext {
    pub(in crate::viewer::runtime_live) request: cognition_context::ProviderContextState,
    pub(in crate::viewer::runtime_live) response: ContinuousAgentResponseContextV1,
    /// Runtime's durable admission lease remains correlated with the provider
    /// response until the Runtime receipt or terminal disposition closes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::viewer::runtime_live) cognition_lease: Option<crate::runtime::CognitionLeaseV1>,
    /// Keep provider memory intents correlated with the response until the
    /// Runtime receipt/readback authorizes their durable projection.
    #[serde(default)]
    pub(in crate::viewer::runtime_live) memory_write_intents:
        Vec<crate::simulator::MemoryWriteIntent>,
}
const BUILTIN_LLM_DECISION_SOURCE: &str = "builtin_llm";
const PROVIDER_BACKED_DECISION_SOURCE: &str = "provider_backed";
const PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION: &str = "provider_loopback_http";
const LOCAL_BRIDGE_PROVIDER_BACKEND: &str = "provider_local_bridge";
const LOCAL_MOCK_PROVIDER_BACKEND: &str = "provider_local_mock";
const WORLDSIM_PROVIDER_CONTRACT: &str = "worldsim_provider_v1";
const LOOPBACK_HTTP_PROVIDER_TRANSPORT: &str = "loopback_http";
const REMOTE_HTTPS_PROVIDER_TRANSPORT: &str = "remote_https";
const AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS: &str = "agent_direct_connect";
const DEFAULT_PROVIDER_CONNECT_TIMEOUT_MS: u64 = 3_000;
const DEFAULT_PROVIDER_AGENT_PROFILE: &str = "oasis7_p0_low_freq_npc";
const VIEWER_AGENT_DECISION_SOURCE_ENV: &str = "OASIS7_AGENT_DECISION_SOURCE";
const VIEWER_AGENT_PROVIDER_BACKEND_ENV: &str = "OASIS7_AGENT_PROVIDER_BACKEND";
const VIEWER_AGENT_PROVIDER_CONTRACT_ENV: &str = "OASIS7_AGENT_PROVIDER_CONTRACT";
const VIEWER_AGENT_PROVIDER_TRANSPORT_ENV: &str = "OASIS7_AGENT_PROVIDER_TRANSPORT";
const VIEWER_AGENT_PROVIDER_URL_ENV: &str = "OASIS7_AGENT_PROVIDER_URL";
const VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV: &str = "OASIS7_AGENT_PROVIDER_AUTH_TOKEN";
const VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV: &str =
    "OASIS7_AGENT_PROVIDER_CONNECT_TIMEOUT_MS";
const VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV: &str =
    "OASIS7_AGENT_PROVIDER_DECISION_TIMEOUT_MS";
const VIEWER_AGENT_PROVIDER_PROFILE_ENV: &str = "OASIS7_AGENT_PROVIDER_PROFILE";
const VIEWER_AGENT_EXECUTION_LANE_ENV: &str = "OASIS7_AGENT_EXECUTION_LANE";
const VIEWER_AGENT_PROVIDER_MODE_ENV: &str = "OASIS7_AGENT_PROVIDER_MODE";
const RUNTIME_PROVIDER_CHECK_CACHE_MS: u64 = 2_000;
const MAX_PROVIDER_TRANSPORT_ATTEMPTS: u64 = 2;
const PROVIDER_AGENT_CHAT_CAPABILITY: &str = "agent_chat";
const ENV_RUNTIME_LIVE_LLM_TIMEOUT_MS: &str = "OASIS7_RUNTIME_LIVE_LLM_TIMEOUT_MS";
const DEFAULT_RUNTIME_LIVE_LLM_TIMEOUT_MS: u64 = 30_000;
const LOCAL_TEST_PLAYER_ID_PREFIX: &str = "local-test-player-";

#[path = "llm_sidecar_agent_chat.rs"]
mod agent_chat_support;
#[path = "llm_sidecar_async.rs"]
mod async_support;
#[path = "llm_sidecar_chat.rs"]
mod chat_support;
#[path = "llm_sidecar_cognition.rs"]
mod cognition_context;
#[path = "llm_sidecar_continuation.rs"]
mod continuation_support;
#[path = "llm_sidecar_decision.rs"]
mod decision;
#[path = "llm_sidecar_hosted_outcome.rs"]
mod hosted_outcome;
#[path = "llm_sidecar_initialization.rs"]
mod initialization;
#[path = "llm_sidecar_lease_continuity.rs"]
mod lease_continuity;
#[path = "llm_sidecar_lineage.rs"]
mod lineage;
#[path = "llm_sidecar_lineage_generation_recovery.rs"]
mod lineage_generation_recovery;
#[path = "llm_sidecar_lineage_persistence.rs"]
pub(in crate::viewer::runtime_live) mod lineage_persistence;
#[path = "llm_sidecar_lineage_recovery.rs"]
mod lineage_recovery;
#[path = "llm_sidecar_lineage_settlement.rs"]
mod lineage_settlement;
#[path = "llm_sidecar_metadata_probe.rs"]
mod metadata_probe;
#[path = "llm_sidecar_payer.rs"]
mod payer_support;
#[path = "llm_sidecar_provider.rs"]
mod provider_support;
#[path = "llm_sidecar_release_recovery.rs"]
mod release_recovery;
#[path = "llm_sidecar_restored_memory.rs"]
mod restored_memory;
#[path = "llm_sidecar_runner.rs"]
mod runner_init;
#[path = "llm_sidecar_runtime_feedback.rs"]
mod runtime_feedback;
#[path = "llm_sidecar_runtime_support.rs"]
mod runtime_support;
#[path = "llm_sidecar_scheduler_checkpoint.rs"]
mod scheduler_checkpoint;
#[path = "llm_sidecar_stale.rs"]
mod stale_replan;
#[path = "llm_sidecar_world_events.rs"]
mod world_events;
use self::agent_chat_support::{
    RuntimeProviderAgentChatFailure, RuntimeProviderAgentChatRequestError,
};
pub(in crate::viewer::runtime_live) use self::async_support::runtime_provider_context_digest;
use self::async_support::{RuntimeLlmDecision, provider_trace_retryable};
pub(in crate::viewer::runtime_live) use self::provider_support::hosted_local_mock_test_lane_enabled;
pub(in crate::viewer::runtime_live) use self::provider_support::install_hosted_local_mock_test_capability_fixtures;
pub(in crate::viewer::runtime_live) use self::provider_support::provider_settings_from_env;
use self::provider_support::{
    env_requests_provider_backend, provider_phase1_action_catalog, provider_phase1_memory_summary,
};
use self::runtime_support::{
    hash_chat_message, normalize_optional_public_key, restore_behavior_long_term_memory_from_model,
    runtime_provider_check_cache_key, runtime_provider_check_now_unix_ms,
};
pub(in crate::viewer::runtime_live) use self::runtime_support::{
    resolve_runtime_live_llm_timeout_ms, simulator_action_label, simulator_action_to_runtime,
};
use self::stale_replan::{
    MAX_PROVIDER_STALE_REPLANS, ProviderStaleReplanCause, ProviderStaleReplanState,
};
use self::world_events::RuntimePendingProviderWorldEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::viewer::runtime_live) struct ProviderDecisionSettings {
    pub(in crate::viewer::runtime_live) requested_provider_mode: String,
    pub(in crate::viewer::runtime_live) provider_transport: String,
    pub(in crate::viewer::runtime_live) base_url: String,
    pub(in crate::viewer::runtime_live) auth_token: Option<String>,
    pub(in crate::viewer::runtime_live) connect_timeout_ms: u64,
    pub(in crate::viewer::runtime_live) decision_timeout_ms: u64,
    pub(in crate::viewer::runtime_live) agent_profile: String,
    pub(in crate::viewer::runtime_live) execution_mode: ProviderExecutionMode,
    pub(in crate::viewer::runtime_live) fallback_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::viewer::runtime_live) struct RuntimeProviderCheckSnapshot {
    pub(in crate::viewer::runtime_live) source: String,
    pub(in crate::viewer::runtime_live) status: String,
    pub(in crate::viewer::runtime_live) capabilities: Vec<String>,
    pub(in crate::viewer::runtime_live) supported_action_sets: Vec<String>,
    pub(in crate::viewer::runtime_live) fallback_reason: Option<String>,
    pub(in crate::viewer::runtime_live) error: Option<String>,
    checked_at_unix_ms: u64,
    cache_key: String,
}

enum RuntimeDecisionRunner {
    #[cfg(not(target_arch = "wasm32"))]
    Builtin(AsyncAgentRunner),
    #[cfg(target_arch = "wasm32")]
    Builtin(AgentRunner<LlmAgentBehavior<OpenAiChatCompletionClient>>),
    #[cfg(not(target_arch = "wasm32"))]
    ProviderBacked(AsyncAgentRunner),
    #[cfg(target_arch = "wasm32")]
    ProviderBacked(AgentRunner<ProviderBackedAgentBehavior<ProviderLoopbackAdapter>>),
}

#[cfg(not(target_arch = "wasm32"))]
impl RuntimeDecisionRunner {
    fn async_runner(&self) -> Option<&AsyncAgentRunner> {
        match self {
            Self::Builtin(runner) | Self::ProviderBacked(runner) => Some(runner),
        }
    }

    fn async_runner_mut(&mut self) -> Option<&mut AsyncAgentRunner> {
        match self {
            Self::Builtin(runner) | Self::ProviderBacked(runner) => Some(runner),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(in crate::viewer::runtime_live) struct RuntimeChatIntentAckRecord {
    ack: AgentChatAck,
    message_hash: String,
    public_key: Option<String>,
    intent_tick: Option<u64>,
}

#[derive(Clone, Debug)]
pub(super) struct RuntimePendingProviderAgentChat {
    pub(super) agent_id: String,
    player_id: String,
    message: String,
    /// Durable runtime identity for the request that caused this provider call.
    /// Legacy callers may leave these absent; the V2 integration must use the
    /// identity-aware enqueue method.
    pub(super) intent_id: Option<String>,
    pub(super) request_digest: Option<String>,
}

fn runtime_live_phase1_short_term_goal() -> String {
    concat!(
        "post_onboarding.establish_first_capability 阶段，建设可持续工业线。 ",
        "开局种子资源已足够首个 smelter：若 self_resources.electricity>=10 且 data>=5 且还没有 factory.smelter.mk1，",
        "优先 build_factory(factory.smelter.mk1)，不要先 harvest_radiation。 ",
        "只有在 electricity<10 时才先 harvest_radiation；只有在 data<5 时才先 mine_compound/refine_compound。 ",
        "smelter 建好后立刻 schedule_recipe(recipe.smelter.iron_ingot 或其他首批 smelter 配方)，避免 wait。"
    )
    .to_string()
}

pub(in crate::viewer::runtime_live) struct RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) decision_mode: ViewerLiveDecisionMode,
    pub(in crate::viewer::runtime_live) prompt_profiles: BTreeMap<String, AgentPromptProfile>,
    pub(in crate::viewer::runtime_live) prompt_profile_history:
        BTreeMap<String, BTreeMap<u64, AgentPromptProfile>>,
    pub(in crate::viewer::runtime_live) agent_player_bindings: BTreeMap<String, String>,
    pub(in crate::viewer::runtime_live) player_agent_bindings: BTreeMap<String, String>,
    pub(in crate::viewer::runtime_live) agent_public_key_bindings: BTreeMap<String, String>,
    pub(in crate::viewer::runtime_live) player_auth_last_nonce: BTreeMap<String, u64>,
    pub(in crate::viewer::runtime_live) player_chat_intent_acks:
        BTreeMap<(String, String, u64), RuntimeChatIntentAckRecord>,
    pub(in crate::viewer::runtime_live) primary_intents:
        BTreeMap<String, super::agent_chat_intent::RuntimePrimaryIntent>,
    llm_decision_mailbox: u64,
    runner: Option<RuntimeDecisionRunner>,
    shadow_kernel: Option<WorldKernel>,
    pending_actions: BTreeMap<u64, RuntimePendingAction>,
    pending_provider_agent_chats: VecDeque<RuntimePendingProviderAgentChat>,
    provider_check_snapshot: Option<RuntimeProviderCheckSnapshot>,
    pub(in crate::viewer::runtime_live) hosted_service_phase:
        Option<crate::viewer::runtime_live::agent_service_phase::HostedServicePhase>,
    pub(in crate::viewer::runtime_live) hosted_service_inflight:
        Option<crate::viewer::runtime_live::agent_service_io::AgentServiceIoToken>,
    pub(in crate::viewer::runtime_live) hosted_service_generation: u64,
    pub(in crate::viewer::runtime_live) hosted_service_config_binding: Option<String>,
    #[cfg(any(test, feature = "test_tier_required"))]
    pub(in crate::viewer::runtime_live) hosted_service_memory_failure: Option<&'static str>,
    provider_metadata_generation: u64,
    provider_metadata_inflight: Option<(String, u64)>,
    runtime_seed_locations: Vec<Location>,
    runtime_seed_model: Option<WorldModel>,
    pub(in crate::viewer::runtime_live) chunk_runtime: ChunkRuntimeConfig,
    provider_session_ids: BTreeMap<String, String>,
    provider_agent_ids: BTreeSet<String>,
    hosted_local_mock_test_lane: bool,
    #[cfg(any(test, feature = "test_tier_required"))]
    service_test_model_calls: BTreeMap<
        String,
        std::sync::Arc<std::sync::Mutex<crate::simulator::MockDecisionProviderState>>,
    >,
    #[cfg(any(test, feature = "test_tier_required"))]
    service_test_actor_agents: BTreeSet<String>,
    provider_context_seq: BTreeMap<String, u64>,
    provider_contexts: BTreeMap<String, cognition_context::ProviderContextState>,
    provider_retry_contexts: BTreeMap<String, cognition_context::ProviderContextState>,
    provider_active_turns: BTreeMap<String, cognition_context::ProviderContextState>,
    /// The admission lease for an in-flight provider request. This is kept
    /// separately from the response because provider failures have no response
    /// envelope, while recovery still needs the exact lease identity.
    provider_cognition_leases: BTreeMap<String, crate::runtime::CognitionLeaseV1>,
    /// Runtime-authorized owner/generation identity used by the provider
    /// capability subject. RuntimeBindingV1 does not include this identity,
    /// so it must be persisted separately to fence a rotated Agent after
    /// restart before a fresh grant is selected.
    provider_capability_identities: BTreeMap<String, crate::runtime::CapabilityAgentIdentity>,
    /// Exact simulator proposals admitted into the Harness for Runtime-owned
    /// continuations.  Runtime projections alone cannot recreate the
    /// provider-side chain identity after restart.
    provider_continuation_proposals: BTreeMap<String, SimulatorContinuationProposalV1>,
    /// A continuation hydration mismatch fences only the affected Agent until
    /// its authoritative lineage is repaired; it must not fall back to a new
    /// provider turn or mutate Runtime state.
    provider_continuation_recovery_pending: BTreeMap<String, String>,
    /// Durable active markers without matching contexts fence recovery and
    /// prevent restart from allocating a duplicate provider turn.
    provider_recovery_pending: BTreeMap<String, lineage_persistence::ProviderRecoveryPending>,
    /// A Runtime wake handoff may fail after the provider action is terminal.
    /// Retain the exact context and disposition until Runtime accepts that
    /// wake so retry cannot allocate a duplicate provider turn.
    provider_wake_recovery_pending:
        BTreeMap<String, lineage_persistence::ProviderWakeRecoveryPending>,
    pub(in crate::viewer::runtime_live) provider_service_pending:
        BTreeMap<String, lineage_persistence::PendingProviderServiceIntent>,
    pub(in crate::viewer::runtime_live) hosted_wait: Option<cognition_context::HostedWait>,
    pub(in crate::viewer::runtime_live) hosted_restored_wait: Option<(
        cognition_context::HostedWait,
        Option<lineage_persistence::PendingProviderSchedulerIntent>,
    )>,
    pub(in crate::viewer::runtime_live) hosted_restored_wait_decisions:
        BTreeMap<String, async_support::RuntimeLlmDecision>,
    pub(in crate::viewer::runtime_live) hosted_resume: Option<cognition_context::HostedResume>,
    pub(in crate::viewer::runtime_live) hosted_restored_resume: Option<(
        cognition_context::HostedResume,
        Option<lineage_persistence::PendingProviderSchedulerIntent>,
    )>,
    pub(in crate::viewer::runtime_live) hosted_fresh_view_ready: bool,
    pub(in crate::viewer::runtime_live) hosted_admission:
        Option<cognition_context::HostedAdmission>,
    pub(in crate::viewer::runtime_live) provider_restored_service_checkpoints:
        BTreeMap<String, lineage_persistence::PendingProviderServiceIntent>,
    pub(in crate::viewer::runtime_live) provider_scheduler_pending:
        BTreeMap<String, lineage_persistence::PendingProviderSchedulerIntent>,
    pub(in crate::viewer::runtime_live) provider_service_config:
        Option<crate::world_service::client::WorldServiceClientConfig>,
    pub(in crate::viewer::runtime_live) provider_service_query_state:
        crate::world_service::client::WorldServiceQueryState,
    pub(in crate::viewer::runtime_live) provider_service_required: bool,
    pub(in crate::viewer::runtime_live) provider_service_lineage_store_explicit: bool,
    pub(in crate::viewer::runtime_live) canonical_owner_goal_commit:
        Option<oasis7_client_api::world_service::CommitRef>,
    // Execution-only authenticated owner goal; never serialized into a Viewer frame.
    pub(in crate::viewer::runtime_live) canonical_owner_goal:
        Option<oasis7_proto::viewer::CanonicalAgentChatViewV1>,
    pub(in crate::viewer::runtime_live) canonical_goal_prompt_applied:
        Option<(String, Option<String>)>,
    pub(in crate::viewer::runtime_live) provider_service_projection:
        Option<crate::world_service::projection::WorldServiceProjection>,
    pub(in crate::viewer::runtime_live) provider_service_signer:
        Option<crate::world_service::client::WorldServiceAgentSignerConfig>,
    provider_wait_until: BTreeMap<String, u64>,
    provider_feedback_seq: BTreeMap<String, u64>,
    /// Compatibility feedback sequencing is partitioned by Agent session;
    /// Runtime-issued feedback identity remains the eventual authority.
    provider_feedback_seq_by_session: BTreeMap<String, u64>,
    provider_memory_store: MemoryWriteStore,
    provider_completed_decisions: VecDeque<RuntimeLlmDecision>,
    provider_stale_replans: BTreeMap<String, ProviderStaleReplanState>,
    provider_transport_exhausted: BTreeSet<String>,
    provider_terminal_states: BTreeMap<String, lineage_persistence::ProviderTerminalState>,
    provider_late_response_diagnostics:
        VecDeque<lineage_persistence::ProviderLateResponseDiagnostic>,
    provider_held_decisions: BTreeMap<String, async_support::RuntimeLlmDecision>,
    pending_provider_world_events: BTreeMap<String, RuntimePendingProviderWorldEvent>,
    provider_world_event_quarantine: BTreeMap<String, String>,
    provider_lineage_store: Option<PathBuf>,
    provider_lineage_binding: Option<RuntimeBindingV1>,
    provider_lineage_restored: bool,
    provider_lineage_hydrated: bool,
    /// A lineage checkpoint that cannot be decoded is a recovery fence.  The
    /// sidecar must not rebuild fresh contexts while its durable identity
    /// evidence is unavailable.
    provider_lineage_recovery_pending: Option<String>,
    pending_runtime_wakes: BTreeMap<String, SchedulerWakeV1>,
}
pub(in crate::viewer::runtime_live) struct RuntimePlayerBindingPlan {
    agent_player_bindings: BTreeMap<String, String>,
    player_agent_bindings: BTreeMap<String, String>,
    agent_public_key_bindings: BTreeMap<String, String>,
    events: Vec<WorldEventKind>,
}

impl RuntimePlayerBindingPlan {
    pub(in crate::viewer::runtime_live) fn has_binding_transition(&self) -> bool {
        self.events.iter().any(|event| {
            matches!(
                event,
                WorldEventKind::AgentPlayerBound { .. } | WorldEventKind::AgentPlayerUnbound { .. }
            )
        })
    }
}
impl RuntimeLlmSidecar {
    #[cfg(any(test, feature = "test_tier_required"))]
    pub(in crate::viewer::runtime_live) fn service_test_summary(&self) -> serde_json::Value {
        #[cfg(not(target_arch = "wasm32"))]
        let native_runner_present = self
            .runner
            .as_ref()
            .and_then(RuntimeDecisionRunner::async_runner)
            .is_some();
        #[cfg(target_arch = "wasm32")]
        let native_runner_present = false;
        serde_json::json!({
            "pending_intent_count": self.provider_service_pending.len(),
            "native_runner_present": native_runner_present,
            "native_model_call_count": self.service_test_model_calls.values().map(|state|state.lock().unwrap().recorded_requests.len()).sum::<usize>(),
            "hosted_service_phase": self.hosted_service_phase.as_ref().map(|phase| phase.test_phase_label()),
            "hosted_service_memory_failure": self.hosted_service_memory_failure,
            "pending_action_count": self.pending_actions.len(),
            "terminal_states": self.provider_terminal_states,
            "memory_store": self.provider_memory_store,
            "pending_wake_ids": self.pending_runtime_wakes.keys().collect::<Vec<_>>(),
            "mirrored_lease_count": self.provider_cognition_leases.len(),
            "mirrored_lease_identities": self.provider_cognition_leases.iter().take(16)
                .map(|(agent_id, lease)| serde_json::json!({
                    "agent_id": agent_id,
                    "lease_id": lease.lease_id,
                    "request_digest": lease.request_digest,
                    "matches_terminal_request": self.provider_terminal_states.get(agent_id)
                        .is_some_and(|terminal| terminal.request_digest == lease.request_digest),
                })).collect::<Vec<_>>(),
        })
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    pub(in crate::viewer::runtime_live) fn queue_service_test_response(
        &mut self,
        world: &mut RuntimeWorld,
        mut cognition: RuntimeProviderActionContext,
        _action: oasis7_simulator::Action,
    ) -> Result<(), String> {
        self.ensure_canonical_agent_durable_admission()?;
        let agent_id = cognition.request.request_context.agent_subject.clone();
        let observation = self
            .shadow_kernel
            .as_mut()
            .ok_or("test requires authenticated projection observation")?
            .observe(&agent_id)
            .map_err(|error| format!("{error:?}"))?;
        let provider = crate::simulator::MockDecisionProvider::with_scripted_responses(
            "canonical-test-provider",
            vec![Ok(cognition.response.base_decision_response.clone())],
        );
        let model_state = provider.shared_state();
        let behavior = crate::simulator::ProviderBackedAgentBehavior::new(
            agent_id.clone(),
            provider,
            provider_phase1_action_catalog(),
        )
        .require_continuous_request_context();
        let registered = self.service_test_actor_agents.contains(&agent_id);
        let mut runner = match self.runner.take() {
            Some(RuntimeDecisionRunner::ProviderBacked(runner))
                if registered || !runner.has_agent(&agent_id) =>
            {
                runner
            }
            Some(RuntimeDecisionRunner::Builtin(runner))
                if !registered && !runner.has_agent(&agent_id) =>
            {
                runner
            }
            Some(runner) => {
                self.runner = Some(runner);
                return Err(format!(
                    "test provider runner cannot replace existing actor for {agent_id}"
                ));
            }
            None if !registered => crate::simulator::AsyncAgentRunner::with_default_capacity(),
            None => {
                return Err(format!(
                    "test provider actor runner is unavailable for {agent_id}"
                ));
            }
        };
        // Keep this exact Runner on the host side while validating its durable
        // history, before either lease reservation or provider dispatch. Idle
        // runners are revalidated even when already loaded; active runners
        // follow the production no-refresh rule and must already be recovered.
        if runner.active_turn_count() == 0 {
            if let Err(error) =
                crate::viewer::runtime_live::llm_sidecar_feedback_recovery::
                    restore_agent_feedback_history(world, &mut runner)
            {
                self.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
                return Err(error);
            }
        } else if !runner.runtime_feedback_history_loaded() {
            self.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
            return Err(format!(
                "active provider runner has no host-validated feedback history for {agent_id}"
            ));
        }
        let session_id = cognition.request.request_context.agent_session_id.as_str();
        if crate::viewer::runtime_live::llm_sidecar_feedback_recovery::
            feedback_history_blocks_dispatch(&runner, &agent_id, session_id)
        {
            self.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
            return Err(format!(
                "Runtime feedback history fences provider test dispatch for {agent_id}/{session_id}"
            ));
        }
        // A successfully restored history can still fence this exact Agent
        // session. Do not register the test actor or publish its bookkeeping
        // until that recovery fence has been checked.
        if !registered {
            if let Err(error) = runner.register(behavior) {
                self.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
                return Err(format!("{error:?}"));
            }
            self.service_test_model_calls
                .insert(agent_id.clone(), model_state);
            self.service_test_actor_agents.insert(agent_id.clone());
        }
        self.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
        let lease = self.reserve_provider_lease_at_authority(world, &cognition.request)?;
        self.provider_prefix_at_authority(world, &cognition.request)?;
        cognition.cognition_lease = Some(lease.clone());
        let mut runner = match self.runner.take() {
            Some(RuntimeDecisionRunner::ProviderBacked(runner)) => runner,
            _ => return Err("test provider runner unavailable after lease reservation".into()),
        };
        runner.sync_logical_tick(cognition.request.request_context.runtime_binding.base_tick);
        let started = runner
            .start_turn_with_request_context_and_observation_and_lease(
                &agent_id,
                observation,
                cognition.request.turn_context.clone(),
                cognition.request.request_context.clone(),
                lease.clone(),
            )
            .map_err(|error| format!("{error:?}"));
        self.decision_mode = ViewerLiveDecisionMode::Llm;
        self.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
        started?;
        self.service_test_actor_agents.insert(agent_id.clone());
        self.provider_agent_ids.insert(agent_id.clone());
        self.provider_active_turns
            .insert(agent_id.clone(), cognition.request.clone());
        self.provider_contexts
            .insert(agent_id.clone(), cognition.request.clone());
        self.provider_cognition_leases.insert(
            agent_id.clone(),
            cognition
                .cognition_lease
                .clone()
                .expect("test reserved canonical lease"),
        );
        self.llm_decision_mailbox = 1;
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::viewer::runtime_live) fn pending_actions_empty(&self) -> bool {
        self.pending_actions.is_empty()
    }

    #[cfg(test)]
    pub(in crate::viewer::runtime_live) fn provider_contexts_empty(&self) -> bool {
        self.provider_contexts.is_empty()
    }

    #[cfg(test)]
    pub(in crate::viewer::runtime_live) fn provider_has_terminal_status(
        &self,
        status: &str,
    ) -> bool {
        self.provider_terminal_states
            .values()
            .any(|terminal| terminal.status == status)
    }

    #[cfg(test)]
    pub(in crate::viewer::runtime_live) fn provider_memory_store(&self) -> &MemoryWriteStore {
        &self.provider_memory_store
    }

    pub(in crate::viewer::runtime_live) fn provider_cognition_lease(
        &self,
        agent_id: &str,
    ) -> Option<crate::runtime::CognitionLeaseV1> {
        self.provider_cognition_leases.get(agent_id).cloned()
    }

    pub(in crate::viewer::runtime_live) fn bind_provider_cognition_lease(
        &mut self,
        agent_id: impl Into<String>,
        lease: crate::runtime::CognitionLeaseV1,
    ) {
        self.provider_cognition_leases
            .insert(agent_id.into(), lease);
    }

    pub(in crate::viewer::runtime_live) fn clear_provider_cognition_lease(
        &mut self,
        agent_id: &str,
    ) {
        self.provider_cognition_leases.remove(agent_id);
    }

    pub(in crate::viewer::runtime_live) fn with_runtime_seed_model(
        mut self,
        model: &WorldModel,
    ) -> Self {
        self.runtime_seed_locations = model.locations.values().cloned().collect();
        self.runtime_seed_model = Some(model.clone());
        self
    }
    pub(in crate::viewer::runtime_live) fn seed_location_for_pos(
        &self,
        pos: GeoPos,
    ) -> Option<Location> {
        self.runtime_seed_locations
            .iter()
            .find(|location| location.pos == pos)
            .cloned()
    }
    pub(in crate::viewer::runtime_live) fn is_llm_mode(&self) -> bool {
        matches!(self.decision_mode, ViewerLiveDecisionMode::Llm)
    }

    pub(in crate::viewer::runtime_live) fn enable_hosted_local_mock_test_lane(&mut self) {
        self.hosted_local_mock_test_lane = true;
    }

    pub(in crate::viewer::runtime_live) fn supports_prompt_control(&self) -> bool {
        !env_requests_provider_backend()
    }
    pub(in crate::viewer::runtime_live) fn supports_prompt_control_result(&self) -> bool {
        self.is_llm_mode() && (self.supports_prompt_control() || self.hosted_local_mock_test_lane)
    }
    pub(in crate::viewer::runtime_live) fn supports_agent_chat(&self) -> bool {
        true
    }
    pub(in crate::viewer::runtime_live) fn refresh_provider_check_snapshot(&mut self) {
        let Ok(Some(settings)) = provider_settings_from_env() else {
            self.provider_check_snapshot = None;
            return;
        };

        let cache_key = runtime_provider_check_cache_key(&settings);
        let checked_at_unix_ms = runtime_provider_check_now_unix_ms();
        if self
            .provider_check_snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.cache_key == cache_key
                    && checked_at_unix_ms.saturating_sub(snapshot.checked_at_unix_ms)
                        < RUNTIME_PROVIDER_CHECK_CACHE_MS
            })
        {
            return;
        }
        let probe_timeout_ms = if settings.provider_transport == REMOTE_HTTPS_PROVIDER_TRANSPORT {
            settings.connect_timeout_ms.max(1_500)
        } else {
            settings.connect_timeout_ms.min(500)
        };
        self.provider_check_snapshot = Some(
            match ProviderLoopbackHttpClient::new_with_transport(
                settings.base_url.as_str(),
                settings.auth_token.as_deref(),
                probe_timeout_ms,
                settings.provider_transport.as_str(),
            ) {
                Ok(client) => match (client.provider_info(), client.provider_health()) {
                    (Ok(info), Ok(health)) => {
                        let compatibility = evaluate_provider_compatibility(&info, Some(&health));
                        RuntimeProviderCheckSnapshot {
                            source: "runtime_live_probe".to_string(),
                            status: compatibility.status.as_str().to_string(),
                            capabilities: info.capabilities,
                            supported_action_sets: info.supported_action_sets,
                            fallback_reason: compatibility.fallback_reason,
                            error: None,
                            checked_at_unix_ms,
                            cache_key,
                        }
                    }
                    (Err(err), _) | (_, Err(err)) => RuntimeProviderCheckSnapshot {
                        source: "runtime_live_probe".to_string(),
                        status: "check_failed".to_string(),
                        capabilities: Vec::new(),
                        supported_action_sets: Vec::new(),
                        fallback_reason: None,
                        error: Some(err.to_string()),
                        checked_at_unix_ms,
                        cache_key,
                    },
                },
                Err(err) => RuntimeProviderCheckSnapshot {
                    source: "runtime_live_probe".to_string(),
                    status: "check_failed".to_string(),
                    capabilities: Vec::new(),
                    supported_action_sets: Vec::new(),
                    fallback_reason: None,
                    error: Some(err.to_string()),
                    checked_at_unix_ms,
                    cache_key,
                },
            },
        );
    }
    pub(in crate::viewer::runtime_live) fn provider_check_snapshot(
        &self,
    ) -> Option<&RuntimeProviderCheckSnapshot> {
        self.provider_check_snapshot.as_ref()
    }
    pub(in crate::viewer::runtime_live) fn ensure_gameplay_ready(
        &mut self,
        world: &RuntimeWorld,
        config: &WorldConfig,
    ) -> Result<(), String> {
        if !self.is_llm_mode() {
            return Err("gameplay requires runtime live server running with --llm".to_string());
        }
        self.sync_shadow_kernel(world, config)?;
        self.ensure_runner_initialized().map_err(|message| {
            format!("gameplay requires a configured and reachable LLM provider: {message}")
        })?;
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn consume_player_auth_nonce(
        &mut self,
        player_id: &str,
        nonce: u64,
    ) -> Result<(), String> {
        self.validate_player_auth_nonce(player_id, nonce)?;
        self.commit_player_auth_nonce(player_id, nonce);
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn validate_player_auth_nonce(
        &self,
        player_id: &str,
        nonce: u64,
    ) -> Result<(), String> {
        let player_id = player_id.trim();
        if player_id.is_empty() {
            return Err("player_id cannot be empty".to_string());
        }
        if nonce == 0 {
            return Err("auth nonce must be greater than zero".to_string());
        }
        if let Some(last_nonce) = self.player_auth_last_nonce.get(player_id)
            && nonce <= *last_nonce
        {
            return Err(format!(
                "auth nonce replay for {}: expected nonce > {}, received {}",
                player_id, last_nonce, nonce
            ));
        }
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn commit_player_auth_nonce(
        &mut self,
        player_id: &str,
        nonce: u64,
    ) {
        self.player_auth_last_nonce
            .insert(player_id.trim().to_string(), nonce);
    }
    pub(in crate::viewer::runtime_live) fn find_chat_intent_replay(
        &self,
        player_id: &str,
        agent_id: &str,
        intent_seq: u64,
        intent_tick: Option<u64>,
        message: &str,
        public_key: Option<&str>,
    ) -> Result<Option<AgentChatAck>, String> {
        let key = (
            player_id.trim().to_string(),
            agent_id.trim().to_string(),
            intent_seq,
        );
        let Some(record) = self.player_chat_intent_acks.get(&key) else {
            return Ok(None);
        };
        let normalized_public_key = normalize_optional_public_key(public_key);
        let message_hash = hash_chat_message(message);
        if record.message_hash != message_hash
            || record.intent_tick != intent_tick
            || record.public_key != normalized_public_key
        {
            return Err(format!(
                "agent_chat intent_seq conflict for {} on {} seq {}",
                key.0, key.1, intent_seq
            ));
        }
        let mut ack = record.ack.clone();
        ack.idempotent_replay = true;
        Ok(Some(ack))
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Stable chat acknowledgement persistence seam keeps signed intent identity fields explicit."
    )]
    pub(in crate::viewer::runtime_live) fn record_chat_intent_ack(
        &mut self,
        player_id: &str,
        agent_id: &str,
        intent_seq: u64,
        intent_tick: Option<u64>,
        message: &str,
        public_key: Option<&str>,
        ack: &AgentChatAck,
    ) {
        let key = (
            player_id.trim().to_string(),
            agent_id.trim().to_string(),
            intent_seq,
        );
        let record = RuntimeChatIntentAckRecord {
            ack: ack.clone(),
            message_hash: hash_chat_message(message),
            public_key: normalize_optional_public_key(public_key),
            intent_tick,
        };
        self.player_chat_intent_acks.insert(key, record);
    }
    pub(in crate::viewer::runtime_live) fn clear_chat_intent_acks_for_player(
        &mut self,
        player_id: &str,
    ) {
        let player_id = player_id.trim();
        self.player_chat_intent_acks
            .retain(|(record_player_id, _, _), _| record_player_id != player_id);
    }
    pub(in crate::viewer::runtime_live) fn bound_agent_for_player(
        &self,
        player_id: &str,
    ) -> Option<&str> {
        self.player_agent_bindings
            .get(player_id.trim())
            .map(String::as_str)
    }
    pub(in crate::viewer::runtime_live) fn clear_player_binding(
        &mut self,
        player_id: &str,
    ) -> Option<WorldEventKind> {
        let player_id = player_id.trim();
        let agent_id = self.player_agent_bindings.remove(player_id)?;
        self.agent_player_bindings.remove(agent_id.as_str());
        let public_key = self.agent_public_key_bindings.remove(agent_id.as_str());
        Some(WorldEventKind::AgentPlayerUnbound {
            agent_id,
            player_id: player_id.to_string(),
            public_key,
        })
    }
    pub(in crate::viewer::runtime_live) fn clear_stale_local_test_bindings_for_world(
        &mut self,
        world: &RuntimeWorld,
    ) -> usize {
        let world_agent_ids: BTreeSet<&str> =
            world.state().agents.keys().map(String::as_str).collect();
        let stale_agent_ids: Vec<String> = self
            .agent_player_bindings
            .iter()
            .filter(|(agent_id, player_id)| {
                player_id.starts_with(LOCAL_TEST_PLAYER_ID_PREFIX)
                    && !world_agent_ids.contains(agent_id.as_str())
            })
            .map(|(agent_id, _)| agent_id.clone())
            .collect();
        let stale_count = stale_agent_ids.len();
        for agent_id in stale_agent_ids {
            if let Some(player_id) = self.agent_player_bindings.remove(agent_id.as_str()) {
                self.player_agent_bindings.remove(player_id.as_str());
            }
            self.agent_public_key_bindings.remove(agent_id.as_str());
        }
        stale_count
    }
    pub(in crate::viewer::runtime_live) fn bind_agent_player(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
        allow_player_rebind: bool,
    ) -> Result<Vec<WorldEventKind>, String> {
        let agent_id = agent_id.trim();
        let player_id = player_id.trim();
        if agent_id.is_empty() {
            return Err("agent_id cannot be empty".to_string());
        }
        if player_id.is_empty() {
            return Err("player_id cannot be empty".to_string());
        }
        let requested_public_key = public_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let current_player = self.agent_player_bindings.get(agent_id).cloned();
        let current_public_key = self.agent_public_key_bindings.get(agent_id).cloned();
        let previous_agent_id = self.player_agent_bindings.get(player_id).cloned();
        if previous_agent_id
            .as_deref()
            .is_some_and(|bound_agent_id| bound_agent_id != agent_id)
            && !allow_player_rebind
        {
            return Err(format!(
                "player {} is already bound to agent {}, explicit rebind required",
                player_id,
                previous_agent_id.unwrap_or_default()
            ));
        }
        let target_public_key = if current_player.as_deref() == Some(player_id) {
            requested_public_key
                .clone()
                .or_else(|| current_public_key.clone())
        } else {
            requested_public_key.clone()
        };
        if current_player.as_deref() == Some(player_id) && current_public_key == target_public_key {
            return Ok(Vec::new());
        }
        let mut events = Vec::new();
        if let Some(previous_agent_id) = previous_agent_id
            .as_deref()
            .filter(|bound_agent_id| *bound_agent_id != agent_id)
        {
            self.player_agent_bindings.remove(player_id);
            self.agent_player_bindings.remove(previous_agent_id);
            let previous_public_key = self.agent_public_key_bindings.remove(previous_agent_id);
            events.push(WorldEventKind::AgentPlayerUnbound {
                agent_id: previous_agent_id.to_string(),
                player_id: player_id.to_string(),
                public_key: previous_public_key,
            });
        }
        if let Some(previous_player_id) = current_player
            .as_deref()
            .filter(|bound_player_id| *bound_player_id != player_id)
        {
            self.player_agent_bindings.remove(previous_player_id);
            events.push(WorldEventKind::AgentPlayerUnbound {
                agent_id: agent_id.to_string(),
                player_id: previous_player_id.to_string(),
                public_key: current_public_key.clone(),
            });
        }

        self.agent_player_bindings
            .insert(agent_id.to_string(), player_id.to_string());
        self.player_agent_bindings
            .insert(player_id.to_string(), agent_id.to_string());
        match target_public_key.clone() {
            Some(value) => {
                self.agent_public_key_bindings
                    .insert(agent_id.to_string(), value);
            }
            None => {
                self.agent_public_key_bindings.remove(agent_id);
            }
        }
        events.push(WorldEventKind::AgentPlayerBound {
            agent_id: agent_id.to_string(),
            player_id: player_id.to_string(),
            public_key: target_public_key,
        });
        Ok(events)
    }

    pub(in crate::viewer::runtime_live) fn plan_agent_player_binding(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
        allow_player_rebind: bool,
    ) -> Result<RuntimePlayerBindingPlan, String> {
        let original_agent_player_bindings = self.agent_player_bindings.clone();
        let original_player_agent_bindings = self.player_agent_bindings.clone();
        let original_agent_public_key_bindings = self.agent_public_key_bindings.clone();
        let result = self.bind_agent_player(agent_id, player_id, public_key, allow_player_rebind);
        let plan = result.map(|events| RuntimePlayerBindingPlan {
            agent_player_bindings: self.agent_player_bindings.clone(),
            player_agent_bindings: self.player_agent_bindings.clone(),
            agent_public_key_bindings: self.agent_public_key_bindings.clone(),
            events,
        });
        self.agent_player_bindings = original_agent_player_bindings;
        self.player_agent_bindings = original_player_agent_bindings;
        self.agent_public_key_bindings = original_agent_public_key_bindings;
        plan
    }

    pub(in crate::viewer::runtime_live) fn apply_agent_player_binding_plan(
        &mut self,
        plan: RuntimePlayerBindingPlan,
    ) -> Vec<WorldEventKind> {
        self.agent_player_bindings = plan.agent_player_bindings;
        self.player_agent_bindings = plan.player_agent_bindings;
        self.agent_public_key_bindings = plan.agent_public_key_bindings;
        plan.events
    }

    pub(super) fn upsert_prompt_profile(&mut self, profile: AgentPromptProfile) {
        self.prompt_profile_history
            .entry(profile.agent_id.clone())
            .or_default()
            .insert(profile.version, profile.clone());
        self.prompt_profiles
            .insert(profile.agent_id.clone(), profile);
    }

    pub(in crate::viewer::runtime_live) fn request_decision(&mut self) {
        if self.is_llm_mode() {
            self.llm_decision_mailbox = self.llm_decision_mailbox.saturating_add(1);
        }
    }

    /// Mirror Runtime-selected wake leases into the provider adapter without
    /// inventing a second scheduler. The durable wake remains owned by
    /// Runtime; this map only tells the next provider request which exact
    /// continuation identity must be presented.
    pub(in crate::viewer::runtime_live) fn sync_runtime_wakes(
        &mut self,
        world: &RuntimeWorld,
    ) -> Result<(), String> {
        if self.provider_service_required {
            let view = self
                .provider_service_projection
                .as_ref()
                .ok_or("authorized canonical wake view missing")?;
            for wake in &view.scheduler_wakes {
                wake.validate()
                    .map_err(|error| format!("canonical wake invalid: {error:?}"))?;
            }
            self.pending_runtime_wakes =
                self.reconcile_projected_service_wakes(view.scheduler_wakes.clone())?;
            return self.persist_provider_lineage();
        }
        let wakes = world
            .cognition_in_flight_wakes()
            .map_err(|error| format!("Runtime cognition wake read failed: {error:?}"))?;
        let live_wake_ids = wakes
            .iter()
            .map(|wake| wake.wake_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut pending_runtime_wakes = self.pending_runtime_wakes.clone();
        let pending_runtime_wakes_migrated =
            lineage_recovery::hydrate_pending_runtime_wake_identities(
                &mut pending_runtime_wakes,
                &wakes,
                &self.provider_terminal_states,
            )?;
        pending_runtime_wakes.retain(|wake_id, _| live_wake_ids.contains(wake_id.as_str()));
        for wake in wakes {
            if !self.provider_lineage_hydrated
                && self
                    .provider_active_turns
                    .get(wake.agent_id.as_str())
                    .is_some_and(|context| {
                        lineage_persistence::provider_context_matches_wake(context, &wake)
                    })
            {
                // A process-local async runner cannot survive restart. Keep
                // the exact context identity but make it eligible for one
                // fresh provider dispatch through the recovered wake.
                self.provider_active_turns.remove(wake.agent_id.as_str());
            }
            pending_runtime_wakes
                .entry(wake.wake_id.clone())
                .or_insert(wake);
        }
        self.pending_runtime_wakes = pending_runtime_wakes;
        if pending_runtime_wakes_migrated {
            self.persist_provider_lineage_best_effort();
        }
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn has_pending_runtime_wake_for_agent(
        &self,
        agent_id: &str,
    ) -> bool {
        self.pending_runtime_wakes
            .values()
            .any(|wake| wake.agent_id == agent_id)
    }

    pub(in crate::viewer::runtime_live) fn pending_runtime_wake_id_for_context(
        &self,
        agent_id: &str,
        context: &cognition_context::ProviderContextState,
    ) -> Option<&str> {
        self.pending_runtime_wakes
            .values()
            .filter(|wake| {
                wake.agent_id == agent_id
                    && lineage_persistence::provider_context_matches_wake(context, wake)
            })
            .min_by_key(|wake| (wake.wake_seq, wake.wake_id.as_str()))
            .map(|wake| wake.wake_id.as_str())
    }

    pub(in crate::viewer::runtime_live) fn pending_runtime_wake_id_for_terminal(
        &self,
        agent_id: &str,
    ) -> Option<&str> {
        let terminal = self.provider_terminal_states.get(agent_id)?;
        self.pending_runtime_wakes
            .values()
            .filter(|wake| {
                wake.agent_id == agent_id
                    && wake.agent_session_id == terminal.agent_session_id
                    && wake.agent_turn_id == terminal.agent_turn_id
                    && wake.decision_request_id == terminal.decision_request_id
                    && !wake.request_digest.is_empty()
                    && wake.request_digest == terminal.request_digest
            })
            .min_by_key(|wake| (wake.wake_seq, wake.wake_id.as_str()))
            .map(|wake| wake.wake_id.as_str())
    }

    pub(in crate::viewer::runtime_live) fn clear_runtime_wake(&mut self, wake_id: &str) {
        self.pending_runtime_wakes.remove(wake_id);
    }

    /// Queue a provider-backed chat request with the exact durable Intent
    /// identity that authorized it.  The caller must use this after the
    /// accepted Intent has been committed; the sidecar never invents or
    /// replaces the identity.
    pub(super) fn push_chat_message_for_intent(
        &mut self,
        world: &RuntimeWorld,
        config: &WorldConfig,
        intent: &AgentIntentV2,
        player_id: &str,
        message: &str,
    ) -> Result<(), AgentChatError> {
        self.push_chat_message_with_intent(
            world,
            config,
            intent.agent_id.as_str(),
            player_id,
            message,
            Some((intent.intent_id.as_str(), intent.request_digest.as_str())),
        )
    }

    /// Drain provider replies while retaining the exact Intent identity for
    /// durable success/failure disposition handling.
    pub(super) fn drain_provider_agent_chat_replies_with_identity(
        &mut self,
        world: &RuntimeWorld,
    ) -> (
        Vec<(RuntimePendingProviderAgentChat, String)>,
        Vec<RuntimeProviderAgentChatFailure>,
    ) {
        let pending: Vec<_> = self.pending_provider_agent_chats.drain(..).collect();
        let mut replies = Vec::new();
        let mut failures = Vec::new();
        for request in pending {
            match self.request_provider_agent_chat(
                world,
                request.agent_id.as_str(),
                request.player_id.as_str(),
                request.message.as_str(),
            ) {
                Ok(Some(reply)) => replies.push((request, reply)),
                Ok(None) => {}
                Err(request_error) => {
                    let RuntimeProviderAgentChatRequestError { error, retryable } = request_error;
                    tracing::warn!(
                        agent_id = error.agent_id.as_deref().unwrap_or(""),
                        error_code = error.code.as_str(),
                        error_message = error.message.as_str(),
                        "provider-backed agent chat reply failed after ack"
                    );
                    failures.push(RuntimeProviderAgentChatFailure {
                        pending: request,
                        retryable,
                        error,
                    });
                }
            }
        }
        (replies, failures)
    }
}

#[cfg(test)]
#[path = "llm_sidecar_budget_tests.rs"]
mod budget_tests;
#[cfg(test)]
#[path = "llm_sidecar_tests.rs"]
mod tests;

#[path = "llm_sidecar_service_feedback_ack.rs"]
pub(in crate::viewer::runtime_live) mod service_feedback_ack;
