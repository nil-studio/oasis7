//! Core service fixtures: genuine Book/Source/LF/private2/RKV and real paid Control.
//! Not actual CF/Native HTTP counters or protected Native SQLite qualification.
use super::super::super::super::super::control_tests as control_fixture;
use super::*;
use oasis7_custody_core::execution_dispatch_gate_v2::*;
fn remaining(r: &ReadyPrepare) -> u64 {
    r.f.state
        .test_session_v2_snapshot(&r.f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::Execution]
        .budgets
        .transport
        .remaining()
        .unwrap()
}
fn gated() -> (
    ReadyPrepare,
    NativeExecutionPrepareRequestV2,
    ExecutionNativeGatedDispatchV2,
) {
    let (r, paid) = ready_new();
    gated_from(r, paid)
}
fn gated_from(
    mut r: ReadyPrepare,
    paid: String,
) -> (
    ReadyPrepare,
    NativeExecutionPrepareRequestV2,
    ExecutionNativeGatedDispatchV2,
) {
    let observed = private(&mut r, &paid);
    let authority = native_authority(&mut r, &paid, &observed);
    let ns = scope(&r.f.pin);
    let (key, _, pin, floor) = issuer(
        AccountMachinePurposeV2::NativeExecutionPrepareV2,
        ns.http_origin,
    );
    let certificate = authority.certificate().clone();
    let signed = SignedAccountNativeExecutionPrepareV2 {
        signature_hex: encode_hex(key.sign(&certificate.signing_bytes().unwrap()).to_bytes()),
        certificate,
    };
    let before = remaining(&r);
    let token =
        r.f.state
            .record_test_session_v2_native_prepare_gated(
                &r.f.session_id,
                authority,
                signed,
                &pin,
                &floor,
                UnixMillis::new(14_011),
            )
            .unwrap();
    assert_eq!(before, remaining(&r) + 34);
    let q = token.request().clone();
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&r.f.state).unwrap()).unwrap();
    assert_eq!(cold, r.f.state);
    (r, q, token)
}
fn staged_gated(
    kind: SessionControlKindV1,
    salt: u8,
) -> (
    ReadyPrepare,
    NativeExecutionPrepareRequestV2,
    ExecutionNativeGatedDispatchV2,
    SessionControlRequestV1,
    SessionChallengeV1,
) {
    let f = super::super::super::super::super::super::machine_tests::ready_world3(9000);
    let f = claimed_execution_from(f);
    let mut staged = None;
    let mut r = ready_with_actual_session(f, None, None, false, |f| {
        let q = SessionControlRequestV1 {
            api_version: 1,
            session_id: f.session_id.clone(),
            request_id: format!("native45-close-{salt}"),
            kind,
        };
        // The actual helper issues at +2, before execution Source 14000.
        // This nonce is live until15052; the original 14012 F/A time is kept.
        let nonce =
            control_fixture::challenge(f, &q, SessionPurposeV1::ReconcileRead, salt, 13_050);
        staged = Some((q, nonce));
    });
    let a = allocation();
    let paid =
        r.f.state
            .charge_test_session_v2_execution_prepare_owner_factory_attempt(
                &r.f.session_id,
                &a,
                SessionProofRequest {
                    ingress_attempt_id: &a.factory_ingress_id,
                    original_bytes: &r.body,
                    possession: &r.pop,
                },
                UnixMillis::new(14_006),
            )
            .unwrap();
    assert!(
        r.f.state
            .claim_test_session_v2_prepare_current_ports(
                &r.f.session_id,
                &paid,
                UnixMillis::new(14_006)
            )
            .unwrap()
            .is_some()
    );
    let (r, q, token) = gated_from(r, paid);
    let (control, nonce) = staged.unwrap();
    (r, q, token, control, nonce)
}
fn with_config<T>(
    r: &mut ReadyPrepare,
    op: impl FnOnce(&mut ReadyPrepare, &ExecutionDispatchIssueContextV2<'_>) -> T,
) -> T {
    let native = r.f.pin.clone();
    let ns = scope(&native);
    let (_, _, np, nf) = issuer(
        AccountMachinePurposeV2::NativeExecutionPrepareV2,
        ns.http_origin,
    );
    let (_, _, dp, df) = issuer(
        AccountMachinePurposeV2::ExecutionDispatchClaimV2,
        ns.http_origin,
    );
    let configured = ExecutionDispatchIssueContextV2 {
        native_scope: &ns,
        native_prepare_pin: &np,
        native_prepare_floor: &nf,
        dispatch_pin: &dp,
        dispatch_floor: &df,
    };
    op(r, &configured)
}
fn claim_first(
    r: &mut ReadyPrepare,
    dispatch: ExecutionNativeGatedDispatchV2,
) -> ExecutionNativeLeafSigningAuthorityV2 {
    with_config(r, |r, configured| {
        r.f.state
            .claim_test_session_v2_native_first_leaf(
                &r.f.session_id,
                dispatch,
                configured,
                UnixMillis::new(14_012),
                &mut StrictCrypto,
            )
            .unwrap()
    })
}
fn sign_claim(
    r: &mut ReadyPrepare,
    authority: ExecutionNativeLeafSigningAuthorityV2,
) -> SignedExecutionDispatchClaimV2 {
    let key = SigningKey::from_bytes(&[97; 32]);
    let signature = encode_hex(key.sign(&authority.signing_bytes().unwrap()).to_bytes());
    r.f.state
        .record_test_session_v2_native_leaf_signature(
            &r.f.session_id,
            authority,
            signature,
            UnixMillis::new(14_012),
        )
        .unwrap()
}
struct NoNative;
impl oasis7_custody_core::approval_admission::ApprovalSignatureVerifier for NoNative {
    fn verify(
        &mut self,
        _: &oasis7_custody_core::approval_admission::ExactSignatureChecks,
    ) -> core::result::Result<(), oasis7_custody_core::approval_admission::ApprovalAdmissionError>
    {
        panic!("Finish/Abort must not acquire Native Current")
    }
}
fn close(r: &mut ReadyPrepare, q: &SessionControlRequestV1, nonce: &SessionChallengeV1) {
    let kind = q.kind;
    let body = serde_json::to_vec(&q).unwrap();
    let route = match kind {
        SessionControlKindV1::Finish => HttpPopRoute::Finish(r.f.session_id.clone()),
        SessionControlKindV1::Abort => HttpPopRoute::Abort(r.f.session_id.clone()),
        _ => unreachable!(),
    };
    let pop = control_fixture::control_pop(
        &r.f,
        &body,
        (route, HttpPopProfileV1::Custody),
        &q.request_id,
        Some(nonce),
        SessionPurposeV1::ReconcileRead,
        14_012,
    );
    let ingress = format!("native45-close-http-{}", q.request_id);
    control_fixture::control_paid(&mut r.f, &pop, &ingress, 14_012);
    let ack =
        r.f.state
            .control_test_session_v2(
                SessionProofRequest {
                    ingress_attempt_id: &ingress,
                    original_bytes: &body,
                    possession: &pop,
                },
                None,
                UnixMillis::new(14_012),
                &mut NoNative,
            )
            .unwrap();
    assert_eq!(ack.kind, kind);
}
#[test]
fn session_v2_native45_prepaid_24_and_one_leaf_receipt_restore_exact() {
    let (mut r, q, dispatch) = gated();
    let authority = claim_first(&mut r, dispatch);
    let signed = sign_claim(&mut r, authority);
    let ns = scope(&r.f.pin);
    let (_, _, pin, floor) = issuer(
        AccountMachinePurposeV2::ExecutionDispatchClaimV2,
        ns.http_origin,
    );
    signed.verify_issuer(&q, &pin, &floor, 14_012).unwrap();
    assert_eq!(
        signed.claim.leaf.edge,
        ExecutionPreparePhysicalEdgeV2::AccountToIdentityService
    );
    assert!(signed.claim.callbacks.is_empty());
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&r.f.state).unwrap()).unwrap();
    assert_eq!(cold, r.f.state);
    let before = r.f.state.clone();
    assert!(rejected(
        &mut r,
        &q,
        ExecutionPreparePhysicalEdgeV2::AccountToIdentityService,
        oasis7_custody_core::http::Role::Owner
    ));
    assert_eq!(before, r.f.state);
}
fn rejected(
    r: &mut ReadyPrepare,
    q: &NativeExecutionPrepareRequestV2,
    edge: ExecutionPreparePhysicalEdgeV2,
    role: oasis7_custody_core::http::Role,
) -> bool {
    let body = oasis7_custody_core::approval::bytes_digest(&q.wire_bytes().unwrap());
    with_config(r, |r, configured| {
        r.f.state
            .claim_test_session_v2_native_identity_leaf(
                &r.f.session_id,
                (edge, &body),
                configured,
                role,
                UnixMillis::new(14_012),
                &mut StrictCrypto,
            )
            .is_err()
    })
}
fn closing_orders(kind: SessionControlKindV1, salt: u8) {
    let (mut before, _, dispatch, control, nonce) = staged_gated(kind, salt);
    close(&mut before, &control, &nonce);
    let ns = scope(&before.f.pin);
    let (_, _, pin, floor) = issuer(
        AccountMachinePurposeV2::ExecutionDispatchClaimV2,
        ns.http_origin,
    );
    let old = before.f.state.clone();
    assert!(with_config(&mut before, |r, configured| r
        .f
        .state
        .claim_test_session_v2_native_first_leaf(
            &r.f.session_id,
            dispatch,
            configured,
            UnixMillis::new(14_012),
            &mut StrictCrypto
        )
        .is_err()));
    assert_eq!(old, before.f.state);
    let (mut after, q, dispatch, control, nonce) = staged_gated(kind, salt + 1);
    let authority = claim_first(&mut after, dispatch);
    close(&mut after, &control, &nonce);
    let signed = sign_claim(&mut after, authority);
    signed.verify_issuer(&q, &pin, &floor, 14_012).unwrap();
    let old = after.f.state.clone();
    assert!(rejected(
        &mut after,
        &q,
        ExecutionPreparePhysicalEdgeV2::IdentityServiceToNativeActor,
        oasis7_custody_core::http::Role::Identity
    ));
    assert_eq!(old, after.f.state);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&after.f.state).unwrap()).unwrap();
    assert_eq!(cold, after.f.state);
}
#[test]
fn session_v2_native45_finish_before_and_after_claim_causal_even_same_ms() {
    closing_orders(SessionControlKindV1::Finish, 101);
}
#[test]
fn session_v2_native45_abort_before_and_after_claim_causal_even_same_ms() {
    closing_orders(SessionControlKindV1::Abort, 103);
}

#[path = "test_session_v2_native_dispatch_cold_start_tests.rs"]
mod cold_start_tests;

#[path = "test_session_v2_native_dispatch_last_paid_tests.rs"]
mod last_paid_tests;

#[path = "test_session_v2_native_dispatch_caller_tests.rs"]
mod caller_tests;
