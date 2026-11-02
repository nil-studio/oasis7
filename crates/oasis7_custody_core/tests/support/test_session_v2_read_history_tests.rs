//! Genuine V2 Root/Read token/HTTP-PoP plus strong Source/LF fixture component.
//! No actual paid Worker/CF Current graph or Native archive lookup is claimed.
use super::*;
use oasis7_custody_core::provisioning::{
    ReadPrepareHistoryReplyV2, SessionSourceFundingV1, SessionSourceReservation,
};
fn ready() -> PreparedService {
    let mut p = prepared_service(None, |_| {});
    let signed = original(&p);
    record(&mut p);
    p.f.state
        .archive_test_session_v2_prepare_original(&p.f.session_id, signed, UnixMillis::new(14_009))
        .unwrap();
    p
}
use super::super::super::read_history_http_fixture::read_pop;

fn budget(f: &SessionFixture, p: SessionPurposeV1) -> (u64, u64) {
    let s = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    let r = &s.roots()[&p];
    (
        r.budgets.requests.remaining().unwrap(),
        r.budgets.transport.remaining().unwrap(),
    )
}
fn known(p: &PreparedService) -> String {
    p.f.state
        .test_session_v2_prepare_original_history(&p.f.session_id)
        .unwrap()
        .unwrap()
        .request
        .original
        .request
        .client_operation_id
}
#[test]
fn read_history_real_read_fee_is_three_once_and_opaque_not_recreated_after_cold() {
    let mut p = ready();
    let operation = known(&p);
    let (body, pop) = read_pop(&mut p.f, "read-known", &operation, 14_010);
    let ingress = "read-known-ingress";
    let before = p.f.state.clone();
    assert!(
        p.f.state
            .charge_test_session_v2_prepare_history_owner_attempt(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(14_012)
            )
            .is_err()
    );
    assert_eq!(
        p.f.state, before,
        "unpaid Host cannot yield a read continuation"
    );
    let read_before = budget(&p.f, SessionPurposeV1::ReconcileRead);
    let execute = budget(&p.f, SessionPurposeV1::Execution);
    charge(
        &mut p.f,
        &body,
        &pop,
        ingress,
        SessionPurposeV1::ReconcileRead,
        14_012,
    )
    .unwrap();
    let paid =
        p.f.state
            .charge_test_session_v2_prepare_history_owner_attempt(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop,
                },
                UnixMillis::new(14_012),
            )
            .unwrap()
            .unwrap();
    assert_eq!(
        budget(&p.f, SessionPurposeV1::ReconcileRead),
        (read_before.0 - 1, read_before.1 - 3)
    );
    assert_eq!(budget(&p.f, SessionPurposeV1::Execution), execute);
    drop(paid); // unknown after durable payment: never reissue the first opaque
    let raw = serde_json::to_value(&p.f.state).unwrap();
    let cold: AccountProvisionApprovalState = serde_json::from_value(raw.clone()).unwrap();
    p.f.state = cold;
    let before = p.f.state.clone();
    assert!(
        p.f.state
            .charge_test_session_v2_prepare_history_owner_attempt(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(14_013)
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(before, p.f.state);
    let native = p.f.pin.clone();
    let ns = scope(&native);
    assert!(matches!(
        p.f.state
            .test_session_v2_prepare_history_paid_reply(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop
                },
                &ns,
                UnixMillis::new(14_013)
            )
            .unwrap(),
        ReadPrepareHistoryReplyV2::Pending
    ));
    let mut missing = raw;
    missing["test_sessions_v2"][&p.f.session_id]["purpose_http_admissions"][ingress]["class"]["fee"].as_object_mut().unwrap().remove("outcome");
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err(),
        "missing strict outcome cannot restore as unpaid or fresh"
    );
}
#[test]
fn read_history_unknown_scope_pays_three_but_gets_no_opaque_or_execution_right() {
    let mut p = ready();
    let (body, pop) = read_pop(&mut p.f, "read-unknown", "not-a-session-original", 14_010);
    let ingress = "read-unknown-ingress";
    let before = budget(&p.f, SessionPurposeV1::ReconcileRead);
    let execute = budget(&p.f, SessionPurposeV1::Execution);
    charge(
        &mut p.f,
        &body,
        &pop,
        ingress,
        SessionPurposeV1::ReconcileRead,
        14_012,
    )
    .unwrap();
    assert!(
        p.f.state
            .charge_test_session_v2_prepare_history_owner_attempt(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(14_012)
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        budget(&p.f, SessionPurposeV1::ReconcileRead),
        (before.0 - 1, before.1 - 3)
    );
    assert_eq!(budget(&p.f, SessionPurposeV1::Execution), execute);
    let pin = p.f.pin.clone();
    let ns = scope(&pin);
    assert!(matches!(
        p.f.state
            .test_session_v2_prepare_history_paid_reply(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop
                },
                &ns,
                UnixMillis::new(14_013)
            )
            .unwrap(),
        ReadPrepareHistoryReplyV2::UnknownOriginal
    ));
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&p.f.state).unwrap()).unwrap();
    assert_eq!(cold, p.f.state);
}
#[test]
fn read_history_pending_is_exact_read_observation_with_real_source_signatures() {
    let mut p = ready();
    let operation = known(&p);
    let (body, pop) = read_pop(&mut p.f, "read-pending", &operation, 14_010);
    let ingress = "read-pending-ingress";
    charge(
        &mut p.f,
        &body,
        &pop,
        ingress,
        SessionPurposeV1::ReconcileRead,
        14_012,
    )
    .unwrap();
    let paid =
        p.f.state
            .charge_test_session_v2_prepare_history_owner_attempt(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop,
                },
                UnixMillis::new(14_012),
            )
            .unwrap()
            .unwrap();
    let source = "read-history-source";
    let nonce = oasis7_custody_core::approval::bytes_digest(source.as_bytes());
    p.f.state
        .reserve_test_session_v2_source(
            &p.f.session_id,
            SessionSourceReservation {
                operation_id: source,
                nonce: &nonce,
                funding: SessionSourceFundingV1::ReconcileRead,
            },
            UnixMillis::new(14_013),
        )
        .unwrap();
    complete_execution_source(&mut p.f, source, nonce, 14_013);
    let nonce =
        p.f.state
            .test_session_v2_lifetime_nonce(&p.f.session_id, source)
            .unwrap();
    let lf = lifetime(&p.f, nonce, 14_014);
    let context = p.f.context.clone();
    let policy = Policy::new(p.f.state.draft());
    let c = policy.current(&context);
    let native = p.f.pin.clone();
    let ns = scope(&native);
    let current = TestSessionLiveCurrent {
        provision: &c,
        native_scope: &ns,
        lifetime: &lf,
        private_sid_generation: DecimalU64(1),
        source_operation_id: source,
        source_ticket: 3,
        observed_at: UnixMillis::new(14_014),
    };
    let reply =
        p.f.state
            .finish_test_session_v2_prepare_history(
                &p.f.session_id,
                paid,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop,
                },
                &current,
                UnixMillis::new(14_014),
                &mut StrictCrypto,
            )
            .unwrap();
    assert!(
        matches!(reply, ReadPrepareHistoryReplyV2::Pending),
        "absent Account public history is not a recovered Native archive"
    );
    let snapshot = serde_json::to_value(&p.f.state).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_value(snapshot.clone()).unwrap();
    assert_eq!(restored, p.f.state);
    let fee = &snapshot["test_sessions_v2"][&p.f.session_id]["purpose_http_admissions"][ingress]["class"]
        ["fee"];
    assert!(fee["outcome"].is_object());
    assert!(fee["outcome"]["fact_digest"].is_null());
    assert!(
        p.f.state
            .test_session_v2_prepare_history_paid_reply(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop
                },
                &ns,
                UnixMillis::new(14_015)
            )
            .is_ok()
    );
}

#[test]
fn read_history_execution_funded_current_cannot_finish_read_and_fee_stays_spent() {
    let mut p = ready();
    let operation = known(&p);
    let (body, pop) = read_pop(&mut p.f, "read-wrong-source", &operation, 14_010);
    let ingress = "read-wrong-source-ingress";
    charge(
        &mut p.f,
        &body,
        &pop,
        ingress,
        SessionPurposeV1::ReconcileRead,
        14_012,
    )
    .unwrap();
    let paid =
        p.f.state
            .charge_test_session_v2_prepare_history_owner_attempt(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop,
                },
                UnixMillis::new(14_012),
            )
            .unwrap()
            .unwrap();
    let before = p.f.state.clone();
    let policy = Policy::new(p.f.state.draft());
    let context = p.f.context.clone();
    let provision = policy.current(&context);
    let pin = p.f.pin.clone();
    let ns = scope(&pin);
    let current = TestSessionLiveCurrent {
        provision: &provision,
        native_scope: &ns,
        lifetime: &p.fact,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "actual-prepare-factory-source",
        source_ticket: 5,
        observed_at: UnixMillis::new(14_012),
    };
    assert!(
        p.f.state
            .finish_test_session_v2_prepare_history(
                &p.f.session_id,
                paid,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop
                },
                &current,
                UnixMillis::new(14_012),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(
        p.f.state, before,
        "failed Current cannot add a result or refund its Read fee"
    );
    assert!(
        p.f.state
            .charge_test_session_v2_prepare_history_owner_attempt(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(14_013)
            )
            .unwrap()
            .is_none()
    );
    let mut raw = serde_json::to_value(&p.f.state).unwrap();
    raw["test_sessions_v2"][&p.f.session_id]["events"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err(),
        "paid row cannot lose its chronological fee event"
    );
}

#[path = "test_session_v2_read_current_tests.rs"]
mod current_tests;

#[path = "test_session_v2_read_fence_codec_tests.rs"]
mod read_fence_codec_tests;
