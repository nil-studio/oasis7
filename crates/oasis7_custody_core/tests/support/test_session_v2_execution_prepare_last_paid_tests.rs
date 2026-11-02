//! Genuine final transport fees and real Control interleaving, no scalar edits.
use super::super::super::super::super::control_tests as control_fixture;
use super::*;
fn record(
    p: &mut PreparedService,
) -> core::result::Result<Option<ExecutionPrepareDispatchV2>, ProvisionApprovalError> {
    p.f.state.record_test_session_v2_prepare_service_permits(
        &p.f.session_id,
        p.proofs.clone(),
        &p.pin,
        &p.floor,
        UnixMillis::new(14_007),
    )
}
fn check(
    p: &PreparedService,
    ordinal: u64,
    at: u64,
) -> core::result::Result<(), ProvisionApprovalError> {
    let policy = Policy::new(p.f.state.draft());
    let c = policy.current(&p.f.context);
    let ns = scope(&p.f.pin);
    let current = TestSessionLiveCurrent {
        provision: &c,
        native_scope: &ns,
        lifetime: &p.fact,
        private_sid_generation: DecimalU64(1),
        source_ticket: 5,
        source_operation_id: "actual-prepare-factory-source",
        observed_at: UnixMillis::new(14_000),
    };
    p.f.state.continue_test_session_v2_prepare_dispatch(
        &p.f.session_id,
        &p.proofs,
        ordinal,
        &current,
        UnixMillis::new(at),
        &mut StrictCrypto,
    )
}
#[test]
fn session_v2_rkv_last_four_is_one_exact_paid_completion_and_three_is_atomic_rejection() {
    let mut p = prepared_service(Some(4), |_| {});
    let paid = record(&mut p).unwrap().unwrap();
    let ordinal = paid.accepted_ordinal();
    let raw = serde_json::to_value(&p.f.state).unwrap();
    assert_eq!(
        raw["test_sessions_v2"][&p.f.session_id]["closing"][0],
        "budget_exhausted"
    );
    assert_eq!(
        p.f.state
            .test_session_v2_snapshot(&p.f.session_id)
            .unwrap()
            .roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap(),
        0
    );
    let committed = p.f.state.clone();
    check(&p, ordinal, 14_008).unwrap();
    assert_eq!(committed, p.f.state);
    assert!(check(&p, ordinal.checked_add(1).unwrap(), 14_008).is_err());
    assert!(record(&mut p).unwrap().is_none());
    assert_eq!(committed, p.f.state);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&p.f.state).unwrap()).unwrap();
    assert_eq!(cold, p.f.state);
    let mut three = prepared_service(Some(3), |_| {});
    let before = three.f.state.clone();
    assert!(record(&mut three).is_err());
    assert_eq!(before, three.f.state);
    assert!(
        three
            .f
            .state
            .test_session_v2_prepare_service_permits(&three.f.session_id)
            .is_none()
    );
}
struct NoNative;
impl oasis7_custody_core::approval_admission::ApprovalSignatureVerifier for NoNative {
    fn verify(
        &mut self,
        _: &oasis7_custody_core::approval_admission::ExactSignatureChecks,
    ) -> core::result::Result<(), oasis7_custody_core::approval_admission::ApprovalAdmissionError>
    {
        panic!("Finish/Abort continuation uses no new Native observation")
    }
}
fn closing_case(kind: SessionControlKindV1, salt: u8) {
    let mut staged = None;
    let mut p = prepared_service(Some(4), |f| {
        let q = SessionControlRequestV1 {
            api_version: 1,
            session_id: f.session_id.clone(),
            request_id: format!("rkv-interleaved-{salt}"),
            kind,
        };
        let challenge =
            control_fixture::challenge(f, &q, SessionPurposeV1::ReconcileRead, salt, 13_050);
        staged = Some((q, challenge));
    });
    let paid = record(&mut p).unwrap().unwrap();
    let ordinal = paid.accepted_ordinal();
    check(&p, ordinal, 14_008).unwrap(); // same prerequisite immediately before the simulated init await
    let (q, nonce) = staged.unwrap();
    let bytes = serde_json::to_vec(&q).unwrap();
    let route = match kind {
        SessionControlKindV1::Finish => HttpPopRoute::Finish(p.f.session_id.clone()),
        SessionControlKindV1::Abort => HttpPopRoute::Abort(p.f.session_id.clone()),
        _ => unreachable!(),
    };
    let pop = control_fixture::control_pop(
        &p.f,
        &bytes,
        (route, HttpPopProfileV1::Custody),
        &q.request_id,
        Some(&nonce),
        SessionPurposeV1::ReconcileRead,
        14_008,
    );
    let ingress = format!("rkv-closing-http-{salt}");
    control_fixture::control_paid(&mut p.f, &pop, &ingress, 14_008);
    let ack =
        p.f.state
            .control_test_session_v2(
                SessionProofRequest {
                    ingress_attempt_id: &ingress,
                    original_bytes: &bytes,
                    possession: &pop,
                },
                None,
                UnixMillis::new(14_008),
                &mut NoNative,
            )
            .unwrap();
    assert_eq!(ack.kind, kind);
    let raw = serde_json::to_value(&p.f.state).unwrap();
    // First cause is still BudgetExhausted: only causal Control records can
    // distinguish the later real Finish/Abort, never millisecond ordering.
    assert_eq!(
        raw["test_sessions_v2"][&p.f.session_id]["closing"][0],
        "budget_exhausted"
    );
    let before = p.f.state.clone();
    assert!(check(&p, ordinal, 14_008).is_err());
    assert_eq!(before, p.f.state);
    let _: AccountProvisionApprovalState = serde_json::from_value(raw).unwrap();
}
#[test]
fn session_v2_rkv_finish_after_paid_init_boundary_denies_the_unsent_sign() {
    closing_case(SessionControlKindV1::Finish, 83);
}
#[test]
fn session_v2_rkv_abort_after_paid_init_boundary_denies_the_unsent_sign() {
    closing_case(SessionControlKindV1::Abort, 84);
}
#[test]
fn session_v2_rkv_paid_completion_does_not_survive_deadline_freeze_or_trusted_epoch_change() {
    let mut p = prepared_service(Some(4), |_| {});
    let paid = record(&mut p).unwrap().unwrap();
    let ordinal = paid.accepted_ordinal();
    check(&p, ordinal, 14_008).unwrap();
    let mut epoch = p.f.context.clone();
    epoch.authority.service_epoch =
        DecimalU64(epoch.authority.service_epoch.0.checked_add(1).unwrap());
    let original = std::mem::replace(&mut p.f.context, epoch);
    assert!(check(&p, ordinal, 14_008).is_err());
    p.f.context = original;
    let baseline = p.f.state.clone();
    p.f.state.freeze(2).unwrap();
    assert!(check(&p, ordinal, 14_008).is_err());
    p.f.state = baseline;
    let deadline =
        p.f.state
            .test_session_v2_header(&p.f.session_id)
            .unwrap()
            .0
            .write_deadline
            .0;
    p.f.state
        .close_expired_test_session_v2(&p.f.session_id, UnixMillis::new(deadline))
        .unwrap();
    assert!(check(&p, ordinal, deadline).is_err());
}
