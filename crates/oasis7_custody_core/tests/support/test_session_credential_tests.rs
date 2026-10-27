use super::*;
#[test]
fn test_session_replacement_cannot_reuse_another_purpose_token_digest() {
    let (mut f, _, read) = claimed();
    let life = control_source(&mut f, "replacement-source", &"c1".repeat(32), 12_000);
    let policy = Policy::new(f.state.draft());
    let p = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = TestSessionLiveCurrent {
        provision: &p,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "replacement-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(12_002),
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
        &f,
        &qbytes,
        (
            HttpPopProfileV1::ClaimChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &q.request_id,
        None,
        None,
        12_003,
    );
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: proof.request_id(),
                possession: &proof,
            },
            UnixMillis::new(12_003),
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
            UnixMillis::new(12_003),
            &mut StrictCrypto,
        )
        .unwrap();
    let proof = http(
        &f,
        &body,
        (
            HttpPopProfileV1::CredentialClaim,
            HttpPopRoute::EnrollmentClaim(f.fact.registration.enrollment_id.clone()),
        ),
        &request.request_id,
        Some(&challenge.nonce),
        None,
        12_004,
    );
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: proof.request_id(),
                possession: &proof,
            },
            UnixMillis::new(12_004),
        )
        .unwrap();
    // Every proof/current/nonce/expected generation is valid; only the factory's
    // proposed token collides with the independently approved Read purpose.
    let allocation = [SessionCredentialAllocation {
        purpose: SessionPurposeV1::Execution,
        digest: Sha256::digest(read.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    }];
    let before = f.state.clone();
    assert!(
        f.state
            .claim_test_session_credentials(
                ID,
                SessionCredentialClaim {
                    allocations: &allocation,
                    proof: SessionProofRequest {
                        ingress_attempt_id: proof.request_id(),
                        original_bytes: &body,
                        possession: &proof
                    }
                },
                &current,
                UnixMillis::new(12_004),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
}

#[test]
fn test_session_replacement_cannot_reactivate_a_retired_generation_token_digest() {
    let (mut f, execution, _) = claimed();
    let life = control_source(&mut f, "replacement-source", &"c2".repeat(32), 12_000);
    let replacement = URL_SAFE_NO_PAD.encode([17; 32]);
    // Both claims have fresh, authentic same-key PoP and exact expected generations.
    // The second factory allocation alone tries to make the retired E1 token live again.
    for (generation, token, now) in [
        (1, replacement.as_str(), 12_002),
        (2, execution.as_str(), 13_002),
    ] {
        let policy = Policy::new(f.state.draft());
        let p = policy.current(&f.context);
        let ns = scope(&f.pin);
        let current = TestSessionLiveCurrent {
            provision: &p,
            native_scope: &ns,
            lifetime: &life,
            private_sid_generation: DecimalU64(1),
            source_operation_id: "replacement-source",
            source_ticket: 2,
            observed_at: UnixMillis::new(now),
        };
        let request = ClientCredentialClaimRequestV1 {
            api_version: 1,
            enrollment_id: f.fact.registration.enrollment_id.clone(),
            test_session_id: ID.into(),
            request_id: format!("replace-execution-{generation}"),
            purposes: vec![TestSessionPurposeV1::Execute],
            replace: true,
            expected_generation: Some(DecimalU64(generation)),
        };
        let body = serde_json::to_vec(&request).unwrap();
        let q = SessionChallengeRequestV1 {
            api_version: 1,
            session_id: ID.into(),
            request_id: format!("replace-challenge-{generation}"),
            purpose: SessionPurposeV1::Execution,
            kind: SessionChallengeKindV1::CredentialClaim,
            logical_request_id: request.request_id.clone(),
            operation_request_digest: request.test_session_digest().unwrap(),
        };
        let qbytes = serde_json::to_vec(&q).unwrap();
        let proof = http(
            &f,
            &qbytes,
            (
                HttpPopProfileV1::ClaimChallenge,
                HttpPopRoute::ProofChallenges,
            ),
            &q.request_id,
            None,
            None,
            now + 1,
        );
        f.state
            .charge_test_session_control_http_attempt(
                ID,
                SessionHttpAdmission {
                    ingress_attempt_id: proof.request_id(),
                    possession: &proof,
                },
                UnixMillis::new(now + 1),
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
                &URL_SAFE_NO_PAD.encode([36 + generation as u8; 32]),
                &current,
                UnixMillis::new(now + 1),
                &mut StrictCrypto,
            )
            .unwrap();
        let proof = http(
            &f,
            &body,
            (
                HttpPopProfileV1::CredentialClaim,
                HttpPopRoute::EnrollmentClaim(f.fact.registration.enrollment_id.clone()),
            ),
            &request.request_id,
            Some(&challenge.nonce),
            None,
            now + 2,
        );
        f.state
            .charge_test_session_control_http_attempt(
                ID,
                SessionHttpAdmission {
                    ingress_attempt_id: proof.request_id(),
                    possession: &proof,
                },
                UnixMillis::new(now + 2),
            )
            .unwrap();
        let allocation = [SessionCredentialAllocation {
            purpose: SessionPurposeV1::Execution,
            digest: Sha256::digest(token.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        }];
        let before = f.state.clone();
        let result = f.state.claim_test_session_credentials(
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
            UnixMillis::new(now + 2),
            &mut StrictCrypto,
        );
        if generation == 1 {
            result.unwrap();
            let original = ClientCredentialClaimRequestV1 {
                api_version: 1,
                enrollment_id: f.fact.registration.enrollment_id.clone(),
                test_session_id: ID.into(),
                request_id: "claim-one".into(),
                purposes: vec![TestSessionPurposeV1::Execute, TestSessionPurposeV1::Read],
                replace: false,
                expected_generation: None,
            };
            let original_bytes = serde_json::to_vec(&original).unwrap();
            let retry = http(
                &f,
                &original_bytes,
                (
                    HttpPopProfileV1::CredentialClaim,
                    HttpPopRoute::EnrollmentClaim(f.fact.registration.enrollment_id.clone()),
                ),
                "claim-one",
                Some(&URL_SAFE_NO_PAD.encode([3; 32])),
                None,
                now + 3,
            );
            f.state
                .charge_test_session_control_http_attempt(
                    ID,
                    SessionHttpAdmission {
                        ingress_attempt_id: "fixture-history-retry",
                        possession: &retry,
                    },
                    UnixMillis::new(now + 3),
                )
                .unwrap();
            let before_retry = f.state.clone();
            let outcome = f
                .state
                .claim_test_session_credentials(
                    ID,
                    SessionCredentialClaim {
                        allocations: &[],
                        proof: SessionProofRequest {
                            ingress_attempt_id: "fixture-history-retry",
                            original_bytes: &original_bytes,
                            possession: &retry,
                        },
                    },
                    &current,
                    UnixMillis::new(now + 3),
                    &mut StrictCrypto,
                )
                .unwrap();
            assert!(!outcome.newly_claimed);
            assert!(outcome.roots.iter().all(|(_, g)| g.0 == 1));
            assert_eq!(
                f.state, before_retry,
                "historical retry must not restore E1 as current"
            );
            let snapshot = serde_json::to_value(&f.state).unwrap();
            assert_eq!(
                serde_json::from_value::<AccountProvisionApprovalState>(snapshot.clone()).unwrap(),
                f.state
            );
            let mut forged = snapshot;
            forged["test_sessions"][ID]["roots"]["execution"]["credential"] =
                forged["test_sessions"][ID]["roots"]["execution"]["credential_history"][0].clone();
            assert!(serde_json::from_value::<AccountProvisionApprovalState>(forged).is_err());
        } else {
            assert!(
                result.is_err(),
                "retired E1 token must never become live as E3"
            );
            assert_eq!(f.state, before);
        }
    }
}

#[test]
fn test_session_credential_history_restore_requires_complete_originals_and_unique_digests() {
    let (f, _, _) = claimed();
    let original = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(original.clone()).unwrap(),
        f.state
    );
    for mutation in 0..4 {
        let mut forged = original.clone();
        let root = &mut forged["test_sessions"][ID]["roots"]["execution"];
        match mutation {
            0 => {
                root.as_object_mut().unwrap().remove("credential_history");
            }
            1 => root["credential_history"] = serde_json::json!([]),
            2 => {
                root["credential_history"][0]["claim_request"]["purposes"] =
                    serde_json::json!(["execute"])
            }
            _ => root["credential_history"][0]["generation"] = "2".into(),
        }
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(forged).is_err());
    }
}
