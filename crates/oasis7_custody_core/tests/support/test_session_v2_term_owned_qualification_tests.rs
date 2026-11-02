//! Actual World3 Ready + real E/R claim and strong Control H PoP.
//! This is the first Account/installation-read component, not an Actor response
//! or Source6/global Current/Term24 qualification.
use super::*;
struct NoNativeChecks;
impl oasis7_custody_core::approval_admission::ApprovalSignatureVerifier for NoNativeChecks {
    fn verify(
        &mut self,
        _: &oasis7_custody_core::approval_admission::ExactSignatureChecks,
    ) -> std::result::Result<(), oasis7_custody_core::approval_admission::ApprovalAdmissionError>
    {
        panic!("pure closing/read must not obtain free Native observation")
    }
}
#[test]
fn first_actual_closing_control_owns_one_paid_installation_read_and_history_cannot_recreate_it() {
    let mut f = super::super::machine_tests::ready_world3(9000);
    let mut q = request(&f, "term-owned-initial-claim", None);
    q.purposes = vec![TestSessionPurposeV1::Execute, TestSessionPurposeV1::Read];
    let mut read = allocation(32);
    read.purpose = SessionPurposeV1::ReconcileRead;
    assert!(
        claim(&mut f, &q, &[allocation(31), read], 41, 12_000)
            .unwrap()
            .newly_claimed
    );
    let q = SessionControlRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "term-owned-finish".into(),
        kind: SessionControlKindV1::Finish,
    };
    let nonce = control_tests::challenge(&mut f, &q, SessionPurposeV1::Execution, 71, 13_000);
    let bytes = serde_json::to_vec(&q).unwrap();
    let pop = control_tests::control_pop(
        &f,
        &bytes,
        (
            HttpPopRoute::Finish(f.session_id.clone()),
            HttpPopProfileV1::Custody,
        ),
        &q.request_id,
        Some(&nonce),
        SessionPurposeV1::Execution,
        13_003,
    );
    let ingress = "term-owned-finish-http";
    control_tests::control_paid(&mut f, &pop, ingress, 13_003);
    let proof = || SessionProofRequest {
        ingress_attempt_id: ingress,
        original_bytes: &bytes,
        possession: &pop,
    };
    let (ack, owned) = f
        .state
        .control_test_session_v2_and_begin_owned_term(
            proof(),
            UnixMillis::new(13_005),
            &mut NoNativeChecks,
        )
        .unwrap();
    assert_eq!(ack.kind, SessionControlKindV1::Finish);
    let owned = owned.expect("real first closing Control in Ready session owns local continuation");
    let immutable = f.state.clone();
    assert!(
        f.state
            .claim_test_session_term_owner_slot_read_v2(
                &owned,
                "before-actual-control-ack",
                UnixMillis::new(13_004)
            )
            .is_err()
    );
    assert_eq!(f.state, immutable);
    let before = serde_json::to_value(&f.state).unwrap();
    let first = f
        .state
        .claim_test_session_term_owner_slot_read_v2(
            &owned,
            "term-paid-qualification",
            UnixMillis::new(13_006),
        )
        .unwrap();
    assert_eq!(first.session_id(), f.session_id);
    assert_eq!(first.attempt_id(), "term-paid-qualification");
    assert_eq!(
        first.lineage().custody_account_id,
        f.state.draft().entry.custody_account_id
    );
    let after = serde_json::to_value(&f.state).unwrap();
    let term = |v: &serde_json::Value| {
        v["test_sessions_v2"][&f.session_id]["roots"]["termination_plan"]["root"]["budgets"]["transport"]["spent"].as_str().unwrap().parse::<u64>().unwrap()
    };
    assert_eq!(term(&after), term(&before) + 1);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(after.clone()).unwrap(),
        f.state
    );
    let exact = f.state.clone();
    assert!(
        f.state
            .claim_test_session_term_owner_slot_read_v2(
                &owned,
                "another-paid-qualification",
                UnixMillis::new(13_007)
            )
            .is_err()
    );
    assert_eq!(f.state, exact);
    let mut cold: AccountProvisionApprovalState = serde_json::from_value(after).unwrap();
    let (again, recreated) = cold
        .control_test_session_v2_and_begin_owned_term(
            proof(),
            UnixMillis::new(13_008),
            &mut NoNativeChecks,
        )
        .unwrap();
    assert_eq!(again, ack);
    assert!(recreated.is_none());
    assert_eq!(cold, exact);
}
