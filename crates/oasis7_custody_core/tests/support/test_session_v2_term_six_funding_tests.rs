//! Component payer/once only: no Current, signature, Actor reply or physical send.
use super::*;
use oasis7_custody_core::provisioning::{
    TermObservationPortV2 as Port, TermObservationRoundV2 as Round,
};
#[test]
fn term_six_real_finish_pays_two_fixed_rounds_and_claims_each_port_once() {
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

    f.state
        .claim_test_session_term_owner_slot_read_v2(
            &owned,
            "term-six-qualification",
            UnixMillis::new(13_006),
        )
        .unwrap();
    let spent = |state: &AccountProvisionApprovalState| {
        serde_json::to_value(state).unwrap()["test_sessions_v2"][&f.session_id]["roots"]["termination_plan"]["root"]["budgets"]["transport"]["spent"].as_str().unwrap().parse::<u64>().unwrap()
    };
    assert_eq!(spent(&f.state), 1);
    let first = f
        .state
        .reserve_test_session_term_six_v2(&owned, Round::First, UnixMillis::new(13_007))
        .unwrap();
    assert_eq!(spent(&f.state), 7);
    let exact = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_term_six_v2(&owned, Round::First, UnixMillis::new(13_008))
            .is_err()
    );
    assert!(
        f.state
            .reserve_test_session_term_six_v2(&owned, Round::Fresh, UnixMillis::new(13_008))
            .is_err()
    );
    assert_eq!(f.state, exact);
    let ports = [
        Port::Directory,
        Port::RfService,
        Port::RfActor,
        Port::IdentityService,
        Port::NativeSessionsActor,
        Port::Native,
    ];
    for (n, port) in ports.into_iter().enumerate() {
        let token = f
            .state
            .claim_test_session_term_six_port_v2(&first, port, UnixMillis::new(13_010 + n as u64))
            .unwrap();
        assert_eq!(token.kind(), port);
        assert_eq!(token.paid_index(), 1 + n as u32);
        assert_eq!(token.until(), first.until());
        if port == Port::Native {
            assert_eq!(
                token.native_request(&f.state).unwrap().session_id,
                f.session_id
            );
        }
        let exact = f.state.clone();
        assert!(
            f.state
                .claim_test_session_term_six_port_v2(&first, port, UnixMillis::new(13_020))
                .is_err()
        );
        assert_eq!(f.state, exact);
        assert_eq!(
            serde_json::from_value::<AccountProvisionApprovalState>(
                serde_json::to_value(&f.state).unwrap()
            )
            .unwrap(),
            f.state
        );
    }
    assert_eq!(spent(&f.state), 7);
    // The original qualification first token is not renewed or recreated here.
    // This is reservation/once metadata only, with no later permission implied.
    let fresh = f
        .state
        .reserve_test_session_term_six_v2(&owned, Round::Fresh, UnixMillis::new(18_008))
        .unwrap();
    assert_eq!(spent(&f.state), 13);
    assert!(fresh.started_at().get() > first.until().get());
    let exact = f.state.clone();
    assert!(
        f.state
            .claim_test_session_term_six_port_v2(&first, Port::Native, UnixMillis::new(18_009))
            .is_err()
    );
    assert!(
        f.state
            .reserve_test_session_term_six_v2(&owned, Round::Fresh, UnixMillis::new(18_009))
            .is_err()
    );
    assert_eq!(f.state, exact);
    let raw = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw.clone()).unwrap(),
        f.state
    );
    for path in ["input_digest", "invocation_id", "source_id", "nonce"] {
        let mut bad = raw.clone();
        bad["test_sessions_v2"][&f.session_id]["termination_state"]["term_six_funding"][0]["header"]
            [path] = serde_json::Value::String("wrong".into());
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
            "{path}"
        );
    }
}
