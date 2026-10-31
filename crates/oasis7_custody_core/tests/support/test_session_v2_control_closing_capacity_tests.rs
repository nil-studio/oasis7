//! Genuine paid/signature prefixes; no caller purpose/Current flag grants a challenge.
use super::super::*;
struct PreparedCredentialChallenge {
    body: Vec<u8>,
    proof: HttpPossessionReceipt,
    ingress: String,
    source: String,
    lifetime: SignedManagedNativeSessionLifetimeV1,
    nonce: String,
    observed_at: u64,
}
fn prepare_credential_challenge(
    f: &mut SessionFixture,
    request: &ClientCredentialClaimRequestV1,
    salt: u8,
    now: u64,
) -> PreparedCredentialChallenge {
    let challenge = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: format!("closing-credential-challenge-{salt}"),
        purpose: match request.purposes[0] {
            TestSessionPurposeV1::Execute => SessionPurposeV1::Execution,
            TestSessionPurposeV1::Read => SessionPurposeV1::ReconcileRead,
            _ => unreachable!(),
        },
        kind: SessionChallengeKindV1::CredentialClaim,
        logical_request_id: request.request_id.clone(),
        operation_request_digest: request.test_session_digest().unwrap(),
    };
    let body = serde_json::to_vec(&challenge).unwrap();
    let proof = http(
        f,
        &body,
        HttpPopRoute::ProofChallenges,
        HttpPopProfileV1::ClaimChallenge,
        &challenge.request_id,
        None,
        now,
    );
    let ingress = format!("closing-credential-http-{salt}");
    paid(f, &proof, &ingress, now);
    let source = format!("closing-credential-source-{salt}");
    let lifetime = current_source(f, &proof, &ingress, &source, now);
    PreparedCredentialChallenge {
        body,
        proof,
        ingress,
        source,
        lifetime,
        nonce: public_challenge_nonce(salt),
        observed_at: now,
    }
}
fn issue_credential_challenge(
    f: &mut SessionFixture,
    p: &PreparedCredentialChallenge,
    now: u64,
) -> std::result::Result<SessionChallengeV1, ProvisionApprovalError> {
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let native = scope(&f.pin);
    let current = TestSessionLiveCurrent {
        provision: &provision,
        native_scope: &native,
        lifetime: &p.lifetime,
        private_sid_generation: DecimalU64(1),
        source_operation_id: &p.source,
        source_ticket: 4,
        observed_at: UnixMillis::new(p.observed_at),
    };
    let before = f.state.clone();
    let out = f.state.issue_test_session_v2_credential_challenge(
        SessionProofRequest {
            ingress_attempt_id: &p.ingress,
            original_bytes: &p.body,
            possession: &p.proof,
        },
        &p.nonce,
        &current,
        UnixMillis::new(now),
        &mut StrictCrypto,
    );
    if out.is_err() {
        assert_eq!(f.state, before);
    }
    out
}
#[test]
fn session_v2_control_restore_execution_issue_cannot_cross_same_ms_finish_capacity_barrier() {
    let mut f = claimed();
    let finish = q(&f, "capacity-closing-finish", SessionControlKindV1::Finish);
    let finish_challenge = challenge(
        &mut f,
        &finish,
        SessionPurposeV1::ReconcileRead,
        150,
        13_000,
    );
    for i in 0..6u8 {
        let q = q(
            &f,
            &format!("closing-pending-{i}"),
            SessionControlKindV1::Initialize,
        );
        challenge(
            &mut f,
            &q,
            SessionPurposeV1::Execution,
            151 + i,
            14_000 + u64::from(i) * 1000,
        );
    }
    let execution = request(&f, "closing-new-execution", Some(1));
    // This real raw PoP and Native Source/LF is paid while there are seven slots.
    // It is the exact same prepared input in both independent historical prefixes.
    let prepared = prepare_credential_challenge(&mut f, &execution, 160, 19_500);
    let seven = f.state.clone();
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&seven).unwrap()
        )
        .is_ok()
    );
    let eighth = q(&f, "closing-eighth", SessionControlKindV1::Initialize);
    challenge(&mut f, &eighth, SessionPurposeV1::Execution, 161, 19_700);
    assert!(issue_credential_challenge(&mut f, &prepared, 21_000).is_err());
    super::complete_existing(
        &mut f,
        &finish,
        SessionPurposeV1::ReconcileRead,
        162,
        20_995,
        finish_challenge.clone(),
    )
    .unwrap();
    assert!(issue_credential_challenge(&mut f, &prepared, 21_000).is_err());
    let closed = serde_json::to_value(&f.state).unwrap();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(closed.clone()).is_ok());
    // Read is permitted after Closing, unlike Execution; this is an actual
    // different closed request/PoP/paid Source, not a modified purpose boolean.
    let mut read = request(&f, "closing-new-read", Some(1));
    read.purposes = vec![TestSessionPurposeV1::Read];
    let read_prepared = prepare_credential_challenge(&mut f, &read, 163, 21_100);
    issue_credential_challenge(&mut f, &read_prepared, 21_102).unwrap();
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .is_ok()
    );
    // The seven-pending prefix really accepts the exact Execution original at T.
    f.state = serde_json::from_value(serde_json::to_value(&seven).unwrap()).unwrap();
    let original = issue_credential_challenge(&mut f, &prepared, 21_000).unwrap();
    let accepted = serde_json::to_value(&f.state).unwrap();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(accepted.clone()).is_ok());
    // A legal same-ms ordering must remain admissible: at seven pending,
    // Execution really issues first and Finish really commits afterwards at T.
    super::complete_existing(
        &mut f,
        &finish,
        SessionPurposeV1::ReconcileRead,
        164,
        20_995,
        finish_challenge,
    )
    .unwrap();
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .is_ok()
    );
    let mut impossible = closed;
    let target = &mut impossible["test_sessions_v2"][&f.session_id];
    let valid = &accepted["test_sessions_v2"][&f.session_id];
    target["challenges"][&original.request_id] = valid["challenges"][&original.request_id].clone();
    target["control_logicals"][&execution.request_id] =
        valid["control_logicals"][&execution.request_id].clone();
    target["control"]["requests"]["reservations"][&execution.request_id] =
        valid["control"]["requests"]["reservations"][&execution.request_id].clone();
    let issue = valid["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["event"]["kind"] == "challenge_issued"
                && e["event"]["request_id"] == original.request_id
        })
        .unwrap()
        .clone();
    // Complete retained fact+event pairs in both possible relative orders.
    // Source, fee and LF events are already present before these insertions.
    for before_finish in [true, false] {
        let mut ordered = impossible.clone();
        let target = &mut ordered["test_sessions_v2"][&f.session_id];
        let events = target["events"].as_array_mut().unwrap();
        let finish_index = events
            .iter()
            .position(|e| {
                e["event"]["kind"] == "control_acknowledged"
                    && e["event"]["request_id"] == finish.request_id
            })
            .unwrap();
        events.insert(finish_index + usize::from(!before_finish), issue.clone());
        super::reindex_journal(target);
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(ordered).is_err(),
            "complete issue-before-Finish must fail capacity; issue-after-Finish must fail phase"
        );
    }
}
