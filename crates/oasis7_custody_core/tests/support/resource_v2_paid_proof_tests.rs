//! Pure configured signature tests, not an issuer or payment authority.
use super::*;
fn key43() -> SigningKey {
    SigningKey::from_bytes(&[43; 32])
}
fn hexbytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn signed(payload: PaidResourceReadPayloadV1) -> PaidResourceReadProofV1 {
    let signature_hex = hexbytes(&key43().sign(&payload.signing_bytes().unwrap()).to_bytes());
    PaidResourceReadProofV1 {
        payload,
        signature_hex,
    }
}
fn fixture_paid() -> (
    PaidResourceReadProofV1,
    PaidResourceReadIssuerPinV1,
    PaidResourceReadIssuerFloorV1,
) {
    let payload = PaidResourceReadPayloadV1 {
        api_version: 1,
        issuer_namespace: "custody:test".into(),
        issuer_service_id: "identity".into(),
        issuer_environment: "test".into(),
        issuer_key_id: "resource-issuer".into(),
        issuer_epoch: DecimalU64(2),
        issuer_config_generation: DecimalU64(3),
        native_audience_origin: "https://native.example".into(),
        world_audience: plan().world_identity,
        native_path: wire::NATIVE_OWNER_RESOURCE_VIEW_PATH_V1.into(),
        world_path: wire::OWNER_RESOURCE_VIEW_PATH_V1.into(),
        stage: PaidResourceReadStageV1::TestSessionOwnerRead,
        custody_account_id: "custody-one".into(),
        manager_native_account_id: "manager-one".into(),
        participant_native_account_id: "participant-one".into(),
        provision_id: "provision".into(),
        setup_id: "setup".into(),
        test_session_id: Some("session".into()),
        approval_operation_id: "approval-operation".into(),
        approval_input_digest: "a1".repeat(32),
        plan_digest: "a2".repeat(32),
        prepared_resource_basis_digest: Some("a3".repeat(32)),
        selection_digest: "a4".repeat(32),
        owner_public_key: "a5".repeat(32),
        owner_binding: "owner".into(),
        task_fence_digest: "a6".repeat(32),
        signer_record_digest: "a7".repeat(32),
        native_request_digest: "a8".repeat(32),
        world_request_digest: "a9".repeat(32),
        preflight_world_request_digest: None,
        source_operation_id: "source".into(),
        source_record_digest: "b1".repeat(32),
        source_security_epoch: DecimalU64(2),
        service_epoch: DecimalU64(3),
        directory_generation: DecimalU64(4),
        resource_flag_generation: DecimalU64(5),
        paid_attempt_id: "attempt".into(),
        paid_record_digest: "b2".repeat(32),
        native_http_units: DecimalU64(1),
        world_http_units: DecimalU64(1),
        issued_at: DecimalU64(1000),
        expires_at: DecimalU64(6000),
    };
    let pin = PaidResourceReadIssuerPinV1 {
        namespace: payload.issuer_namespace.clone(),
        service_id: payload.issuer_service_id.clone(),
        environment: payload.issuer_environment.clone(),
        key_id: payload.issuer_key_id.clone(),
        public_key: hexbytes(&key43().verifying_key().to_bytes()),
        issuer_epoch: payload.issuer_epoch,
        issuer_config_generation: payload.issuer_config_generation,
        not_before: DecimalU64(1),
        not_after: DecimalU64(10_000),
        native_audience_origin: payload.native_audience_origin.clone(),
        world_audience: payload.world_audience.clone(),
        allowed_stage: payload.stage,
    };
    (
        signed(payload),
        pin,
        PaidResourceReadIssuerFloorV1 {
            issuer_epoch: DecimalU64(2),
            config_generation: DecimalU64(3),
        },
    )
}
#[test]
fn resource_v2_paid_proof_real_signature_exact_pins_floor_and_deadline() {
    let (proof, pin, floor) = fixture_paid();
    proof.verify_issuer(&pin, &floor, 1000).unwrap();
    proof.verify_issuer(&pin, &floor, 5999).unwrap();
    assert!(proof.verify_issuer(&pin, &floor, 999).is_err());
    assert!(proof.verify_issuer(&pin, &floor, 6000).is_err());
    for mutation in 0..5 {
        let mut p = pin.clone();
        match mutation {
            0 => {
                p.public_key =
                    hexbytes(&SigningKey::from_bytes(&[44; 32]).verifying_key().to_bytes())
            }
            1 => p.issuer_epoch.0 += 1,
            2 => p.issuer_config_generation.0 += 1,
            3 => p.native_audience_origin = "https://other.example".into(),
            _ => p.world_audience.branch_id = "other-branch".into(),
        };
        assert!(proof.verify_issuer(&p, &floor, 1000).is_err());
    }
    let mut raised = floor.clone();
    raised.config_generation.0 += 1;
    assert!(proof.verify_issuer(&pin, &raised, 1000).is_err());
    let mut wrong = proof.clone();
    wrong.payload.native_request_digest = "ee".repeat(32);
    assert!(wrong.verify_issuer(&pin, &floor, 1000).is_err());
    let mut expired = proof.payload.clone();
    expired.expires_at.0 += 1;
    assert!(expired.signing_bytes().is_err());
    let mut overflow = proof.payload;
    overflow.issued_at.0 = u64::MAX - 1;
    overflow.expires_at.0 = u64::MAX;
    assert!(overflow.signing_bytes().is_err());
}
#[test]
fn resource_v2_paid_proof_stage_units_and_nullable_fields_are_closed() {
    let (proof, _, _) = fixture_paid();
    let mut p = proof.payload.clone();
    p.world_http_units.0 = 2;
    assert!(p.validate_shape().is_err());
    p = proof.payload.clone();
    p.stage = PaidResourceReadStageV1::OwnerSetupFinal;
    assert!(p.validate_shape().is_err());
    p.test_session_id = None;
    p.prepared_resource_basis_digest = None;
    p.preflight_world_request_digest = Some("cc".repeat(32));
    p.native_path = "/entry-owner/control".into();
    p.world_path = wire::RESOURCE_GOAL_PATH_V2.into();
    p.world_http_units = DecimalU64(2);
    p.validate_shape().unwrap();
    let mut json = serde_json::to_value(&proof).unwrap();
    json["payload"]
        .as_object_mut()
        .unwrap()
        .remove("preflight_world_request_digest");
    assert!(serde_json::from_value::<PaidResourceReadProofV1>(json).is_err());
    let mut json = serde_json::to_value(&proof).unwrap();
    json["payload"]["issuer_epoch"] = json!(2);
    assert!(serde_json::from_value::<PaidResourceReadProofV1>(json).is_err());
    let mut json = serde_json::to_value(&proof).unwrap();
    json["payload"]["verified"] = json!(true);
    assert!(serde_json::from_value::<PaidResourceReadProofV1>(json).is_err());
}

fn reject_non_origin_with_real_issuer_signature(origin: &str) {
    let (original, mut pin, floor) = fixture_paid();
    let mut payload = original.payload;
    payload.native_audience_origin = origin.into();
    pin.native_audience_origin = origin.into();
    // Sign the exact canonical bytes independently of the shape validator so a
    // later shape fix cannot turn this test into a signing-constructor panic.
    let bytes =
        oasis7_wasm_abi::encode_canonical_cbor(&(PAID_RESOURCE_READ_DOMAIN, &payload)).unwrap();
    let signature = key43().sign(&bytes);
    key43()
        .verifying_key()
        .verify_strict(&bytes, &signature)
        .unwrap();
    let proof = PaidResourceReadProofV1 {
        payload,
        signature_hex: hexbytes(&signature.to_bytes()),
    };
    assert!(
        proof.verify_issuer(&pin, &floor, 1000).is_err(),
        "even a genuine issuer signature and identical configured pin must reject a non-origin audience"
    );
}
#[test]
fn resource_v2_paid_proof_rejects_signed_origin_path() {
    reject_non_origin_with_real_issuer_signature("https://native.example/private");
}
#[test]
fn resource_v2_paid_proof_rejects_signed_origin_query() {
    reject_non_origin_with_real_issuer_signature("https://native.example?next=private");
}
#[test]
fn resource_v2_paid_proof_rejects_signed_origin_userinfo() {
    reject_non_origin_with_real_issuer_signature("https://manager@native.example");
}
#[test]
fn resource_v2_paid_proof_rejects_signed_origin_fragment() {
    reject_non_origin_with_real_issuer_signature("https://native.example#private");
}
