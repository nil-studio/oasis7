//! Authorized observation material, never an execution snapshot.
use oasis7_runtime::{World, WorldState};
pub use oasis7_world_projection::{
    WorldServiceAgentContext, WorldServiceContinuationContext, WorldServiceProjection,
};
mod committed;
pub use committed::{from_committed_world, from_formal_committed_world};
mod feedback_history;
pub use feedback_history::{
    WorldServiceFeedbackHistory, WorldServiceFeedbackReplayRecord, feedback_history_from_world,
};

/// Called after scope authentication. Owner chat metadata has a stricter
/// audience than ordinary Agent cognition/delegation observation material.
pub fn from_authenticated_world(
    world: &World,
    authorized_agent: Option<&str>,
    caller_public_key: &str,
) -> Result<WorldServiceProjection, String> {
    let mut projection = from_world(world, authorized_agent)?;
    if let Some(agent) = authorized_agent {
        projection.native_delegation_authorizations = world
            .agent_delegation_authorizations(agent)
            .map_err(|_| "native delegation read unavailable")?;
        if super::agent_authority::owner_public_key(world, agent)? == caller_public_key {
            projection.canonical_agent_chat = super::agent_chat::owner_view(world, agent)?;
        }
        if let Some(control) = super::game_task::current_control(world, agent)? {
            let fence = oasis7_agent_api::game::GameTaskFenceV1::from_control(&control);
            if super::game_task::validate_task_reader(world, &fence, caller_public_key).is_ok() {
                projection.game_task_control = Some(control);
            }
        }
    }
    Ok(projection)
}

/// Called only after the service has authorized the requested audience.
/// An allowlist prevents newly added persistence fields leaking by default.
pub fn from_world(
    world: &World,
    authorized_agent: Option<&str>,
) -> Result<WorldServiceProjection, String> {
    let source = world.state();
    let mut state = WorldState {
        world_rules: source.world_rules.as_ref().map(|rules| {
            let mut visible = rules.for_current_reader(world.state(), authorized_agent);
            visible
                .agent_orders
                .retain(|_, order| world.can_read_agent_industrial_order(order, authorized_agent));
            visible
        }),
        time: source.time,
        agents: source.agents.clone(),
        resources: source.resources.clone(),
        materials: source.materials.clone(),
        material_ledgers: source.material_ledgers.clone(),
        material_profiles: source.material_profiles.clone(),
        logistics_routes: source.logistics_routes.clone(),
        factories: source.factories.clone(),
        recipe_profiles: source.recipe_profiles.clone(),
        factory_profiles: source.factory_profiles.clone(),
        product_profiles: source.product_profiles.clone(),
        latest_product_validation: source.latest_product_validation.clone(),
        pending_material_transits: source.pending_material_transits.clone(),
        pending_factory_builds: source.pending_factory_builds.clone(),
        factory_site_authorities: source.factory_site_authorities.clone(),
        agent_location_authorities: source.agent_location_authorities.clone(),
        industry_progress: source.industry_progress.clone(),
        alliances: source.alliances.clone(),
        gameplay_policy: source.gameplay_policy.clone(),
        agent_claims: source.agent_claims.clone(),
        agent_claim_acquisition_sequences: source
            .agent_claim_acquisition_sequences
            .iter()
            .filter(|(target, _)| source.agent_claims.contains_key(*target))
            .map(|(target, sequence)| (target.clone(), *sequence))
            .collect(),
        starter_oc_claims: source.starter_oc_claims.clone(),
        reputation_scores: source.reputation_scores.clone(),
        wars: source.wars.clone(),
        governance_votes: source.governance_votes.clone(),
        governance_proposals: source.governance_proposals.clone(),
        crises: source.crises.clone(),
        meta_progress: source.meta_progress.clone(),
        module_visual_entities: source.module_visual_entities.clone(),
        main_token_config: source.main_token_config.clone(),
        main_token_supply: source.main_token_supply.clone(),
        factory_construction_power_profiles: source.factory_construction_power_profiles.clone(),
        location_anchors: source.location_anchors.clone(),
        ..WorldState::default()
    };
    // Construction authority is scoped observation material. Preserve the
    // established public profiles/anchors, but never publish other Agents'
    // location assignments or a site's full private membership list.
    state.agent_location_authorities.retain(|id, authority| {
        authorized_agent == Some(id.as_str())
            && authorized_agent == Some(authority.agent_id.as_str())
    });
    state.factory_site_authorities.retain(|_, authority| {
        authorized_agent.is_some_and(|agent| {
            authority.owner_agent_id == agent
                || authority.authorized_agent_ids.iter().any(|id| id == agent)
        })
    });
    for authority in state.factory_site_authorities.values_mut() {
        authority
            .authorized_agent_ids
            .retain(|id| authorized_agent == Some(id.as_str()));
        if authorized_agent != Some(authority.owner_agent_id.as_str()) {
            authority.owner_agent_id.clear();
        }
    }
    state
        .pending_factory_builds
        .retain(|_, job| authorized_agent == Some(job.builder_agent_id.as_str()));
    for (id, agent) in &mut state.agents {
        agent.mailbox.clear();
        if authorized_agent != Some(id.as_str()) {
            agent.intent = None;
        }
    }
    // Generic journals may contain private cognition/prompt artifacts.
    // Authorized committed changes are delivered by the changes route.
    Ok(WorldServiceProjection {
        world_rule_status: world.world_rule_status(authorized_agent),
        canonical_agent_owner: authorized_agent
            .map(|agent| super::agent_chat::owner_view(world, agent))
            .transpose()?
            .flatten()
            .as_ref()
            .map(Into::into),
        canonical_agent_chat: None,
        game_task_control: None,
        native_delegation_authorizations: Vec::new(),
        feedback_history: authorized_agent
            .map(|agent| feedback_history_from_world(world, agent))
            .transpose()?,
        state,
        events: Vec::new(),
        formal_commit_header: None,
        runtime_binding: world.current_cognition_runtime_binding().ok(),
        agent_context: None,
        scheduler_wakes: Vec::new(),
        continuation_contexts: Default::default(),
        continuations: Vec::new(),
        cognition_leases: Vec::new(),
    })
}

#[cfg(test)]
pub(crate) fn legacy_projection_from_world(
    world: &World,
    authorized_agent: Option<&str>,
) -> Result<WorldServiceProjection, String> {
    let mut value = serde_json::to_value(world.state()).map_err(|e| e.to_string())?;
    let fields = [
        "world_rules",
        "time",
        "agents",
        "resources",
        "materials",
        "material_ledgers",
        "material_profiles",
        "logistics_routes",
        "factories",
        "recipe_profiles",
        "factory_profiles",
        "product_profiles",
        "latest_product_validation",
        "pending_material_transits",
        "pending_factory_builds",
        "factory_site_authorities",
        "agent_location_authorities",
        "industry_progress",
        "alliances",
        "gameplay_policy",
        "agent_claims",
        "agent_claim_acquisition_sequences",
        "starter_oc_claims",
        "reputation_scores",
        "wars",
        "governance_votes",
        "governance_proposals",
        "crises",
        "meta_progress",
        "module_visual_entities",
        "main_token_config",
        "main_token_supply",
        "factory_construction_power_profiles",
        "location_anchors",
    ];
    value
        .as_object_mut()
        .ok_or("invalid world state")?
        .retain(|key, _| fields.contains(&key.as_str()));
    let mut state: WorldState = serde_json::from_value(value).map_err(|e| e.to_string())?;
    state.world_rules = state.world_rules.map(|rules| {
        let mut visible = rules.for_current_reader(world.state(), authorized_agent);
        visible
            .agent_orders
            .retain(|_, order| world.can_read_agent_industrial_order(order, authorized_agent));
        visible
    });
    state
        .agent_claim_acquisition_sequences
        .retain(|target, _| state.agent_claims.contains_key(target));
    // Construction authority is scoped observation material. Preserve the
    // established public profiles/anchors, but never publish other Agents'
    // location assignments or a site's full private membership list.
    state.agent_location_authorities.retain(|id, authority| {
        authorized_agent == Some(id.as_str())
            && authorized_agent == Some(authority.agent_id.as_str())
    });
    state.factory_site_authorities.retain(|_, authority| {
        authorized_agent.is_some_and(|agent| {
            authority.owner_agent_id == agent
                || authority.authorized_agent_ids.iter().any(|id| id == agent)
        })
    });
    for authority in state.factory_site_authorities.values_mut() {
        authority
            .authorized_agent_ids
            .retain(|id| authorized_agent == Some(id.as_str()));
        if authorized_agent != Some(authority.owner_agent_id.as_str()) {
            authority.owner_agent_id.clear();
        }
    }
    state
        .pending_factory_builds
        .retain(|_, job| authorized_agent == Some(job.builder_agent_id.as_str()));
    for (id, agent) in &mut state.agents {
        agent.mailbox.clear();
        if authorized_agent != Some(id.as_str()) {
            agent.intent = None;
        }
    }
    // Generic journals may contain private cognition/prompt artifacts.
    // Authorized committed changes are delivered by the changes route.
    Ok(WorldServiceProjection {
        world_rule_status: world.world_rule_status(authorized_agent),
        canonical_agent_owner: authorized_agent
            .map(|agent| super::agent_chat::owner_view(world, agent))
            .transpose()?
            .flatten()
            .as_ref()
            .map(Into::into),
        canonical_agent_chat: None,
        game_task_control: None,
        native_delegation_authorizations: Vec::new(),
        feedback_history: authorized_agent
            .map(|agent| feedback_history_from_world(world, agent))
            .transpose()?,
        state,
        events: Vec::new(),
        formal_commit_header: None,
        runtime_binding: world.current_cognition_runtime_binding().ok(),
        agent_context: None,
        scheduler_wakes: Vec::new(),
        continuation_contexts: Default::default(),
        continuations: Vec::new(),
        cognition_leases: Vec::new(),
    })
}

#[cfg(test)]
#[path = "projection/native_delegation_tests.rs"]
mod native_delegation_tests;

#[cfg(test)]
#[path = "projection/factory_authority_tests.rs"]
mod factory_authority_tests;

#[cfg(test)]
mod read_contract_tests;
