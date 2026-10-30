//! Canonical exporter sources only; generated artifacts are not hand-edited.
use super::*;
pub(super) fn schemas() -> Value {
    json!({
        "OwnerSetupPlanV2":schemars::schema_for!(OwnerSetupPlanV2),
        "TestSessionPlanV2":schemars::schema_for!(TestSessionPlanV2),
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
