//! Real issuer signatures validate the wire contract, not Account prepayment.
use super::*;
use crate::world_service::sign_read_request;
fn fixture() -> (
    PaidCleanupTransportProofV1,
    CleanupIssuerPinV1,
    CleanupIssuerFloorV1,
) {
    let issuer = "11".repeat(32);
    let public_key = sign_read_request("key", (), &issuer)
        .unwrap()
        .subject_public_key;
    let world = WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
    };
    let pin = CleanupIssuerPinV1 {
        namespace: "custody".into(),
        service_id: "account".into(),
        environment: "test".into(),
        key_id: "issuer-1".into(),
        public_key,
        epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
        not_before: DecimalU64(100),
        not_after: DecimalU64(10000),
        native_origin: "https://native.test".into(),
        world_origin: "https://world.test".into(),
        world: world.clone(),
    };
    let p = PaidCleanupTransportV1 {
        api_revision: 1,
        issuer_namespace: pin.namespace.clone(),
        issuer_service_id: pin.service_id.clone(),
        issuer_environment: pin.environment.clone(),
        issuer_key_id: pin.key_id.clone(),
        issuer_epoch: pin.epoch,
        issuer_config_generation: pin.config_generation,
        native: CleanupPeerInstanceV1 {
            origin: pin.native_origin.clone(),
            incarnation: "22".repeat(32),
        },
        world_peer: CleanupPeerInstanceV1 {
            origin: pin.world_origin.clone(),
            incarnation: "33".repeat(32),
        },
        world,
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        operation: CleanupHttpOperationV1::Lookup,
        custody_account_id: "account".into(),
        provision_id: "provision".into(),
        setup_id: "setup".into(),
        session_id: "session".into(),
        approval_operation_id: "approve".into(),
        approval_input_digest: "44".repeat(32),
        approved_intent_digest: "45".repeat(32),
        test_session_plan_digest: "46".repeat(32),
        termination_plan_digest: "47".repeat(32),
        original_digest: "48".repeat(32),
        scope_digest: "49".repeat(32),
        canonical_operation_id: "close".into(),
        source_operation_id: "source".into(),
        source_record_digest: "50".repeat(32),
        source_security_epoch: DecimalU64(1),
        directory_generation: DecimalU64(1),
        resource_flag_generation: DecimalU64(1),
        attempt_id: "attempt".into(),
        native_transport_id: "native-attempt".into(),
        world_transport_id: "world-attempt".into(),
        paid_record_digest: "51".repeat(32),
        native_body_digest: "52".repeat(32),
        world_body_digest: "53".repeat(32),
        native_paid_index: DecimalU64(0),
        world_paid_index: DecimalU64(1),
        native_http_units: DecimalU64(1),
        world_http_units: DecimalU64(1),
        issued_at: DecimalU64(1000),
        expires_at: DecimalU64(4000),
        cleanup_until: DecimalU64(9000),
    };
    (
        sign_read_request(PAID_CLEANUP_TRANSPORT_DOMAIN_V1, p, &issuer)
            .unwrap()
            .into(),
        pin,
        CleanupIssuerFloorV1 {
            epoch: DecimalU64(1),
            config_generation: DecimalU64(1),
        },
    )
}
fn verify(
    proof: &PaidCleanupTransportProofV1,
    pin: &CleanupIssuerPinV1,
    floor: &CleanupIssuerFloorV1,
) -> Result<(), String> {
    verify_paid_cleanup_transport_v1(
        proof,
        pin,
        floor,
        &CleanupTransportReceiveV1 {
            role: CleanupConsumerRoleV1::World,
            origin: "https://world.test",
            method: "POST",
            path: CleanupHttpOperationV1::Lookup.world_path(),
            body_digest: &"53".repeat(32),
            incarnation: &"33".repeat(32),
            now: 2000,
        },
    )
}
#[test]
fn paid_cleanup_real_signature_exact_target_and_required_codec() {
    let (proof, pin, floor) = fixture();
    verify(&proof, &pin, &floor).unwrap();
    let generic = sign_read_request(
        PAID_CLEANUP_TRANSPORT_DOMAIN_V1,
        proof.request.clone(),
        &"11".repeat(32),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_vec(&proof).unwrap(),
        serde_json::to_vec(&generic).unwrap()
    );
    assert_eq!(
        serde_cbor::to_vec(&proof).unwrap(),
        serde_cbor::to_vec(&generic).unwrap()
    );
    let bytes = serde_json::to_vec(&proof).unwrap();
    let round: PaidCleanupTransportProofV1 = serde_json::from_slice(&bytes).unwrap();
    assert!(round == proof);
    let mut extra: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    extra["unrecognized_authority"] = serde_json::json!(true);
    assert!(serde_json::from_value::<PaidCleanupTransportProofV1>(extra).is_err());
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["request"]
        .as_object_mut()
        .unwrap()
        .remove("approval_input_digest");
    assert!(serde_json::from_value::<PaidCleanupTransportProofV1>(value).is_err());
    let mut altered = proof.clone();
    altered.request.world_body_digest = "99".repeat(32);
    assert!(verify(&altered, &pin, &floor).is_err());
    let mut wrong_floor = floor.clone();
    wrong_floor.config_generation = DecimalU64(2);
    assert!(verify(&proof, &pin, &wrong_floor).is_err());
    for (method, path, origin, nonce, now) in [
        (
            "GET",
            CleanupHttpOperationV1::Lookup.world_path(),
            "https://world.test",
            "33".repeat(32),
            2000,
        ),
        (
            "POST",
            CleanupHttpOperationV1::Submit.world_path(),
            "https://world.test",
            "33".repeat(32),
            2000,
        ),
        (
            "POST",
            CleanupHttpOperationV1::Lookup.world_path(),
            "https://other.test",
            "33".repeat(32),
            2000,
        ),
        (
            "POST",
            CleanupHttpOperationV1::Lookup.world_path(),
            "https://world.test",
            "34".repeat(32),
            2000,
        ),
        (
            "POST",
            CleanupHttpOperationV1::Lookup.world_path(),
            "https://world.test",
            "33".repeat(32),
            4000,
        ),
    ] {
        assert!(
            verify_paid_cleanup_transport_v1(
                &proof,
                &pin,
                &floor,
                &CleanupTransportReceiveV1 {
                    role: CleanupConsumerRoleV1::World,
                    origin,
                    method,
                    path,
                    body_digest: &"53".repeat(32),
                    incarnation: &nonce,
                    now
                }
            )
            .is_err()
        );
    }
}
#[test]
fn paid_cleanup_issuer_signed_bad_origins_and_budget_shapes_are_rejected() {
    let (proof, pin, floor) = fixture();
    verify(&proof, &pin, &floor).unwrap();
    for origin in [
        "https://world.test/path",
        "https://user@world.test",
        "https://world.test?query",
        "https://world.test#fragment",
    ] {
        let mut p = proof.request.clone();
        p.world_peer.origin = origin.into();
        let signed: PaidCleanupTransportProofV1 =
            sign_read_request(PAID_CLEANUP_TRANSPORT_DOMAIN_V1, p, &"11".repeat(32))
                .unwrap()
                .into();
        assert!(verify(&signed, &pin, &floor).is_err());
    }
    for (native, world, until) in [(0, 0, 4000), (0, 24, 4000), (0, 1, 999), (0, 1, 9001)] {
        let mut p = proof.request.clone();
        p.native_paid_index = DecimalU64(native);
        p.world_paid_index = DecimalU64(world);
        p.expires_at = DecimalU64(until);
        let signed: PaidCleanupTransportProofV1 =
            sign_read_request(PAID_CLEANUP_TRANSPORT_DOMAIN_V1, p, &"11".repeat(32))
                .unwrap()
                .into();
        assert!(verify(&signed, &pin, &floor).is_err());
    }
}
