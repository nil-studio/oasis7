use super::*;
fn rotate_execution(f: &mut SessionFixture, token: &str) {
    let life = control_source(f, "metadata-rotation-source", &"c1".repeat(32), 13_000);
    let policy = Policy::new(f.state.draft());
    let p = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = TestSessionLiveCurrent {
        provision: &p,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "metadata-rotation-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(13_002),
    };
    let request = ClientCredentialClaimRequestV1 {
        api_version: 1,
        enrollment_id: f.fact.registration.enrollment_id.clone(),
        test_session_id: ID.into(),
        request_id: "replace-execution".into(),
        purposes: vec![TestSessionPurposeV1::Execute],
        replace: true,
        expected_generation: Some(DecimalU64(1)),
    };
    let body = serde_json::to_vec(&request).unwrap();
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "replace-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::CredentialClaim,
        logical_request_id: request.request_id.clone(),
        operation_request_digest: request.test_session_digest().unwrap(),
    };
    let qbytes = serde_json::to_vec(&q).unwrap();
    let proof = http(
        f,
        &qbytes,
        (
            HttpPopProfileV1::ClaimChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &q.request_id,
        None,
        None,
        13_003,
    );
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: proof.request_id(),
                possession: &proof,
            },
            UnixMillis::new(13_003),
        )
        .unwrap();
    let challenge = f
        .state
        .issue_test_session_challenge(
            SessionProofRequest {
                ingress_attempt_id: proof.request_id(),
                original_bytes: &qbytes,
                possession: &proof,
            },
            &URL_SAFE_NO_PAD.encode([36; 32]),
            &current,
            UnixMillis::new(13_003),
            &mut StrictCrypto,
        )
        .unwrap();
    let proof = http(
        f,
        &body,
        (
            HttpPopProfileV1::CredentialClaim,
            HttpPopRoute::EnrollmentClaim(f.fact.registration.enrollment_id.clone()),
        ),
        &request.request_id,
        Some(&challenge.nonce),
        None,
        13_004,
    );
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: proof.request_id(),
                possession: &proof,
            },
            UnixMillis::new(13_004),
        )
        .unwrap();
    let allocation = [SessionCredentialAllocation {
        purpose: SessionPurposeV1::Execution,
        digest: Sha256::digest(token.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    }];
    f.state
        .claim_test_session_credentials(
            ID,
            SessionCredentialClaim {
                allocations: &allocation,
                proof: SessionProofRequest {
                    ingress_attempt_id: proof.request_id(),
                    original_bytes: &body,
                    possession: &proof,
                },
            },
            &current,
            UnixMillis::new(13_004),
            &mut StrictCrypto,
        )
        .unwrap();
}
#[test]
fn tool_nonce_retired_generation_history_restores_but_old_token_is_not_usable() {
    let (mut f, token, _) = claimed();
    let q = request(SessionPurposeV1::Execution, "rotation");
    let (body, receipt) = signed(&f, &q, &token, 12_000);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("rotation-A", &body, &receipt),
            UnixMillis::new(12_000),
        )
        .unwrap();
    f.state
        .issue_test_session_tool_nonce(
            proof("rotation-A", &body, &receipt),
            &URL_SAFE_NO_PAD.encode([126; 32]),
            UnixMillis::new(12_001),
        )
        .unwrap();
    let new_token = URL_SAFE_NO_PAD.encode([127; 32]);
    rotate_execution(&mut f, &new_token);
    assert_eq!(
        f.state.test_sessions()[ID].roots[&q.purpose]
            .credential
            .as_ref()
            .unwrap()
            .generation,
        DecimalU64(2)
    );
    assert_eq!(
        f.state.test_sessions()[ID].tool_nonce_links[&q.request_id].credential_generation,
        DecimalU64(1)
    );
    restore(&f.state);
    let before = f.state.clone();
    assert!(
        f.state
            .charge_test_session_tool_nonce_http_attempt(
                proof("rotation-old-B", &body, &receipt),
                UnixMillis::new(13_005)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let (new_body, new_receipt) = signed(&f, &q, &new_token, 13_005);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("rotation-new-B", &new_body, &new_receipt),
            UnixMillis::new(13_005),
        )
        .unwrap();
    let paid = f.state.clone();
    assert!(
        f.state
            .issue_test_session_tool_nonce(
                proof("rotation-new-B", &new_body, &new_receipt),
                &URL_SAFE_NO_PAD.encode([128; 32]),
                UnixMillis::new(13_005)
            )
            .is_err()
    );
    assert_eq!(
        f.state, paid,
        "generation replacement cannot resurrect or overwrite a retired nonce"
    );
    restore(&f.state);
}
