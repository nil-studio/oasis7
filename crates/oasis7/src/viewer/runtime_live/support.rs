use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::runtime::{
    AgentLocationAuthorityV1, FactoryConstructionPowerMode, FactoryConstructionPowerProfileV1,
    FactoryProfileV1, FactorySiteAuthorityV1, LocationAnchorV1,
    MajorWorldEventVisibilityPermission, MaterialLedgerId,
};
#[cfg(test)]
use crate::simulator::RuntimePerfHealth;
use crate::simulator::runtime_perf::unsupported_runtime_perf_snapshot;
use crate::simulator::{RuntimePerfSnapshot, WorldKernel};
use crate::viewer::gameplay_actions::{
    FACTORY_ASSEMBLER_MK1, FACTORY_SMELTER_MK1, STARTER_INDUSTRIAL_ELECTRICITY,
    formal_release_default_seed_model,
};
use oasis7_simulator::{ChunkRuntimeConfig, Location, WorldModel};

use super::*;

pub(crate) const FORMAL_RELEASE_DEFAULT_WORLD_ID: &str = "live-formal-release-default";
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

pub(super) fn bootstrap_runtime_world_with_chunk_runtime(
    scenario: WorldScenario,
) -> Result<(RuntimeWorld, WorldConfig, ChunkRuntimeConfig), String> {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(scenario, &config);
    let (model, _) = build_world_model(&config, &init)
        .map_err(|err| format!("runtime live bootstrap build_world_model failed: {err:?}"))?;
    let chunk_runtime = ChunkRuntimeConfig {
        world_seed: init.seed,
        asteroid_fragment_enabled: init.asteroid_fragment.enabled,
        asteroid_fragment_seed_offset: init.asteroid_fragment.seed_offset,
        min_fragment_spacing_cm: init.asteroid_fragment.min_fragment_spacing_cm,
    };
    let (world, config) =
        bootstrap_runtime_world_from_model(config, &model, "runtime live bootstrap")?;
    Ok((world, config, chunk_runtime))
}

pub fn bootstrap_generated_sidecar_runtime_world(
    generated_world_dir: &Path,
) -> Result<(RuntimeWorld, WorldConfig, WorldModel), String> {
    let (world, config, model, _) =
        bootstrap_generated_sidecar_runtime_world_with_chunk_runtime(generated_world_dir)?;
    Ok((world, config, model))
}

pub(super) fn bootstrap_generated_sidecar_runtime_world_with_chunk_runtime(
    generated_world_dir: &Path,
) -> Result<(RuntimeWorld, WorldConfig, WorldModel, ChunkRuntimeConfig), String> {
    let sidecar_dir = generated_world_dir.join("generated-scenario-world");
    let provenance_path = generated_world_dir.join("world-generation-provenance.json");
    if !provenance_path.is_file() {
        return Err(format!(
            "generated world provenance missing: {}",
            provenance_path.display()
        ));
    }
    let kernel = WorldKernel::load_from_dir(&sidecar_dir).map_err(|err| {
        format!(
            "runtime live generated sidecar load failed dir={} err={err:?}",
            sidecar_dir.display()
        )
    })?;
    let snapshot = kernel.snapshot();
    let chunk_runtime = snapshot.chunk_runtime.clone();
    let config = snapshot.config;
    let model = snapshot.model;
    let (world, config) =
        bootstrap_runtime_world_from_model(config, &model, "runtime live generated sidecar")?;
    Ok((world, config, model, chunk_runtime))
}

fn bootstrap_runtime_world_from_model(
    config: WorldConfig,
    model: &WorldModel,
    label: &str,
) -> Result<(RuntimeWorld, WorldConfig), String> {
    let mut world =
        crate::runtime_host::world::new_production_hardened_with_initial_module_visuals(
            &model.module_visual_entities,
        )
        .map_err(|err| format!("{label} initialize module visuals failed: {err:?}"))?;
    world.set_resource_balance(ResourceKind::Electricity, 400);
    for (material, amount) in [
        ("structural_frame", 40),
        ("circuit_board", 4),
        ("servo_motor", 2),
        ("heat_coil", 6),
        ("refractory_brick", 8),
        ("iron_ore", 60),
        ("carbon_fuel", 20),
        ("copper_ore", 60),
        ("silicate_ore", 20),
        ("hardware_part", 40),
    ] {
        world
            .set_material_balance(material, amount)
            .map_err(|err| {
                format!(
                    "{label} set material balance failed material={} err={err:?}",
                    material
                )
            })?;
    }
    let mut seed_agents: Vec<(String, GeoPos, i64, i64)> = model
        .agents
        .iter()
        .map(|(agent_id, agent)| {
            (
                agent_id.clone(),
                agent.pos,
                agent.resources.get(ResourceKind::Electricity),
                agent.resources.get(ResourceKind::Data),
            )
        })
        .collect();
    seed_agents.sort_by(|left, right| left.0.cmp(&right.0));

    if seed_agents.is_empty() {
        seed_agents.push(("runtime-agent-0".to_string(), GeoPos::new(0, 0, 0), 32, 8));
        seed_agents.push(("runtime-agent-1".to_string(), GeoPos::new(0, 0, 0), 32, 8));
    }

    for (agent_id, pos, _, _) in &seed_agents {
        world.submit_action(RuntimeAction::RegisterAgent {
            agent_id: agent_id.clone(),
            pos: *pos,
        });
    }

    if world.pending_actions_len() > 0 {
        world
            .step()
            .map_err(|err| format!("{label} register step failed: {err:?}"))?;
    }

    for (agent_id, electricity, data) in world
        .state()
        .agents
        .keys()
        .cloned()
        .map(|agent_id| {
            let maybe_seed = seed_agents
                .iter()
                .find(|entry| entry.0 == agent_id)
                .cloned();
            match maybe_seed {
                Some((_, _, electricity, data)) => (agent_id, electricity.max(32), data.max(8)),
                None => (agent_id, 32, 8),
            }
        })
        .collect::<Vec<_>>()
    {
        world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, electricity)
            .map_err(|err| {
                format!(
                    "{label} set electricity failed agent={} err={err:?}",
                    agent_id
                )
            })?;
        world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, data)
            .map_err(|err| format!("{label} set data failed agent={} err={err:?}", agent_id))?;
    }
    let authority_agents = world
        .state()
        .agents
        .keys()
        .map(|agent_id| {
            let location_id = model
                .agents
                .get(agent_id)
                .map(|agent| agent.location_id.clone())
                .unwrap_or_else(|| "runtime-bootstrap-location".to_string());
            (agent_id.clone(), location_id)
        })
        .collect::<Vec<_>>();
    install_starter_factory_authorities(&mut world, authority_agents.as_slice(), label)?;
    world
        .step()
        .map_err(|err| format!("{label} resource seed consensus step failed: {err:?}"))?;

    Ok((world, config))
}

pub fn bootstrap_formal_release_runtime_world() -> Result<(RuntimeWorld, WorldConfig), String> {
    let (config, seed_model) = formal_release_default_seed_model()?;
    let starter_agent = seed_model
        .agents
        .get(FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID)
        .ok_or_else(|| {
            format!(
                "formal release seed model missing bootstrap agent {}",
                FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID
            )
        })?;
    let mut world = crate::runtime_host::world::new_production_hardened();
    world.set_resource_balance(ResourceKind::Electricity, 400);
    for (material, amount) in [
        ("structural_frame", 40),
        ("circuit_board", 4),
        ("servo_motor", 2),
        ("heat_coil", 6),
        ("refractory_brick", 8),
        ("iron_ore", 60),
        ("carbon_fuel", 20),
        ("copper_ore", 60),
        ("silicate_ore", 20),
        ("hardware_part", 40),
    ] {
        world
            .set_material_balance(material, amount)
            .map_err(|err| {
                format!(
                    "formal release bootstrap set material balance failed material={} err={err:?}",
                    material
                )
            })?;
    }
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID.to_string(),
        pos: starter_agent.pos,
    });
    world
        .step()
        .map_err(|err| format!("formal release bootstrap register step failed: {err:?}"))?;
    world
        .set_agent_resource_balance(
            FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID,
            ResourceKind::Electricity,
            starter_agent
                .resources
                .get(ResourceKind::Electricity)
                .max(STARTER_INDUSTRIAL_ELECTRICITY),
        )
        .map_err(|err| {
            format!(
                "formal release bootstrap set electricity failed agent={} err={err:?}",
                FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID
            )
        })?;
    world
        .set_agent_resource_balance(
            FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID,
            ResourceKind::Data,
            starter_agent.resources.get(ResourceKind::Data).max(8),
        )
        .map_err(|err| {
            format!(
                "formal release bootstrap set data failed agent={} err={err:?}",
                FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID
            )
        })?;
    install_starter_factory_authorities(
        &mut world,
        &[(
            FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID.to_string(),
            starter_agent.location_id.clone(),
        )],
        "formal release bootstrap",
    )?;
    world.step().map_err(|err| {
        format!("formal release bootstrap resource consensus step failed: {err:?}")
    })?;
    Ok((world, config))
}

fn install_starter_factory_authorities(
    world: &mut RuntimeWorld,
    agents: &[(String, String)],
    label: &str,
) -> Result<(), String> {
    let Some((owner_agent_id, starter_location_id)) = agents.first() else {
        return Ok(());
    };
    let owner_power = world
        .agent_resource_balance(owner_agent_id.as_str(), ResourceKind::Electricity)
        .map_err(|err| format!("{label} read starter electricity failed: {err:?}"))?;
    if owner_power < STARTER_INDUSTRIAL_ELECTRICITY {
        world
            .set_agent_resource_balance(
                owner_agent_id.as_str(),
                ResourceKind::Electricity,
                STARTER_INDUSTRIAL_ELECTRICITY,
            )
            .map_err(|err| format!("{label} seed starter electricity failed: {err:?}"))?;
    }
    let mut anchored_locations = HashSet::new();
    for (_, location_id) in agents {
        if anchored_locations.insert(location_id.clone()) {
            world
                .set_location_anchor(LocationAnchorV1 {
                    location_id: location_id.clone(),
                    active: true,
                    authority_revision: 1,
                    effective_at: 0,
                })
                .map_err(|err| format!("{label} install location anchor failed: {err:?}"))?;
        }
    }
    for (agent_id, location_id) in agents {
        world
            .set_agent_location_authority(AgentLocationAuthorityV1 {
                agent_id: agent_id.clone(),
                location_id: location_id.clone(),
                active: true,
                authority_revision: 1,
                effective_at: world.state().time,
            })
            .map_err(|err| format!("{label} install agent location authority failed: {err:?}"))?;
    }
    let authorized_agent_ids = agents
        .iter()
        .skip(1)
        .map(|(agent_id, _)| agent_id.clone())
        .collect::<Vec<_>>();
    for site_id in ["site-smelter", "site-assembler"] {
        world
            .set_factory_site_authority(FactorySiteAuthorityV1 {
                site_id: site_id.to_string(),
                location_id: starter_location_id.clone(),
                owner_agent_id: owner_agent_id.clone(),
                authorized_agent_ids: authorized_agent_ids.clone(),
                chunk_ready: true,
                active: true,
                authority_revision: 1,
                registered_at: world.state().time,
            })
            .map_err(|err| format!("{label} install factory site authority failed: {err:?}"))?;
    }
    for factory_id in [FACTORY_SMELTER_MK1, FACTORY_ASSEMBLER_MK1] {
        world
            .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
                factory_id: factory_id.to_string(),
                factory_kind: factory_id.to_string(),
                source_module_id: None,
                electricity_amount: 10,
                mode: FactoryConstructionPowerMode::StartOnlySink,
                authority_revision: 1,
                active: true,
            })
            .map_err(|err| {
                format!("{label} install factory construction power profile failed: {err:?}")
            })?;
    }
    for (factory_id, tier, tags) in [
        (
            FACTORY_SMELTER_MK1,
            2,
            vec!["smelter".to_string(), "thermal".to_string()],
        ),
        (
            FACTORY_ASSEMBLER_MK1,
            3,
            vec!["assembler".to_string(), "precision".to_string()],
        ),
    ] {
        world
            .upsert_factory_profile(FactoryProfileV1 {
                factory_id: factory_id.to_string(),
                tier,
                recipe_slots: 2,
                tags,
            })
            .map_err(|err| format!("{label} install factory profile failed: {err:?}"))?;
    }
    let owner_ledger = MaterialLedgerId::agent(owner_agent_id);
    for (material, amount) in [
        // BuildFactory consumes from the owner ledger, and no executable
        // material-transfer action currently connects the smelter site ledger
        // back to that builder ledger. Keep both starter construction beats
        // attainable by seeding the exact authoritative obligations here.
        ("structural_frame", 20),
        ("heat_coil", 4),
        ("refractory_brick", 6),
        ("iron_ingot", 10),
        ("copper_wire", 8),
    ] {
        world
            .set_ledger_material_balance(owner_ledger.clone(), material, amount)
            .map_err(|err| format!("{label} seed starter build material failed: {err:?}"))?;
    }
    let smelter_ledger = MaterialLedgerId::site("site-smelter");
    for (material, amount) in [("iron_ore", 60), ("carbon_fuel", 20)] {
        world
            .set_ledger_material_balance(smelter_ledger.clone(), material, amount)
            .map_err(|err| format!("{label} seed starter recipe material failed: {err:?}"))?;
    }
    // The first canonical assembler recipe consumes two iron ingots per batch
    // and the gameplay action schedules four batches. Keep its inputs on the
    // assembler's authoritative site ledger; recipe admission no longer
    // falls back to the world ledger and the Viewer exposes no transfer
    // action that could bridge this bootstrap gap.
    let assembler_ledger = MaterialLedgerId::site("site-assembler");
    world
        .set_ledger_material_balance(assembler_ledger, "iron_ingot", 8)
        .map_err(|err| format!("{label} seed starter assembler recipe material failed: {err:?}"))?;
    Ok(())
}

pub(super) fn formal_release_default_seed_location_for_pos(pos: GeoPos) -> Option<Location> {
    let (_, model) = formal_release_default_seed_model().ok()?;
    let location_id = model
        .agents
        .values()
        .find(|agent| agent.pos == pos)
        .map(|agent| agent.location_id.clone())?;
    model.locations.get(&location_id).cloned()
}

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
