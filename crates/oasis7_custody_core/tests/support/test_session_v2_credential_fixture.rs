//! Genuine V2 activation, real Ed25519 HTTP PoP and Native signatures.
//! Fixture service keys are not evidence of an actual Native/Cloud deployment.
use super::*;
use oasis7_custody_core::http_pop::*;
use sha2::{Digest, Sha256};

fn public_challenge_nonce(salt: u8) -> String {
    let mut hash = Sha256::new();
    hash.update(b"oasis7.fixture.test-session-v2.public-challenge-nonce.v1");
    hash.update([salt]);
    let nonce = URL_SAFE_NO_PAD.encode(hash.finalize());
    assert!(nonce != URL_SAFE_NO_PAD.encode([salt; 32]));
    nonce
}

fn http(
    f: &SessionFixture,
    body: &[u8],
    route: HttpPopRoute,
    profile: HttpPopProfileV1,
    request_id: &str,
    nonce: Option<&str>,
    now: u64,
) -> HttpPossessionReceipt {
    let saved = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    let CustodyIntentV1::TestSessionV2 { plan, scope } = &saved.input().request.intent else {
        panic!("actual V2 session input required");
    };
    let host = SigningKey::from_bytes(&[8; 32]);
    let public: String = host
        .verifying_key()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert!(public == scope.client_pairing.client_public_key);
    assert!(public == f.fact.registration.request.descriptor.client_public_key);
    assert!(scope.client_pairing.client_key_id == f.fact.registration.key_id);
    let (purpose, challenge) = if let Some(nonce) = nonce {
        let value = serde_json::to_value(saved).unwrap();
        let selected = value["challenges"]
            .as_object()
            .unwrap()
            .values()
            .map(|v| serde_json::from_value::<SessionChallengeV1>(v.clone()).unwrap())
            .find(|c| c.nonce == nonce)
            .expect("actual recorded nonce");
        (selected.purpose, Some(selected))
    } else {
        let q: SessionChallengeRequestV1 = serde_json::from_slice(body).unwrap();
        (q.purpose, None)
    };
    let mut until = match purpose {
        SessionPurposeV1::Execution => plan.write_deadline.0,
        SessionPurposeV1::ReconcileRead => plan.read_until.0,
        SessionPurposeV1::TerminationPlan => panic!("no public termination fixture"),
    };
    // A consumed nonce is an authenticated history reference, not a new grant.
    // Only an unconsumed challenge adds its own earlier proof expiry bound.
    if let Some(c) = challenge.filter(|c| c.consumed_by.is_none()) {
        until = until.min(c.expires_at.get());
    }
    let target = CanonicalHttpTarget::new(
        "https://custody.example",
        HttpPopRoute::ScopedClient {
            custody_account_id: f.state.draft().entry.custody_account_id.clone(),
            provision_id: f.state.draft().entry.provision_id.clone(),
            route: Box::new(route),
        },
    )
    .unwrap();
    let created = UnixMillis::new(now).floor_seconds();
    let expires = UnixSeconds::new((created.get() + 20).min(until / 1000));
    assert!(expires.checked_millis().unwrap().get() > now);
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile,
        target: &target,
        key_id: &scope.client_pairing.client_key_id,
        binding_id: &f.session_id,
        request_id,
        nonce,
        token: None,
        created,
        expires,
        body,
    })
    .unwrap();
    let signed = material.signed_headers(&host.sign(material.signature_base()).to_bytes());
    let request = SessionClientHttpRequestV1 {
        session_id: f.session_id.clone(),
        method: target.method().into(),
        target_uri: target.uri().into(),
        headers: signed
            .into_iter()
            .map(|h| SessionClientHeaderV1 {
                name: h.name,
                value: h.value,
            })
            .collect(),
        body: std::str::from_utf8(body).unwrap().into(),
        nonce_hint: nonce.map(str::to_owned),
    };
    let context = SessionClientHttpContext {
        public_origin: "https://custody.example",
        custody_account_id: &f.state.draft().entry.custody_account_id,
        provision_id: &f.state.draft().entry.provision_id,
        operation: if profile == HttpPopProfileV1::ClaimChallenge {
            SessionClientOperation::Challenge
        } else {
            SessionClientOperation::CredentialClaim
        },
        representation: HttpHeaderRepresentation::RawInstances,
    };
    let prepared = f
        .state
        .prepare_test_session_v2_credential_http(&request, &context, UnixMillis::new(now))
        .unwrap();
    verify_http_possession(prepared, UnixMillis::new(now), &mut StrictCrypto).unwrap()
}

fn paid(f: &mut SessionFixture, proof: &HttpPossessionReceipt, ingress: &str, now: u64) {
    f.state
        .charge_test_session_v2_claim_http_attempt(
            &f.session_id,
            SessionHttpAdmission {
                ingress_attempt_id: ingress,
                possession: proof,
            },
            UnixMillis::new(now),
        )
        .unwrap();
}

fn current_source(
    f: &mut SessionFixture,
    proof: &HttpPossessionReceipt,
    ingress: &str,
    source_id: &str,
    now: u64,
) -> SignedManagedNativeSessionLifetimeV1 {
    let nonce = oasis7_custody_core::approval::bytes_digest(source_id.as_bytes());
    f.state
        .reserve_test_session_v2_source(
            &f.session_id,
            SessionSourceReservation {
                operation_id: source_id,
                nonce: &nonce,
                funding: SessionSourceFundingV1::Control,
            },
            UnixMillis::new(now),
        )
        .unwrap();
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(&f.session_id, source_id, UnixMillis::new(now))
            .unwrap(),
        1
    );
    f.state
        .claim_test_session_v2_source(&f.session_id, source_id, &c, UnixMillis::new(now))
        .unwrap();
    let mut v = verification(
        f.state.test_session_v2_original(&f.session_id).unwrap(),
        &f.context,
    )
    .verification;
    v.purpose = VerificationPurposeV1::InspectOrigin;
    v.verify_nonce = nonce;
    v.observed_at = DecimalU64(now);
    v.proof_state = ProofStateV1::Active;
    f.state
        .complete_test_session_v2_source(
            &f.session_id,
            source_id,
            sign_verification(v),
            &c,
            UnixMillis::new(now),
            &mut StrictCrypto,
        )
        .unwrap();
    for ticket in [2, 3] {
        assert_eq!(
            f.state
                .reserve_test_session_v2_source_read(
                    &f.session_id,
                    source_id,
                    UnixMillis::new(now + 1)
                )
                .unwrap(),
            ticket
        );
    }
    let nonce = f
        .state
        .claim_test_session_v2_credential_lifetime(
            &f.session_id,
            source_id,
            SessionHttpAdmission {
                ingress_attempt_id: ingress,
                possession: proof,
            },
            &c,
            UnixMillis::new(now + 1),
            &mut StrictCrypto,
        )
        .unwrap();
    // A paid dispatch claim is not a second Native HTTP permit after unknown.
    let before = f.state.clone();
    assert!(
        f.state
            .claim_test_session_v2_credential_lifetime(
                &f.session_id,
                source_id,
                SessionHttpAdmission {
                    ingress_attempt_id: ingress,
                    possession: proof
                },
                &c,
                UnixMillis::new(now + 1),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let status = fixture::root_session(&f.state, nonce, now + 1)
        .status
        .status;
    let lifetime = oasis7_custody_core::native_binding::ManagedNativeSessionLifetimeV1 {
        api_version: 1,
        status,
        session_expires_at: DecimalU64(90_000),
        session_idle_expires_at: DecimalU64(90_000),
    };
    let signed = SignedManagedNativeSessionLifetimeV1 {
        signature: URL_SAFE_NO_PAD
            .encode(key().sign(&lifetime.signing_bytes().unwrap()).to_bytes()),
        key_id: "native-key".into(),
        lifetime,
    };
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(&f.session_id, source_id, UnixMillis::new(now + 2))
            .unwrap(),
        4
    );
    signed
}

fn claim(
    f: &mut SessionFixture,
    request: &ClientCredentialClaimRequestV1,
    proposed: &[SessionCredentialAllocation],
    salt: u8,
    now: u64,
) -> std::result::Result<SessionCredentialClaimOutcome, ProvisionApprovalError> {
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
    let bytes = serde_json::to_vec(request).unwrap();
    let pop = http(
        f,
        &bytes,
        HttpPopRoute::EnrollmentClaim(f.fact.registration.enrollment_id.clone()),
        HttpPopProfileV1::CredentialClaim,
        &request.request_id,
        Some(&challenge.nonce),
        now + 3,
    );
    let ingress = format!("claim-ingress-{salt}");
    paid(f, &pop, &ingress, now + 3);
    let source = format!("claim-source-{salt}");
    let life = current_source(f, &pop, &ingress, &source, now + 3);
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
        observed_at: UnixMillis::new(now + 3),
    };
    let before = f.state.clone();
    let result = f.state.claim_test_session_v2_credentials(
        &f.session_id,
        SessionCredentialClaim {
            allocations: proposed,
            proof: SessionProofRequest {
                ingress_attempt_id: &ingress,
                original_bytes: &bytes,
                possession: &pop,
            },
        },
        &current,
        UnixMillis::new(now + 5),
        &mut StrictCrypto,
    );
    if result.is_err() {
        assert_eq!(f.state, before);
    }
    result
}

fn request(
    f: &SessionFixture,
    id: &str,
    generation: Option<u64>,
) -> ClientCredentialClaimRequestV1 {
    ClientCredentialClaimRequestV1 {
        api_version: 1,
        enrollment_id: f.fact.registration.enrollment_id.clone(),
        test_session_id: f.session_id.clone(),
        request_id: id.into(),
        purposes: vec![TestSessionPurposeV1::Execute],
        replace: generation.is_some(),
        expected_generation: generation.map(DecimalU64),
    }
}
fn allocation(byte: u8) -> SessionCredentialAllocation {
    let token = URL_SAFE_NO_PAD.encode([byte; 32]);
    assert_eq!(token.len(), 43);
    SessionCredentialAllocation {
        purpose: SessionPurposeV1::Execution,
        digest: Sha256::digest(token.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    }
}

#[test]
fn session_v2_real_claim_and_replacement_preserve_retired_history() {
    let mut f = activation_tests::activated(9000);
    let first = request(&f, "claim-one", None);
    assert!(
        claim(&mut f, &first, &[allocation(31)], 31, 12_000)
            .unwrap()
            .newly_claimed
    );
    let second = request(&f, "claim-two", Some(1));
    assert!(
        claim(&mut f, &second, &[allocation(32)], 32, 13_000)
            .unwrap()
            .newly_claimed
    );
    let body = serde_json::to_vec(&first).unwrap();
    let nonce = public_challenge_nonce(31);
    let pop = http(
        &f,
        &body,
        HttpPopRoute::EnrollmentClaim(f.fact.registration.enrollment_id.clone()),
        HttpPopProfileV1::CredentialClaim,
        &first.request_id,
        Some(&nonce),
        13_100,
    );
    paid(&mut f, &pop, "history-ingress", 13_100);
    let before = f.state.clone();
    let history = f
        .state
        .test_session_v2_credential_claim_history(
            &f.session_id,
            &SessionProofRequest {
                ingress_attempt_id: "history-ingress",
                original_bytes: &body,
                possession: &pop,
            },
            UnixMillis::new(13_100),
        )
        .unwrap()
        .unwrap();
    assert!(!history.newly_claimed);
    assert_eq!(
        history.roots,
        vec![(SessionPurposeV1::Execution, DecimalU64(1))]
    );
    assert_eq!(f.state, before);
    let retired = request(&f, "claim-three", Some(2));
    assert!(claim(&mut f, &retired, &[allocation(31)], 33, 14_000).is_err());
    let bytes = serde_json::to_vec(&f.state).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, f.state);
    assert!(
        !std::str::from_utf8(&bytes)
            .unwrap()
            .contains(&URL_SAFE_NO_PAD.encode([31; 32]))
    );
    // Removing both sides is not permitted even with unchanged, balanced budgets:
    // the exact consumed challenge and complete token history still reference it.
    let mut corrupt = serde_json::to_value(&f.state).unwrap();
    corrupt["test_sessions_v2"][&f.session_id]["credential_claims"]
        .as_object_mut()
        .unwrap()
        .remove("claim-one");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(corrupt).is_err());
}

#[test]
fn session_v2_real_claim_both_purposes_then_read_replacement() {
    let mut f = activation_tests::activated(9000);
    let mut q = request(&f, "both-first", None);
    q.purposes.push(TestSessionPurposeV1::Read);
    let execute = allocation(41);
    let mut read = allocation(42);
    read.purpose = SessionPurposeV1::ReconcileRead;
    let outcome = claim(&mut f, &q, &[execute, read], 41, 12_000).unwrap();
    assert!(outcome.newly_claimed);
    assert_eq!(
        outcome.roots,
        vec![
            (SessionPurposeV1::Execution, DecimalU64(1)),
            (SessionPurposeV1::ReconcileRead, DecimalU64(1))
        ]
    );
    let mut q = request(&f, "read-replacement", Some(1));
    q.purposes = vec![TestSessionPurposeV1::Read];
    let mut colliding = allocation(41);
    colliding.purpose = SessionPurposeV1::ReconcileRead;
    assert!(claim(&mut f, &q, &[colliding], 42, 13_000).is_err());
    let mut q = request(&f, "read-replacement-valid", Some(1));
    q.purposes = vec![TestSessionPurposeV1::Read];
    let mut fresh = allocation(43);
    fresh.purpose = SessionPurposeV1::ReconcileRead;
    let outcome = claim(&mut f, &q, &[fresh], 43, 14_000).unwrap();
    assert_eq!(
        outcome.roots,
        vec![(SessionPurposeV1::ReconcileRead, DecimalU64(2))]
    );
    let snapshot = serde_json::to_vec(&f.state).unwrap();
    assert_eq!(
        serde_json::from_slice::<AccountProvisionApprovalState>(&snapshot).unwrap(),
        f.state
    );
    for byte in [41, 42, 43] {
        assert!(
            !std::str::from_utf8(&snapshot)
                .unwrap()
                .contains(&URL_SAFE_NO_PAD.encode([byte; 32]))
        );
    }
}

#[path = "test_session_v2_credential_native_history_tests.rs"]
mod native_history;

#[path = "test_session_v2_control_fixture.rs"]
mod control_tests;

#[path = "test_session_v2_control_preflight_restore_tests.rs"]
mod control_preflight_restore_tests;

#[path = "test_session_v2_execution_http_tests.rs"]
mod execution_http_tests;
