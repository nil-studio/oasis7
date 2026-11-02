//! The actual V2 Book and a previously paid result callback supply material.
//! This readonly accessor creates no Current, permit, observation or V1 record.
use super::*;
use crate::approval::{ApprovedWorldV1, SignedApprovalVerificationV1};
use crate::approval_admission::{ApprovalAdmissionInput, ApprovalAdmissionStage};
impl AccountProvisionApprovalState {
    pub fn owner_setup_paid_result_source_v2(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        world: &ApprovedWorldV1,
        now: u64,
    ) -> Result<(&ApprovalAdmissionInput, &SignedApprovalVerificationV1)> {
        self.validate_persistence()?;
        self.owner_setup_local(id, now)?;
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        let record = self.owner_setup_engine_record(id)?;
        if v.plan.setup_id != id
            || self.draft.world_identity != *world
            || !matches!(
                stage,
                OwnerSetupStageV1::Delegation
                    | OwnerSetupStageV1::Bind
                    | OwnerSetupStageV1::FinalObservation
            )
            || stage == OwnerSetupStageV1::FinalObservation && v.plan.selection.resource().is_some()
        {
            return Err(invalid());
        }
        let outbox = record.stages.get(&stage).ok_or_else(invalid)?;
        let callback = outbox.callback_claimed_at.ok_or_else(invalid)?;
        if outbox.claimed_at.is_none() || outbox.result.is_some() || now < callback.0 {
            return Err(invalid());
        }
        let original = v
            .book
            .original(id)
            .filter(|r| r.stage() == ApprovalAdmissionStage::Consumed)
            .ok_or_else(invalid)?;
        if original.input().request.intent != v.plan.intent().map_err(|_| invalid())? {
            return Err(invalid());
        }
        let source = outbox.source.verification.as_ref().ok_or_else(invalid)?;
        Ok((original.input(), source))
    }
}
