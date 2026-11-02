//! Canonical exporter sources only; generated artifacts are not hand-edited.
use super::*;
#[path = "http_schema_resource_v2_fixtures.rs"]
mod examples;
pub(super) fn fixtures() -> Value {
    examples::fixtures()
}
pub(super) fn schemas() -> Value {
    json!({
        "OwnerSetupPlanV2":schemars::schema_for!(OwnerSetupPlanV2),
        "TestSessionPlanV2":schemars::schema_for!(TestSessionPlanV2),
        "OwnerSetupPrepareRequestV2":schemars::schema_for!(OwnerSetupPrepareRequestV2),
        "OwnerSetupApproveRequestV2":schemars::schema_for!(OwnerSetupApproveRequestV2),
        "TestSessionBasisRequestV2":schemars::schema_for!(TestSessionBasisRequestV2),
        "TestSessionBasisV2":schemars::schema_for!(TestSessionBasisV2),
        "TestSessionPrepareRequestV2":schemars::schema_for!(TestSessionPrepareRequestV2),
        "TestSessionApproveRequestV2":schemars::schema_for!(TestSessionApproveRequestV2),
        "TestSessionContinueRequestV2":schemars::schema_for!(TestSessionContinueRequestV2),
        "TestSessionApprovalProgressV2":schemars::schema_for!(TestSessionApprovalProgressV2),
        "TestSessionActivationProgressV2":schemars::schema_for!(TestSessionActivationProgressV2),
        "ResourceTestSessionScopeV2":schemars::schema_for!(ResourceTestSessionScopeV2),
        "PaidResourceReadProofV1":schemars::schema_for!(PaidResourceReadProofV1),
        "PaidResourceReadIssuerPinV1":schemars::schema_for!(PaidResourceReadIssuerPinV1),
        "PaidResourceReadIssuerFloorV1":schemars::schema_for!(PaidResourceReadIssuerFloorV1)
    })
}

#[test]
fn resource_v2_paid_nullable_fields_are_required() {
    let document = schemas();
    let payload = &document["PaidResourceReadProofV1"]["$defs"]["PaidResourceReadPayloadV1"];
    let required = payload["required"]
        .as_array()
        .expect("required payload fields");
    for field in [
        "test_session_id",
        "prepared_resource_basis_digest",
        "preflight_world_request_digest",
    ] {
        assert!(
            required.contains(&json!(field)),
            "missing required field: {field}"
        );
        assert_eq!(
            payload["properties"][field]["type"],
            json!(["string", "null"])
        );
    }
}

#[test]
fn resource_v2_schema_roots_all_have_nonempty_typed_examples() {
    let schemas = schemas();
    let fixtures = fixtures();
    assert_eq!(
        schemas.as_object().unwrap().len(),
        fixtures.as_object().unwrap().len()
    );
    for name in schemas.as_object().unwrap().keys() {
        assert!(
            !fixtures[name]
                .as_array()
                .expect("typed root fixtures")
                .is_empty(),
            "{name}"
        );
    }
}
