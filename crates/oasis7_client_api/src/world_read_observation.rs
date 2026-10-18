//! Explicit observations for independent readers, not an execution snapshot.
use crate::world_service::{
    FormalProjectionCommitHeaderV1, ProjectionVersion, ReadWorldViewRequest,
};
use crate::world_service_feedback::WorldServiceFeedbackHistory;
use oasis7_agent_api::{
    RuntimeBindingV1,
    world_action::{BodyKernelView, GeoPos, MaterialLedgerId, ResourceKind},
    world_entity::RobotBodySpec,
    world_resource::ResourceStock,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldReadAgentStateV1 {
    pub agent_id: String,
    pub pos: GeoPos,
    pub body: RobotBodySpec,
    #[serde(default)]
    pub resources: ResourceStock,
    #[serde(default)]
    pub body_view: BodyKernelView,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldReadAgentCellV1 {
    pub state: WorldReadAgentStateV1,
    pub last_active: u64,
}
/// Only read values have named fields. In particular, no pending action pool,
/// admission ledger, private module state or mutable Runtime state is carried.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldReadStateV1 {
    pub time: u64,
    pub agents: BTreeMap<String, WorldReadAgentCellV1>,
    #[serde(default)]
    pub resources: BTreeMap<ResourceKind, i64>,
    #[serde(default)]
    pub materials: BTreeMap<String, i64>,
    #[serde(default = "default_ledgers")]
    pub material_ledgers: BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
}
fn default_ledgers() -> BTreeMap<MaterialLedgerId, BTreeMap<String, i64>> {
    BTreeMap::from([(MaterialLedgerId::world(), BTreeMap::new())])
}
/// An audience check needs a subject, never an executable continuation or lease.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedAgentReadReferenceV1 {
    pub agent_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldReadContinuationContextV1 {
    pub baseline_observation_digest: String,
    pub goal_digest: String,
    pub policy_digest: String,
    pub policy_revision: u64,
    pub precondition_summary: String,
    pub precondition_digest: String,
}
/// Narrow schema parsed alongside the untouched projection encoding. These
/// values become authenticated observations only inside a verified client view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldReadObservationV1 {
    pub state: WorldReadStateV1,
    pub runtime_binding: Option<RuntimeBindingV1>,
    #[serde(default)]
    pub formal_commit_header: Option<FormalProjectionCommitHeaderV1>,
    pub agent_context: Option<oasis7_agent_api::world_context::WorldServiceAgentContext>,
    #[serde(default)]
    pub feedback_history: Option<WorldServiceFeedbackHistory>,
    pub scheduler_wakes: Vec<ScopedAgentReadReferenceV1>,
    pub continuations: Vec<ScopedAgentReadReferenceV1>,
    pub cognition_leases: Vec<ScopedAgentReadReferenceV1>,
    pub continuation_contexts: BTreeMap<String, WorldReadContinuationContextV1>,
    pub events: Vec<serde_json::Value>,
    #[serde(default)]
    pub game_task_control: Option<oasis7_agent_api::game::GameTaskControlV1>,
    #[serde(default)]
    pub native_delegation_authorizations:
        Vec<oasis7_agent_api::world_quote::AgentDelegationAuthorizationV1>,
}
impl WorldReadObservationV1 {
    pub fn metadata(&self) -> super::world_service_read::WorldServiceProjectionMetadataV1 {
        super::world_service_read::WorldServiceProjectionMetadataV1 {
            runtime_binding: self.runtime_binding.clone(),
            formal_commit_header: self.formal_commit_header.clone(),
            events: self.events.clone(),
        }
    }
    pub fn validate_bound_contents(
        &self,
        version: &ProjectionVersion,
        logical_tick: u64,
        request: &ReadWorldViewRequest,
    ) -> Result<(), String> {
        if self.state.time != logical_tick {
            return Err("projection tick differs from declared logical tick".into());
        }
        if let Some(binding) = &self.runtime_binding {
            binding.validate().map_err(|e| format!("{e:?}"))?;
            let expected = &version.commit.binding;
            if binding.world_id != expected.provider_world_id
                || binding.branch_id != expected.branch_id
                || binding.reorg_epoch != expected.reorg_generation
                || binding.base_tick != logical_tick
                || binding.runtime_manifest_hash.to_string() != expected.governing_manifest_ref
                || crate::world_service::request_digest(
                    "finality",
                    &(
                        binding.finality_epoch,
                        &binding.finality_status,
                        &binding.finality_block_hash,
                    ),
                )? != expected.finality_ref
            {
                return Err("projection runtime binding differs from committed view".into());
            }
        }
        let agent = request.scope_id.strip_prefix("agent:");
        let feedback_history_out_of_scope = match (agent, self.feedback_history.as_ref()) {
            (None, Some(_)) => true,
            (Some(agent), Some(history)) => {
                history.agent_id != agent
                    || history
                        .records
                        .iter()
                        .any(|record| record.feedback.agent_subject != agent)
            }
            _ => false,
        };
        if self
            .agent_context
            .as_ref()
            .is_some_and(|context| Some(context.agent_id.as_str()) != agent)
            || self
                .scheduler_wakes
                .iter()
                .any(|wake| Some(wake.agent_id.as_str()) != agent)
            || self
                .continuations
                .iter()
                .any(|entry| Some(entry.agent_id.as_str()) != agent)
            || self
                .cognition_leases
                .iter()
                .any(|lease| Some(lease.agent_id.as_str()) != agent)
            || feedback_history_out_of_scope
        {
            return Err("Agent observation differs from authorized visibility scope".into());
        }
        Ok(())
    }
}
