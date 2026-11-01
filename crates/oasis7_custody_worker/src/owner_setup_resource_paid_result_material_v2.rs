//! Pure selection of the exact already-paid Resource callback bytes. No grant.
use oasis7_custody_core::{
    approval::{ApprovedWorldV1, SignedApprovalVerificationV1},
    approval_admission::ApprovalAdmissionInput,
    provisioning::{
        AccountProvisionApprovalState, OWNER_SETUP_STAGE_TRANSPORT_COST, OwnerSetupStageV1,
    },
};
pub(crate) struct Material {
    pub input: ApprovalAdmissionInput,
    pub signed: SignedApprovalVerificationV1,
}
pub(crate) fn material(
    state: &AccountProvisionApprovalState,
    id: &str,
    world: &ApprovedWorldV1,
    now: u64,
) -> Result<Material, String> {
    let invalid = || "Resource paid result material unavailable".to_string();
    let plan = state
        .owner_setup_phase_sdk_plan(id)
        .map_err(|_| invalid())?;
    let mut spent = 8_u64;
    let mut cursor = Some(OwnerSetupStageV1::FinalObservation);
    while let Some(stage) = cursor {
        if state.owner_setup_outbox(id, stage).is_some() {
            spent = spent
                .checked_add(OWNER_SETUP_STAGE_TRANSPORT_COST)
                .ok_or_else(invalid)?;
        }
        cursor = stage.previous();
    }
    let remaining = state
        .owner_setup_progress(id)
        .map_err(|_| invalid())?
        .transport_remaining
        .0;
    if state.frozen()
        || state.draft().world_identity != *world
        || plan.transport_limit.0.checked_sub(spent) != Some(remaining)
    {
        return Err(invalid());
    }
    let (input, signed) = state
        .owner_setup_resource_result_callback_source_v2(id, now)
        .map_err(|_| invalid())?;
    Ok(Material {
        input: input.clone(),
        signed: signed.clone(),
    })
}
