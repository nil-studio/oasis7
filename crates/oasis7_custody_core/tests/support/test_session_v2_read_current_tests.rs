//! Core fixture contract only: real HTTP PoP and persisted Prepare original;
//! no Worker routing, Read DFRF/Identity producer or full Current qualification.
use super::*;
use oasis7_custody_core::provisioning::ReadCurrentPortV3;
#[test]
fn read_current_known_prepaid_sixty_once_five_tickets_and_strict_cold() {
    let mut p = ready();
    let operation = known(&p);
    let (body, pop) = read_pop(&mut p.f, "rc-known", &operation, 14_010);
    let before = budget(&p.f, SessionPurposeV1::ReconcileRead);
    let execution = budget(&p.f, SessionPurposeV1::Execution);
    let ingress = "rc-ingress";
    let paid =
        p.f.state
            .settle_test_session_v2_read_current_five(
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
        (before.0 - 1, before.1 - 5)
    );
    let nonce = oasis7_custody_core::approval::bytes_digest(b"read-source-five");
    let mut source =
        p.f.state
            .reserve_test_session_v2_read_current_source_five(paid, &nonce, UnixMillis::new(14_013))
            .unwrap();
    assert_eq!(
        budget(&p.f, SessionPurposeV1::ReconcileRead),
        (before.0 - 1, before.1 - 60)
    );
    assert_eq!(budget(&p.f, SessionPurposeV1::Execution), execution);
    for n in 1..=5 {
        assert_eq!(
            p.f.state
                .reserve_test_session_v2_read_current_ticket(&mut source, UnixMillis::new(14_014))
                .unwrap(),
            n
        );
    }
    let context = p.f.context.clone();
    let policy = Policy::new(p.f.state.draft());
    let current = policy.current(&context);
    let stable = p.f.state.clone();
    assert!(
        p.f.state
            .claim_test_session_v2_source(
                &p.f.session_id,
                source.source_operation_id(),
                &current,
                UnixMillis::new(14_014)
            )
            .is_err(),
        "no legacy Native source request before actual Read DFRF"
    );
    assert_eq!(p.f.state, stable);
    assert!(
        p.f.state
            .reserve_test_session_v2_read_current_ticket(&mut source, UnixMillis::new(14_014))
            .is_err()
    );
    assert!(
        p.f.state
            .reserve_test_session_v2_source_read(
                &p.f.session_id,
                source.source_operation_id(),
                UnixMillis::new(14_014)
            )
            .is_err()
    );
    assert_eq!(p.f.state, stable);
    let port = ReadCurrentPortV3::ClaimSourceDirectory;
    let prepaid = p
        .f
        .state
        .claim_test_session_v2_read_current_prepaid_port(&mut source, port, UnixMillis::new(14_014))
        .unwrap();
    assert_eq!(prepaid.port(), port);
    assert_eq!(port.id(), "claim_source.directory");
    assert_eq!(port.authority(), "ManagerDirectory Actor");
    let stable = p.f.state.clone();
    assert!(
        p.f.state
            .claim_test_session_v2_read_current_prepaid_port(
                &mut source,
                port,
                UnixMillis::new(14_014)
            )
            .is_err()
    );
    assert_eq!(p.f.state, stable);
    let raw = serde_json::to_value(&stable).unwrap();
    p.f.state = serde_json::from_value(raw.clone()).unwrap();
    assert!(
        p.f.state
            .settle_test_session_v2_read_current_five(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(14_015)
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        p.f.state, stable,
        "cold must not reissue the first opaque or refund"
    );
    let mut missing_root = raw.clone();
    assert!(
        missing_root["test_sessions_v2"][&p.f.session_id]["roots"]
            .as_object_mut()
            .unwrap()
            .remove("reconcile_read")
            .is_some()
    );
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(missing_root).is_err(),
        "a real paid ReadCurrent record must retain its independent Read root"
    );
    for key in ["original_digest", "reservation"] {
        let mut bad = raw.clone();
        bad["test_sessions_v2"][&p.f.session_id]["purpose_http_admissions"][ingress]["class"]["preparation"].as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
    }
    let mut bad = raw;
    bad["test_sessions_v2"][&p.f.session_id]["purpose_http_admissions"][ingress]["class"]["preparation"]["reservation"]["claims"].as_object_mut().unwrap().clear();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
}
#[test]
fn read_current_unknown_permanent_pays_only_five_no_source_or_current() {
    let mut p = ready();
    let (body, pop) = read_pop(&mut p.f, "rc-unknown", "unknown-original", 14_010);
    let before = budget(&p.f, SessionPurposeV1::ReconcileRead);
    for now in [14_012, 14_013] {
        assert!(
            p.f.state
                .settle_test_session_v2_read_current_five(
                    &p.f.session_id,
                    SessionProofRequest {
                        ingress_attempt_id: "rc-unknown-ingress",
                        original_bytes: &body,
                        possession: &pop
                    },
                    UnixMillis::new(now)
                )
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(
        budget(&p.f, SessionPurposeV1::ReconcileRead),
        (before.0 - 1, before.1 - 5)
    );
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&p.f.state).unwrap()).unwrap();
    assert_eq!(cold, p.f.state);
}
#[test]
fn read_current_one_short_preserves_first_fee_and_rejects_source_atomically() {
    let mut p = ready();
    let operation = known(&p);
    let (body, pop) = read_pop(&mut p.f, "rc-short", &operation, 14_010);
    let remaining = budget(&p.f, SessionPurposeV1::ReconcileRead).1;
    for n in 0..remaining - 59 {
        charge(
            &mut p.f,
            &body,
            &pop,
            &format!("rc-burn-{n}"),
            SessionPurposeV1::ReconcileRead,
            14_012,
        )
        .unwrap();
    }
    let paid =
        p.f.state
            .settle_test_session_v2_read_current_five(
                &p.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: "rc-short-ingress",
                    original_bytes: &body,
                    possession: &pop,
                },
                UnixMillis::new(14_012),
            )
            .unwrap()
            .unwrap();
    assert_eq!(budget(&p.f, SessionPurposeV1::ReconcileRead).1, 54);
    let stable = p.f.state.clone();
    let nonce = oasis7_custody_core::approval::bytes_digest(b"short");
    assert!(
        p.f.state
            .reserve_test_session_v2_read_current_source_five(paid, &nonce, UnixMillis::new(14_013))
            .is_err()
    );
    assert_eq!(
        p.f.state, stable,
        "failed 55 reservation retains exactly the first 5"
    );
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&stable).unwrap()).unwrap();
    assert_eq!(cold, stable);
}
