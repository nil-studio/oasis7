//! Purpose HTTP is never paid from Control240 or Control Native160.
use super::*;
#[test]
fn session_v2_metadata_and_actions_preserve_all_control_and_native_balances() {
    let mut f = claimed_execution();
    let before = serde_json::to_value(&f.state).unwrap();
    let initial = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::Execution]
        .budgets
        .transport
        .remaining()
        .unwrap();
    let (body, proof) = action_pop(&mut f, "independent-budget-action", 13_000, false);
    charge(
        &mut f,
        &body,
        &proof,
        "independent-budget-ingress",
        SessionPurposeV1::Execution,
        13_002,
    )
    .unwrap();
    let after = serde_json::to_value(&f.state).unwrap();
    for field in [
        "control",
        "identity_transport",
        "connector_transport",
        "control_transports",
        "client_http_admissions",
    ] {
        assert_eq!(
            after["test_sessions_v2"][&f.session_id][field],
            before["test_sessions_v2"][&f.session_id][field]
        );
    }
    assert_eq!(
        f.state
            .test_session_v2_snapshot(&f.session_id)
            .unwrap()
            .roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap(),
        initial - 4
    );
    assert!(after["test_sessions_v2"][&f.session_id]["execution_book"].is_null());
}
