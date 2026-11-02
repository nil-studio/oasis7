//! Last exact graph fees use actual authenticated HTTP fee mutators, not ledger edits.
use super::*;
struct PendingGate {
    ready: ReadyPrepare,
    authority: ExecutionNativePrepareAuthorityV2,
    proof: SignedAccountNativeExecutionPrepareV2,
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
}
fn pending(target: u64) -> PendingGate {
    let (mut ready, paid) = ready_new();
    let observed = private(&mut ready, &paid);
    // This real call completes fixed RKV issuance/archive. Its pure Native
    // candidate is dropped before fee changes, then rebuilt from true Current.
    drop(native_authority(&mut ready, &paid, &observed));
    let n = remaining(&ready).checked_sub(target).unwrap();
    for i in 0..n {
        let ingress = format!("native45-threshold-http-{i}");
        ready
            .f
            .state
            .charge_test_session_v2_purpose_http_attempt(
                &ready.f.session_id,
                SessionPurposeV1::Execution,
                SessionProofRequest {
                    ingress_attempt_id: &ingress,
                    original_bytes: &ready.body,
                    possession: &ready.pop,
                },
                UnixMillis::new(14_011),
            )
            .unwrap();
    }
    assert_eq!(remaining(&ready), target);
    let policy = Policy::new(ready.f.state.draft());
    let provision = policy.current(&ready.f.context);
    let ns = scope(&ready.f.pin);
    let (key, issuer, pin, floor) = issuer(
        AccountMachinePurposeV2::NativeExecutionPrepareV2,
        ns.http_origin,
    );
    let current = TestSessionLiveCurrent {
        provision: &provision,
        native_scope: &ns,
        lifetime: &ready.fact,
        private_sid_generation: observed.session().private_sid_generation,
        source_ticket: 5,
        source_operation_id: observed.source_operation_id(),
        observed_at: observed.observed_at(),
    };
    let authority = ready
        .f
        .state
        .prepare_test_session_v2_native_prepare(
            &ready.f.session_id,
            &current,
            issuer,
            UnixMillis::new(14_011),
            &mut StrictCrypto,
        )
        .unwrap();
    let certificate = authority.certificate().clone();
    let proof = SignedAccountNativeExecutionPrepareV2 {
        signature_hex: encode_hex(key.sign(&certificate.signing_bytes().unwrap()).to_bytes()),
        certificate,
    };
    PendingGate {
        ready,
        authority,
        proof,
        pin,
        floor,
    }
}
#[test]
fn session_v2_native45_last_34_prepaid_still_allows_only_original_first_handle() {
    let PendingGate {
        mut ready,
        authority,
        proof,
        pin,
        floor,
    } = pending(34);
    let dispatch = ready
        .f
        .state
        .record_test_session_v2_native_prepare_gated(
            &ready.f.session_id,
            authority,
            proof,
            &pin,
            &floor,
            UnixMillis::new(14_011),
        )
        .unwrap();
    assert_eq!(remaining(&ready), 0);
    let authority = claim_first(&mut ready, dispatch);
    let signed = sign_claim(&mut ready, authority);
    assert_eq!(
        signed.claim.leaf.edge,
        ExecutionPreparePhysicalEdgeV2::AccountToIdentityService
    );
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&ready.f.state).unwrap()).unwrap();
    assert_eq!(cold, ready.f.state);
}
#[test]
fn session_v2_native45_only_33_is_atomic_before_any_native_or_callback_dispatch() {
    let PendingGate {
        mut ready,
        authority,
        proof,
        pin,
        floor,
    } = pending(33);
    let before = ready.f.state.clone();
    assert!(
        ready
            .f
            .state
            .record_test_session_v2_native_prepare_gated(
                &ready.f.session_id,
                authority,
                proof,
                &pin,
                &floor,
                UnixMillis::new(14_011)
            )
            .is_err()
    );
    assert_eq!(before, ready.f.state);
    assert_eq!(remaining(&ready), 33);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&ready.f.state).unwrap()).unwrap();
    assert_eq!(cold, ready.f.state);
}
