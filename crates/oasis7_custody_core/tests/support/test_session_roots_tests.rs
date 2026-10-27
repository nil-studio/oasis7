use super::*;
const ID: &str = "session-one";
fn control_source(
    f: &mut SessionFixture,
    source_id: &str,
    nonce: &str,
    now: u64,
) -> SignedManagedNativeSessionLifetimeV1 {
    funded_source(
        f,
        SessionSourceFundingV1::Control,
        SessionPurposeV1::Execution,
        source_id,
        nonce,
        now,
    )
}
fn funded_source(
    f: &mut SessionFixture,
    funding: SessionSourceFundingV1,
    purpose: SessionPurposeV1,
    source_id: &str,
    nonce: &str,
    now: u64,
) -> SignedManagedNativeSessionLifetimeV1 {
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    f.state
        .reserve_test_session_source(
            ID,
            SessionSourceReservation {
                operation_id: source_id,
                nonce,
                funding,
            },
            UnixMillis::new(now),
        )
        .unwrap();
    f.state
        .reserve_test_session_source_read(ID, source_id, UnixMillis::new(now))
        .unwrap();
    f.state
        .claim_test_session_source(ID, source_id, &current, UnixMillis::new(now))
        .unwrap();
    let mut v = verification(f.state.test_session_original(ID).unwrap(), &f.context).verification;
    v.purpose = VerificationPurposeV1::InspectOrigin;
    v.verify_nonce = nonce.into();
    v.observed_at = DecimalU64(now + 1);
    v.proof_state = if now + 1 >= 38_000 {
        ProofStateV1::Expired
    } else {
        ProofStateV1::Active
    };
    f.state
        .complete_test_session_source(
            ID,
            source_id,
            sign_verification(v),
            &current,
            UnixMillis::new(now + 1),
            &mut StrictCrypto,
        )
        .unwrap();
    f.state
        .reserve_test_session_source_read(ID, source_id, UnixMillis::new(now + 2))
        .unwrap();
    let d = f
        .state
        .test_session_live_descriptor(ID, purpose, source_id)
        .unwrap();
    super::lifetime(&f.state, d.lifetime_nonce, now + 2)
}
fn http(
    f: &SessionFixture,
    body: &[u8],
    target: (HttpPopProfileV1, HttpPopRoute),
    request_id: &str,
    nonce: Option<&str>,
    token: Option<&str>,
    now: u64,
) -> HttpPossessionReceipt {
    let (profile, route) = target;
    let descriptor = &f.fact.registration.request.descriptor;
    let target = CanonicalHttpTarget::new("https://custody.example", route).unwrap();
    let created = UnixMillis::new(now).floor_seconds();
    let expires = UnixSeconds::new(created.get() + 20);
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile,
        target: &target,
        key_id: &f.fact.registration.key_id,
        binding_id: ID,
        request_id,
        nonce,
        token,
        created,
        expires,
        body,
    })
    .unwrap();
    let headers = material.signed_headers(
        &SigningKey::from_bytes(&[8; 32])
            .sign(material.signature_base())
            .to_bytes(),
    );
    let raw = headers
        .iter()
        .map(|h| HttpHeader {
            name: &h.name,
            value: &h.value,
        })
        .collect::<Vec<_>>();
    let digest: [u8; 32] = Sha256::digest(token.unwrap_or("").as_bytes()).into();
    let prepared = prepare_http_pop(
        &HttpPopMessage {
            method: target.method(),
            target_uri: target.uri(),
            headers: &raw,
            representation: HttpHeaderRepresentation::RawInstances,
            body,
        },
        &HttpPopBinding {
            profile,
            target: &target,
            key_id: &f.fact.registration.key_id,
            public_key_hex: &descriptor.client_public_key,
            binding_id: ID,
            request_id,
            nonce,
            credential_digest: &digest,
            valid_until: UnixMillis::new(50_000),
        },
        UnixMillis::new(now),
    )
    .unwrap();
    verify_http_possession(prepared, UnixMillis::new(now), &mut StrictCrypto).unwrap()
}
fn claimed() -> (SessionFixture, String, String) {
    let mut f = super::activated(8005);
    let life = control_source(&mut f, "claim-source", &"a1".repeat(32), 9000);
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    let ns = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &current,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "claim-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(9002),
    };
    let claim = ClientCredentialClaimRequestV1 {
        api_version: 1,
        enrollment_id: f.fact.registration.enrollment_id.clone(),
        test_session_id: ID.into(),
        request_id: "claim-one".into(),
        purposes: vec![TestSessionPurposeV1::Execute, TestSessionPurposeV1::Read],
        replace: false,
        expected_generation: None,
    };
    let body = serde_json::to_vec(&claim).unwrap();
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "claim-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::CredentialClaim,
        logical_request_id: claim.request_id.clone(),
        operation_request_digest: claim.test_session_digest().unwrap(),
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
        9003,
    );
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: proof.request_id(),
                possession: &proof,
            },
            UnixMillis::new(9003),
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
            &URL_SAFE_NO_PAD.encode([3; 32]),
            &live,
            UnixMillis::new(9003),
            &mut StrictCrypto,
        )
        .unwrap();
    let execute = URL_SAFE_NO_PAD.encode([7; 32]);
    let read = URL_SAFE_NO_PAD.encode([9; 32]);
    let hex = |token: &str| {
        Sha256::digest(token.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let allocations = [
        SessionCredentialAllocation {
            purpose: SessionPurposeV1::Execution,
            digest: hex(&execute),
        },
        SessionCredentialAllocation {
            purpose: SessionPurposeV1::ReconcileRead,
            digest: hex(&read),
        },
    ];
    let proof = http(
        &f,
        &body,
        (
            HttpPopProfileV1::CredentialClaim,
            HttpPopRoute::EnrollmentClaim(f.fact.registration.enrollment_id.clone()),
        ),
        &claim.request_id,
        Some(&challenge.nonce),
        None,
        9004,
    );
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: proof.request_id(),
                possession: &proof,
            },
            UnixMillis::new(9004),
        )
        .unwrap();
    let outcome = f
        .state
        .claim_test_session_credentials(
            ID,
            SessionCredentialClaim {
                allocations: &allocations,
                proof: SessionProofRequest {
                    ingress_attempt_id: proof.request_id(),
                    original_bytes: &body,
                    possession: &proof,
                },
            },
            &live,
            UnixMillis::new(9004),
            &mut StrictCrypto,
        )
        .unwrap();
    assert!(outcome.newly_claimed);
    assert_eq!(outcome.roots.len(), 2);
    assert_eq!(f.state.test_sessions()[ID].control_logicals.len(), 2);
    assert_eq!(f.state.test_sessions()[ID].control_transports.len(), 2);
    let snapshot = f.state.clone();
    let repeated = f
        .state
        .claim_test_session_credentials(
            ID,
            SessionCredentialClaim {
                allocations: &allocations,
                proof: SessionProofRequest {
                    ingress_attempt_id: proof.request_id(),
                    original_bytes: &body,
                    possession: &proof,
                },
            },
            &live,
            UnixMillis::new(9004),
            &mut StrictCrypto,
        )
        .unwrap();
    assert!(!repeated.newly_claimed);
    assert_eq!(f.state, snapshot);
    assert!(
        f.state.test_sessions()[ID].roots[&SessionPurposeV1::TerminationPlan]
            .credential
            .is_none()
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .unwrap(),
        f.state
    );
    (f, execute, read)
}
#[test]
fn test_session_claim_empty_token_is_once_private_and_cleanup_is_never_client_credential() {
    let _ = claimed();
}
#[test]
fn test_session_heartbeat_challenge_and_call_share_one_logical_two_http_and_absolute_lease() {
    let (mut f, execute, _) = claimed();
    let life = control_source(&mut f, "heartbeat-source", &"a2".repeat(32), 10_000);
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    let ns = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &current,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "heartbeat-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(10_002),
    };
    let command = SessionControlRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "heartbeat-one".into(),
        kind: SessionControlKindV1::Heartbeat,
    };
    let bytes = serde_json::to_vec(&command).unwrap();
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "heartbeat-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::Control(command.kind),
        logical_request_id: command.request_id.clone(),
        operation_request_digest: command.digest().unwrap(),
    };
    let qbytes = serde_json::to_vec(&q).unwrap();
    let before = f.state.test_sessions()[ID].control_logicals.len();
    let hops = f.state.test_sessions()[ID].control_transports.len();
    let proof = http(
        &f,
        &qbytes,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &q.request_id,
        None,
        Some(&execute),
        10_003,
    );
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: proof.request_id(),
                possession: &proof,
            },
            UnixMillis::new(10_003),
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
            &URL_SAFE_NO_PAD.encode([4; 32]),
            &live,
            UnixMillis::new(10_003),
            &mut StrictCrypto,
        )
        .unwrap();
    let proof = http(
        &f,
        &bytes,
        (
            HttpPopProfileV1::Custody,
            HttpPopRoute::Heartbeat(ID.into()),
        ),
        &command.request_id,
        Some(&challenge.nonce),
        Some(&execute),
        10_004,
    );
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: proof.request_id(),
                possession: &proof,
            },
            UnixMillis::new(10_004),
        )
        .unwrap();
    let ack = f
        .state
        .control_test_session(
            SessionProofRequest {
                ingress_attempt_id: proof.request_id(),
                original_bytes: &bytes,
                possession: &proof,
            },
            Some(&live),
            UnixMillis::new(10_004),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(ack.lease_until, UnixMillis::new(50_000));
    assert_eq!(
        f.state.test_sessions()[ID].control_logicals.len(),
        before + 1
    );
    assert_eq!(
        f.state.test_sessions()[ID].control_transports.len(),
        hops + 2
    );
    let snapshot = f.state.clone();
    let repeat = f
        .state
        .control_test_session(
            SessionProofRequest {
                ingress_attempt_id: proof.request_id(),
                original_bytes: &bytes,
                possession: &proof,
            },
            Some(&live),
            UnixMillis::new(10_004),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(repeat, ack);
    assert_eq!(f.state, snapshot);
    f.state
        .expire_test_sessions(UnixMillis::new(50_000))
        .unwrap();
    assert_eq!(
        f.state.test_sessions()[ID].closing.as_ref().unwrap().0,
        TestSessionClosingReasonV1::Deadline
    );
    assert_eq!(
        f.state.test_sessions()[ID].roots[&SessionPurposeV1::ReconcileRead].deadline,
        UnixMillis::new(80_000)
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .unwrap(),
        f.state
    );
}

#[test]
fn test_session_possession_precedes_slot_rate_budget_and_eight_pending_bound() {
    let (mut f, execute, _) = claimed();
    let life = control_source(&mut f, "slots-source", &"a3".repeat(32), 11_000);
    let policy = Policy::new(f.state.draft());
    let p = policy.current(&f.context);
    let ns = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &p,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "slots-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(11_002),
    };
    let request = |i: usize| {
        let command = SessionControlRequestV1 {
            api_version: 1,
            session_id: ID.into(),
            request_id: format!("heartbeat-{i}"),
            kind: SessionControlKindV1::Heartbeat,
        };
        SessionChallengeRequestV1 {
            api_version: 1,
            session_id: ID.into(),
            request_id: format!("challenge-{i}"),
            purpose: SessionPurposeV1::Execution,
            kind: SessionChallengeKindV1::Control(command.kind),
            logical_request_id: command.request_id.clone(),
            operation_request_digest: command.digest().unwrap(),
        }
    };
    let q = request(0);
    let bytes = serde_json::to_vec(&q).unwrap();
    let unknown = URL_SAFE_NO_PAD.encode([55; 32]);
    let proof = http(
        &f,
        &bytes,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &q.request_id,
        None,
        Some(&unknown),
        11_003,
    );
    let before = f.state.clone();
    assert!(
        f.state
            .charge_test_session_control_http_attempt(
                ID,
                SessionHttpAdmission {
                    ingress_attempt_id: proof.request_id(),
                    possession: &proof
                },
                UnixMillis::new(11_003)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    for i in 0..8 {
        let now = 11_003 + (i / 2) as u64 * 1000;
        let q = request(i);
        let bytes = serde_json::to_vec(&q).unwrap();
        let proof = http(
            &f,
            &bytes,
            (
                HttpPopProfileV1::CustodyChallenge,
                HttpPopRoute::ProofChallenges,
            ),
            &q.request_id,
            None,
            Some(&execute),
            now,
        );
        f.state
            .charge_test_session_control_http_attempt(
                ID,
                SessionHttpAdmission {
                    ingress_attempt_id: proof.request_id(),
                    possession: &proof,
                },
                UnixMillis::new(now),
            )
            .unwrap();
        f.state
            .issue_test_session_challenge(
                SessionProofRequest {
                    ingress_attempt_id: proof.request_id(),
                    original_bytes: &bytes,
                    possession: &proof,
                },
                &URL_SAFE_NO_PAD.encode([i as u8 + 12; 32]),
                &live,
                UnixMillis::new(now),
                &mut StrictCrypto,
            )
            .unwrap();
        if i == 1 {
            let q = request(99);
            let bytes = serde_json::to_vec(&q).unwrap();
            let proof = http(
                &f,
                &bytes,
                (
                    HttpPopProfileV1::CustodyChallenge,
                    HttpPopRoute::ProofChallenges,
                ),
                &q.request_id,
                None,
                Some(&execute),
                now,
            );
            f.state
                .charge_test_session_control_http_attempt(
                    ID,
                    SessionHttpAdmission {
                        ingress_attempt_id: proof.request_id(),
                        possession: &proof,
                    },
                    UnixMillis::new(now),
                )
                .unwrap();
            let paid = f.state.clone();
            assert!(
                f.state
                    .issue_test_session_challenge(
                        SessionProofRequest {
                            ingress_attempt_id: proof.request_id(),
                            original_bytes: &bytes,
                            possession: &proof
                        },
                        &URL_SAFE_NO_PAD.encode([99; 32]),
                        &live,
                        UnixMillis::new(now),
                        &mut StrictCrypto
                    )
                    .is_err()
            );
            assert_eq!(f.state, paid)
        }
    }
    let q = request(8);
    let bytes = serde_json::to_vec(&q).unwrap();
    let proof = http(
        &f,
        &bytes,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &q.request_id,
        None,
        Some(&execute),
        15_003,
    );
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: proof.request_id(),
                possession: &proof,
            },
            UnixMillis::new(15_003),
        )
        .unwrap();
    let paid = f.state.clone();
    assert!(
        f.state
            .issue_test_session_challenge(
                SessionProofRequest {
                    ingress_attempt_id: proof.request_id(),
                    original_bytes: &bytes,
                    possession: &proof
                },
                &URL_SAFE_NO_PAD.encode([22; 32]),
                &live,
                UnixMillis::new(15_003),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, paid);
    assert_eq!(
        f.state.test_sessions()[ID]
            .challenges
            .values()
            .filter(|c| c.consumed_by.is_none())
            .count(),
        8
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .unwrap(),
        f.state
    );
    let mut forged = serde_json::to_value(&f.state).unwrap();
    forged["test_sessions"][ID]["source_funding"]["slots-source"] = "termination_plan".into();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(forged).is_err());
}

#[path = "test_session_termination_tests.rs"]
mod termination;

#[path = "test_session_credential_tests.rs"]
mod credentials;

#[path = "test_session_http_admission_tests.rs"]
mod http_admission;
