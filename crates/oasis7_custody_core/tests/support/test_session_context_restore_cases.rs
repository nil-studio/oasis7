use super::*;
use oasis7_custody_core::budget::BudgetLedger;

pub(super) fn assert_context_ledger_restore(
    state: &AccountProvisionApprovalState,
    before_requests: &BudgetLedger,
) {
    let wire = serde_json::to_vec(state).unwrap();
    assert!(wire.len() < 2 * 1024 * 1024);
    let original_state = serde_json::to_value(state).unwrap();
    let mut missing = original_state.clone();
    missing["test_sessions"][ID]
        .as_object_mut()
        .unwrap()
        .remove("context");
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err(),
        "stripped context leaves its request and port costs unaccounted"
    );
    assert!(
        state.test_sessions()[ID].actions.is_empty(),
        "the saved pre-context request ledger contains no action charges"
    );
    let mut balanced = original_state.clone();
    balanced["test_sessions"][ID]["context"] = serde_json::Value::Null;
    balanced["test_sessions"][ID]["roots"]["execution"]["budgets"]["requests"] =
        serde_json::to_value(before_requests).unwrap();
    assert_eq!(
        balanced["test_sessions"][ID]["purpose_transports"]
            .as_object()
            .unwrap()
            .len(),
        6
    );
    let mut allowed_changes_only = balanced.clone();
    allowed_changes_only["test_sessions"][ID]["context"] =
        original_state["test_sessions"][ID]["context"].clone();
    allowed_changes_only["test_sessions"][ID]["roots"]["execution"]["budgets"]["requests"] =
        original_state["test_sessions"][ID]["roots"]["execution"]["budgets"]["requests"].clone();
    assert!(
        allowed_changes_only == original_state,
        "only context and logical request math changed"
    );
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(balanced).is_err(),
        "balanced request math must not turn six ContextBody port charges into unrelated generic transports"
    );
    let keys = original_state["test_sessions"][ID]["purpose_transports"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for key in keys {
        assert_eq!(
            original_state["test_sessions"][ID]["purpose_transports"][&key]["kind"],
            "context_body"
        );
        for case in 0..5 {
            let mut bad = original_state.clone();
            let marker = &mut bad["test_sessions"][ID]["purpose_transports"][&key];
            match case {
                0 => {
                    marker.as_object_mut().unwrap().remove("kind");
                }
                1 => {
                    *marker = serde_json::Value::String("execution".into());
                }
                2 => {
                    marker["reference_digest"] = "ab".repeat(32).into();
                }
                3 => {
                    marker["expected_scope"]["session_id"] = "foreign-session".into();
                }
                _ => {
                    marker.as_object_mut().unwrap().remove("reference_digest");
                }
            }
            assert!(
                serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
                "closed ContextBody charge restore case {case}"
            );
        }
    }
}
#[test]
fn context_body_legacy_generic_transport_wire_and_restore_remain_unchanged() {
    let (mut f, _, _) = claimed();
    f.state
        .reserve_test_session_purpose_transport(
            ID,
            SessionPurposeV1::Execution,
            "legacy-generic-execution",
            UnixMillis::new(12_000),
        )
        .unwrap();
    f.state
        .reserve_test_session_purpose_transport(
            ID,
            SessionPurposeV1::ReconcileRead,
            "legacy-generic-read",
            UnixMillis::new(12_000),
        )
        .unwrap();
    assert!(f.state.test_sessions()[ID].context.is_none());
    let mut wire = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        wire["test_sessions"][ID]["purpose_transports"]["legacy-generic-execution"],
        "execution"
    );
    assert_eq!(
        wire["test_sessions"][ID]["purpose_transports"]["legacy-generic-read"],
        "reconcile_read"
    );
    wire["test_sessions"][ID]
        .as_object_mut()
        .unwrap()
        .remove("context");
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(wire).is_ok(),
        "legacy absent context carries no new ContextBody authority"
    );
    restore(&f.state);
}
