//! Historical full proof codec only. There is intentionally no accept constructor:
//! an Account V2 phase-replay producer does not exist in this component.
use super::*;
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedResourceBasisV1 {
    api_version: u8,
    setup_id: String,
    provision_id: String,
    owner_setup_plan_digest: String,
    final_read_original: wire::SignedResourceReadV1<wire::OwnerSetupFinalResourceReadV2>,
    final_observation: wire::SignedResourceResponseV1<wire::OwnerSetupFinalResourceObservationV2>,
    final_observation_digest: String,
    selection: wire::ResourceBasisSelectionV1,
    as_of_commit: wire::ResourceCommitRefV1,
    as_of_tick: oasis7_agent_api::game::DecimalU64,
    derived_resource_refs: Vec<String>,
}
impl core::fmt::Debug for PreparedResourceBasisV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PreparedResourceBasisV1(<private historical proof>)")
    }
}
impl PreparedResourceBasisV1 {
    /// Strong proof/provenance checks, never current permission, phase acceptance
    /// or an approval grant. Caller must separately replay the actual four writes.
    pub fn validate_provenance(&self, plan: &OwnerSetupPlanV2) -> Result<(), String> {
        plan.validate()?;
        let q = &self.final_read_original.request;
        let p = &self.final_observation.payload;
        let original = p.current_goal.active_goal.as_ref().ok_or_else(invalid)?;
        if self.api_version != 1
            || self.setup_id != plan.setup_id
            || self.provision_id != plan.provision_id
            || self.owner_setup_plan_digest != plan.digest()?
            || q.plan_digest != plan.digest()?
            || q.setup_id != self.setup_id
            || q.provision_id != self.provision_id
            || self.selection != plan.selection
            || q.selection != self.selection
            || p.selection != self.selection
            || q.world.world_id != plan.world_identity.world_id
            || q.world.genesis_digest != plan.world_identity.genesis_digest
            || q.expected_fence.branch_id != plan.world_identity.branch_id
            || q.expected_fence.reorg_epoch.0 != plan.world_identity.reorg_epoch.0
            || self.as_of_commit != p.owner.read_commit
            || self.as_of_tick != p.owner.logical_tick
            || self.final_observation_digest
                != digest(
                    "oasis7.custody.final-resource-observation.v1",
                    &self.final_observation,
                )?
            || original.owner_proof.request.content != plan.goal_content
            || original.owner_proof.request.client_operation_id != plan.prepare_operation_id
            || original.owner_proof.request.client_nonce != plan.prepare_nonce
            || q.goal_reference != original.goal_ref
        {
            return Err(invalid());
        }
        wire::verify_resource_goal_response(
            &self.final_read_original,
            &self.final_observation,
            &plan.world_identity.service_public_key,
        )?;
        let wire::ResourceBasisResolutionV1::Ready { fact } = &p.resource else {
            return Err(invalid());
        };
        if self.derived_resource_refs != fact.derived_resource_refs {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn digest(&self, plan: &OwnerSetupPlanV2) -> Result<String, String> {
        self.validate_provenance(plan)?;
        digest("oasis7.custody.prepared-resource-basis.v1", self)
    }
    pub fn selection(&self) -> &wire::ResourceBasisSelectionV1 {
        &self.selection
    }
    pub fn derived_refs(&self) -> &[String] {
        &self.derived_resource_refs
    }
    pub fn as_of_commit(&self) -> &wire::ResourceCommitRefV1 {
        &self.as_of_commit
    }
}
