//! The same pure validation used by the real Identity receiver. DTO checks do
//! not confer MFA/Current/payment authority or replace the browser pipeline.
#[path = "../src/owner_setup_stage_selector.rs"]
mod checks;
use oasis7_custody_core::provisioning::*;
use serde::de::DeserializeOwned;
fn fixture<T: DeserializeOwned>(name: &str) -> T {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../contract/http.schema.json")).unwrap();
    serde_json::from_value(v["fixtures"][name][0].clone()).unwrap()
}
#[test]
fn owner_setup_stage_closed_v2_selector_and_exact_projection() {
    let q2: OwnerSetupPrepareRequestV2 = fixture("OwnerSetupPrepareRequestV2");
    let mut q: OwnerSetupDriveRequestV1 = fixture("OwnerSetupDriveRequestV1");
    q.api_version = 2;
    q.provision_id = q2.plan.provision_id.clone();
    q.setup_id = q2.plan.setup_id.clone();
    let projection = if q2.plan.selection.resource().is_some() {
        derive_owner_setup_resource_sdk_plan_v2(&q2.plan)
    } else {
        derive_owner_setup_move0_sdk_plan_v2(&q2.plan)
    }
    .unwrap();
    assert!(checks::validate_selector(&q, true).is_ok());
    assert!(checks::validate_projection(&q, &projection, Some(&q2.plan), true).is_ok());
    assert!(
        checks::validate_selector(&q, false).is_err(),
        "disabled V2 is rejected before Actor send"
    );
    assert!(checks::validate_projection(&q, &projection, Some(&q2.plan), false).is_err());
    assert!(
        checks::validate_projection(&q, &projection, None, true).is_err(),
        "V1 record cannot upgrade"
    );
    let mut other = projection.clone();
    other.setup_id = "different-setup".into();
    assert!(
        checks::validate_projection(&q, &other, Some(&q2.plan), true).is_err(),
        "full exact projection remains required"
    );
    q.api_version = 99;
    assert!(checks::validate_selector(&q, true).is_err());
}
#[test]
fn owner_setup_stage_v1_keeps_its_exact_closed_record_kind() {
    let q1: OwnerSetupPrepareRequestV1 = fixture("OwnerSetupPrepareRequestV1");
    let q2: OwnerSetupPrepareRequestV2 = fixture("OwnerSetupPrepareRequestV2");
    let mut q: OwnerSetupDriveRequestV1 = fixture("OwnerSetupDriveRequestV1");
    q.api_version = 1;
    assert!(checks::validate_selector(&q, false).is_ok());
    assert!(checks::validate_projection(&q, &q1.plan, None, false).is_ok());
    assert!(checks::validate_projection(&q, &q1.plan, Some(&q2.plan), true).is_err());
    q.custody_account_id.clear();
    assert!(checks::validate_selector(&q, true).is_err());
}
