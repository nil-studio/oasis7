//! Pure adapter tests use canonical Rust-generated DTO fixtures. They confer
//! no MFA/current/paid/World authority and do not mock a runtime receiver.
#[path = "../src/management_owner_setup_response.rs"]
mod checks;
use checks::ExpectedOwnerSetupResponse as Expected;
use oasis7_custody_core::{approval::ApprovalPhaseV1, provisioning::*};
use serde::de::DeserializeOwned;
fn fixture<T: DeserializeOwned>(name: &str) -> T {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../contract/http.schema.json")).unwrap();
    serde_json::from_value(v["fixtures"][name][0].clone()).unwrap()
}
#[test]
fn management_owner_setup_closed_v1_v2_prepare_approval_and_sdk_progress() {
    let q1: OwnerSetupPrepareRequestV1 = fixture("OwnerSetupPrepareRequestV1");
    let r1: OwnerSetupPrepareResponseV1 = fixture("OwnerSetupPrepareResponseV1");
    let raw1 = serde_json::to_string(&r1).unwrap();
    assert!(Expected::PrepareV1(Box::new(q1)).matches(&raw1));
    let q2: OwnerSetupPrepareRequestV2 = fixture("OwnerSetupPrepareRequestV2");
    let a2: OwnerSetupApproveRequestV2 = fixture("OwnerSetupApproveRequestV2");
    assert!(
        Expected::PrepareV2(Box::new(q2))
            .matches(&serde_json::to_string(&a2.input.request).unwrap())
    );
    let progress2 = OwnerSetupApprovalProgressV2 {
        api_version: 2,
        custody_account_id: a2.input.request.context.custody_account_id.clone(),
        provision_id: a2.plan.provision_id.clone(),
        setup_id: a2.plan.setup_id.clone(),
        phase: ApprovalPhaseV1::Completed,
        model_budget_digest: a2
            .plan
            .model_budget
            .as_ref()
            .map(|b| b.digest(a2.plan.deadline.0))
            .transpose()
            .unwrap(),
    };
    assert!(Expected::ApproveV2(Box::new(a2)).matches(&serde_json::to_string(&progress2).unwrap()));
    let a1: OwnerSetupApproveRequestV1 = fixture("OwnerSetupApproveRequestV1");
    let mut progress1: OwnerSetupProgressV1 = fixture("OwnerSetupProgressV1");
    progress1.custody_account_id = a1.input.request.context.custody_account_id.clone();
    progress1.provision_id = a1.plan.provision_id.clone();
    progress1.setup_id = a1.plan.setup_id.clone();
    assert!(Expected::ApproveV1(Box::new(a1)).matches(&serde_json::to_string(&progress1).unwrap()));
    for api_version in [1, 2] {
        let mut q: OwnerSetupDriveRequestV1 = fixture("OwnerSetupDriveRequestV1");
        q.api_version = api_version;
        progress1.api_version = api_version;
        progress1.custody_account_id = q.custody_account_id.clone();
        progress1.provision_id = q.provision_id.clone();
        progress1.setup_id = q.setup_id.clone();
        assert!(
            Expected::Continue(Box::new(q)).matches(&serde_json::to_string(&progress1).unwrap()),
            "shared progress structure retains the exact stored engine version"
        );
        progress1.api_version = if api_version == 1 { 2 } else { 1 };
        let mut q: OwnerSetupDriveRequestV1 = fixture("OwnerSetupDriveRequestV1");
        q.api_version = api_version;
        assert!(
            !Expected::Continue(Box::new(q)).matches(&serde_json::to_string(&progress1).unwrap()),
            "a different engine version cannot substitute for the request"
        );
    }
}
#[test]
fn management_owner_setup_rejects_version_scope_original_budget_and_unknown_changes() {
    let q: OwnerSetupPrepareRequestV2 = fixture("OwnerSetupPrepareRequestV2");
    let a: OwnerSetupApproveRequestV2 = fixture("OwnerSetupApproveRequestV2");
    let original = serde_json::to_value(&a.input.request).unwrap();
    let expected = Expected::PrepareV2(Box::new(q));
    assert!(expected.matches(&serde_json::to_string(&original).unwrap()));
    for field in [
        "approval_request_id",
        "context-account",
        "context-security",
        "plan-budget",
        "plan-scope",
        "unknown",
    ] {
        let mut value = original.clone();
        match field {
            "approval_request_id" => {
                value["approval_request_id"] = serde_json::json!("wrong-original")
            }
            "context-account" => value["context"]["custody_account_id"] = serde_json::json!(""),
            "context-security" => value["context"]["security_version"] = serde_json::json!("0"),
            "plan-budget" => value["intent"]["plan"]["transport_limit"] = serde_json::json!("585"),
            "plan-scope" => value["intent"]["plan"]["setup_id"] = serde_json::json!("wrong-scope"),
            "unknown" => value["unexpected"] = serde_json::json!(true),
            _ => unreachable!(),
        }
        assert!(
            !expected.matches(&serde_json::to_string(&value).unwrap()),
            "changed typed prepare must be rejected"
        );
    }
    let legacy: OwnerSetupPrepareResponseV1 = fixture("OwnerSetupPrepareResponseV1");
    assert!(
        !expected.matches(&serde_json::to_string(&legacy).unwrap()),
        "legacy intent cannot upgrade through the V2 branch"
    );
    let progress = OwnerSetupApprovalProgressV2 {
        api_version: 2,
        custody_account_id: a.input.request.context.custody_account_id.clone(),
        provision_id: a.plan.provision_id.clone(),
        setup_id: a.plan.setup_id.clone(),
        phase: ApprovalPhaseV1::Completed,
        model_budget_digest: a
            .plan
            .model_budget
            .as_ref()
            .map(|b| b.digest(a.plan.deadline.0))
            .transpose()
            .unwrap(),
    };
    let expected = Expected::ApproveV2(Box::new(a));
    let raw = serde_json::to_value(progress).unwrap();
    for field in [
        "api_version",
        "custody_account_id",
        "provision_id",
        "setup_id",
        "model_budget_digest",
    ] {
        let mut value = raw.clone();
        value[field] = if field == "api_version" {
            serde_json::json!(1)
        } else {
            serde_json::json!("wrong")
        };
        assert!(
            !expected.matches(&serde_json::to_string(&value).unwrap()),
            "changed V2 approval must be rejected"
        );
    }
    let legacy: OwnerSetupProgressV1 = fixture("OwnerSetupProgressV1");
    assert!(
        !expected.matches(&serde_json::to_string(&legacy).unwrap()),
        "V1 progress cannot stand in for V2 approval"
    );
}
