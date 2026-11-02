//! Genuine consumed Session + first preflight. Component keys do not prove live HTTP funding.
use super::*;

#[test]
fn session_v2_source_first_claim_requires_prepaid_oracle_before_current() {
    let mut f = pending_fixture();
    let d = f
        .state
        .claim_test_session_v2_preflight(&f.session_id, UnixMillis::new(9000))
        .unwrap();
    let p = Policy::new(f.state.draft());
    let c = p.current(&f.context);
    let before = f.state.clone();
    assert!(
        f.state
            .claim_test_session_v2_source(
                &f.session_id,
                &d.source_operation_id,
                &c,
                UnixMillis::new(9001),
            )
            .is_err(),
        "an observation without the first durable oracle reservation must not be admitted"
    );
    assert_eq!(f.state, before);
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(
                &f.session_id,
                &d.source_operation_id,
                UnixMillis::new(9002),
            )
            .unwrap(),
        1
    );
    let paid = f.state.clone();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&paid).unwrap(),
        )
        .unwrap(),
        paid
    );
    let request = f
        .state
        .claim_test_session_v2_source(
            &f.session_id,
            &d.source_operation_id,
            &c,
            UnixMillis::new(9003),
        )
        .unwrap();
    assert_eq!(request.verify_nonce, d.source_nonce);
    assert!(
        f.state
            .test_session_v2_snapshot(&f.session_id)
            .unwrap()
            .roots()
            .is_empty()
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap(),
        )
        .unwrap(),
        f.state
    );
}
