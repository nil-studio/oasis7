//! Core service-fixture evidence only: genuine World3 Ready/claimed/Source/LF.
//! Native SQLite protected lineage and actual HTTP counters require the separate
//! Native/Worker harness; fixed test signatures do not claim those qualifications.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::{
    execution_private_current_v2::*, native_cleanup_v2::*, native_execution_prepare_v2::*,
};
fn issuer(
    purpose: AccountMachinePurposeV2,
    origin: &str,
) -> (
    SigningKey,
    AccountMachineIssuerV2,
    AccountMachineIssuerPinV2,
    AccountMachineIssuerFloorV2,
) {
    let key = SigningKey::from_bytes(&[97; 32]);
    let pin = AccountMachineIssuerPinV2 {
        namespace: "account-fixture".into(),
        service_id: "account-service".into(),
        environment: "test".into(),
        key_id: "account-issuer".into(),
        public_key: encode_hex(key.verifying_key().to_bytes()),
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
        not_before: DecimalU64(0),
        not_after: DecimalU64(100_000),
        native_audience_origin: origin.into(),
        allowed_purpose: purpose,
    };
    let issuer = AccountMachineIssuerV2 {
        namespace: pin.namespace.clone(),
        service_id: pin.service_id.clone(),
        environment: pin.environment.clone(),
        key_id: pin.key_id.clone(),
        issuer_epoch: pin.issuer_epoch,
        config_generation: pin.config_generation,
        native_audience_origin: pin.native_audience_origin.clone(),
    };
    (
        key,
        issuer,
        pin,
        AccountMachineIssuerFloorV2 {
            issuer_epoch: DecimalU64(1),
            config_generation: DecimalU64(1),
        },
    )
}
fn ready_new() -> (ReadyPrepare, String) {
    let f = super::super::super::super::super::machine_tests::ready_world3(9000);
    let f = claimed_execution_from(f);
    let mut ready = ready_with_actual_session(f, None, None, false, |_| {});
    let a = allocation();
    let paid = ready
        .f
        .state
        .charge_test_session_v2_execution_prepare_owner_factory_attempt(
            &ready.f.session_id,
            &a,
            SessionProofRequest {
                ingress_attempt_id: &a.factory_ingress_id,
                original_bytes: &ready.body,
                possession: &ready.pop,
            },
            UnixMillis::new(14_006),
        )
        .unwrap();
    assert!(
        ready
            .f
            .state
            .claim_test_session_v2_prepare_current_ports(
                &ready.f.session_id,
                &paid,
                UnixMillis::new(14_006)
            )
            .unwrap()
            .is_some()
    );
    (ready, paid)
}
fn private(ready: &mut ReadyPrepare, paid: &str) -> ExecutionPrivateCurrentObservationV2 {
    let ns = scope(&ready.f.pin);
    let token = ready
        .f
        .state
        .claim_test_session_v2_prepare_private_current(
            &ready.f.session_id,
            paid,
            &ns,
            UnixMillis::new(14_007),
            UnixMillis::new(19_000),
            &mut StrictCrypto,
        )
        .unwrap()
        .unwrap();
    let (key, issuer, pin, floor) = issuer(
        AccountMachinePurposeV2::ExecutionPrivateCurrentV2,
        ns.http_origin,
    );
    let certificate = token.certificate(issuer).unwrap();
    let proof = SignedAccountExecutionPrivateCurrentV2 {
        signature_hex: encode_hex(key.sign(&certificate.signing_bytes().unwrap()).to_bytes()),
        certificate,
    };
    let token = ready
        .f
        .state
        .record_test_session_v2_prepare_private_proof(
            &ready.f.session_id,
            token,
            proof,
            &pin,
            &floor,
            UnixMillis::new(14_007),
        )
        .unwrap();
    let meta = ExecutionPrivateCurrentMetadataV2 {
        session_ref: token.session().session_ref.clone(),
        private_sid_generation: token.session().private_sid_generation,
        lifetime_digest: token.lifetime().lifetime.digest().unwrap(),
    };
    ready
        .f
        .state
        .complete_test_session_v2_prepare_private_current(
            &ready.f.session_id,
            token,
            meta,
            UnixMillis::new(14_008),
        )
        .unwrap()
        .unwrap()
}
fn native_authority(
    ready: &mut ReadyPrepare,
    paid: &str,
    observed: &ExecutionPrivateCurrentObservationV2,
) -> ExecutionNativePrepareAuthorityV2 {
    let f = &mut ready.f;
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    let ns = scope(&f.pin);
    let c = TestSessionLiveCurrent {
        provision: &current,
        native_scope: &ns,
        lifetime: &ready.fact,
        private_sid_generation: observed.session().private_sid_generation,
        source_ticket: 5,
        source_operation_id: observed.source_operation_id(),
        observed_at: observed.observed_at(),
    };
    let authority = f
        .state
        .allocate_test_session_v2_execution_prepare(
            &f.session_id,
            paid,
            &c,
            UnixMillis::new(14_009),
            &mut StrictCrypto,
        )
        .unwrap();
    let bundle = authority.bundle().clone();
    let (key, iss, pin, floor) = issuer(
        AccountMachinePurposeV2::ExecutionPrepareSignerV2,
        ns.http_origin,
    );
    let p = authority
        .into_service_permits(
            iss.clone(),
            DecimalU64(14_006),
            DecimalU64(14_006),
            UnixMillis::new(19_000),
        )
        .unwrap();
    let sign = |permit: AccountExecutionPreparePermitV2| SignedAccountExecutionPreparePermitV2 {
        signature_hex: encode_hex(key.sign(&permit.signing_bytes().unwrap()).to_bytes()),
        permit,
    };
    let permits = SignedExecutionPreparePermitsV2 {
        initialize: sign(p.initialize),
        sign: sign(p.sign),
    };
    assert!(
        f.state
            .record_test_session_v2_prepare_service_permits(
                &f.session_id,
                permits,
                &pin,
                &floor,
                UnixMillis::new(14_009)
            )
            .unwrap()
            .is_some()
    );
    use oasis7_client_api::world_service::{
        self as ws, entry_executor::EntryExecutorAccessV1, task_context::TASK_CONTEXT_PREPARE_PATH,
        task_context_projection::TaskContextPrepareAttestationRequestV2,
    };
    let ExecutionSigningActionV2::Prepare {
        original,
        observation_nonce,
    } = bundle.action.as_ref()
    else {
        unreachable!()
    };
    let signed = ExecutionPrepareSignedOriginalV2 {
        access: EntryExecutorAccessV1 {
            read: ws::sign_read_request(
                ws::VIEW_JSON_PATH,
                bundle.observation.clone(),
                &"41".repeat(32),
            )
            .unwrap(),
            fence: bundle.fence.clone(),
        },
        request: TaskContextPrepareAttestationRequestV2 {
            original: ws::sign_read_request(
                TASK_CONTEXT_PREPARE_PATH,
                original.as_ref().clone(),
                &"41".repeat(32),
            )
            .unwrap(),
            observation_nonce: observation_nonce.clone(),
        },
    };
    bundle.verify_prepare_original(&signed).unwrap();
    f.state
        .archive_test_session_v2_prepare_original(&f.session_id, signed, UnixMillis::new(14_010))
        .unwrap();
    f.state
        .prepare_test_session_v2_native_prepare(
            &f.session_id,
            &c,
            iss,
            UnixMillis::new(14_011),
            &mut StrictCrypto,
        )
        .unwrap()
}
#[test]
fn session_v2_native_prepare_private_two_once_and_cold_before_real_signer() {
    let (mut r, paid) = ready_new();
    let before =
        r.f.state
            .test_session_v2_snapshot(&r.f.session_id)
            .unwrap()
            .roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap();
    let observed = private(&mut r, &paid);
    let after =
        r.f.state
            .test_session_v2_snapshot(&r.f.session_id)
            .unwrap()
            .roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap();
    assert_eq!(before, after + 2);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&r.f.state).unwrap()).unwrap();
    assert_eq!(cold, r.f.state);
    let ns = scope(&r.f.pin);
    let unchanged = r.f.state.clone();
    assert!(
        r.f.state
            .claim_test_session_v2_prepare_private_current(
                &r.f.session_id,
                &paid,
                &ns,
                UnixMillis::new(14_008),
                UnixMillis::new(19_000),
                &mut StrictCrypto
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(unchanged, r.f.state);
    assert_eq!(
        observed.source_operation_id(),
        "actual-prepare-factory-source"
    );
}
#[test]
fn session_v2_native_prepare_ten_fee_exact_original_and_cold_no_reconstructed_dispatch() {
    let (mut r, paid) = ready_new();
    let observed = private(&mut r, &paid);
    let authority = native_authority(&mut r, &paid, &observed);
    let ns = scope(&r.f.pin);
    let (key, _, pin, floor) = issuer(
        AccountMachinePurposeV2::NativeExecutionPrepareV2,
        ns.http_origin,
    );
    let certificate = authority.certificate().clone();
    let proof = SignedAccountNativeExecutionPrepareV2 {
        signature_hex: encode_hex(key.sign(&certificate.signing_bytes().unwrap()).to_bytes()),
        certificate,
    };
    let before =
        r.f.state
            .test_session_v2_snapshot(&r.f.session_id)
            .unwrap()
            .roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap();
    let dispatch =
        r.f.state
            .record_test_session_v2_native_prepare(
                &r.f.session_id,
                authority,
                proof.clone(),
                &pin,
                &floor,
                UnixMillis::new(14_011),
            )
            .unwrap();
    assert!(dispatch.request().proof == proof);
    let after =
        r.f.state
            .test_session_v2_snapshot(&r.f.session_id)
            .unwrap()
            .roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap();
    assert_eq!(before, after + 10);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&r.f.state).unwrap()).unwrap();
    assert_eq!(cold, r.f.state);
    let policy = Policy::new(r.f.state.draft());
    let pc = policy.current(&r.f.context);
    let c = TestSessionLiveCurrent {
        provision: &pc,
        native_scope: &ns,
        lifetime: &r.fact,
        private_sid_generation: observed.session().private_sid_generation,
        source_ticket: 5,
        source_operation_id: observed.source_operation_id(),
        observed_at: observed.observed_at(),
    };
    let no_second = r.f.state.clone();
    assert!(
        r.f.state
            .prepare_test_session_v2_native_prepare(
                &r.f.session_id,
                &c,
                proof.certificate.issuer.clone(),
                UnixMillis::new(14_012),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(no_second, r.f.state);
    let ids: std::collections::BTreeSet<_> = dispatch
        .request()
        .proof
        .certificate
        .ports
        .iter()
        .map(|p| &p.attempt_id)
        .collect();
    assert_eq!(ids.len(), 10);
    let original = dispatch.request().original.clone();
    assert_eq!(
        proof.certificate.original_digest,
        oasis7_wasm_abi::canonical_hash(original.as_ref()).unwrap()
    );
}

#[test]
fn session_v2_native_prepare_private_failure_keeps_paid_pending_original() {
    let (mut r, paid) = ready_new();
    let ns = scope(&r.f.pin);
    let token =
        r.f.state
            .claim_test_session_v2_prepare_private_current(
                &r.f.session_id,
                &paid,
                &ns,
                UnixMillis::new(14_007),
                UnixMillis::new(19_000),
                &mut StrictCrypto,
            )
            .unwrap()
            .unwrap();
    let (key, iss, pin, floor) = issuer(
        AccountMachinePurposeV2::ExecutionPrivateCurrentV2,
        ns.http_origin,
    );
    let certificate = token.certificate(iss).unwrap();
    let proof = SignedAccountExecutionPrivateCurrentV2 {
        signature_hex: encode_hex(key.sign(&certificate.signing_bytes().unwrap()).to_bytes()),
        certificate,
    };
    let token =
        r.f.state
            .record_test_session_v2_prepare_private_proof(
                &r.f.session_id,
                token,
                proof,
                &pin,
                &floor,
                UnixMillis::new(14_007),
            )
            .unwrap();
    let mut meta = ExecutionPrivateCurrentMetadataV2 {
        session_ref: token.session().session_ref.clone(),
        private_sid_generation: token.session().private_sid_generation,
        lifetime_digest: token.lifetime().lifetime.digest().unwrap(),
    };
    meta.private_sid_generation = DecimalU64(meta.private_sid_generation.0.checked_add(1).unwrap());
    let paid_state = r.f.state.clone();
    assert!(
        r.f.state
            .complete_test_session_v2_prepare_private_current(
                &r.f.session_id,
                token,
                meta,
                UnixMillis::new(14_008)
            )
            .is_err()
    );
    assert_eq!(paid_state, r.f.state);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&paid_state).unwrap()).unwrap();
    assert_eq!(cold, paid_state);
    assert!(
        r.f.state
            .claim_test_session_v2_prepare_private_current(
                &r.f.session_id,
                &paid,
                &ns,
                UnixMillis::new(14_008),
                UnixMillis::new(19_000),
                &mut StrictCrypto
            )
            .unwrap()
            .is_none()
    );
}

fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[path = "test_session_v2_execution_native_source_collision_tests.rs"]
mod native_source_collision_tests;

#[path = "test_session_v2_execution_private_source_collision_tests.rs"]
mod private_source_collision_tests;

#[path = "test_session_v2_execution_prepare_native_wire_tests.rs"]
mod native_wire_tests;

#[path = "test_session_v2_native_dispatch_gate_tests.rs"]
mod dispatch_gate_tests;
