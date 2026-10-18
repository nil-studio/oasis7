use self::periodic_connection_diagnostic::TraceSink;
use super::auth::verify_session_register_auth_proof_for_recovery;
use super::live::ViewerLiveDecisionMode;
use super::protocol::{
    AuthoritativeBatchFinality, AuthoritativeChallengeAck, AuthoritativeChallengeCommand,
    AuthoritativeChallengeError, AuthoritativeChallengeResolveRequest,
    AuthoritativeChallengeStatus, AuthoritativeChallengeSubmitRequest, AuthoritativeFinalityState,
    AuthoritativeReconnectSyncRequest, AuthoritativeRecoveryAck, AuthoritativeRecoveryCommand,
    AuthoritativeRecoveryError, AuthoritativeRecoveryStatus, AuthoritativeRollbackReceipt,
    AuthoritativeRollbackRequest, AuthoritativeRollbackV2Request,
    AuthoritativeSessionRegisterRequest, AuthoritativeSessionRevokeRequest,
    AuthoritativeSessionRotateRequest, ControlCompletionAck, ControlCompletionStatus,
    GameplayActionError, PROMPT_CONTROL_RESULT_CAPABILITY, REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY,
    RollbackAuthorizationEnvelope, RollbackIntent, VIEWER_PROTOCOL_VERSION, ViewerControl,
    ViewerControlProfile, ViewerEventKind, ViewerRequest, ViewerResponse, ViewerStream,
    viewer_event_kind_matches, viewer_protocol_supports_prompt_control_result,
    viewer_protocol_supports_revoke_social_fact_quote,
};
use crate::geometry::GeoPos;
use crate::observability::emit_stderr_or_event;
use crate::runtime::{
    Action as RuntimeAction, DomainEvent as RuntimeDomainEvent, Journal as RuntimeJournal,
    ReleaseSecurityPolicy, Snapshot as RuntimeSnapshot, World as RuntimeWorld,
    WorldError as RuntimeWorldError, WorldEventBody as RuntimeWorldEventBody, blake3_hex,
};
use crate::simulator::{
    AgentDecisionTrace, PlayerGameplayRecentFeedback, RunnerMetrics, WorldEvent, WorldSnapshot,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_simulator::{
    CHUNK_GENERATION_SCHEMA_VERSION, RejectReason as SimulatorRejectReason, SNAPSHOT_VERSION,
    WorldConfig, WorldInitConfig, WorldModel, WorldScenario, build_world_model,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
mod agent_service_io;
#[cfg(any(test, feature = "test_tier_required"))]
mod agent_service_io_diagnostics;
mod agent_service_phase;
mod agent_service_pump;
mod periodic_connection_diagnostic;
mod periodic_service_io;
mod response_outbox;
use tracing::Level;

#[cfg(test)]
pub(super) fn canonical_runtime_provider_env_lock() -> &'static Mutex<()> {
    static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[path = "runtime_live/advance_runtime_server.rs"]
mod advance_runtime_server;
#[path = "runtime_live/advance_tick.rs"]
mod advance_tick;
mod agency_control;
mod authoritative;
mod auto_play;
mod branch_commitment;
#[path = "runtime_live/chain_link.rs"]
mod chain_link;
mod claim_snapshot;
mod collect_data;
#[path = "runtime_live/config.rs"]
mod config;
mod constants;
#[path = "runtime_live/control_blocking.rs"]
mod control_blocking;
#[path = "runtime_live/control_feedback.rs"]
mod control_feedback;
#[path = "runtime_live/control_mode.rs"]
mod control_mode;
#[path = "runtime_live/control_plane.rs"]
mod control_plane;
#[path = "runtime_live/control_utils.rs"]
mod control_utils;
#[path = "runtime_live/decision_trace.rs"]
mod decision_trace;
mod fragment_refill_preview;
mod gameplay_feedback_scope;
mod gameplay_snapshot;
mod gameplay_snapshot_feedback;
mod gameplay_snapshot_helpers;
mod gameplay_snapshot_lane;
mod gameplay_validation_preview;
mod governance_vote_quote;
mod governance_vote_quote_debug;
#[cfg(not(target_arch = "wasm32"))]
#[path = "runtime_live/harness_authority.rs"]
mod harness_authority;
#[cfg(not(target_arch = "wasm32"))]
#[path = "runtime_live/llm_sidecar_feedback_recovery.rs"]
mod llm_sidecar_feedback_recovery;
mod mapping;
mod market_quote_decision;
#[cfg(test)]
mod module_visual_driver;
mod owner_read;
mod player_agency_projection;
mod player_gameplay;
#[path = "runtime_live/power_projection.rs"]
mod power_projection;
mod power_sale_quote;
mod prompt_control_result;
mod recovery;
mod recovery_audit;
mod recovery_compensation;
mod recovery_persistence;
mod recovery_receipt;
mod recovery_rollback_v2;
mod recovery_session;
mod request_dispatch;
mod request_prime;
mod runtime_script;
mod schedule_recipe_quote;
mod session_policy;
#[path = "runtime_live/smelter_affordability_debug.rs"]
mod smelter_affordability_debug;
mod snapshot_privacy;
#[path = "runtime_live/social_quote.rs"]
mod social_quote;
mod starter_industrial_outcome;
mod stream_session;
mod support;
#[cfg(test)]
mod tests;
mod transfer_material_quote;
#[path = "runtime_live/wake_dispatch.rs"]
mod wake_dispatch;
#[path = "runtime_live/war_declaration_quote.rs"]
mod war_declaration_quote;
mod world_feed;
mod world_service_link;
mod world_view_source;
pub use crate::runtime::ProviderBackedBootstrapAuthorityV1;
use authoritative::{
    RuntimeAuthoritativeBatchRecord, RuntimeAuthoritativeChallengeRecord,
    RuntimeSettlementRankingGate, RuntimeStableCheckpoint,
};
use claim_snapshot::build_player_agent_claim_snapshot;
pub use config::{ChainLinkPolicy, ViewerRuntimeLiveServerConfig, ViewerRuntimeLiveServerError};
use constants::*;
pub use control_plane::runtime_agent_chat_echo_enabled_from_env;
use control_plane::{RuntimeLlmSidecar, RuntimePlayerBindingPlan};
use control_utils::{
    control_mode_for_action, control_mode_label, runtime_control_error_details,
    should_emit_runtime_advance_snapshot,
};
use decision_trace::{append_decision_upstream_trace, decision_trace_provider_error_retryable};
use gameplay_snapshot::{
    apply_runtime_snapshot_empty_entities_blocker, build_player_gameplay_snapshot,
    player_gameplay_feedback_from_control_ack,
};
use mapping::{map_runtime_event, runtime_state_to_simulator_model};
use player_agency_projection::{
    PlayerGameplayCausalitySignal, player_gameplay_causality_from_runtime_events,
};
use prompt_control_result::PromptControlRuntimeAuthority;
use runtime_script::RuntimeLiveScript;
use session_policy::{
    RuntimeSessionPolicy, RuntimeSessionRevokeMetadata, location_id_for_pos,
    map_session_policy_error_code, normalize_optional_string, session_revoke_metadata_key,
};
pub use support::bootstrap_formal_release_runtime_world as viewer_bootstrap_formal_release_runtime_world;
pub use support::bootstrap_generated_sidecar_runtime_world as viewer_bootstrap_generated_sidecar_runtime_world;
use support::{
    FORMAL_RELEASE_DEFAULT_WORLD_ID, RuntimeLiveSession,
    apply_provider_backed_bootstrap_authorities, bootstrap_runtime_live_world,
    is_expected_disconnect_error, is_timeout_error, latest_runtime_event_seq, lock_shared_server,
    runtime_metrics, send_response,
};
pub const VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID: &str = FORMAL_RELEASE_DEFAULT_WORLD_ID;
pub struct ViewerRuntimeLiveServer {
    config: ViewerRuntimeLiveServerConfig,
    world: RuntimeWorld,
    initial_world_time: u64,
    auto_play_paused: bool,
    next_auto_play_step_at: Option<Instant>,
    last_chain_committed_height: u64,
    verified_world_view: Option<crate::world_service::verified_view::VerifiedWorldView>,
    world_service_query_state: crate::world_service::client::WorldServiceQueryState,
    periodic_service_executor: Option<Arc<periodic_service_io::Executor>>,
    prepared_owner_read: Option<Box<owner_read::PreparedOwnerRead>>,
    prepared_world_service_submission: Option<chain_link::PreparedWorldServiceSubmission>,
    pending_world_service_gameplay: Vec<(
        oasis7_client_api::world_service::RequestCorrelation,
        crate::world_service::wire::WorldServicePayloadV1,
    )>,
    chain_observer_loader: Arc<Mutex<chain_link::ObserverLoader>>,
    last_chain_observer_identity: Option<(u64, u64, u64, String)>,
    confirmed_player_gameplay_progress_time: Option<u64>,
    snapshot_config: WorldConfig,
    seed_model: Option<WorldModel>,
    script: RuntimeLiveScript,
    llm_sidecar: RuntimeLlmSidecar,
    agent_service_pump_started: bool,
    #[cfg(any(test, feature = "test_tier_required"))]
    agent_service_io_diagnostics: std::sync::Arc<agent_service_io_diagnostics::IoDiagnostics>,
    agent_service_session_count: usize,
    agent_service_pump_error: Option<&'static str>,
    pending_virtual_events: VecDeque<WorldEvent>,
    next_virtual_event_id: u64,
    authoritative_batches: VecDeque<RuntimeAuthoritativeBatchRecord>,
    next_authoritative_batch_id: u64,
    authoritative_challenges: VecDeque<RuntimeAuthoritativeChallengeRecord>,
    next_authoritative_challenge_id: u64,
    stable_checkpoints: VecDeque<RuntimeStableCheckpoint>,
    reorg_epoch: u64,
    session_policy: RuntimeSessionPolicy,
    session_revoke_metadata: BTreeMap<(String, String), RuntimeSessionRevokeMetadata>,
    rollback_readiness: BTreeMap<String, recovery_receipt::RuntimeRollbackReadinessRecord>,
    consumed_strict_audit_nonces: BTreeSet<String>,
    settlement_ranking_gate: RuntimeSettlementRankingGate,
    latest_player_gameplay_feedback: Option<PlayerGameplayRecentFeedback>,
    latest_player_gameplay_causality: Option<PlayerGameplayCausalitySignal>,
    scoped_gameplay_feedback: BTreeMap<String, gameplay_feedback_scope::ScopedGameplayFeedback>,
    pending_gameplay_feedback:
        BTreeMap<(String, String), gameplay_feedback_scope::PendingGameplayFeedback>,
    active_gameplay_feedback_origin: Option<gameplay_feedback_scope::GameplayFeedbackOrigin>,
    active_gameplay_feedback_authorized: bool,
    next_gameplay_feedback_order: u64,
    runtime_action_players: BTreeMap<u64, String>,
    consumed_rollback_operator_nonces: BTreeSet<String>,
    prompt_control_authority: PromptControlRuntimeAuthority,
    hosted_local_mock_test_lane_active: bool,
    authoritative_recovery_write_fence: Option<String>,
    smelter_affordability_debug_agent_id: Option<String>,
    governance_vote_quote_debug_agent_id: Option<String>,
    #[cfg(test)]
    recovery_fault_injection: Option<recovery::RuntimeRecoveryFaultInjection>,
    #[cfg(test)]
    authoritative_recovery_dir_override: Option<PathBuf>,
}
impl ViewerRuntimeLiveServer {
    pub fn new(
        mut config: ViewerRuntimeLiveServerConfig,
    ) -> Result<Self, ViewerRuntimeLiveServerError> {
        config
            .validate_response_limits()
            .map_err(ViewerRuntimeLiveServerError::Init)?;
        if config.world_service.is_none() {
            config.world_service =
                crate::world_service::client::WorldServiceClientConfig::from_env()
                    .map_err(ViewerRuntimeLiveServerError::Init)?;
        }
        if config.world_service_agent_signer.is_none() {
            config.world_service_agent_signer =
                crate::world_service::client::WorldServiceAgentSignerConfig::from_env()
                    .map_err(ViewerRuntimeLiveServerError::Init)?;
        }
        if let Some(service) = &config.world_service {
            config.world_id = service.expected_world.world_id.clone();
        } else if config.world_service_agent_signer.is_some() {
            return Err(ViewerRuntimeLiveServerError::Init(
                "Agent signer requires a world service connection".into(),
            ));
        }
        #[cfg(not(test))]
        if config.chain_status_bind.is_some() && config.world_service.is_none() {
            return Err(ViewerRuntimeLiveServerError::Init(
                "node-directory chain sync is closed; configure the authenticated world service"
                    .into(),
            ));
        }
        config
            .validate_prompt_result_limits()
            .map_err(ViewerRuntimeLiveServerError::Init)?;
        let prompt_control_authority = PromptControlRuntimeAuthority::new(
            config.prompt_result_cache_capacity,
            config.prompt_result_receipt_max_bytes,
        )
        .map_err(ViewerRuntimeLiveServerError::Init)?;
        let (mut world, snapshot_config, seed_model, chunk_runtime) =
            bootstrap_runtime_live_world(&config).map_err(ViewerRuntimeLiveServerError::Init)?;
        let mut recovered_generation = None;
        if let Some(recovery_dir) = config
            .generated_world_dir
            .as_deref()
            .map(|dir| dir.join("runtime-live-authoritative-recovery"))
        {
            std::fs::create_dir_all(&recovery_dir)?;
            let preflight_path = recovery_dir.join(format!(
                ".authoritative-recovery-preflight-{}",
                std::process::id()
            ));
            std::fs::write(&preflight_path, b"authoritative-recovery-preflight")?;
            std::fs::remove_file(&preflight_path)?;
            if let Some(committed) =
                crate::runtime_host::world::load_authoritative_recovery_generation(&recovery_dir)
                    .map_err(ViewerRuntimeLiveServerError::Runtime)?
            {
                let metadata = committed.recovery_metadata;
                let generation = serde_json::from_slice::<
                    recovery_receipt::RuntimeAuthoritativeRecoveryGeneration,
                >(&metadata)
                .or_else(|generation_error| {
                    serde_json::from_slice::<AuthoritativeRecoveryAck<u64>>(&metadata)
                        .map(
                            |ack| recovery_receipt::RuntimeAuthoritativeRecoveryGeneration {
                                schema_version: 0,
                                reorg_epoch: ack.reorg_epoch,
                                ack,
                                authoritative_batches: VecDeque::new(),
                                next_authoritative_batch_id: 1,
                                authoritative_challenges: VecDeque::new(),
                                next_authoritative_challenge_id: 1,
                                stable_checkpoints: VecDeque::new(),
                                session_policy: RuntimeSessionPolicy::default(),
                                session_revoke_metadata: Vec::new(),
                                rollback_readiness: BTreeMap::new(),
                                consumed_strict_audit_nonces: BTreeSet::new(),
                                runtime_action_players: BTreeMap::new(),
                                consumed_rollback_operator_nonces: BTreeSet::new(),
                                session_side_effects: Default::default(),
                            },
                        )
                        .map_err(|_| generation_error)
                })
                .map_err(|err| ViewerRuntimeLiveServerError::Serde(err.to_string()))?;
                world = committed.world;
                if generation.ack.reorg_epoch != generation.reorg_epoch
                    || generation
                        .ack
                        .rollback_receipt
                        .as_ref()
                        .is_some_and(|receipt| {
                            world
                                .rollback_nonce_outcome(receipt.authorization_nonce.as_str())
                                .is_none_or(|outcome| {
                                    outcome.canonical_intent_hash != receipt.canonical_intent_digest
                                        || outcome.committed_reorg_epoch != generation.reorg_epoch
                                })
                        })
                {
                    return Err(ViewerRuntimeLiveServerError::Init(
                        "authoritative recovery generation metadata/world integrity mismatch"
                            .to_string(),
                    ));
                }
                recovered_generation = Some(generation);
            }
        }
        wake_dispatch::ensure_viewer_runtime_binding(&mut world, &config)?;
        let chain_linked = config.world_service.is_some()
            || config
                .chain_status_bind
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
        if !chain_linked {
            apply_provider_backed_bootstrap_authorities(
                &mut world,
                &config.provider_backed_bootstrap_authorities,
            )
            .map_err(ViewerRuntimeLiveServerError::Init)?;
        }
        let hosted_local_mock_test_lane_enabled =
            control_plane::hosted_local_mock_test_lane_enabled(config.hosted_public_join_mode);
        let hosted_local_mock_test_lane_active = if hosted_local_mock_test_lane_enabled
            && chain_linked
            && world.state().agents.is_empty()
        {
            true
        } else {
            control_plane::install_hosted_local_mock_test_capability_fixtures(
                &mut world,
                config.hosted_public_join_mode,
            )
            .map_err(ViewerRuntimeLiveServerError::Init)?
        };
        let initial_world_time = world.state().time;
        let world_service_query_state =
            crate::world_service::client::WorldServiceQueryState::default();
        let mut llm_sidecar = match seed_model.as_ref() {
            Some(model) => {
                RuntimeLlmSidecar::new(config.decision_mode).with_runtime_seed_model(model)
            }
            None => RuntimeLlmSidecar::new(config.decision_mode),
        };
        llm_sidecar.provider_service_required = config.world_service.is_some() || chain_linked;
        llm_sidecar.provider_service_config = config.world_service.clone();
        llm_sidecar.provider_service_lineage_store_explicit =
            config.world_service.is_some() && config.ensure_service_agent_lineage_store().is_ok();
        llm_sidecar.provider_service_query_state = world_service_query_state.clone();
        llm_sidecar.provider_service_signer = config.world_service_agent_signer.clone();
        if hosted_local_mock_test_lane_active {
            llm_sidecar.enable_hosted_local_mock_test_lane();
        }
        if let Some(provider_lineage_store) = config.provider_lineage_store_path() {
            llm_sidecar.configure_provider_lineage_store(provider_lineage_store);
            llm_sidecar
                .restore_provider_lineage(&world)
                .map_err(ViewerRuntimeLiveServerError::Init)?;
        }
        llm_sidecar.chunk_runtime = chunk_runtime;
        if let Some(generation) = recovered_generation.as_ref() {
            Self::restore_persisted_session_side_effects(
                &mut llm_sidecar,
                &generation.session_side_effects,
            );
        }
        let next_virtual_event_id = latest_runtime_event_seq(&world).saturating_add(1).max(1);
        let mut server = Self {
            auto_play_paused: !config.auto_play_on_connect,
            config,
            world,
            initial_world_time,
            next_auto_play_step_at: None,
            last_chain_committed_height: 0,
            verified_world_view: None,
            world_service_query_state,
            periodic_service_executor: None,
            prepared_owner_read: None,
            prepared_world_service_submission: None,
            pending_world_service_gameplay: Vec::new(),
            chain_observer_loader: Arc::new(Mutex::new(chain_link::ObserverLoader::default())),
            last_chain_observer_identity: None,
            confirmed_player_gameplay_progress_time: None,
            snapshot_config,
            seed_model,
            script: RuntimeLiveScript::default(),
            llm_sidecar,
            agent_service_pump_started: false,
            #[cfg(any(test, feature = "test_tier_required"))]
            agent_service_io_diagnostics: std::sync::Arc::default(),
            agent_service_session_count: 0,
            agent_service_pump_error: None,
            pending_virtual_events: recovered_generation
                .as_ref()
                .map(|generation| {
                    generation
                        .session_side_effects
                        .pending_virtual_events
                        .clone()
                })
                .unwrap_or_default(),
            next_virtual_event_id,
            authoritative_batches: recovered_generation
                .as_ref()
                .map(|generation| generation.authoritative_batches.clone())
                .unwrap_or_default(),
            next_authoritative_batch_id: recovered_generation
                .as_ref()
                .map(|generation| generation.next_authoritative_batch_id)
                .unwrap_or(1),
            authoritative_challenges: recovered_generation
                .as_ref()
                .map(|generation| generation.authoritative_challenges.clone())
                .unwrap_or_default(),
            next_authoritative_challenge_id: recovered_generation
                .as_ref()
                .map(|generation| generation.next_authoritative_challenge_id)
                .unwrap_or(1),
            stable_checkpoints: recovered_generation
                .as_ref()
                .map(|generation| generation.stable_checkpoints.clone())
                .unwrap_or_default(),
            reorg_epoch: recovered_generation
                .as_ref()
                .map(|generation| generation.reorg_epoch)
                .unwrap_or(0),
            session_policy: recovered_generation
                .as_ref()
                .map(|generation| generation.session_policy.clone())
                .unwrap_or_default(),
            session_revoke_metadata: recovered_generation
                .as_ref()
                .map(|generation| {
                    generation
                        .session_revoke_metadata
                        .iter()
                        .map(|entry| {
                            (
                                session_revoke_metadata_key(
                                    entry.player_id.as_str(),
                                    entry.session_pubkey.as_str(),
                                ),
                                entry.metadata.clone(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            rollback_readiness: recovered_generation
                .as_ref()
                .map(|generation| generation.rollback_readiness.clone())
                .unwrap_or_default(),
            consumed_strict_audit_nonces: recovered_generation
                .as_ref()
                .map(|generation| generation.consumed_strict_audit_nonces.clone())
                .unwrap_or_default(),
            settlement_ranking_gate: RuntimeSettlementRankingGate::default(),
            latest_player_gameplay_feedback: None,
            latest_player_gameplay_causality: None,
            scoped_gameplay_feedback: BTreeMap::new(),
            pending_gameplay_feedback: BTreeMap::new(),
            active_gameplay_feedback_origin: None,
            active_gameplay_feedback_authorized: false,
            next_gameplay_feedback_order: 0,
            runtime_action_players: recovered_generation
                .as_ref()
                .map(|generation| generation.runtime_action_players.clone())
                .unwrap_or_default(),
            consumed_rollback_operator_nonces: recovered_generation
                .as_ref()
                .map(|generation| generation.consumed_rollback_operator_nonces.clone())
                .unwrap_or_default(),
            prompt_control_authority,
            hosted_local_mock_test_lane_active,
            authoritative_recovery_write_fence: None,
            smelter_affordability_debug_agent_id: None,
            governance_vote_quote_debug_agent_id: None,
            #[cfg(test)]
            recovery_fault_injection: None,
            #[cfg(test)]
            authoritative_recovery_dir_override: None,
        };
        server.rebuild_settlement_ranking_gate();
        Ok(server)
    }
    pub fn run(self) -> Result<(), ViewerRuntimeLiveServerError> {
        let listener = TcpListener::bind(&self.config.bind_addr)?;
        let shared = Arc::new(Mutex::new(self));
        let diagnostics = periodic_connection_diagnostic::TraceSink::from_env();
        Self::start_agent_service_pump(&shared)?;
        for incoming in listener.incoming() {
            let stream = incoming?;
            let shared = Arc::clone(&shared);
            let diagnostics = diagnostics.clone();
            thread::spawn(move || {
                if let Err(err) = Self::serve_shared_stream(shared, stream, diagnostics) {
                    emit_stderr_or_event(
                        Level::WARN,
                        format!("viewer runtime live server error: {err:?}").as_str(),
                        "viewer runtime live server error",
                    );
                }
            });
        }
        Ok(())
    }

    pub fn run_once(&mut self) -> Result<(), ViewerRuntimeLiveServerError> {
        let listener = TcpListener::bind(&self.config.bind_addr)?;
        let (stream, _) = listener.accept()?;
        self.serve_stream(stream)
    }

    fn hosted_public_join_mode(&self) -> bool {
        self.config.hosted_public_join_mode
    }

    /// Validate and consume a server-issued Director visibility grant.
    pub fn consume_director_capability_grant(
        &mut self,
        grant: &crate::viewer::DirectorCapabilityGrant,
        expected_server: &str,
        required_signer_public_key: &str,
        now_unix_ms: u64,
    ) -> Result<(), String> {
        self.session_policy
            .validate_and_consume_director_capability_grant(
                grant,
                expected_server,
                required_signer_public_key,
                now_unix_ms,
            )
    }

    fn chain_link_enabled(&self) -> bool {
        self.config.world_service.is_some()
            || self
                .config
                .chain_status_bind
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
    }

    pub(super) fn world_service_client(
        &self,
    ) -> Result<
        Option<crate::world_service::client::RemoteWorldServiceClient>,
        ViewerRuntimeLiveServerError,
    > {
        self.config
            .world_service
            .clone()
            .map(
                |config| -> Result<_, crate::world_service::client::WorldServiceClientError> {
                    let formal = config.formal_read_trust.is_some();
                    let client =
                        crate::world_service::client::RemoteWorldServiceClient::new(config)?;
                    if formal && let Some(view) = &self.verified_world_view {
                        client.resume_formal_view(view)?;
                    }
                    Ok(client.with_query_state(self.world_service_query_state.clone()))
                },
            )
            .transpose()
            .map_err(|error| ViewerRuntimeLiveServerError::Init(error.to_string()))
    }

    fn supports_agent_chat(&self) -> bool {
        self.config.world_service.is_some()
            || self.llm_sidecar.supports_agent_chat()
            || self.config.agent_chat_echo_enabled
    }

    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_serve_shared_stream(
        shared: Arc<Mutex<Self>>,
        stream: TcpStream,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        Self::serve_shared_stream(
            shared,
            stream,
            periodic_connection_diagnostic::TraceSink::from_env(),
        )
    }

    #[cfg(test)]
    fn test_serve_shared_stream_with_connection(
        shared: Arc<Mutex<Self>>,
        stream: TcpStream,
        connection: Option<periodic_connection_diagnostic::ConnectionTrace>,
        session_already_primed: bool,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        Self::serve_shared_stream_with_connection(
            shared,
            stream,
            connection,
            session_already_primed,
        )
    }

    fn serve_shared_stream(
        shared: Arc<Mutex<Self>>,
        stream: TcpStream,
        diagnostics: Option<Arc<periodic_connection_diagnostic::TraceSink>>,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let connection = diagnostics.as_ref().map(TraceSink::connection);
        Self::serve_shared_stream_with_connection(shared, stream, connection, false)
    }

    fn serve_shared_stream_with_connection(
        shared: Arc<Mutex<Self>>,
        stream: TcpStream,
        connection: Option<periodic_connection_diagnostic::ConnectionTrace>,
        session_already_primed: bool,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let result = Self::serve_shared_stream_inner(
            shared,
            stream,
            connection.clone(),
            session_already_primed,
        );
        if let Some(connection) = connection {
            connection.emit_close(connection.close_snapshot());
        }
        result
    }

    fn serve_shared_stream_inner(
        shared: Arc<Mutex<Self>>,
        stream: TcpStream,
        connection: Option<periodic_connection_diagnostic::ConnectionTrace>,
        session_already_primed: bool,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        Self::start_agent_service_pump(&shared)?;
        let mut agent_presence = agent_service_pump::AgentServicePresence::new(&shared);
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_millis(50)))?;

        let reader_stream = stream.try_clone()?;
        let mut reader = BufReader::new(reader_stream);
        let mut socket = stream;
        let limits = {
            let server = lock_shared_server(&shared)?;
            response_outbox::ResponseLimits::from_config(&server.config)
        };
        let mut session = RuntimeLiveSession::new_with_playing(false);
        session.chain_runtime_authoritatively_primed = session_already_primed;
        let mut periodic_io = periodic_service_io::SessionPoll::with_connection(connection.clone());

        loop {
            Self::start_agent_service_pump(&shared)?;
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    if let Some(connection) = &connection {
                        connection
                            .set_close_reason(periodic_connection_diagnostic::CloseReason::Eof);
                    }
                    return Ok(());
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        let mut output = response_outbox::ResponseOutbox::new(limits);
                        let mut handled = Ok(());
                        match agency_control::parse_agency_control_frame(trimmed) {
                            agency_control::ParsedAgencyControlFrame::Request(request) => {
                                let response = {
                                    let mut server = lock_shared_server(&shared)?;
                                    server.handle_agency_control_request(request)
                                };
                                agency_control::write_agency_control_response(
                                    &mut output,
                                    &response,
                                )?;
                            }
                            agency_control::ParsedAgencyControlFrame::Invalid(response) => {
                                agency_control::write_agency_control_response(
                                    &mut output,
                                    &response,
                                )?;
                            }
                            agency_control::ParsedAgencyControlFrame::NotAgencyControl => {
                                if let Ok(request) = serde_json::from_str::<ViewerRequest>(trimmed)
                                {
                                    let chain_prime = Self::prime_shared_request_if_needed(
                                        &shared, &request, &session,
                                    )?;
                                    let prepared_submission =
                                        Self::prepare_shared_world_service_submission(
                                            &shared, &request,
                                        )?;
                                    let prepared_owner_read = Self::prepare_shared_owner_read(
                                        &shared, &request, &session,
                                    )?;
                                    handled = {
                                        let mut server = lock_shared_server(&shared)?;
                                        server.prepared_owner_read =
                                            prepared_owner_read.map(Box::new);
                                        server.prepared_world_service_submission =
                                            prepared_submission;
                                        let handled = server.handle_request_with_chain_prime(
                                            request,
                                            &mut session,
                                            &mut output,
                                            chain_prime,
                                        );
                                        server.prepared_world_service_submission = None;
                                        server.prepared_owner_read = None;
                                        agent_presence.observe(&mut server, &session);
                                        handled
                                    };
                                }
                            }
                        }
                        if let Err(error) = output.deliver(&mut socket) {
                            if let Some(connection) = &connection {
                                connection.set_close_reason(
                                    periodic_connection_diagnostic::CloseReason::WriteError,
                                );
                            }
                            return Err(error.into());
                        }
                        if let Err(error) = handled {
                            if let Some(connection) = &connection {
                                connection.set_close_reason(
                                    periodic_connection_diagnostic::CloseReason::RequestError,
                                );
                            }
                            return Err(error);
                        }
                    }
                }
                Err(err) if is_timeout_error(&err) => {}
                Err(err) if is_expected_disconnect_error(&err) => {
                    if let Some(connection) = &connection {
                        connection.set_close_reason(
                            periodic_connection_diagnostic::CloseReason::ReadError,
                        );
                    }
                    return Ok(());
                }
                Err(err) => {
                    if let Some(connection) = &connection {
                        connection.set_close_reason(
                            periodic_connection_diagnostic::CloseReason::ReadError,
                        );
                    }
                    return Err(ViewerRuntimeLiveServerError::Io(err));
                }
            }

            let (write_fenced, chain_link_enabled, chain_poll_interval, service_mode) = {
                let server = lock_shared_server(&shared)?;
                (
                    server.authoritative_recovery_write_fence.is_some(),
                    server.chain_link_enabled(),
                    server.config.chain_poll_interval,
                    server.config.world_service.is_some(),
                )
            };
            let mut output = response_outbox::ResponseOutbox::new(limits);
            if service_mode
                && periodic_io
                    .poll(&shared, &mut session, &mut output)
                    .is_err()
            {
                emit_stderr_or_event(
                    Level::WARN,
                    "viewer periodic service read deferred",
                    "viewer periodic service read deferred",
                );
            }
            if !write_fenced
                && !service_mode
                && chain_link_enabled
                && session.initial_snapshot_sent
                && session.should_poll_chain(chain_poll_interval)
                && let Err(err) = Self::sync_chain_linked_runtime_minimized_lock(
                    &shared,
                    &mut session,
                    &mut output,
                )
            {
                emit_stderr_or_event(
                    Level::WARN,
                    format!("viewer runtime live: chain sync skipped: {err:?}").as_str(),
                    "viewer runtime live chain sync skipped",
                );
            }

            let driven = {
                let mut server = lock_shared_server(&shared)?;
                agent_presence.observe(&mut server, &session);
                if server.authoritative_recovery_write_fence.is_none()
                    && server.config.world_service.is_none()
                {
                    server.drive_auto_play(&mut session, &mut output)
                } else {
                    Ok(())
                }
            };
            if let Err(error) = periodic_io.deliver_output(&mut output, &mut socket) {
                if let Some(connection) = &connection {
                    connection
                        .set_close_reason(periodic_connection_diagnostic::CloseReason::WriteError);
                }
                return Err(error.into());
            }
            driven?;
        }
    }
}
