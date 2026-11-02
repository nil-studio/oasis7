//! Genuine Core full approval/three World receipts/Ready/HumanFinish prefix.
//! Service keys are fixtures; no actual Actor, Native IO or Current is claimed.
use super::*;
use oasis7_custody_core::provisioning::{
    PreparedTermFiveFundingV3, TermObservationPortV3 as Port, TermObservationRoundV3 as Round,
    TermOwnedInvocationV2,
};

fn finish(mut f: SessionFixture) -> (SessionFixture, TermOwnedInvocationV2) {
    let mut q = request(&f, "v3-term-initial-claim", None);
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
        request_id: "v3-term-human-finish".into(),
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
    let ingress = "v3-term-human-finish-http";
    control_tests::control_paid(&mut f, &pop, ingress, 13_003);
    let (ack, owned) = f
        .state
        .control_test_session_v2_and_begin_owned_term(
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &bytes,
                possession: &pop,
            },
            UnixMillis::new(13_005),
            &mut NoNativeChecks,
        )
        .unwrap();
    assert_eq!(ack.kind, SessionControlKindV1::Finish);
    (
        f,
        owned.expect("only actual first HumanFinish in Ready owns this continuation"),
    )
}
fn spent(f: &SessionFixture) -> u64 {
    serde_json::to_value(&f.state).unwrap()["test_sessions_v2"][&f.session_id]
        ["roots"]["termination_plan"]["root"]["budgets"]["transport"]["spent"]
        .as_str().unwrap().parse().unwrap()
}
fn ports(f: &mut SessionFixture, round: &PreparedTermFiveFundingV3, at: u64, paid: u32) {
    for (n, port) in [
        Port::DirectoryActor,
        Port::RfDispatcherActor,
        Port::FencesActor,
        Port::NativeSessionsActor,
        Port::NativeHttp,
    ]
    .into_iter()
    .enumerate()
    {
        let token = f
            .state
            .claim_test_session_term_five_port_v3(round, port, UnixMillis::new(at + n as u64))
            .unwrap();
        assert_eq!(token.kind(), port);
        assert_eq!(token.paid_index(), paid + n as u32);
        assert_eq!(token.until(), round.until());
        let exact = f.state.clone();
        assert!(
            f.state
                .claim_test_session_term_five_port_v3(round, port, UnixMillis::new(at + n as u64))
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
}

#[test]
fn term_v3_real_human_finish_pays_five_twice_without_qualification_and_claims_once() {
    let (mut f, owned) = finish(super::super::machine_tests::ready_v3(9000));
    assert_eq!(spent(&f), 0);
    let exact = f.state.clone();
    assert!(
        f.state
            .claim_test_session_term_owner_slot_read_v2(
                &owned,
                "must-not-add-qualification",
                UnixMillis::new(13_006)
            )
            .is_err()
    );
    assert!(
        f.state
            .reserve_test_session_term_six_v2(
                &owned,
                oasis7_custody_core::provisioning::TermObservationRoundV2::First,
                UnixMillis::new(13_006)
            )
            .is_err()
    );
    assert_eq!(f.state, exact);
    let first = f
        .state
        .reserve_test_session_term_five_v3(&owned, Round::First, UnixMillis::new(13_007))
        .unwrap();
    assert_eq!(spent(&f), 5);
    let exact = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_term_five_v3(&owned, Round::First, UnixMillis::new(13_008))
            .is_err()
    );
    assert!(
        f.state
            .reserve_test_session_term_five_v3(&owned, Round::Fresh, UnixMillis::new(13_008))
            .is_err()
    );
    assert_eq!(f.state, exact);
    ports(&mut f, &first, 13_010, 0);
    // Ownership survives the first fact window; this does not renew Current.
    let fresh = f
        .state
        .reserve_test_session_term_five_v3(&owned, Round::Fresh, UnixMillis::new(19_000))
        .unwrap();
    assert_eq!(spent(&f), 10);
    assert_eq!(first.until(), UnixMillis::new(18_007));
    assert_eq!(fresh.until(), UnixMillis::new(24_000));
    ports(&mut f, &fresh, 19_001, 5);
    let exact = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_term_five_v3(&owned, Round::Fresh, UnixMillis::new(19_010))
            .is_err()
    );
    assert_eq!(f.state, exact);
    let raw = serde_json::to_value(&f.state).unwrap();
    assert!(
        raw["test_sessions_v2"][&f.session_id]["termination_state"]["qualification_read"].is_null()
    );
    assert!(
        raw["test_sessions_v2"][&f.session_id]["termination_state"]
            .get("term_six_funding")
            .is_none()
    );
}

#[test]
fn term_v3_cold_five_ledger_rejects_missing_slot_old_ports_holes_and_expiry_changes() {
    let (mut f, owned) = finish(super::super::machine_tests::ready_v3(9000));
    let first = f
        .state
        .reserve_test_session_term_five_v3(&owned, Round::First, UnixMillis::new(13_007))
        .unwrap();
    ports(&mut f, &first, 13_010, 0);
    let raw = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw.clone()).unwrap(),
        f.state
    );
    for mode in 0..5 {
        let mut bad = raw.clone();
        let term = &mut bad["test_sessions_v2"][&f.session_id]["termination_state"];
        match mode {
            0 => {
                term.as_object_mut().unwrap().remove("term_five_funding");
            }
            1 => {
                term["term_five_funding"][0]["header"]["ports"][3]["kind"] =
                    "identity_service".into()
            }
            2 => term["term_five_funding"][0]["header"]["ports"][0]["paid_index"] = 7.into(),
            3 => term["term_five_funding"][0]["header"]["until"] = "18008".into(),
            4 => term["term_five_funding"][0]["claims"]["native_http"]["ordinal"] = 1.into(),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
            "cold variant {mode}"
        );
    }
    let (mut old, old_owned) = finish(super::super::machine_tests::ready_world3(9000));
    let exact = old.state.clone();
    assert!(
        old.state
            .reserve_test_session_term_five_v3(&old_owned, Round::First, UnixMillis::new(13_007))
            .is_err()
    );
    assert_eq!(old.state, exact);
    let mut old_raw = serde_json::to_value(&old.state).unwrap();
    assert!(
        old_raw["test_sessions_v2"][&old.session_id]["termination_state"]
            .get("term_five_funding")
            .is_none()
    );
    old_raw["test_sessions_v2"][&old.session_id]["termination_state"]["term_five_funding"] =
        serde_json::json!([]);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(old_raw).is_err());
}
