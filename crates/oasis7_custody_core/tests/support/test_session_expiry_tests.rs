use super::*;
#[test]
fn test_session_local_expiry_schedule_is_phase_exact_pure_and_never_rearms_closing() {
    let mut pending = session_fixture(false);
    let before = pending.state.clone();
    let proof_expiry = pending.state.test_sessions()["session-one"]
        .input
        .approval
        .proof
        .expires_at
        .0;
    assert_eq!(
        pending
            .state
            .next_test_session_expiry_at(UnixMillis::new(9000))
            .unwrap(),
        Some(UnixMillis::new(proof_expiry))
    );
    assert_eq!(
        pending
            .state
            .next_test_session_expiry_at(UnixMillis::new(proof_expiry + 1))
            .unwrap(),
        Some(UnixMillis::new(proof_expiry)),
        "caller must expire, not skip the already-due event"
    );
    assert_eq!(pending.state, before);
    pending
        .state
        .expire_test_sessions(UnixMillis::new(proof_expiry))
        .unwrap();
    assert_eq!(
        pending
            .state
            .next_test_session_expiry_at(UnixMillis::new(proof_expiry))
            .unwrap(),
        None
    );
    let mut consumed = pending_fixture();
    assert_eq!(
        consumed
            .state
            .next_test_session_expiry_at(UnixMillis::new(proof_expiry + 1))
            .unwrap(),
        Some(UnixMillis::new(50_000)),
        "consumed MFA TTL must not kill preflight before signed write cutoff"
    );
    consumed
        .state
        .expire_test_sessions(UnixMillis::new(50_000))
        .unwrap();
    assert_eq!(
        consumed
            .state
            .next_test_session_expiry_at(UnixMillis::new(50_000))
            .unwrap(),
        None
    );
    let mut active = activation::activated(8005);
    let before = active.state.clone();
    assert!(
        active
            .state
            .expire_test_sessions(UnixMillis::new(9000))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        active
            .state
            .next_test_session_expiry_at(UnixMillis::new(9000))
            .unwrap(),
        Some(UnixMillis::new(50_000))
    );
    assert_eq!(
        active.state, before,
        "early wake may not renew lease, spend budgets or close"
    );
    active
        .state
        .expire_test_sessions(UnixMillis::new(50_000))
        .unwrap();
    assert!(
        active.state.test_sessions()["session-one"]
            .closing
            .is_some()
    );
    assert_eq!(
        active
            .state
            .next_test_session_expiry_at(UnixMillis::new(50_000))
            .unwrap(),
        None,
        "disabled cleanup dispatcher must not invent a future cleanup wake"
    );
    assert_eq!(
        active
            .state
            .next_test_session_expiry_at(UnixMillis::new(1_000_000))
            .unwrap(),
        None,
        "past cleanup cutoff is pending, never a busy timer"
    );
}
