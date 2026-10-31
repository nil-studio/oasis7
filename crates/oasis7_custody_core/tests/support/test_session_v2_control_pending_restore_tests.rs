use super::*;
fn challenge_result(
    f: &mut SessionFixture,
    q: &SessionControlRequestV1,
    purpose: SessionPurposeV1,
    salt: u8,
    now: u64,
) -> std::result::Result<SessionChallengeV1, ProvisionApprovalError> {
    let r = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: format!("ctrl-challenge-{salt}"),
        purpose,
        kind: SessionChallengeKindV1::Control(q.kind),
        logical_request_id: q.request_id.clone(),
        operation_request_digest: q.digest().unwrap(),
    };
    let bytes = serde_json::to_vec(&r).unwrap();
    let pop = control_pop(
        f,
        &bytes,
        HttpPopRoute::ProofChallenges,
        HttpPopProfileV1::CustodyChallenge,
        &r.request_id,
        None,
        purpose,
        now,
    );
    let ingress = format!("ctrl-challenge-http-{salt}");
    control_paid(f, &pop, &ingress, now);
    let source = format!("ctrl-challenge-source-{salt}");
    let life = control_source(f, &pop, &ingress, &source, now);
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = TestSessionLiveCurrent {
        provision: &c,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: &source,
        source_ticket: 4,
        observed_at: UnixMillis::new(now),
    };
    let before = f.state.clone();
    let out = f.state.issue_test_session_v2_control_challenge(
        SessionProofRequest {
            ingress_attempt_id: &ingress,
            original_bytes: &bytes,
            possession: &pop,
        },
        &URL_SAFE_NO_PAD.encode([salt; 32]),
        &current,
        UnixMillis::new(now + 2),
        &mut StrictCrypto,
    );
    if out.is_err() {
        assert_eq!(f.state, before);
    }
    out
}
fn complete_existing(
    f: &mut SessionFixture,
    q: &SessionControlRequestV1,
    purpose: SessionPurposeV1,
    salt: u8,
    now: u64,
    nonce: SessionChallengeV1,
) -> std::result::Result<SessionControlAckV1, ProvisionApprovalError> {
    let bytes = serde_json::to_vec(q).unwrap();
    let route = match q.kind {
        SessionControlKindV1::Initialize => HttpPopRoute::Initialize(f.session_id.clone()),
        SessionControlKindV1::ToolsList => HttpPopRoute::ToolsList(f.session_id.clone()),
        SessionControlKindV1::Heartbeat => HttpPopRoute::Heartbeat(f.session_id.clone()),
        SessionControlKindV1::Finish => HttpPopRoute::Finish(f.session_id.clone()),
        SessionControlKindV1::Abort => HttpPopRoute::Abort(f.session_id.clone()),
    };
    let pop = control_pop(
        f,
        &bytes,
        route,
        HttpPopProfileV1::Custody,
        &q.request_id,
        Some(&nonce),
        purpose,
        now + 3,
    );
    let ingress = format!("ctrl-http-{salt}");
    control_paid(f, &pop, &ingress, now + 3);
    let pure = matches!(
        q.kind,
        SessionControlKindV1::Finish | SessionControlKindV1::Abort
    );
    let source = format!("ctrl-source-{salt}");
    let life = if pure {
        None
    } else {
        Some(control_source(f, &pop, &ingress, &source, now + 3))
    };
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = life.as_ref().map(|life| TestSessionLiveCurrent {
        provision: &c,
        native_scope: &ns,
        lifetime: life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: &source,
        source_ticket: 4,
        observed_at: UnixMillis::new(now + 3),
    });
    let before = f.state.clone();
    let proof = SessionProofRequest {
        ingress_attempt_id: &ingress,
        original_bytes: &bytes,
        possession: &pop,
    };
    let out = if pure {
        f.state
            .control_test_session_v2(proof, None, UnixMillis::new(now + 5), &mut NoNativeChecks)
    } else {
        f.state.control_test_session_v2(
            proof,
            current.as_ref(),
            UnixMillis::new(now + 5),
            &mut StrictCrypto,
        )
    };
    if out.is_err() {
        assert_eq!(f.state, before);
    }
    out
}

#[test]
fn session_v2_control_restore_rejects_nine_historically_overlapping_pending_challenges() {
    let mut f = claimed();
    let mut first = None;
    for i in 0..8u8 {
        let request = q(
            &f,
            &format!("pending-control-{i}"),
            SessionControlKindV1::Initialize,
        );
        let nonce = challenge_result(
            &mut f,
            &request,
            SessionPurposeV1::Execution,
            60 + i,
            13_000 + u64::from(i) * 1000,
        )
        .unwrap();
        if i == 0 {
            first = Some((request, nonce));
        }
    }
    let eight = f.state.clone();
    let ninth = q(
        &f,
        "pending-control-ninth",
        SessionControlKindV1::Initialize,
    );
    // All nine use genuine paid PoP/Source/LF; the original live guard rejects 9.
    assert!(challenge_result(&mut f, &ninth, SessionPurposeV1::Execution, 90, 21_000).is_err());
    // Restore the genuine prior eight-challenge state for the independent valid branch.
    f.state = serde_json::from_value(serde_json::to_value(&eight).unwrap()).unwrap();
    let (first_request, first_nonce) = first.unwrap();
    complete_existing(
        &mut f,
        &first_request,
        SessionPurposeV1::Execution,
        91,
        21_000,
        first_nonce,
    )
    .unwrap();
    challenge_result(&mut f, &ninth, SessionPurposeV1::Execution, 92, 22_000).unwrap();
    let mut raw = serde_json::to_value(&f.state).unwrap();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw.clone()).is_ok());
    let s = &mut raw["test_sessions_v2"][&f.session_id];
    s["challenges"]["ctrl-challenge-60"]["consumed_by"] = serde_json::Value::Null;
    s["controls"]
        .as_object_mut()
        .unwrap()
        .remove(&first_request.request_id)
        .unwrap();
    let removed = s["control_records"]
        .as_object_mut()
        .unwrap()
        .remove(&first_request.request_id)
        .unwrap();
    let sequence = removed["sequence"].as_u64().unwrap();
    for record in s["control_records"].as_object_mut().unwrap().values_mut() {
        let value = record["sequence"].as_u64().unwrap();
        if value > sequence {
            record["sequence"] = serde_json::json!(value - 1);
        }
    }
    // Charges, signed Source/LF and all nine original challenge facts remain.
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err(),
        "cold restore must replay the shared historical pending peak of eight"
    );
}

#[path = "test_session_v2_control_source_collision_tests.rs"]
mod source_collision;

#[path = "test_session_v2_control_closing_capacity_tests.rs"]
mod closing_capacity;
fn credential_challenge_only(
    f: &mut SessionFixture,
    request: &ClientCredentialClaimRequestV1,
    salt: u8,
    now: u64,
) -> SessionChallengeV1 {
    let challenge_request = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: format!("challenge-{salt}"),
        purpose: match request.purposes.first().unwrap() {
            TestSessionPurposeV1::Execute => SessionPurposeV1::Execution,
            TestSessionPurposeV1::Read => SessionPurposeV1::ReconcileRead,
            _ => panic!("unsupported fixture purpose"),
        },
        kind: SessionChallengeKindV1::CredentialClaim,
        logical_request_id: request.request_id.clone(),
        operation_request_digest: request.test_session_digest().unwrap(),
    };
    let bytes = serde_json::to_vec(&challenge_request).unwrap();
    let pop = http(
        f,
        &bytes,
        HttpPopRoute::ProofChallenges,
        HttpPopProfileV1::ClaimChallenge,
        &challenge_request.request_id,
        None,
        now,
    );
    let ingress = format!("challenge-ingress-{salt}");
    paid(f, &pop, &ingress, now);
    let source = format!("challenge-source-{salt}");
    let life = current_source(f, &pop, &ingress, &source, now);
    let policy = Policy::new(f.state.draft());
    let p = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = TestSessionLiveCurrent {
        provision: &p,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: &source,
        source_ticket: 4,
        observed_at: UnixMillis::new(now),
    };
    let challenge = f
        .state
        .issue_test_session_v2_credential_challenge(
            SessionProofRequest {
                ingress_attempt_id: &ingress,
                original_bytes: &bytes,
                possession: &pop,
            },
            &public_challenge_nonce(salt),
            &current,
            UnixMillis::new(now + 2),
            &mut StrictCrypto,
        )
        .unwrap();
    challenge
}

#[test]
fn session_v2_control_restore_allows_expired_pending_history_at_exact_expiry() {
    let mut f = claimed();
    for i in 0..8u8 {
        let request = q(
            &f,
            &format!("read-pending-{i}"),
            SessionControlKindV1::Initialize,
        );
        challenge_result(
            &mut f,
            &request,
            SessionPurposeV1::ReconcileRead,
            100 + i,
            13_000 + u64::from(i) * 1000,
        )
        .unwrap();
    }
    let request = q(
        &f,
        "read-after-exact-expiry",
        SessionControlKindV1::Initialize,
    );
    // New issue at 44_002 exactly equals the second original expiry. Both
    // earliest expired facts remain in immutable history but consume no slot.
    challenge_result(
        &mut f,
        &request,
        SessionPurposeV1::ReconcileRead,
        110,
        44_000,
    )
    .unwrap();
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .is_ok()
    );
}
#[test]
fn session_v2_control_restore_pending_peak_is_shared_with_real_credential_challenge() {
    let mut f = claimed();
    let pending_claim = request(&f, "pending-credential-mixed", Some(1));
    credential_challenge_only(&mut f, &pending_claim, 120, 13_000);
    let mut first = None;
    for i in 0..7u8 {
        let request = q(
            &f,
            &format!("mixed-control-{i}"),
            SessionControlKindV1::Initialize,
        );
        let nonce = challenge_result(
            &mut f,
            &request,
            SessionPurposeV1::Execution,
            121 + i,
            14_000 + u64::from(i) * 1000,
        )
        .unwrap();
        if i == 0 {
            first = Some((request, nonce));
        }
    }
    let eight = f.state.clone();
    let ninth = q(&f, "mixed-control-ninth", SessionControlKindV1::Initialize);
    assert!(challenge_result(&mut f, &ninth, SessionPurposeV1::Execution, 130, 21_000).is_err());
    f.state = serde_json::from_value(serde_json::to_value(&eight).unwrap()).unwrap();
    let (first_request, first_nonce) = first.unwrap();
    complete_existing(
        &mut f,
        &first_request,
        SessionPurposeV1::Execution,
        131,
        21_000,
        first_nonce,
    )
    .unwrap();
    challenge_result(&mut f, &ninth, SessionPurposeV1::Execution, 132, 21_003).unwrap();
    let mut raw = serde_json::to_value(&f.state).unwrap();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw.clone()).is_ok());
    // Both actual events commit at 21_005. Preserve their existing Control
    // sequence; a timestamp-only count would miss an issue-before-consume peak.
    let mut reversed = raw.clone();
    let records = &mut reversed["test_sessions_v2"][&f.session_id]["control_records"];
    let consumed_sequence = records[&first_request.request_id]["sequence"].clone();
    let issued_sequence = records["ctrl-challenge-132"]["sequence"].clone();
    records[&first_request.request_id]["sequence"] = issued_sequence;
    records["ctrl-challenge-132"]["sequence"] = consumed_sequence;
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(reversed).is_err(),
        "same-ms issue before consumption reaches nine shared pending slots"
    );
    let s = &mut raw["test_sessions_v2"][&f.session_id];
    s["challenges"]["ctrl-challenge-121"]["consumed_by"] = serde_json::Value::Null;
    s["controls"]
        .as_object_mut()
        .unwrap()
        .remove(&first_request.request_id)
        .unwrap();
    let removed = s["control_records"]
        .as_object_mut()
        .unwrap()
        .remove(&first_request.request_id)
        .unwrap();
    let sequence = removed["sequence"].as_u64().unwrap();
    for record in s["control_records"].as_object_mut().unwrap().values_mut() {
        let n = record["sequence"].as_u64().unwrap();
        if n > sequence {
            record["sequence"] = serde_json::json!(n - 1);
        }
    }
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err(),
        "eight-slot history is shared, not eight Control plus credential slots"
    );
}
