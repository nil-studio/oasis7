use super::*;
fn ready() -> SessionFixture {
    let mut f = pending_fixture();
    let d = f
        .state
        .claim_test_session_preflight("session-one", UnixMillis::new(8005))
        .unwrap();
    source_current(&mut f, &d, 8005);
    for _ in 0..2 {
        f.state
            .reserve_test_session_preflight_read(
                "session-one",
                OwnerSetupReadPurposeV1::Session,
                Role::Identity,
                UnixMillis::new(8008),
            )
            .unwrap();
    }
    f
}
fn leave(f: &mut SessionFixture, remaining: u64) {
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    let available = f.state.test_sessions()["session-one"]
        .identity_transport
        .as_ref()
        .unwrap()
        .remaining()
        .unwrap();
    for i in 0..available - remaining {
        f.state
            .charge_test_session_internal_control_transport(
                "session-one",
                &format!("prior-native-unknown-{i}"),
                &current,
                UnixMillis::new(8009),
            )
            .unwrap();
    }
}
#[test]
fn test_session_fresh_access_reserves_four_actual_native_http_and_three_balance_rejects() {
    let mut f = ready();
    leave(&mut f, 3);
    let before = f.state.clone();
    assert!(
        f.state
            .claim_test_session_preflight_step(
                "session-one",
                SessionPreflightStepV1::FreshAccessPair,
                UnixMillis::new(8010)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    assert!(
        !f.state.test_sessions()["session-one"]
            .preflight_steps
            .contains_key(&SessionPreflightStepV1::FreshAccessPair)
    );
}
#[test]
fn test_session_fresh_access_four_balance_is_once_nonrefundable_and_survives_restart() {
    let mut f = ready();
    leave(&mut f, 4);
    f.state
        .claim_test_session_preflight_step(
            "session-one",
            SessionPreflightStepV1::FreshAccessPair,
            UnixMillis::new(8010),
        )
        .unwrap();
    assert_eq!(
        f.state.test_sessions()["session-one"]
            .identity_transport
            .as_ref()
            .unwrap()
            .remaining()
            .unwrap(),
        0
    );
    assert!(
        f.state.test_sessions()["session-one"]
            .preflight_access
            .is_none()
    );
    let mut restored: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
    assert_eq!(restored, f.state);
    let before = restored.clone();
    assert!(
        restored
            .claim_test_session_preflight_step(
                "session-one",
                SessionPreflightStepV1::FreshAccessPair,
                UnixMillis::new(8011)
            )
            .is_err()
    );
    assert_eq!(restored, before);
    assert_eq!(
        restored.test_sessions()["session-one"]
            .identity_transport
            .as_ref()
            .unwrap()
            .remaining()
            .unwrap(),
        0
    );
    assert!(restored.test_sessions()["session-one"].roots.is_empty());
}
