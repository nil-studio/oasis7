//! Signature-codec fixture only; not a paid RF receiver, Source or Current.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::read_fence_dispatch_v3::*;
fn hx(b: impl AsRef<[u8]>) -> String {
    b.as_ref().iter().map(|x| format!("{x:02x}")).collect()
}
fn material() -> (
    ReadFenceBindingV3,
    AccountMachineIssuerPinV2,
    AccountMachineIssuerFloorV2,
) {
    let p = ready();
    let s = p.f.state.test_session_v2_snapshot(&p.f.session_id).unwrap();
    let issuer = AccountMachineIssuerV2 {
        namespace: "account-fixture".into(),
        service_id: "account-service".into(),
        environment: p.f.context.authority.environment.clone(),
        key_id: "read-rf-fixture".into(),
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
        native_audience_origin: scope(&p.f.pin).http_origin.into(),
    };
    let body = serde_json::to_vec(&ReadFenceQueryV3 {
        api_version: 3,
        expected_service_epoch: p.f.context.authority.service_epoch,
    })
    .unwrap();
    let binding = ReadFenceBindingV3 {
        api_version: 3,
        ingress_attempt_id: "read-rf-ingress".into(),
        original_digest: "a1".repeat(32),
        source_nonce: "b2".repeat(32),
        root_read_deadline: DecimalU64(60_000),
        source_until: DecimalU64(19_000),
        request_until: DecimalU64(19_000),
        issuer: issuer.clone(),
        scope: s.session().scope.role_scope.clone(),
        session_id: p.f.session_id.clone(),
        approved_input_digest: "c3".repeat(32),
        read_graph_digest: "d4".repeat(32),
        source_operation_id: "fixture-read-source".into(),
        reservation_ordinal: DecimalU64(10),
        dispatcher_claim_ordinal: DecimalU64(11),
        phase: ReadFencePhaseV3::ClaimSource,
        dispatcher_attempt_id: "fixture-read-dispatch".into(),
        fence_attempt_id: "fixture-read-fence".into(),
        dispatcher_object_id: "e5".repeat(32),
        fence_object_id: "f6".repeat(32),
        expected_service_epoch: p.f.context.authority.service_epoch,
        business_body_digest: oasis7_custody_core::approval::bytes_digest(&body),
        issued_at: DecimalU64(14_000),
        expires_at: DecimalU64(19_000),
    };
    let pin = AccountMachineIssuerPinV2 {
        namespace: issuer.namespace,
        service_id: issuer.service_id,
        environment: issuer.environment,
        key_id: issuer.key_id,
        public_key: hx(SigningKey::from_bytes(&[113; 32])
            .verifying_key()
            .to_bytes()),
        issuer_epoch: issuer.issuer_epoch,
        config_generation: issuer.config_generation,
        not_before: DecimalU64(1),
        not_after: DecimalU64(60_000),
        native_audience_origin: issuer.native_audience_origin,
        allowed_purpose: AccountMachinePurposeV2::ReadFenceDispatchV3,
    };
    (
        binding,
        pin,
        AccountMachineIssuerFloorV2 {
            issuer_epoch: DecimalU64(1),
            config_generation: DecimalU64(1),
        },
    )
}
fn dispatch(binding: ReadFenceBindingV3) -> SignedReadFenceDispatchV3 {
    let signature_hex = hx(SigningKey::from_bytes(&[113; 32])
        .sign(&SignedReadFenceDispatchV3::signing_bytes(&binding).unwrap())
        .to_bytes());
    SignedReadFenceDispatchV3 {
        binding,
        signature_hex,
    }
}
#[test]
fn read_rf_codec_strong_roundtrip_domain_purpose_receiver_and_deadline() {
    let (b, pin, floor) = material();
    b.validate_shape().unwrap();
    let d = dispatch(b.clone());
    d.verify(&pin, &floor, &b.dispatcher_object_id, 14_001)
        .unwrap();
    let cold: SignedReadFenceDispatchV3 =
        serde_json::from_slice(&serde_json::to_vec(&d).unwrap()).unwrap();
    assert_eq!(cold.digest().unwrap(), d.digest().unwrap());
    cold.verify(&pin, &floor, &b.dispatcher_object_id, 14_001)
        .unwrap();
    let mut wrong = pin.clone();
    wrong.allowed_purpose = AccountMachinePurposeV2::ExecutionFenceDispatchV2;
    assert!(
        d.verify(&wrong, &floor, &b.dispatcher_object_id, 14_001)
            .is_err()
    );
    assert!(d.verify(&pin, &floor, &"aa".repeat(32), 14_001).is_err());
    assert!(
        d.verify(&pin, &floor, &b.dispatcher_object_id, 19_000)
            .is_err()
    );
    let mut corrupt = d.clone();
    corrupt.binding.original_digest = "bb".repeat(32);
    assert!(
        corrupt
            .verify(&pin, &floor, &b.dispatcher_object_id, 14_001)
            .is_err()
    );
    let mut late = b.clone();
    late.expires_at = DecimalU64(19_001);
    assert!(late.validate_shape().is_err());
    let mut rotate = b.issuer.clone();
    rotate.key_id = "rotated".into();
    rotate.issuer_epoch = DecimalU64(2);
    assert_eq!(
        read_fence_receiver_name_v3(&b.issuer, &b.dispatcher_attempt_id).unwrap(),
        read_fence_receiver_name_v3(&rotate, &b.dispatcher_attempt_id).unwrap()
    );
}
#[test]
fn read_rf_codec_causal_read_binds_dispatch_exact_query_and_separate_pin() {
    let (b, pin, floor) = material();
    let d = dispatch(b.clone());
    let claim = ReadFenceReadV3 {
        binding: b.clone(),
        dispatch_digest: d.digest().unwrap(),
        claim_ordinal: DecimalU64(14),
        claimed_at: DecimalU64(14_002),
    };
    let signature_hex = hx(SigningKey::from_bytes(&[113; 32])
        .sign(&claim.signing_bytes().unwrap())
        .to_bytes());
    let proof = SignedReadFenceReadV3 {
        claim,
        signature_hex,
    };
    let mut read_pin = pin.clone();
    read_pin.allowed_purpose = AccountMachinePurposeV2::ReadFenceReadV3;
    proof
        .verify(&read_pin, &floor, &b.fence_object_id, 14_003)
        .unwrap();
    assert!(
        proof
            .verify(&pin, &floor, &b.fence_object_id, 14_003)
            .is_err()
    );
    let query = ReadFenceQueryV3 {
        api_version: 3,
        expected_service_epoch: b.expected_service_epoch,
    };
    let mut request = ReadFenceReadRequestV3 {
        query,
        authorization: proof,
    };
    request.validate_binding().unwrap();
    request.query.expected_service_epoch = DecimalU64(b.expected_service_epoch.0 + 1);
    assert!(request.validate_binding().is_err());
}

#[path = "test_session_v2_read_fence_admission_tests.rs"]
mod admission_tests;
