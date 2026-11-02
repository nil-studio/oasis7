use super::*;
#[test]
fn session_v2_control_last_logical_request_is_the_actual_budget_closing_cause() {
    // Three is actually approved in the new full V2 MFA/Book, never changed
    // after acceptance: preflight1, genuine joint credential claim1, final1.
    let mut f = activation_tests::activated_with_control_requests(9000, 3);
    let mut request = request(&f, "small-budget-real-claim", None);
    request.purposes = vec![TestSessionPurposeV1::Execute, TestSessionPurposeV1::Read];
    let mut read = allocation(32);
    read.purpose = SessionPurposeV1::ReconcileRead;
    assert!(
        claim(&mut f, &request, &[allocation(31), read], 41, 12_000)
            .unwrap()
            .newly_claimed
    );
    let q = q(
        &f,
        "small-budget-last-logical",
        SessionControlKindV1::Initialize,
    );
    challenge(&mut f, &q, SessionPurposeV1::Execution, 170, 13_000);
    let raw = serde_json::to_value(&f.state).unwrap();
    let session = &raw["test_sessions_v2"][&f.session_id];
    assert_eq!(session["control_logicals"].as_object().unwrap().len(), 3);
    assert_eq!(session["closing"][0], serde_json::json!("budget_exhausted"));
    assert_eq!(
        session["closing"][1],
        session["challenges"]["ctrl-challenge-170"]["created_at"]
    );
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw).is_ok(),
        "the last legal issue may cause same-ms BudgetExhausted after its own admission"
    );
}
