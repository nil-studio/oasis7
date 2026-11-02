//! Closed SDK selector/projection checks only, never Current or a stage permit.
use oasis7_custody_core::provisioning::{
    OwnerSetupDriveRequestV1, OwnerSetupPlanV1, OwnerSetupPlanV2,
    derive_owner_setup_move0_sdk_plan_v2, derive_owner_setup_resource_sdk_plan_v2,
};
fn denied() -> String {
    "owner setup selector rejected".into()
}
pub(super) fn validate_selector(
    q: &OwnerSetupDriveRequestV1,
    v2_enabled: bool,
) -> Result<(), String> {
    q.validate_sdk_selector().map_err(|_| denied())?;
    if q.api_version == 2 && !v2_enabled {
        return Err(denied());
    }
    Ok(())
}
pub(super) fn validate_projection(
    q: &OwnerSetupDriveRequestV1,
    plan: &OwnerSetupPlanV1,
    header: Option<&OwnerSetupPlanV2>,
    v2_enabled: bool,
) -> Result<(), String> {
    validate_selector(q, v2_enabled)?;
    plan.validate().map_err(|_| denied())?;
    match (header, q.api_version) {
        (None, 1) => Ok(()),
        (Some(h), 2) => {
            let expected = if h.selection.resource().is_some() {
                derive_owner_setup_resource_sdk_plan_v2(h)
            } else {
                derive_owner_setup_move0_sdk_plan_v2(h)
            }
            .map_err(|_| denied())?;
            if &expected != plan {
                return Err(denied());
            }
            Ok(())
        }
        _ => Err(denied()),
    }
}
