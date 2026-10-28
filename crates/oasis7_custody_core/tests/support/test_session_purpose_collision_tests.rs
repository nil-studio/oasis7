use super::*;
fn charge(
    f: &mut SessionFixture,
    purpose: SessionPurposeV1,
    ingress: &str,
    proof: &HttpPossessionReceipt,
) -> std::result::Result<(), ProvisionApprovalError> {
    f.state.charge_test_session_purpose_http_attempt(
        ID,
        purpose,
        SessionHttpAdmission {
            ingress_attempt_id: ingress,
            possession: proof,
        },
        UnixMillis::new(12_006),
    )
}
fn fixture() -> (SessionFixture, HttpPossessionReceipt) {
    let (f, execution, _) = claimed();
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "collision-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: "collision-action".into(),
        operation_request_digest: "ab".repeat(32),
    };
    let proof = http(
        &f,
        &serde_json::to_vec(&q).unwrap(),
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &q.request_id,
        None,
        Some(&execution),
        12_006,
    );
    (f, proof)
}
#[test]
fn test_session_source_then_purpose_http_id_collision_rejects_without_mutation() {
    let (mut f, proof) = fixture();
    funded_source(
        &mut f,
        SessionSourceFundingV1::Execution,
        SessionPurposeV1::Execution,
        "shared-collision-id",
        &"d1".repeat(32),
        12_000,
    );
    let before = f.state.clone();
    assert!(
        charge(
            &mut f,
            SessionPurposeV1::Execution,
            "shared-collision-id",
            &proof
        )
        .is_err()
    );
    assert_eq!(before, f.state);
}
#[test]
fn test_session_purpose_http_then_source_id_collision_rejects_without_mutation() {
    let (mut f, proof) = fixture();
    charge(
        &mut f,
        SessionPurposeV1::Execution,
        "shared-collision-id",
        &proof,
    )
    .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_source(
                ID,
                SessionSourceReservation {
                    operation_id: "shared-collision-id",
                    nonce: &"d1".repeat(32),
                    funding: SessionSourceFundingV1::Execution
                },
                UnixMillis::new(12_006)
            )
            .is_err()
    );
    assert_eq!(before, f.state);
}
fn action_fixture() -> (
    SessionFixture,
    HttpPossessionReceipt,
    SignedManagedNativeSessionLifetimeV1,
) {
    let (mut f, execution, _) = claimed();
    let life = funded_source(
        &mut f,
        SessionSourceFundingV1::Execution,
        SessionPurposeV1::Execution,
        "action-live-source",
        &"d2".repeat(32),
        12_000,
    );
    let action = match &f.state.test_sessions()[ID].input.request.intent {
        CustodyIntentV1::TestSession { actions, .. } => actions[0].clone(),
        _ => unreachable!(),
    };
    let request = SessionToolRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "actual-collision-action".into(),
        action: SessionToolActionV1::Execute(action),
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "actual-collision-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: request.request_id.clone(),
        operation_request_digest: request.digest().unwrap(),
    };
    let qbytes = serde_json::to_vec(&q).unwrap();
    let qproof = http(
        &f,
        &qbytes,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &q.request_id,
        None,
        Some(&execution),
        12_003,
    );
    charge(
        &mut f,
        SessionPurposeV1::Execution,
        "action-challenge-ingress",
        &qproof,
    )
    .unwrap();
    let policy = Policy::new(f.state.draft());
    let context = f.context.clone();
    let pin = f.pin.clone();
    let current = policy.current(&context);
    let ns = scope(&pin);
    let live = TestSessionLiveCurrent {
        provision: &current,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "action-live-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(12_002),
    };
    let nonce = URL_SAFE_NO_PAD.encode([9; 32]);
    f.state
        .issue_test_session_challenge(
            SessionProofRequest {
                ingress_attempt_id: "action-challenge-ingress",
                original_bytes: &qbytes,
                possession: &qproof,
            },
            &nonce,
            &live,
            UnixMillis::new(12_006),
            &mut StrictCrypto,
        )
        .unwrap();
    let proof = http(
        &f,
        &bytes,
        (HttpPopProfileV1::Custody, HttpPopRoute::Actions),
        &request.request_id,
        Some(&nonce),
        Some(&execution),
        12_006,
    );
    charge(
        &mut f,
        SessionPurposeV1::Execution,
        "action-call-ingress",
        &proof,
    )
    .unwrap();
    f.state
        .admit_test_session_action(
            SessionProofRequest {
                ingress_attempt_id: "action-call-ingress",
                original_bytes: &bytes,
                possession: &proof,
            },
            SessionActionCost { business_units: 0 },
            &live,
            UnixMillis::new(12_006),
            &mut StrictCrypto,
        )
        .unwrap();
    (f, proof, life)
}
fn outbound(
    f: &mut SessionFixture,
    life: &SignedManagedNativeSessionLifetimeV1,
) -> std::result::Result<(), ProvisionApprovalError> {
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    let ns = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &current,
        native_scope: &ns,
        lifetime: life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "action-live-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(12_002),
    };
    f.state.claim_test_session_action_transport(
        ID,
        "actual-collision-action",
        "shared-collision-id",
        &live,
        UnixMillis::new(12_006),
        &mut StrictCrypto,
    )
}
#[test]
fn test_session_action_outbound_then_purpose_http_collision_rejects_without_mutation() {
    let (mut f, proof, life) = action_fixture();
    outbound(&mut f, &life).unwrap();
    let before = f.state.clone();
    assert!(
        charge(
            &mut f,
            SessionPurposeV1::Execution,
            "shared-collision-id",
            &proof
        )
        .is_err()
    );
    assert_eq!(before, f.state);
}
#[test]
fn test_session_purpose_http_then_action_outbound_collision_rejects_without_mutation() {
    let (mut f, proof, life) = action_fixture();
    charge(
        &mut f,
        SessionPurposeV1::Execution,
        "shared-collision-id",
        &proof,
    )
    .unwrap();
    let before = f.state.clone();
    assert!(outbound(&mut f, &life).is_err());
    assert_eq!(before, f.state);
}

fn renamed_http_collision(state: &AccountProvisionApprovalState) -> serde_json::Value {
    let mut serialized = serde_json::to_value(state).unwrap();
    let rows = serialized["test_sessions"][ID]["purpose_http_admissions"]
        .as_object_mut()
        .unwrap();
    let row = rows.remove("valid-paid-id").unwrap();
    rows.insert("shared-collision-id".into(), row);
    serialized
}
#[test]
fn test_session_balanced_source_http_collision_restore_rejects() {
    let (mut f, proof) = fixture();
    funded_source(
        &mut f,
        SessionSourceFundingV1::Execution,
        SessionPurposeV1::Execution,
        "shared-collision-id",
        &"d1".repeat(32),
        12_000,
    );
    charge(&mut f, SessionPurposeV1::Execution, "valid-paid-id", &proof).unwrap();
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(renamed_http_collision(&f.state))
            .is_err(),
        "changing only an ingress ID preserves all budget math but must not alias a source original"
    );
}
#[test]
fn test_session_balanced_outbound_http_collision_restore_rejects() {
    let (mut f, proof, life) = action_fixture();
    outbound(&mut f, &life).unwrap();
    charge(&mut f, SessionPurposeV1::Execution, "valid-paid-id", &proof).unwrap();
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(renamed_http_collision(&f.state))
            .is_err(),
        "changing only an ingress ID preserves all budget math but must not alias an outbound attempt"
    );
}

#[test]
fn test_session_purpose_http_ids_cannot_alias_control_host_or_identity_hops() {
    for identity_hop in [false, true] {
        for purpose_first in [false, true] {
            let (mut f, proof) = fixture();
            if purpose_first {
                charge(
                    &mut f,
                    SessionPurposeV1::Execution,
                    "shared-collision-id",
                    &proof,
                )
                .unwrap();
            }
            let policy = Policy::new(f.state.draft());
            let context = f.context.clone();
            let current = policy.current(&context);
            let before = f.state.clone();
            let control = if identity_hop {
                f.state.charge_test_session_internal_control_transport(
                    ID,
                    "shared-collision-id",
                    &current,
                    UnixMillis::new(12_006),
                )
            } else {
                f.state.charge_test_session_control_http_attempt(
                    ID,
                    SessionHttpAdmission {
                        ingress_attempt_id: "shared-collision-id",
                        possession: &proof,
                    },
                    UnixMillis::new(12_006),
                )
            };
            if purpose_first {
                assert!(control.is_err());
                assert_eq!(before, f.state);
            } else {
                control.unwrap();
                let before = f.state.clone();
                assert!(
                    charge(
                        &mut f,
                        SessionPurposeV1::Execution,
                        "shared-collision-id",
                        &proof
                    )
                    .is_err()
                );
                assert_eq!(before, f.state);
                charge(&mut f, SessionPurposeV1::Execution, "valid-paid-id", &proof).unwrap();
                assert!(
                    serde_json::from_value::<AccountProvisionApprovalState>(
                        renamed_http_collision(&f.state)
                    )
                    .is_err(),
                    "balanced purpose HTTP cannot alias an actual Control hop"
                );
            }
        }
    }
}
