//! Genuine V2 activation, real Ed25519 HTTP PoP and Native signatures.
//! Fixture service keys are not evidence of an actual Native/Cloud deployment.
use super::*;
use oasis7_custody_core::http_pop::*;
use sha2::{Digest, Sha256};

fn http(
    f: &SessionFixture,
    body: &[u8],
    route: HttpPopRoute,
    profile: HttpPopProfileV1,
    request_id: &str,
    nonce: Option<&str>,
    now: u64,
) -> HttpPossessionReceipt {
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
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile,
        target: &target,
        key_id: &f.fact.registration.key_id,
        binding_id: &f.session_id,
        request_id,
        nonce,
        token: None,
        created,
        expires: UnixSeconds::new(created.get() + 20),
        body,
    })
    .unwrap();
    let signed = material.signed_headers(
        &SigningKey::from_bytes(&[8; 32])
            .sign(material.signature_base())
            .to_bytes(),
    );
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
        purpose: SessionPurposeV1::Execution,
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
            &URL_SAFE_NO_PAD.encode([salt; 32]),
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
    let nonce = URL_SAFE_NO_PAD.encode([31; 32]);
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
