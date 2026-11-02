//! Genuine paid first-admit + Native signatures. Not live Cloud/Native qualification.
use super::*;
fn source_id(f: &SessionFixture, ingress: &str, control: bool) -> String {
    let (prefix, domain) = if control {
        ("control", "oasis7.custody.control-ingress-source.v2")
    } else {
        ("client", "oasis7.custody.client-ingress-source.v2")
    };
    format!(
        "{prefix}-v2-source-{}",
        oasis7_custody_core::approval::bytes_digest(
            &serde_json::to_vec(&(domain, &f.session_id, ingress)).unwrap()
        )
    )
}
fn cold(state: &AccountProvisionApprovalState) {
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(state).unwrap()
        )
        .unwrap(),
        *state
    );
}
fn five_source(
    f: &mut SessionFixture,
    pop: &HttpPossessionReceipt,
    ingress: &str,
    body: &[u8],
    control: bool,
    now: u64,
) -> (String, SignedManagedNativeSessionLifetimeV1) {
    let source = source_id(f, ingress, control);
    let nonce = oasis7_custody_core::approval::bytes_digest(source.as_bytes());
    let reservation = SessionSourceReservation {
        operation_id: &source,
        nonce: &nonce,
        funding: SessionSourceFundingV1::Control,
    };
    let proof = SessionProofRequest {
        ingress_attempt_id: ingress,
        original_bytes: body,
        possession: pop,
    };
    let wrapper = if control {
        f.state
            .reserve_test_session_v2_control_five_source(
                &f.session_id,
                reservation,
                proof,
                UnixMillis::new(now),
            )
            .unwrap()
    } else {
        f.state
            .reserve_test_session_v2_credential_five_source(
                &f.session_id,
                reservation,
                proof,
                UnixMillis::new(now),
            )
            .unwrap()
    };
    let value = serde_json::to_value(wrapper).unwrap();
    assert_eq!(
        value["observation_profile"],
        if control {
            "client_control_five_oracle_v2"
        } else {
            "client_credential_five_oracle_v2"
        }
    );
    assert!(value["client_admission"].is_object());
    cold(&f.state);
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(&f.session_id, &source, UnixMillis::new(now))
            .unwrap(),
        1
    );
    f.state
        .claim_test_session_v2_source(&f.session_id, &source, &current, UnixMillis::new(now))
        .unwrap();
    let mut verification = verification(
        f.state.test_session_v2_original(&f.session_id).unwrap(),
        &f.context,
    )
    .verification;
    verification.purpose = VerificationPurposeV1::InspectOrigin;
    verification.verify_nonce = nonce;
    verification.observed_at = DecimalU64(now);
    verification.proof_state = ProofStateV1::Active;
    f.state
        .complete_test_session_v2_source(
            &f.session_id,
            &source,
            sign_verification(verification),
            &current,
            UnixMillis::new(now),
            &mut StrictCrypto,
        )
        .unwrap();
    for ticket in [2, 3] {
        assert_eq!(
            f.state
                .reserve_test_session_v2_source_read(
                    &f.session_id,
                    &source,
                    UnixMillis::new(now + 1)
                )
                .unwrap(),
            ticket
        );
    }
    let before = f.state.clone();
    let proof = SessionHttpAdmission {
        ingress_attempt_id: ingress,
        possession: pop,
    };
    let premature = if control {
        f.state.claim_test_session_v2_control_lifetime(
            &f.session_id,
            &source,
            proof,
            &current,
            UnixMillis::new(now + 1),
            &mut StrictCrypto,
        )
    } else {
        f.state.claim_test_session_v2_credential_lifetime(
            &f.session_id,
            &source,
            proof,
            &current,
            UnixMillis::new(now + 1),
            &mut StrictCrypto,
        )
    };
    assert!(
        premature.is_err(),
        "the new profile must not accept the old LF ticket three"
    );
    assert_eq!(f.state, before);
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(&f.session_id, &source, UnixMillis::new(now + 1))
            .unwrap(),
        4
    );
    let proof = SessionHttpAdmission {
        ingress_attempt_id: ingress,
        possession: pop,
    };
    let nonce = if control {
        f.state.claim_test_session_v2_control_lifetime(
            &f.session_id,
            &source,
            proof,
            &current,
            UnixMillis::new(now + 1),
            &mut StrictCrypto,
        )
    } else {
        f.state.claim_test_session_v2_credential_lifetime(
            &f.session_id,
            &source,
            proof,
            &current,
            UnixMillis::new(now + 1),
            &mut StrictCrypto,
        )
    }
    .unwrap();
    let before = f.state.clone();
    let proof = SessionHttpAdmission {
        ingress_attempt_id: ingress,
        possession: pop,
    };
    let repeated = if control {
        f.state.claim_test_session_v2_control_lifetime(
            &f.session_id,
            &source,
            proof,
            &current,
            UnixMillis::new(now + 1),
            &mut StrictCrypto,
        )
    } else {
        f.state.claim_test_session_v2_credential_lifetime(
            &f.session_id,
            &source,
            proof,
            &current,
            UnixMillis::new(now + 1),
            &mut StrictCrypto,
        )
    };
    assert!(repeated.is_err());
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
            .reserve_test_session_v2_source_read(&f.session_id, &source, UnixMillis::new(now + 2))
            .unwrap(),
        5
    );
    cold(&f.state);
    (source, signed)
}
fn mutations(state: &AccountProvisionApprovalState, session: &str, source: &str, control: bool) {
    let value = serde_json::to_value(state).unwrap();
    let mut missing = value.clone();
    let wrapper = missing["test_sessions_v2"][session]["sources"][source]
        .as_object_mut()
        .unwrap();
    wrapper.remove("observation_profile");
    wrapper.remove("client_admission");
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err(),
        "a new five-ticket history cannot be downgraded to old None"
    );
    let mut wrong = value.clone();
    wrong["test_sessions_v2"][session]["sources"][source]["observation_profile"] =
        serde_json::json!(if control {
            "client_credential_five_oracle_v2"
        } else {
            "client_control_five_oracle_v2"
        });
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(wrong).is_err(),
        "the other purpose is not an interchangeable profile"
    );
    let mut partial = value.clone();
    partial["test_sessions_v2"][session]["sources"][source]
        .as_object_mut()
        .unwrap()
        .remove("client_admission");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(partial).is_err());
}
#[test]
fn session_v2_five_source_credential_paid_factory_claim_and_strict_cold() {
    let mut f = activation_tests::activated(9000);
    let now = 12_000;
    let request = request(&f, "five-claim", None);
    let challenge = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "five-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::CredentialClaim,
        logical_request_id: request.request_id.clone(),
        operation_request_digest: request.test_session_digest().unwrap(),
    };
    let body = serde_json::to_vec(&challenge).unwrap();
    let pop = http(
        &f,
        &body,
        HttpPopRoute::ProofChallenges,
        HttpPopProfileV1::ClaimChallenge,
        &challenge.request_id,
        None,
        now,
    );
    let ingress = "five-challenge-http";
    paid(&mut f, &pop, ingress, now);
    let (source, life) = five_source(&mut f, &pop, ingress, &body, false, now);
    let policy = Policy::new(f.state.draft());
    let p = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = TestSessionLiveCurrent {
        provision: &p,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: &source,
        source_ticket: 5,
        observed_at: UnixMillis::new(now),
    };
    let challenge = f
        .state
        .issue_test_session_v2_credential_challenge(
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &body,
                possession: &pop,
            },
            &public_challenge_nonce(81),
            &current,
            UnixMillis::new(now + 2),
            &mut StrictCrypto,
        )
        .unwrap();
    cold(&f.state);
    mutations(&f.state, &f.session_id, &source, false);
    let body = serde_json::to_vec(&request).unwrap();
    let pop = http(
        &f,
        &body,
        HttpPopRoute::EnrollmentClaim(f.fact.registration.enrollment_id.clone()),
        HttpPopProfileV1::CredentialClaim,
        &request.request_id,
        Some(&challenge.nonce),
        now + 3,
    );
    let ingress = "five-claim-http";
    paid(&mut f, &pop, ingress, now + 3);
    let (source, life) = five_source(&mut f, &pop, ingress, &body, false, now + 3);
    let policy = Policy::new(f.state.draft());
    let p = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = TestSessionLiveCurrent {
        provision: &p,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: &source,
        source_ticket: 5,
        observed_at: UnixMillis::new(now + 3),
    };
    assert!(
        f.state
            .claim_test_session_v2_credentials(
                &f.session_id,
                SessionCredentialClaim {
                    allocations: &[allocation(82)],
                    proof: SessionProofRequest {
                        ingress_attempt_id: ingress,
                        original_bytes: &body,
                        possession: &pop
                    }
                },
                &current,
                UnixMillis::new(now + 5),
                &mut StrictCrypto
            )
            .unwrap()
            .newly_claimed
    );
    cold(&f.state);
    let mut wrong = serde_json::to_value(&f.state).unwrap();
    wrong["test_sessions_v2"][&f.session_id]["credential_claims"][&request.request_id]["source_ticket"] =
        serde_json::json!(4);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(wrong).is_err());
    mutations(&f.state, &f.session_id, &source, false);
}
#[test]
fn session_v2_five_source_control_paid_factory_and_strict_cold() {
    let mut f = activation_tests::activated(9000);
    let request = request(&f, "five-control-root", None);
    assert!(
        claim(&mut f, &request, &[allocation(31)], 83, 12_000)
            .unwrap()
            .newly_claimed
    );
    let q = SessionControlRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "five-control".into(),
        kind: SessionControlKindV1::ToolsList,
    };
    let now = 14_000;
    let challenge = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "five-control-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::Control(q.kind),
        logical_request_id: q.request_id.clone(),
        operation_request_digest: q.digest().unwrap(),
    };
    let body = serde_json::to_vec(&challenge).unwrap();
    let pop = control_tests::control_pop(
        &f,
        &body,
        (
            HttpPopRoute::ProofChallenges,
            HttpPopProfileV1::CustodyChallenge,
        ),
        &challenge.request_id,
        None,
        SessionPurposeV1::Execution,
        now,
    );
    let ingress = "five-control-http";
    control_tests::control_paid(&mut f, &pop, ingress, now);
    let (source, life) = five_source(&mut f, &pop, ingress, &body, true, now);
    let policy = Policy::new(f.state.draft());
    let p = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = TestSessionLiveCurrent {
        provision: &p,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: &source,
        source_ticket: 5,
        observed_at: UnixMillis::new(now),
    };
    f.state
        .issue_test_session_v2_control_challenge(
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &body,
                possession: &pop,
            },
            &public_challenge_nonce(84),
            &current,
            UnixMillis::new(now + 2),
            &mut StrictCrypto,
        )
        .unwrap();
    cold(&f.state);
    mutations(&f.state, &f.session_id, &source, true);
}
#[test]
fn session_v2_five_source_preserves_none_four_bytes_and_refuses_upgrade() {
    let mut f = activation_tests::activated(9000);
    let now = 12_000;
    let request = request(&f, "old-four-logical", None);
    let challenge = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "old-four-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::CredentialClaim,
        logical_request_id: request.request_id.clone(),
        operation_request_digest: request.test_session_digest().unwrap(),
    };
    let body = serde_json::to_vec(&challenge).unwrap();
    let pop = http(
        &f,
        &body,
        HttpPopRoute::ProofChallenges,
        HttpPopProfileV1::ClaimChallenge,
        &challenge.request_id,
        None,
        now,
    );
    let ingress = "old-four-http";
    paid(&mut f, &pop, ingress, now);
    let source = source_id(&f, ingress, false);
    let life = current_source(&mut f, &pop, ingress, &source, now);
    let wrapper = f
        .state
        .test_session_v2_source(&f.session_id, &source)
        .unwrap();
    let original = serde_json::to_string(wrapper).unwrap();
    let value = serde_json::to_value(wrapper).unwrap();
    assert!(
        !value
            .as_object()
            .unwrap()
            .contains_key("observation_profile")
    );
    assert!(!value.as_object().unwrap().contains_key("client_admission"));
    let before = f.state.clone();
    let nonce = oasis7_custody_core::approval::bytes_digest(source.as_bytes());
    assert!(
        f.state
            .reserve_test_session_v2_credential_five_source(
                &f.session_id,
                SessionSourceReservation {
                    operation_id: &source,
                    nonce: &nonce,
                    funding: SessionSourceFundingV1::Control
                },
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(now + 2)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    assert_eq!(
        serde_json::to_string(
            f.state
                .test_session_v2_source(&f.session_id, &source)
                .unwrap()
        )
        .unwrap(),
        original
    );
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
    f.state
        .issue_test_session_v2_credential_challenge(
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &body,
                possession: &pop,
            },
            &public_challenge_nonce(85),
            &current,
            UnixMillis::new(now + 2),
            &mut StrictCrypto,
        )
        .unwrap();
    cold(&f.state);
}

#[test]
fn session_v2_five_source_paid_birth_cold_cannot_drop_both_profile_fields() {
    let mut f = activation_tests::activated(9000);
    let now = 12_000;
    let request = request(&f, "five-birth-logical", None);
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "five-birth-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::CredentialClaim,
        logical_request_id: request.request_id.clone(),
        operation_request_digest: request.test_session_digest().unwrap(),
    };
    let body = serde_json::to_vec(&q).unwrap();
    let pop = http(
        &f,
        &body,
        HttpPopRoute::ProofChallenges,
        HttpPopProfileV1::ClaimChallenge,
        &q.request_id,
        None,
        now,
    );
    let ingress = "five-birth-http";
    paid(&mut f, &pop, ingress, now);
    let source = source_id(&f, ingress, false);
    let nonce = oasis7_custody_core::approval::bytes_digest(source.as_bytes());
    f.state
        .reserve_test_session_v2_credential_five_source(
            &f.session_id,
            SessionSourceReservation {
                operation_id: &source,
                nonce: &nonce,
                funding: SessionSourceFundingV1::Control,
            },
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &body,
                possession: &pop,
            },
            UnixMillis::new(now),
        )
        .unwrap();
    f.state
        .require_test_session_v2_credential_five_source(&f.session_id, &source)
        .unwrap();
    assert_eq!(
        f.state
            .test_session_v2_credential_source_ticket(&f.session_id, &source)
            .unwrap(),
        0
    );
    cold(&f.state);
    let original = f.state.clone();
    let mut value = serde_json::to_value(&f.state).unwrap();
    let wrapper = value["test_sessions_v2"][&f.session_id]["sources"][&source]
        .as_object_mut()
        .unwrap();
    assert!(wrapper.remove("observation_profile").is_some());
    assert!(wrapper.remove("client_admission").is_some());
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(value).is_err(),
        "a genuine paid five-profile birth must not cold-restore as a legacy generic Source"
    );
    assert_eq!(f.state, original);
    let original_value = serde_json::to_value(&f.state).unwrap();
    let events = original_value["test_sessions_v2"][&f.session_id]["events"]
        .as_array()
        .unwrap();
    let marker = events
        .iter()
        .position(|event| {
            event["event"]["kind"] == "client_source_profile_born"
                && event["event"]["source_operation_id"] == source
        })
        .expect("actual original birth marker");
    let mut wrong = original_value.clone();
    wrong["test_sessions_v2"][&f.session_id]["events"][marker]["event"]["source_reserved_ordinal"] =
        serde_json::json!(0);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(wrong).is_err());
    let mut wrong = original_value.clone();
    wrong["test_sessions_v2"][&f.session_id]["events"][marker]["event"]["ingress_attempt_id"] =
        serde_json::json!("foreign-birth-ingress");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(wrong).is_err());
    let mut wrong = original_value;
    wrong["test_sessions_v2"][&f.session_id]["events"]
        .as_array_mut()
        .unwrap()
        .remove(marker);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(wrong).is_err());
}
