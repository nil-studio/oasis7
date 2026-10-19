use std::collections::HashSet;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::runtime::MajorWorldEventVisibilityPermission;
#[cfg(test)]
use crate::runtime::MaterialLedgerId;
#[cfg(test)]
use crate::simulator::RuntimePerfHealth;
use crate::simulator::RuntimePerfSnapshot;
use crate::simulator::unsupported_runtime_perf_snapshot;
#[cfg(test)]
use crate::viewer::gameplay_actions::{
    STARTER_INDUSTRIAL_ELECTRICITY, formal_release_default_seed_model,
};
use oasis7_simulator::{ChunkRuntimeConfig, WorldModel};

use super::*;

pub(crate) use oasis7_client_api::bootstrap::FORMAL_RELEASE_DEFAULT_WORLD_ID;
pub(crate) const FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID: &str = "starter-agent-0";

impl ViewerRuntimeLiveServerConfig {
    pub fn new(scenario: WorldScenario) -> Self {
        Self {
            response_frame_max_bytes: 16 * 1024 * 1024,
            response_turn_max_bytes: 64 * 1024 * 1024,
            response_turn_max_frames: 256,
            response_write_timeout: Duration::from_secs(2),
            bind_addr: "127.0.0.1:5010".to_string(),
            world_id: format!("live-runtime-{}", scenario.as_str()),
            scenario: Some(scenario),
            decision_mode: ViewerLiveDecisionMode::Script,
            play_step_interval: Duration::from_millis(800),
            chain_poll_interval: Duration::from_millis(200),
            auto_play_on_connect: false,
            hosted_public_join_mode: false,
            chain_status_bind: None,
            chain_execution_world_dir: None,
            chain_submit_bind: None,
            world_service: None,
            world_service_agent_signer: None,
            chain_link_policy: ChainLinkPolicy::Enforcing,
            agent_chat_echo_enabled: control_plane::runtime_agent_chat_echo_enabled_from_env(),
            major_world_event_visibility: MajorWorldEventVisibilityPermission::Unknown,
            generated_world_dir: None,
            provider_lineage_store: None,
            prompt_result_cache_capacity: config::DEFAULT_PROMPT_RESULT_CACHE_CAPACITY,
            prompt_result_receipt_max_bytes: config::DEFAULT_PROMPT_RESULT_RECEIPT_MAX_BYTES,
            provider_backed_bootstrap_authorities: Vec::new(),
            #[cfg(test)]
            test_cognition_runtime_binding: None,
        }
    }

    pub fn formal_release_default() -> Self {
        Self {
            response_frame_max_bytes: 16 * 1024 * 1024,
            response_turn_max_bytes: 64 * 1024 * 1024,
            response_turn_max_frames: 256,
            response_write_timeout: Duration::from_secs(2),
            bind_addr: "127.0.0.1:5010".to_string(),
            world_id: FORMAL_RELEASE_DEFAULT_WORLD_ID.to_string(),
            scenario: None,
            decision_mode: ViewerLiveDecisionMode::Script,
            play_step_interval: Duration::from_millis(800),
            chain_poll_interval: Duration::from_millis(200),
            auto_play_on_connect: false,
            hosted_public_join_mode: false,
            chain_status_bind: None,
            chain_execution_world_dir: None,
            chain_submit_bind: None,
            world_service: None,
            world_service_agent_signer: None,
            chain_link_policy: ChainLinkPolicy::Enforcing,
            agent_chat_echo_enabled: control_plane::runtime_agent_chat_echo_enabled_from_env(),
            major_world_event_visibility: MajorWorldEventVisibilityPermission::Unknown,
            generated_world_dir: None,
            provider_lineage_store: None,
            prompt_result_cache_capacity: config::DEFAULT_PROMPT_RESULT_CACHE_CAPACITY,
            prompt_result_receipt_max_bytes: config::DEFAULT_PROMPT_RESULT_RECEIPT_MAX_BYTES,
            provider_backed_bootstrap_authorities: Vec::new(),
            #[cfg(test)]
            test_cognition_runtime_binding: None,
        }
    }

    pub fn with_bind_addr(mut self, addr: impl Into<String>) -> Self {
        self.bind_addr = addr.into();
        self
    }

    pub fn with_world_id(mut self, world_id: impl Into<String>) -> Self {
        self.world_id = world_id.into();
        self
    }

    #[cfg(test)]
    pub(crate) fn with_test_cognition_runtime_binding(
        mut self,
        branch_id: impl Into<String>,
        finality_epoch: u64,
        finality_block_hash: Option<String>,
        finality_status: impl Into<String>,
        reorg_epoch: u64,
    ) -> Self {
        self.test_cognition_runtime_binding = Some((
            branch_id.into(),
            finality_epoch,
            finality_block_hash,
            finality_status.into(),
            reorg_epoch,
        ));
        self
    }

    pub fn with_optional_scenario(mut self, scenario: Option<WorldScenario>) -> Self {
        self.scenario = scenario;
        if self.world_id.trim().is_empty() {
            self.world_id = scenario
                .map(|value| format!("live-runtime-{}", value.as_str()))
                .unwrap_or_else(|| FORMAL_RELEASE_DEFAULT_WORLD_ID.to_string());
        }
        self
    }

    pub fn with_decision_mode(mut self, mode: ViewerLiveDecisionMode) -> Self {
        self.decision_mode = mode;
        self
    }

    pub fn with_llm_mode(mut self, enabled: bool) -> Self {
        self.decision_mode = if enabled {
            ViewerLiveDecisionMode::Llm
        } else {
            ViewerLiveDecisionMode::Script
        };
        self
    }

    pub fn with_play_step_interval(mut self, interval: Duration) -> Self {
        self.play_step_interval = interval.max(Duration::from_millis(50));
        self
    }

    pub fn with_chain_poll_interval(mut self, interval: Duration) -> Self {
        self.chain_poll_interval = interval.max(Duration::from_millis(50));
        self
    }

    pub fn with_auto_play_on_connect(mut self, enabled: bool) -> Self {
        self.auto_play_on_connect = enabled;
        self
    }

    pub fn with_hosted_public_join_mode(mut self, enabled: bool) -> Self {
        self.hosted_public_join_mode = enabled;
        self
    }

    pub fn with_chain_execution_world_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.chain_execution_world_dir = Some(path.into());
        self
    }

    pub fn with_chain_status_bind(mut self, addr: impl Into<String>) -> Self {
        self.chain_status_bind = Some(addr.into());
        self
    }

    pub fn with_chain_submit_bind(mut self, addr: impl Into<String>) -> Self {
        self.chain_submit_bind = Some(addr.into());
        self
    }

    pub fn with_chain_link_policy(mut self, policy: ChainLinkPolicy) -> Self {
        self.chain_link_policy = policy;
        self
    }

    pub fn with_agent_chat_echo_enabled(mut self, enabled: bool) -> Self {
        self.agent_chat_echo_enabled = enabled;
        self
    }

    pub fn with_major_world_event_visibility(
        mut self,
        visibility: MajorWorldEventVisibilityPermission,
    ) -> Self {
        self.major_world_event_visibility = visibility;
        self
    }

    pub fn with_generated_world_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.generated_world_dir = Some(dir.into());
        self
    }

    /// Select the operator-owned durable provider lineage checkpoint. When a
    /// generated world is configured and this is omitted, the checkpoint is
    /// kept beside that world's immutable sidecar.
    pub fn with_provider_lineage_store(mut self, path: impl Into<PathBuf>) -> Self {
        self.provider_lineage_store = Some(path.into());
        self
    }

    pub fn with_prompt_result_cache_capacity(mut self, capacity: usize) -> Self {
        self.prompt_result_cache_capacity = capacity;
        self
    }

    pub fn with_prompt_result_receipt_max_bytes(mut self, max_bytes: usize) -> Self {
        self.prompt_result_receipt_max_bytes = max_bytes;
        self
    }

    /// Add one explicit, proof-bearing ProviderBacked authority bundle.
    /// Runtime owns validation, installation, and cognition provisioning.
    pub fn with_provider_backed_bootstrap_authority(
        mut self,
        authority: ProviderBackedBootstrapAuthorityV1,
    ) -> Self {
        self.provider_backed_bootstrap_authorities.push(authority);
        self
    }

    pub(super) fn provider_lineage_store_path(&self) -> Option<PathBuf> {
        self.provider_lineage_store.clone().or_else(|| {
            self.generated_world_dir
                .as_deref()
                .map(|dir| dir.join("runtime-live-provider-lineage.json"))
        })
    }
}

pub(super) fn apply_provider_backed_bootstrap_authorities(
    world: &mut RuntimeWorld,
    authorities: &[ProviderBackedBootstrapAuthorityV1],
) -> Result<(), String> {
    if authorities.is_empty() {
        return Ok(());
    }
    world
        .bootstrap_provider_backed_authorities(authorities)
        .map(|_| ())
        .map_err(|error| format!("ProviderBacked authority bootstrap failed: {error:?}"))
}

pub(super) struct RuntimeLiveSession {
    pub(super) world_view_projection: Option<crate::viewer::world_view::SessionProjection>,
    pub(super) owner_read_context: Option<super::owner_read::OwnerReadContext>,
    pub(super) owner_read_commit: Option<oasis7_client_api::world_service::CommitRef>,
    pub(super) owner_read_view: Option<oasis7_proto::viewer::CanonicalAgentChatViewV1>,
    pub(super) subscribed: HashSet<ViewerStream>,
    pub(super) event_filters: Option<HashSet<ViewerEventKind>>,
    pub(super) current_player_id: Option<String>,
    pub(super) gameplay_feedback_owner:
        Option<super::gameplay_feedback_scope::GameplayFeedbackOwner>,
    pub(super) local_gameplay_feedback:
        Arc<Mutex<super::gameplay_feedback_scope::SessionGameplayFeedback>>,
    pub(super) playing: bool,
    pub(super) next_play_step_at: Option<Instant>,
    pub(super) next_background_snapshot_at: Option<Instant>,
    pub(super) next_chain_poll_at: Option<Instant>,
    pub(super) metrics: RunnerMetrics,
    pub(super) transient_play_failures: u8,
    pub(super) initial_snapshot_sent: bool,
    // Projection shared by the server is not evidence that this session's
    // snapshot was assembled from that version.
    pub(super) last_world_service_snapshot: Option<(
        oasis7_client_api::world_service::ProjectionVersion,
        oasis7_client_api::world_service::EventCursor,
    )>,
    pub(super) last_world_service_snapshot_source: Option<String>,
    // A chain prime may be reused only by the same session that requested it.
    // The server projection is shared, but the proof that its first snapshot
    // was freshly authorized is not.
    pub(super) chain_runtime_authoritatively_primed: bool,
    pub(super) negotiated_protocol: crate::viewer::protocol::NegotiatedViewerProtocol,
}

impl RuntimeLiveSession {
    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::new_with_playing(false)
    }

    pub(super) fn new_with_playing(playing: bool) -> Self {
        Self {
            world_view_projection: None,
            owner_read_context: None,
            owner_read_view: None,
            owner_read_commit: None,
            subscribed: HashSet::new(),
            event_filters: None,
            current_player_id: None,
            gameplay_feedback_owner: None,
            local_gameplay_feedback: Arc::new(Mutex::new(Default::default())),
            playing,
            next_play_step_at: None,
            next_background_snapshot_at: None,
            next_chain_poll_at: None,
            metrics: RunnerMetrics::default(),
            transient_play_failures: 0,
            initial_snapshot_sent: false,
            last_world_service_snapshot: None,
            last_world_service_snapshot_source: None,
            chain_runtime_authoritatively_primed: false,
            negotiated_protocol:
                crate::viewer::protocol::NegotiatedViewerProtocol::v1_without_capabilities(),
        }
    }

    pub(super) fn event_allowed(&self, event: &WorldEvent) -> bool {
        match &self.event_filters {
            Some(filters) => filters
                .iter()
                .any(|filter| viewer_event_kind_matches(filter, &event.kind)),
            None => true,
        }
    }

    pub(super) fn explicitly_subscribed_to(&self, stream: ViewerStream) -> bool {
        self.subscribed.contains(&stream)
    }

    pub(super) fn uses_default_subscription(&self) -> bool {
        self.subscribed.is_empty()
    }

    pub(super) fn wants_initial_snapshot(&self) -> bool {
        self.uses_default_subscription() || self.explicitly_subscribed_to(ViewerStream::Snapshot)
    }

    pub(super) fn wants_initial_recovery_metadata(&self) -> bool {
        self.uses_default_subscription()
            || self.explicitly_subscribed_to(ViewerStream::Snapshot)
            || self.explicitly_subscribed_to(ViewerStream::Events)
    }

    pub(super) fn should_emit_background_snapshot(&mut self, interval: Duration) -> bool {
        let now = Instant::now();
        if let Some(next_snapshot_at) = self.next_background_snapshot_at
            && now < next_snapshot_at
        {
            return false;
        }
        self.next_background_snapshot_at = Some(now + interval);
        true
    }

    pub(super) fn should_poll_chain(&mut self, interval: Duration) -> bool {
        let now = Instant::now();
        if let Some(next_poll_at) = self.next_chain_poll_at
            && now < next_poll_at
        {
            return false;
        }
        self.next_chain_poll_at = Some(now + interval);
        true
    }
}

#[cfg(test)]
pub(super) fn bootstrap_runtime_world(
    scenario: WorldScenario,
) -> Result<(RuntimeWorld, WorldConfig), String> {
    let (world, config, _) = bootstrap_runtime_world_with_chunk_runtime(scenario)?;
    Ok((world, config))
}

pub(super) fn bootstrap_runtime_live_world(
    config: &ViewerRuntimeLiveServerConfig,
) -> Result<
    (
        RuntimeWorld,
        WorldConfig,
        Option<WorldModel>,
        ChunkRuntimeConfig,
    ),
    String,
> {
    if config.world_service.is_some() {
        return Ok((
            crate::runtime_host::world::new_production_hardened(),
            WorldConfig::default(),
            None,
            ChunkRuntimeConfig::default(),
        ));
    }
    if let Some(generated_world_dir) = config.generated_world_dir.as_deref() {
        let (world, snapshot_config, seed_model, chunk_runtime) =
            bootstrap_generated_sidecar_runtime_world_with_chunk_runtime(generated_world_dir)?;
        return Ok((world, snapshot_config, Some(seed_model), chunk_runtime));
    }

    match config.scenario {
        Some(scenario) => {
            let (world, snapshot_config, chunk_runtime) =
                bootstrap_runtime_world_with_chunk_runtime(scenario)?;
            Ok((world, snapshot_config, None, chunk_runtime))
        }
        None if config.world_service.is_some() || config.chain_status_bind.is_some() => Ok((
            crate::runtime_host::world::new_production_hardened(),
            WorldConfig::default(),
            None,
            ChunkRuntimeConfig::default(),
        )),
        None => {
            let (world, snapshot_config) = bootstrap_formal_release_runtime_world()?;
            Ok((world, snapshot_config, None, ChunkRuntimeConfig::default()))
        }
    }
}

pub(super) use oasis7::runtime_host::bootstrap::*;

pub(super) fn runtime_metrics(world: &RuntimeWorld) -> RunnerMetrics {
    let total_ticks = world.state().time;
    let total_actions = world.journal().len() as u64;
    let action_rejected = world
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.body,
                RuntimeWorldEventBody::Domain(RuntimeDomainEvent::ActionRejected { .. })
            )
        })
        .count() as u64;

    RunnerMetrics {
        total_ticks,
        total_agents: world.state().agents.len(),
        agents_active: world.state().agents.len(),
        agents_quota_exhausted: 0,
        total_actions,
        total_decisions: 0,
        actions_per_tick: if total_ticks > 0 {
            total_actions as f64 / total_ticks as f64
        } else {
            0.0
        },
        decisions_per_tick: 0.0,
        success_rate: if total_actions > 0 {
            (total_actions.saturating_sub(action_rejected)) as f64 / total_actions as f64
        } else {
            0.0
        },
        runtime_perf: unsupported_runtime_live_perf_snapshot(),
    }
}

pub(super) fn unsupported_runtime_live_perf_snapshot() -> RuntimePerfSnapshot {
    unsupported_runtime_perf_snapshot()
}

pub(super) fn latest_runtime_event_seq(world: &RuntimeWorld) -> u64 {
    world
        .journal()
        .events
        .last()
        .map(|event| event.id)
        .unwrap_or(0)
}

pub(super) fn send_response(
    writer: &mut dyn Write,
    response: &ViewerResponse,
) -> Result<(), ViewerRuntimeLiveServerError> {
    serde_json::to_writer(&mut *writer, response)
        .map_err(|err| ViewerRuntimeLiveServerError::Serde(err.to_string()))?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

pub(super) fn is_timeout_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

pub(super) fn lock_shared_server(
    shared: &Arc<Mutex<ViewerRuntimeLiveServer>>,
) -> Result<MutexGuard<'_, ViewerRuntimeLiveServer>, ViewerRuntimeLiveServerError> {
    shared.lock().map_err(|_| {
        ViewerRuntimeLiveServerError::Io(io::Error::other(
            "viewer runtime live shared state poisoned",
        ))
    })
}

pub(super) fn is_expected_disconnect_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::NotConnected
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_lineage_store_path_requires_explicit_formal_path() {
        let formal = ViewerRuntimeLiveServerConfig::formal_release_default();
        assert_eq!(formal.provider_lineage_store_path(), None);

        let generated = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_generated_world_dir("/var/lib/oasis7/generated-world");
        assert_eq!(
            generated.provider_lineage_store_path(),
            Some(PathBuf::from(
                "/var/lib/oasis7/generated-world/runtime-live-provider-lineage.json"
            ))
        );

        let explicit = formal.with_provider_lineage_store("/var/lib/oasis7/formal-lineage.json");
        assert_eq!(
            explicit.provider_lineage_store_path(),
            Some(PathBuf::from("/var/lib/oasis7/formal-lineage.json"))
        );
    }

    #[test]
    fn default_subscription_requests_initial_snapshot_and_recovery_metadata() {
        let session = RuntimeLiveSession::new();

        assert!(session.uses_default_subscription());
        assert!(session.wants_initial_snapshot());
        assert!(session.wants_initial_recovery_metadata());
        assert!(!session.explicitly_subscribed_to(ViewerStream::Snapshot));
        assert!(!session.explicitly_subscribed_to(ViewerStream::Events));
        assert!(!session.explicitly_subscribed_to(ViewerStream::Metrics));
    }

    #[test]
    fn explicit_subscriptions_drive_initial_request_semantics() {
        let mut snapshot_session = RuntimeLiveSession::new();
        snapshot_session.subscribed.insert(ViewerStream::Snapshot);
        assert!(!snapshot_session.uses_default_subscription());
        assert!(snapshot_session.wants_initial_snapshot());
        assert!(snapshot_session.wants_initial_recovery_metadata());

        let mut event_session = RuntimeLiveSession::new();
        event_session.subscribed.insert(ViewerStream::Events);
        assert!(!event_session.wants_initial_snapshot());
        assert!(event_session.wants_initial_recovery_metadata());

        let mut metrics_session = RuntimeLiveSession::new();
        metrics_session.subscribed.insert(ViewerStream::Metrics);
        assert!(!metrics_session.wants_initial_snapshot());
        assert!(!metrics_session.wants_initial_recovery_metadata());
    }

    #[test]
    fn bootstrap_runtime_world_defaults_to_production_release_policy() {
        let (world, _) =
            bootstrap_runtime_world(WorldScenario::Minimal).expect("bootstrap runtime live world");
        assert!(world.release_security_policy().is_production_hardened());
    }

    #[test]
    fn runtime_live_metrics_mark_runtime_perf_unknown_when_unavailable() {
        let perf = unsupported_runtime_live_perf_snapshot();
        assert_eq!(perf.health, RuntimePerfHealth::Unknown);
        assert_eq!(perf.tick.samples_total, 0);
        assert_eq!(perf.tick.samples_window, 0);
    }

    #[test]
    fn bootstrap_formal_release_runtime_world_uses_seeded_fragment_bootstrap_agent() {
        let (world, _) =
            bootstrap_formal_release_runtime_world().expect("formal release bootstrap");
        let agent = world
            .state()
            .agents
            .get(FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID)
            .expect("bootstrap agent should exist");
        let (_, seed_model) =
            formal_release_default_seed_model().expect("formal release seed model");
        let seed_agent = seed_model
            .agents
            .get(FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID)
            .expect("seed bootstrap agent should exist");
        let seed_location = seed_model
            .locations
            .get(&seed_agent.location_id)
            .expect("seed bootstrap location should exist");
        assert_eq!(world.state().agents.len(), 1);
        assert_eq!(agent.state.pos, seed_agent.pos);
        assert!(seed_agent.location_id.starts_with("frag-"));
        assert!(seed_location.fragment_budget.is_some());
        assert_eq!(
            agent.state.resources.get(ResourceKind::Electricity),
            STARTER_INDUSTRIAL_ELECTRICITY
        );
        assert_eq!(agent.state.resources.get(ResourceKind::Data), 8);
        let smelter_ledger = crate::runtime::MaterialLedgerId::site("site-smelter");
        assert_eq!(
            world.ledger_material_balance(&smelter_ledger, "iron_ore"),
            60
        );
        assert_eq!(
            world.ledger_material_balance(&smelter_ledger, "carbon_fuel"),
            20
        );
    }

    #[test]
    fn bootstrap_formal_release_owner_can_fund_first_twelve_batch_smelter_run() {
        let (mut world, _) =
            bootstrap_formal_release_runtime_world().expect("formal release bootstrap");
        let starter_agent = FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID;

        let build = crate::viewer::gameplay_actions::build_runtime_action_from_gameplay_request(
            &crate::viewer::GameplayActionRequest {
                action_id: crate::viewer::ACTION_BUILD_SMELTER_MK1.to_string(),
                target_agent_id: starter_agent.to_string(),
                actor_agent_id: None,
                player_id: "bootstrap-test".to_string(),
                public_key: None,
                auth: None,
            },
        )
        .expect("formal bootstrap smelter action");
        world.submit_action(build);
        world.step().expect("settle formal smelter construction");
        world.step().expect("complete formal smelter construction");
        assert!(world.has_factory(crate::viewer::FACTORY_SMELTER_MK1));

        assert_eq!(
            world
                .agent_resource_balance(starter_agent, ResourceKind::Electricity)
                .expect("starter agent power after construction"),
            122,
            "construction must leave the first recipe plus assembler-build budget"
        );

        let schedule = crate::viewer::gameplay_actions::build_runtime_action_from_gameplay_request(
            &crate::viewer::GameplayActionRequest {
                action_id: crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT.to_string(),
                target_agent_id: starter_agent.to_string(),
                actor_agent_id: None,
                player_id: "bootstrap-test".to_string(),
                public_key: None,
                auth: None,
            },
        )
        .expect("formal bootstrap first smelter recipe action");
        world.submit_action(schedule);
        world.step().expect("settle formal first smelter recipe");

        assert_eq!(
            world
                .agent_resource_balance(starter_agent, ResourceKind::Electricity)
                .expect("starter agent power after first recipe admission"),
            26,
            "the canonical first run must retain the assembler construction obligation"
        );
        let completed_before = world.state().industry_progress.completed_recipe_jobs;
        for _ in 0..12 {
            world.step().expect("complete formal first smelter recipe");
            if world.state().industry_progress.completed_recipe_jobs > completed_before {
                break;
            }
        }
        assert!(world.state().industry_progress.completed_recipe_jobs > completed_before);

        let build_assembler =
            crate::viewer::gameplay_actions::build_runtime_action_from_gameplay_request(
                &crate::viewer::GameplayActionRequest {
                    action_id: crate::viewer::ACTION_BUILD_ASSEMBLER_MK1.to_string(),
                    target_agent_id: starter_agent.to_string(),
                    actor_agent_id: None,
                    player_id: "bootstrap-test".to_string(),
                    public_key: None,
                    auth: None,
                },
            )
            .expect("formal bootstrap assembler action");
        world.submit_action(build_assembler);
        world.step().expect("settle formal assembler construction");
        world
            .step()
            .expect("complete formal assembler construction");
        assert!(world.has_factory(crate::viewer::FACTORY_ASSEMBLER_MK1));
    }

    #[test]
    fn generated_bootstrap_owner_can_build_assembler_from_builder_ledger() {
        let (mut world, _) =
            bootstrap_runtime_world(WorldScenario::Minimal).expect("generated bootstrap");
        let starter_agent = world
            .state()
            .agents
            .keys()
            .next()
            .cloned()
            .expect("generated bootstrap agent");
        let builder_ledger = MaterialLedgerId::agent(starter_agent.as_str());
        for (material, expected) in [
            ("structural_frame", 20),
            ("iron_ingot", 10),
            ("copper_wire", 8),
        ] {
            assert_eq!(
                world.ledger_material_balance(&builder_ledger, material),
                expected,
                "generated bootstrap must seed {material} on the authoritative builder ledger"
            );
        }
        assert_eq!(
            world
                .agent_resource_balance(starter_agent.as_str(), ResourceKind::Electricity)
                .expect("generated bootstrap starter power"),
            STARTER_INDUSTRIAL_ELECTRICITY
        );

        let build_smelter =
            crate::viewer::gameplay_actions::build_runtime_action_from_gameplay_request(
                &crate::viewer::GameplayActionRequest {
                    action_id: crate::viewer::ACTION_BUILD_SMELTER_MK1.to_string(),
                    target_agent_id: starter_agent.clone(),
                    actor_agent_id: None,
                    player_id: "bootstrap-test".to_string(),
                    public_key: None,
                    auth: None,
                },
            )
            .expect("generated bootstrap smelter action");
        world.submit_action(build_smelter);
        world.step().expect("settle generated smelter construction");
        world
            .step()
            .expect("complete generated smelter construction");
        assert!(world.has_factory(crate::viewer::FACTORY_SMELTER_MK1));

        let smelter_run =
            crate::viewer::gameplay_actions::build_runtime_action_from_gameplay_request(
                &crate::viewer::GameplayActionRequest {
                    action_id: crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT.to_string(),
                    target_agent_id: starter_agent.clone(),
                    actor_agent_id: None,
                    player_id: "bootstrap-test".to_string(),
                    public_key: None,
                    auth: None,
                },
            )
            .expect("generated bootstrap first smelter recipe action");
        world.submit_action(smelter_run);
        world.step().expect("settle generated first smelter recipe");
        let completed_before = world.state().industry_progress.completed_recipe_jobs;
        for _ in 0..12 {
            world
                .step()
                .expect("complete generated first smelter recipe");
            if world.state().industry_progress.completed_recipe_jobs > completed_before {
                break;
            }
        }
        assert!(world.state().industry_progress.completed_recipe_jobs > completed_before);

        let build_assembler =
            crate::viewer::gameplay_actions::build_runtime_action_from_gameplay_request(
                &crate::viewer::GameplayActionRequest {
                    action_id: crate::viewer::ACTION_BUILD_ASSEMBLER_MK1.to_string(),
                    target_agent_id: starter_agent.clone(),
                    actor_agent_id: None,
                    player_id: "bootstrap-test".to_string(),
                    public_key: None,
                    auth: None,
                },
            )
            .expect("generated bootstrap assembler action");
        world.submit_action(build_assembler);
        world
            .step()
            .expect("settle generated assembler construction");
        world
            .step()
            .expect("complete generated assembler construction");

        assert!(world.has_factory(crate::viewer::FACTORY_ASSEMBLER_MK1));

        let gear = crate::viewer::gameplay_actions::build_runtime_action_from_gameplay_request(
            &crate::viewer::GameplayActionRequest {
                action_id: crate::viewer::ACTION_SCHEDULE_ASSEMBLER_GEAR.to_string(),
                target_agent_id: starter_agent,
                actor_agent_id: None,
                player_id: "bootstrap-test".to_string(),
                public_key: None,
                auth: None,
            },
        )
        .expect("generated bootstrap first assembler recipe action");
        world.submit_action(gear);
        world
            .step()
            .expect("settle generated first assembler recipe");
        assert_eq!(world.pending_recipe_jobs_len(), 1);
        world
            .step()
            .expect("complete generated first assembler recipe");
        assert_eq!(
            world.ledger_material_balance(&MaterialLedgerId::site("site-assembler"), "gear"),
            4
        );
    }
}
