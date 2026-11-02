//! Genuine Core World3/claim/nonce/master and opaque signing factories.
//! Native/Ack facts use fixed fixture keys, not a protected Native deployment.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::{execution_fence_dispatch_v2::*, native_cleanup_v2::*};
fn hex_bytes(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}
fn issuer(f: &SessionFixture) -> AccountMachineIssuerV2 {
    AccountMachineIssuerV2 {
        namespace: "account-fixture".into(),
        service_id: "account-service".into(),
        environment: f
            .state
            .test_session_v2_header(&f.session_id)
            .unwrap()
            .1
            .role_scope
            .context
            .identity
            .environment
            .clone(),
        key_id: "account-issuer".into(),
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
        native_audience_origin: scope(&f.pin).http_origin.into(),
    }
}
fn pin(f: &SessionFixture, purpose: AccountMachinePurposeV2) -> AccountMachineIssuerPinV2 {
    let i = issuer(f);
    AccountMachineIssuerPinV2 {
        namespace: i.namespace,
        service_id: i.service_id,
        environment: i.environment,
        key_id: i.key_id,
        public_key: hex_bytes(SigningKey::from_bytes(&[97; 32]).verifying_key().to_bytes()),
        issuer_epoch: i.issuer_epoch,
        config_generation: i.config_generation,
        not_before: DecimalU64(1),
        not_after: DecimalU64(100000),
        native_audience_origin: i.native_audience_origin,
        allowed_purpose: purpose,
    }
}
fn floor() -> AccountMachineIssuerFloorV2 {
    AccountMachineIssuerFloorV2 {
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
    }
}
fn signed(bytes: &[u8]) -> String {
    hex_bytes(SigningKey::from_bytes(&[97; 32]).sign(bytes).to_bytes())
}
fn first() -> (SessionFixture, PendingFenceDispatchV2) {
    let mut f = ready();
    let (body, pop) = action(&mut f, "rf-dispatch-action", 13000);
    let proof = || SessionProofRequest {
        ingress_attempt_id: "rf-dispatch-ingress",
        original_bytes: &body,
        possession: &pop,
    };
    f.state
        .charge_test_session_v2_execution_pending_ingress(proof(), UnixMillis::new(13002))
        .unwrap();
    let accepted = f
        .state
        .reserve_test_session_v2_execution_pending(
            proof(),
            &"a7".repeat(32),
            &scope(&f.pin),
            UnixMillis::new(13003),
            &mut StrictCrypto,
        )
        .unwrap()
        .unwrap();
    let attempt = f
        .state
        .claim_test_session_v2_pending_first_directory(
            accepted,
            &scope(&f.pin),
            UnixMillis::new(13004),
            &mut StrictCrypto,
        )
        .unwrap();
    let authority = f
        .state
        .pending_directory_signing_authority_v2(attempt, issuer(&f))
        .unwrap();
    let signature = signed(&authority.signing_bytes().unwrap());
    let directory_pin = pin(&f, AccountMachinePurposeV2::ExecutionPendingCurrentLeafV2);
    let directory = f
        .state
        .record_pending_directory_signature_v2(
            authority,
            signature,
            directory_pin,
            floor(),
            UnixMillis::new(13005),
        )
        .unwrap()
        .unwrap();
    let i = issuer(&f);
    let authority = f
        .state
        .pending_first_fence_dispatch_authority_v2(
            directory,
            i,
            "ab".repeat(32),
            "bc".repeat(32),
            UnixMillis::new(13006),
        )
        .unwrap();
    let signature = signed(&authority.signing_bytes().unwrap());
    let dispatch_pin = pin(&f, AccountMachinePurposeV2::ExecutionFenceDispatchV2);
    let first = f
        .state
        .record_pending_first_fence_dispatch_v2(
            authority,
            signature,
            dispatch_pin,
            floor(),
            UnixMillis::new(13007),
        )
        .unwrap()
        .unwrap();
    (f, first)
}
#[test]
fn session_v2_pending_fence_genuine_first_issuer_read_once_cold() {
    let (mut f, first) = first();
    let proof = first.proof().clone();
    let dispatch_pin = pin(&f, AccountMachinePurposeV2::ExecutionFenceDispatchV2);
    let before = remaining(&f);
    assert_eq!(proof.binding.dispatcher_object_id, "ab".repeat(32));
    let state = f.state.clone();
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    assert_eq!(cold, state);
    let authority = f
        .state
        .pending_first_fence_read_authority_v2(
            &proof,
            &dispatch_pin,
            &floor(),
            UnixMillis::new(13008),
        )
        .unwrap();
    let signature = signed(&authority.signing_bytes().unwrap());
    let read_pin = pin(&f, AccountMachinePurposeV2::ExecutionFenceReadV2);
    let read = f
        .state
        .record_pending_first_fence_read_v2(
            authority,
            signature,
            read_pin.clone(),
            floor(),
            UnixMillis::new(13009),
        )
        .unwrap();
    read.verify(&read_pin, &floor(), 13009).unwrap();
    assert!(
        read.claim.binding == proof.binding,
        "exact original binding retained"
    );
    assert_eq!(
        remaining(&f),
        before,
        "claimed leaves use already paid master, no second charge"
    );
    let paid = f.state.clone();
    assert!(
        f.state
            .pending_first_fence_read_authority_v2(
                &proof,
                &dispatch_pin,
                &floor(),
                UnixMillis::new(13010)
            )
            .is_err()
    );
    assert_eq!(f.state, paid);
    let value = serde_json::to_value(&paid).unwrap();
    let cold: AccountProvisionApprovalState = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(cold, paid);
    for field in ["fence_dispatch", "fence_read"] {
        let mut bad = value.clone();
        bad["test_sessions_v2"][&f.session_id]["execution_pending_start"][field] =
            serde_json::Value::Null;
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
            "record deletion cannot keep its birth journal"
        );
    }
}
#[test]
fn session_v2_pending_fence_actual_finish_before_child_claim_blocks() {
    let (mut f, first) = first();
    let proof = first.proof().clone();
    let q = SessionControlRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "rf-dispatch-finish".into(),
        kind: SessionControlKindV1::Finish,
    };
    let challenge = super::super::super::control_tests::challenge(
        &mut f,
        &q,
        SessionPurposeV1::Execution,
        73,
        13008,
    );
    let body = serde_json::to_vec(&q).unwrap();
    let pop = super::super::super::control_tests::control_pop(
        &f,
        &body,
        (
            HttpPopRoute::Finish(f.session_id.clone()),
            HttpPopProfileV1::Custody,
        ),
        &q.request_id,
        Some(&challenge),
        SessionPurposeV1::Execution,
        13011,
    );
    super::super::super::control_tests::control_paid(
        &mut f,
        &pop,
        "rf-dispatch-finish-ingress",
        13011,
    );
    f.state
        .control_test_session_v2(
            SessionProofRequest {
                ingress_attempt_id: "rf-dispatch-finish-ingress",
                original_bytes: &body,
                possession: &pop,
            },
            None,
            UnixMillis::new(13011),
            &mut StrictCrypto,
        )
        .unwrap();
    let before = f.state.clone();
    let dispatch_pin = pin(&f, AccountMachinePurposeV2::ExecutionFenceDispatchV2);
    assert!(
        f.state
            .pending_first_fence_read_authority_v2(
                &proof,
                &dispatch_pin,
                &floor(),
                UnixMillis::new(13011)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
}

#[test]
fn session_v2_pending_fence_signed_receiver_object_purpose_and_absolute_window_are_closed() {
    let (f, first) = first();
    let proof = first.proof();
    let p = pin(&f, AccountMachinePurposeV2::ExecutionFenceDispatchV2);
    proof
        .verify(&p, &floor(), &proof.binding.dispatcher_object_id, 13007)
        .unwrap();
    assert!(proof.verify(&p, &floor(), &"cd".repeat(32), 13007).is_err());
    let wrong = pin(&f, AccountMachinePurposeV2::ExecutionFenceReadV2);
    assert!(
        proof
            .verify(&wrong, &floor(), &proof.binding.dispatcher_object_id, 13007)
            .is_err()
    );
    assert!(
        proof
            .verify(
                &p,
                &floor(),
                &proof.binding.dispatcher_object_id,
                proof.binding.expires_at.0
            )
            .is_err()
    );
    let mut altered = proof.clone();
    altered.binding.fence_object_id = "de".repeat(32);
    assert!(
        altered
            .verify(&p, &floor(), &proof.binding.dispatcher_object_id, 13007)
            .is_err()
    );
}
