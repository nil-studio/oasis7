//! Namespace collisions use actual fee/reservation APIs, never patched ledgers.
use super::*;
#[test]
fn live_context_core_payment_ids_collide_both_directions_with_generic_source_http_and_control() {
    let mut v = live_ready();
    let q = request(SessionPurposeV1::Execution, "live-fee-collision");
    let (_, receipt) = signed(&v.r.f, &q, &v.r.execution, 12_022);
    let policy = Policy::new(v.r.f.state.draft());
    let context = v.r.f.context.clone();
    let provision = policy.current(&context);
    let pin = v.r.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &v.life, SOURCE, 12_022);
    let keys = [
        claim().attempt.to_string(),
        v.descriptor.paid_ports[0].clone(),
        v.descriptor.paid_ports[1].clone(),
        v.descriptor.paid_ports[2].clone(),
    ];
    for key in keys {
        // Old namespaces cannot create the 275-byte typed port IDs at all.
        // The <=256-byte claim ID still exercises genuine occupied-state conflicts.
        for kind in 0..if key.len() <= 256 { 4 } else { 0 } {
            let mut state = v.before_claim.clone();
            match kind {
                0 => state
                    .reserve_test_session_purpose_transport(
                        ID,
                        SessionPurposeV1::Execution,
                        &key,
                        UnixMillis::new(12_022),
                    )
                    .unwrap(),
                1 => {
                    state
                        .reserve_test_session_source(
                            ID,
                            SessionSourceReservation {
                                operation_id: &key,
                                nonce: &"e7".repeat(32),
                                funding: SessionSourceFundingV1::Execution,
                            },
                            UnixMillis::new(12_022),
                        )
                        .unwrap();
                }
                2 => state
                    .charge_test_session_purpose_http_attempt(
                        ID,
                        SessionPurposeV1::Execution,
                        SessionHttpAdmission {
                            ingress_attempt_id: &key,
                            possession: &receipt,
                        },
                        UnixMillis::new(12_022),
                    )
                    .unwrap(),
                _ => state
                    .charge_test_session_control_http_attempt(
                        ID,
                        SessionHttpAdmission {
                            ingress_attempt_id: &key,
                            possession: &receipt,
                        },
                        UnixMillis::new(12_022),
                    )
                    .unwrap(),
            }
            restore(&state);
            let before = state.clone();
            assert!(
                state
                    .claim_test_session_live_context_read(
                        ID,
                        claim(),
                        &current,
                        UnixMillis::new(12_022),
                        &mut StrictCrypto
                    )
                    .is_err()
            );
            assert_eq!(state, before);
        }
        let before = v.r.f.state.clone();
        assert!(
            v.r.f
                .state
                .reserve_test_session_purpose_transport(
                    ID,
                    SessionPurposeV1::Execution,
                    &key,
                    UnixMillis::new(12_022)
                )
                .is_err()
        );
        assert_eq!(v.r.f.state, before);
        assert!(
            v.r.f
                .state
                .reserve_test_session_source(
                    ID,
                    SessionSourceReservation {
                        operation_id: &key,
                        nonce: &"e8".repeat(32),
                        funding: SessionSourceFundingV1::Execution
                    },
                    UnixMillis::new(12_022)
                )
                .is_err()
        );
        assert_eq!(v.r.f.state, before);
        assert!(
            v.r.f
                .state
                .charge_test_session_purpose_http_attempt(
                    ID,
                    SessionPurposeV1::Execution,
                    SessionHttpAdmission {
                        ingress_attempt_id: &key,
                        possession: &receipt
                    },
                    UnixMillis::new(12_022)
                )
                .is_err()
        );
        assert_eq!(v.r.f.state, before);
        assert!(
            v.r.f
                .state
                .charge_test_session_control_http_attempt(
                    ID,
                    SessionHttpAdmission {
                        ingress_attempt_id: &key,
                        possession: &receipt
                    },
                    UnixMillis::new(12_022)
                )
                .is_err()
        );
        assert_eq!(v.r.f.state, before);
    }
}
// Reuse the existing legacy Core action proof API only to establish an occupied
// outbound ID. This fixture does not claim World effects or authoritative usage.
fn legacy_action(v: &mut LiveReady) -> String {
    let action = match &v.r.f.state.test_sessions()[ID].input.request.intent {
        CustodyIntentV1::TestSession { actions, .. } => actions[0].clone(),
        _ => unreachable!(),
    };
    let action = SessionToolRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "live-existing-action".into(),
        action: SessionToolActionV1::Execute(action),
    };
    let bytes = serde_json::to_vec(&action).unwrap();
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "live-existing-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: action.request_id.clone(),
        operation_request_digest: action.digest().unwrap(),
    };
    let body = serde_json::to_vec(&q).unwrap();
    let qproof = http(
        &v.r.f,
        &body,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &q.request_id,
        None,
        Some(&v.r.execution),
        12_022,
    );
    v.r.f
        .state
        .charge_test_session_purpose_http_attempt(
            ID,
            SessionPurposeV1::Execution,
            SessionHttpAdmission {
                ingress_attempt_id: "live-existing-challenge-fee",
                possession: &qproof,
            },
            UnixMillis::new(12_022),
        )
        .unwrap();
    let policy = Policy::new(v.r.f.state.draft());
    let context = v.r.f.context.clone();
    let provision = policy.current(&context);
    let pin = v.r.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &v.life, SOURCE, 12_022);
    let nonce = URL_SAFE_NO_PAD.encode([0xc7; 32]);
    v.r.f
        .state
        .issue_test_session_challenge(
            SessionProofRequest {
                ingress_attempt_id: "live-existing-challenge-fee",
                original_bytes: &body,
                possession: &qproof,
            },
            &nonce,
            &current,
            UnixMillis::new(12_022),
            &mut StrictCrypto,
        )
        .unwrap();
    let proof = http(
        &v.r.f,
        &bytes,
        (HttpPopProfileV1::Custody, HttpPopRoute::Actions),
        &action.request_id,
        Some(&nonce),
        Some(&v.r.execution),
        12_022,
    );
    v.r.f
        .state
        .charge_test_session_purpose_http_attempt(
            ID,
            SessionPurposeV1::Execution,
            SessionHttpAdmission {
                ingress_attempt_id: "live-existing-action-fee",
                possession: &proof,
            },
            UnixMillis::new(12_022),
        )
        .unwrap();
    v.r.f
        .state
        .admit_test_session_action(
            SessionProofRequest {
                ingress_attempt_id: "live-existing-action-fee",
                original_bytes: &bytes,
                possession: &proof,
            },
            SessionActionCost { business_units: 0 },
            &current,
            UnixMillis::new(12_022),
            &mut StrictCrypto,
        )
        .unwrap();
    action.request_id
}
#[test]
fn live_context_core_payment_ids_collide_both_directions_with_actual_action_outbound() {
    let mut v = live_ready();
    let key = claim().attempt.to_string();
    v.r.f.state = v.before_claim.clone();
    let action = legacy_action(&mut v);
    let policy = Policy::new(v.r.f.state.draft());
    let context = v.r.f.context.clone();
    let provision = policy.current(&context);
    let pin = v.r.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &v.life, SOURCE, 12_022);
    v.r.f
        .state
        .claim_test_session_action_transport(
            ID,
            &action,
            &key,
            &current,
            UnixMillis::new(12_022),
            &mut StrictCrypto,
        )
        .unwrap();
    let before = v.r.f.state.clone();
    assert!(
        v.r.f
            .state
            .claim_test_session_live_context_read(
                ID,
                claim(),
                &current,
                UnixMillis::new(12_022),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
    let mut v = live_ready();
    let action = legacy_action(&mut v);
    let policy = Policy::new(v.r.f.state.draft());
    let context = v.r.f.context.clone();
    let provision = policy.current(&context);
    let pin = v.r.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &v.life, SOURCE, 12_022);
    for key in [
        claim().attempt.to_string(),
        v.descriptor.paid_ports[0].clone(),
    ] {
        let before = v.r.f.state.clone();
        assert!(
            v.r.f
                .state
                .claim_test_session_action_transport(
                    ID,
                    &action,
                    &key,
                    &current,
                    UnixMillis::new(12_022),
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(v.r.f.state, before);
    }
}
