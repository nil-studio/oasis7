//! Borrowed provenance from actual V2 phase originals; not a V1 Prepared conversion.
use super::*;
pub struct OwnerSetupPreparedV2<'a> {
    pub plan: &'a OwnerSetupPlanV2,
    pub result: &'a OwnerSetupResultV1,
    pub result_digest: String,
    pub control: &'a oasis7_agent_api::game::GameTaskControlV1,
    pub delegation: &'a oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1,
}
impl AccountProvisionApprovalState {
    pub fn owner_setup_prepared_v2(&self, id: &str) -> Result<OwnerSetupPreparedV2<'_>> {
        // Reconstructs stage provenance and cryptographically checks stored originals.
        self.validate_persistence()?;
        if self.provision.stage() != ProvisionStage::Prepared {
            return Err(invalid());
        }
        let plan = self
            .owner_setup_approved_header_v2(id)
            .ok_or_else(invalid)?;
        if !plan.selection.is_exact_move0() {
            return Err(invalid());
        }
        let r = self.owner_setup_engine_record(id)?;
        for stage in [
            OwnerSetupStageV1::CreateTask,
            OwnerSetupStageV1::PrepareGoal,
            OwnerSetupStageV1::PublishGoal,
            OwnerSetupStageV1::Delegation,
            OwnerSetupStageV1::Bind,
        ] {
            if r.stages
                .get(&stage)
                .is_none_or(|o| o.result.as_ref().is_none_or(|v| !v.succeeded()))
            {
                return Err(invalid());
            }
        }
        let outbox = r
            .stages
            .get(&OwnerSetupStageV1::FinalObservation)
            .ok_or_else(invalid)?;
        let result = outbox.result.as_ref().ok_or_else(invalid)?;
        let OwnerSetupResultV1::FinalObservation { owner, goal } = result else {
            return Err(invalid());
        };
        let control = owner
            .payload
            .control
            .view
            .game_task_control
            .as_ref()
            .ok_or_else(invalid)?;
        if &goal.payload.control != control
            || control.task_status != oasis7_agent_api::game::GameTaskStatusV1::Active
        {
            return Err(invalid());
        }
        let delegation = owner
            .payload
            .signer_delegation
            .as_ref()
            .ok_or_else(invalid)?;
        self.owner_setup_active_goal(id, control)?;
        self.owner_setup_delegation(id, Some(delegation))?;
        Ok(OwnerSetupPreparedV2 {
            plan,
            result,
            result_digest: digest(&(
                "oasis7.custody.prepared-result.v2",
                plan.digest().map_err(|_| invalid())?,
                result,
            ))?,
            control,
            delegation,
        })
    }
}
