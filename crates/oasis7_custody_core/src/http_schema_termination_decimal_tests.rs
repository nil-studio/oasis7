//! Custody metadata codec regression only; this shape is not a live authority fixture.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_client_api::world_service::{
    private_goal::verify_owner_signature_strict, signing_bytes,
};
use oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1;
#[test]
fn termination_metadata_keeps_full_width_decimals_without_changing_sdk_signature_bytes() {
    let original = AgentSignerDelegationChangeV1 {
        world_id: "world-one".into(),
        branch_id: "branch-one".into(),
        agent_id: "agent-one".into(),
        owner_binding: "owner-one".into(),
        agent_identity_generation: 9_007_199_254_740_993,
        generation: u64::MAX,
        delegate_public_key: "cc".repeat(32),
        revoked: false,
        nonce: u64::MAX,
    };
    let sdk_before = serde_json::to_value(&original).unwrap();
    assert!(sdk_before["agent_identity_generation"].is_u64());
    assert!(sdk_before["generation"].is_u64());
    let domain = "test-sdk-delegation-compatibility";
    let bytes_before = signing_bytes(domain, &original).unwrap();
    let key = SigningKey::from_bytes(&[17; 32]);
    let signature = hex_signature(&key.sign(&bytes_before).to_bytes());
    let public = hex_signature(&key.verifying_key().to_bytes());
    let options = options();
    let intent = serde_json::to_value(&options[2].intent).unwrap();
    let plan = TerminationPlanV1 {
        api_version: 1,
        session_id: "session-one".into(),
        custody_account_id: "custody-one".into(),
        provision_id: "provision-one".into(),
        setup_id: "setup-one".into(),
        prepared_result_digest: "ee".repeat(32),
        world: serde_json::from_value(intent["world_identity"].clone()).unwrap(),
        owner_key_ref: "owner-one".into(),
        executor_key_ref: "executor-one".into(),
        initial_fence: serde_json::from_value(intent["task_fence"].clone()).unwrap(),
        original_delegation: original.clone(),
        stop_operation_id: "stop-one".into(),
        revoke_operation_id: "revoke-one".into(),
        cleanup_until: crate::time_units::UnixMillis::new(601000),
        request_limit: DecimalU64(2),
        transport_limit: DecimalU64(24),
        trigger: TerminationTriggerV1::ClosingOnlyV1,
        nonce_policy: TerminationNoncePolicyV1::AccountAllocatedHighWaterV1,
    };
    let encoded = serde_json::to_value(&plan).unwrap();
    // Effective RED must reach this assertion, not fail constructing a test shape.
    assert_eq!(
        encoded["original_delegation"]["agent_identity_generation"],
        json!("9007199254740993")
    );
    assert_eq!(
        encoded["original_delegation"]["generation"],
        json!(u64::MAX.to_string())
    );
    assert_eq!(
        encoded["original_delegation"]["nonce"],
        json!(u64::MAX.to_string())
    );
    for field in ["agent_identity_generation", "generation", "nonce"] {
        for invalid in [
            json!(1),
            json!("01"),
            json!("+1"),
            json!("18446744073709551616"),
        ] {
            let mut wrong = encoded.clone();
            wrong["original_delegation"][field] = invalid;
            assert!(serde_json::from_value::<TerminationPlanV1>(wrong).is_err());
        }
    }
    let mut wrong = encoded.clone();
    wrong["original_delegation"]["verified"] = true.into();
    assert!(serde_json::from_value::<TerminationPlanV1>(wrong).is_err());
    let canonical = oasis7_wasm_abi::encode_canonical_cbor(&plan).unwrap();
    let decoded_cbor: TerminationPlanV1 = serde_cbor::from_slice(&canonical).unwrap();
    assert_eq!(decoded_cbor, plan);
    let serde_cbor::Value::Map(map) =
        serde_cbor::from_slice::<serde_cbor::Value>(&canonical).unwrap()
    else {
        panic!("typed map expected")
    };
    let serde_cbor::Value::Map(delegation) = map
        .get(&serde_cbor::Value::Text("original_delegation".into()))
        .unwrap()
    else {
        panic!("typed delegation expected")
    };
    assert_eq!(
        delegation.get(&serde_cbor::Value::Text("generation".into())),
        Some(&serde_cbor::Value::Text(u64::MAX.to_string()))
    );
    let schema = serde_json::to_value(schemars::schema_for!(TerminationPlanV1)).unwrap();
    let metadata = &schema["$defs"]["DelegationMetadataV1"];
    assert_eq!(metadata["additionalProperties"], json!(false));
    for field in ["agent_identity_generation", "generation", "nonce"] {
        let property = &metadata["properties"][field];
        let decimal = property
            .get("$ref")
            .and_then(Value::as_str)
            .map_or(property, |reference| {
                &schema["$defs"][reference.strip_prefix("#/$defs/").unwrap()]
            });
        assert_eq!(decimal["type"], json!("string"));
    }
    let decoded: TerminationPlanV1 = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.original_delegation, original);
    assert_eq!(
        serde_json::to_value(&decoded.original_delegation).unwrap(),
        sdk_before
    );
    assert_eq!(
        signing_bytes(domain, &decoded.original_delegation).unwrap(),
        bytes_before
    );
    verify_owner_signature_strict(domain, &decoded.original_delegation, &public, &signature)
        .unwrap();
}
fn hex_signature(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
