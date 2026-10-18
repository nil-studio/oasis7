//! Authorized observation material, never an execution snapshot.
use oasis7_agent_api::RuntimeBindingV1;
use oasis7_runtime::{World, WorldEvent, WorldState};
use serde::{Deserialize, Serialize};
mod committed;
mod feedback_history;
pub use feedback_history::{
    WorldServiceFeedbackHistory, WorldServiceFeedbackReplayRecord, feedback_history_from_world,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldServiceProjection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_commit_header:
        Option<oasis7_client_api::world_service::FormalProjectionCommitHeaderV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_agent_owner: Option<oasis7_proto::viewer::CanonicalAgentOwnerFenceV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_agent_chat: Option<oasis7_proto::viewer::CanonicalAgentChatViewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_task_control: Option<oasis7_agent_api::game::GameTaskControlV1>,
    /// Native action/object electricity grants, distinct from WASM commands.
    /// Only the authenticated Agent scope receives these read models.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_delegation_authorizations: Vec<oasis7_runtime::AgentDelegationAuthorizationV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback_history: Option<WorldServiceFeedbackHistory>,
    pub state: WorldState,
    pub events: Vec<WorldEvent>,
    pub runtime_binding: Option<RuntimeBindingV1>,
    pub agent_context: Option<WorldServiceAgentContext>,
    pub scheduler_wakes: Vec<oasis7_runtime::SchedulerWakeV1>,
    pub continuations: Vec<oasis7_runtime::AgentContinuation>,
    pub cognition_leases: Vec<oasis7_runtime::CognitionLeaseV1>,
    pub continuation_contexts: std::collections::BTreeMap<String, WorldServiceContinuationContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldServiceContinuationContext {
    pub baseline_observation_digest: String,
    pub goal_digest: String,
    pub policy_digest: String,
    pub policy_revision: u64,
    pub precondition_summary: String,
    pub precondition_digest: String,
}

pub use oasis7_agent_api::world_context::WorldServiceAgentContext;

impl WorldServiceProjection {
    /// Called after scope authentication. Owner chat metadata has a stricter
    /// audience than ordinary Agent cognition/delegation observation material.
    pub fn from_authenticated_world(
        world: &World,
        authorized_agent: Option<&str>,
        caller_public_key: &str,
    ) -> Result<Self, String> {
        let mut projection = Self::from_world(world, authorized_agent)?;
        if let Some(agent) = authorized_agent {
            projection.native_delegation_authorizations = world
                .agent_delegation_authorizations(agent)
                .map_err(|_| "native delegation read unavailable")?;
            if super::agent_authority::owner_public_key(world, agent)? == caller_public_key {
                projection.canonical_agent_chat = super::agent_chat::owner_view(world, agent)?;
            }
            if let Some(control) = super::game_task::current_control(world, agent)? {
                let fence = oasis7_agent_api::game::GameTaskFenceV1::from_control(&control);
                if super::game_task::validate_task_reader(world, &fence, caller_public_key).is_ok()
                {
                    projection.game_task_control = Some(control);
                }
            }
        }
        Ok(projection)
    }

    /// Called only after the service has authorized the requested audience.
    /// An allowlist prevents newly added persistence fields leaking by default.
    pub fn from_world(world: &World, authorized_agent: Option<&str>) -> Result<Self, String> {
        let source = world.state();
        let mut state = WorldState {
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
        Ok(Self {
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
    ) -> Result<Self, String> {
        let mut value = serde_json::to_value(world.state()).map_err(|e| e.to_string())?;
        let fields = [
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
        Ok(Self {
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
}

#[cfg(test)]
#[path = "projection/native_delegation_tests.rs"]
mod native_delegation_tests;

#[cfg(test)]
#[path = "projection/factory_authority_tests.rs"]
mod factory_authority_tests;

#[cfg(test)]
mod read_contract_tests;
