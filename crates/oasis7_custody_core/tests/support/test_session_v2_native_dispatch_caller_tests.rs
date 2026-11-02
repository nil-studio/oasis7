//! Actual Core service transition fixtures; these do not stand for CF/Native HTTP.
use super::*;
#[test]
fn session_v2_native45_service_receipt_chain_is_exact_and_cold_complete() {
    let (mut r, q, dispatch) = gated();
    let first = claim_first(&mut r, dispatch);
    let first = sign_claim(&mut r, first);
    let body = oasis7_custody_core::approval::bytes_digest(&q.wire_bytes().unwrap());
    let next = with_config(&mut r, |r, cfg| {
        r.f.state
            .claim_test_session_v2_native_identity_receipted_leaf(
                &r.f.session_id,
                (&q, &first),
                (
                    ExecutionPreparePhysicalEdgeV2::IdentityServiceToNativeActor,
                    &body,
                ),
                cfg,
                UnixMillis::new(14_012),
                &mut StrictCrypto,
            )
            .unwrap()
    });
    let next = sign_claim(&mut r, next);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&r.f.state).unwrap()).unwrap();
    assert_eq!(cold, r.f.state);
    let before = r.f.state.clone();
    assert!(
        with_config(&mut r, |r, cfg| r
            .f
            .state
            .claim_test_session_v2_native_identity_receipted_leaf(
                &r.f.session_id,
                (&q, &first),
                (ExecutionPreparePhysicalEdgeV2::SidCipherReadBefore, &body),
                cfg,
                UnixMillis::new(14_012),
                &mut StrictCrypto,
            )
            .is_err()),
        "an older valid receipt cannot authorize the next physical edge"
    );
    assert_eq!(before, r.f.state);
    let remaining_before = remaining(&r);
    let authority = with_config(&mut r, |r, cfg| {
        r.f.state
            .claim_test_session_v2_native_identity_receipted_leaf(
                &r.f.session_id,
                (&q, &next),
                (ExecutionPreparePhysicalEdgeV2::SidCipherReadBefore, &body),
                cfg,
                UnixMillis::new(14_012),
                &mut StrictCrypto,
            )
            .unwrap()
    });
    let signed = sign_claim(&mut r, authority);
    assert_eq!(
        signed.claim.leaf.edge,
        ExecutionPreparePhysicalEdgeV2::SidCipherReadBefore
    );
    assert_eq!(
        remaining(&r),
        remaining_before,
        "per-leaf claims consume the original prepaid leaves, never a second fee"
    );
}
#[test]
fn session_v2_native45_service_rejects_changed_receipt_before_next_claim() {
    let (mut r, q, dispatch) = gated();
    let first = claim_first(&mut r, dispatch);
    let mut first = sign_claim(&mut r, first);
    first.signature_hex = "00".repeat(64);
    let body = oasis7_custody_core::approval::bytes_digest(&q.wire_bytes().unwrap());
    let before = r.f.state.clone();
    assert!(with_config(&mut r, |r, cfg| r
        .f
        .state
        .claim_test_session_v2_native_identity_receipted_leaf(
            &r.f.session_id,
            (&q, &first),
            (
                ExecutionPreparePhysicalEdgeV2::IdentityServiceToNativeActor,
                &body
            ),
            cfg,
            UnixMillis::new(14_012),
            &mut StrictCrypto,
        )
        .is_err()));
    assert_eq!(before, r.f.state);
}
