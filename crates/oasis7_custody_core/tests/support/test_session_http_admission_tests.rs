use super::*;
#[test]
fn test_session_identical_signed_proof_on_two_real_ingresses_costs_two_http_attempts() {
    let (mut f, execution, _) = claimed();
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "ingress-replay-proof".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::Control(SessionControlKindV1::Heartbeat),
        logical_request_id: "heartbeat-replayed".into(),
        operation_request_digest: "ab".repeat(32),
    };
    let body = serde_json::to_vec(&q).unwrap();
    let proof = http(
        &f,
        &body,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &q.request_id,
        None,
        Some(&execution),
        12_000,
    );
    let before = f
        .state
        .test_session_progress(ID, UnixMillis::new(12_000))
        .unwrap()
        .control_host_http_remaining
        .0;
    // These are two real incoming HTTP requests, even though the signed octets are identical.
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: "ingress-A",
                possession: &proof,
            },
            UnixMillis::new(12_000),
        )
        .unwrap();
    let second_http = f.state.charge_test_session_control_http_attempt(
        ID,
        SessionHttpAdmission {
            ingress_attempt_id: "ingress-B",
            possession: &proof,
        },
        UnixMillis::new(12_001),
    );
    assert!(
        second_http.is_ok(),
        "a second authentic ingress must get its own paid admission, independent of signed-proof identity"
    );
    let callback_snapshot = f.state.clone();
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: "ingress-A",
                possession: &proof,
            },
            UnixMillis::new(12_002),
        )
        .unwrap();
    assert_eq!(
        f.state, callback_snapshot,
        "same admission callback is not a new HTTP request"
    );
    let mut other = q.clone();
    other.request_id = "different-signed-request".into();
    let other_body = serde_json::to_vec(&other).unwrap();
    let other_proof = http(
        &f,
        &other_body,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &other.request_id,
        None,
        Some(&execution),
        12_002,
    );
    assert!(
        f.state
            .charge_test_session_control_http_attempt(
                ID,
                SessionHttpAdmission {
                    ingress_attempt_id: "ingress-A",
                    possession: &other_proof
                },
                UnixMillis::new(12_002)
            )
            .is_err()
    );
    assert_eq!(
        f.state, callback_snapshot,
        "same admission must not be rebound to another proof"
    );
    let after = f
        .state
        .test_session_progress(ID, UnixMillis::new(12_001))
        .unwrap()
        .control_host_http_remaining
        .0;
    assert_eq!(
        before - after,
        2,
        "signed-proof identity is not HTTP ingress identity"
    );
}

#[test]
fn test_session_paid_http_restore_requires_complete_ingress_proof_binding() {
    let (f, _, _) = claimed();
    let original = serde_json::to_value(&f.state).unwrap();
    for missing in [false, true] {
        let mut forged = original.clone();
        if missing {
            forged["test_sessions"][ID]
                .as_object_mut()
                .unwrap()
                .remove("client_http_admissions");
        } else {
            forged["test_sessions"][ID]["client_http_admissions"]["claim-one"]["signed_attempt_digest"] =
                "not-a-digest".into();
        }
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(forged).is_err());
    }
}

fn raw_request(
    f: &SessionFixture,
    body: &[u8],
    input: (
        HttpPopProfileV1,
        HttpPopRoute,
        &str,
        Option<&str>,
        Option<&str>,
        u64,
    ),
) -> SessionClientHttpRequestV1 {
    let (profile, route, request_id, nonce, token, now) = input;
    let route = HttpPopRoute::ScopedClient {
        provision_id: f.state.draft().entry.provision_id.clone(),
        custody_account_id: f.state.draft().entry.custody_account_id.clone(),
        route: Box::new(route),
    };
    let target = CanonicalHttpTarget::new("https://custody.example", route).unwrap();
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile,
        target: &target,
        key_id: &f.fact.registration.key_id,
        binding_id: ID,
        request_id,
        nonce,
        token,
        created: UnixSeconds::new(now / 1000),
        expires: UnixSeconds::new(now / 1000 + 10),
        body,
    })
    .unwrap();
    let key = SigningKey::from_bytes(&[8; 32]);
    let headers = material.signed_headers(&key.sign(material.signature_base()).to_bytes());
    SessionClientHttpRequestV1 {
        session_id: ID.into(),
        method: target.method().into(),
        target_uri: target.uri().into(),
        headers: headers
            .into_iter()
            .map(|h| SessionClientHeaderV1 {
                name: h.name,
                value: h.value,
            })
            .collect(),
        body: String::from_utf8(body.to_vec()).unwrap(),
        nonce_hint: nonce.map(str::to_owned),
    }
}
#[test]
fn test_session_account_http_preparation_uses_registered_key_current_token_and_exact_originals() {
    let (mut f, execution, read) = claimed();
    let account_id = f.state.draft().entry.custody_account_id.clone();
    let provision_id = f.state.draft().entry.provision_id.clone();
    let context = SessionClientHttpContext {
        public_origin: "https://custody.example",
        custody_account_id: &account_id,
        provision_id: &provision_id,
        operation: SessionClientOperation::Challenge,
        representation: HttpHeaderRepresentation::RawInstances,
    };
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "account-prepare-challenge".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::Control(SessionControlKindV1::Heartbeat),
        logical_request_id: "account-heartbeat".into(),
        operation_request_digest: "ab".repeat(32),
    };
    let body = serde_json::to_vec(&q).unwrap();
    let raw = raw_request(
        &f,
        &body,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
            &q.request_id,
            None,
            Some(&execution),
            12_000,
        ),
    );
    let before = f.state.clone();
    let prepared = f
        .state
        .prepare_test_session_client_http(&raw, &context, UnixMillis::new(12_000))
        .unwrap();
    verify_http_possession(prepared, UnixMillis::new(12_000), &mut StrictCrypto).unwrap();
    assert_eq!(
        f.state, before,
        "preparation and crypto alone never charge or admit"
    );
    let wrong_token = raw_request(
        &f,
        &body,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
            &q.request_id,
            None,
            Some(&read),
            12_000,
        ),
    );
    assert!(
        f.state
            .prepare_test_session_client_http(&wrong_token, &context, UnixMillis::new(12_000))
            .is_err()
    );
    for mutation in 0..4 {
        let mut bad = raw.clone();
        match mutation {
            0 => bad.target_uri = "https://other.example/v1/proof-challenges".into(),
            1 => bad.nonce_hint = Some(URL_SAFE_NO_PAD.encode([3; 32])),
            2 => bad.body.push(' '),
            _ => {
                bad.headers
                    .iter_mut()
                    .find(|h| h.name == "oasis7-request-id")
                    .unwrap()
                    .value = "different".into()
            }
        }
        assert!(
            f.state
                .prepare_test_session_client_http(&bad, &context, UnixMillis::new(12_000))
                .is_err()
        );
    }
    // A consumed original claim nonce can only expose historical metadata in the mutator.
    let original = f.state.test_sessions()[ID].roots[&SessionPurposeV1::Execution]
        .credential_history[0]
        .claim_request
        .clone();
    let body = serde_json::to_vec(&original).unwrap();
    let nonce = URL_SAFE_NO_PAD.encode([3; 32]);
    let raw = raw_request(
        &f,
        &body,
        (
            HttpPopProfileV1::CredentialClaim,
            HttpPopRoute::EnrollmentClaim(f.fact.registration.enrollment_id.clone()),
            &original.request_id,
            Some(&nonce),
            None,
            12_000,
        ),
    );
    let context = SessionClientHttpContext {
        operation: SessionClientOperation::CredentialClaim,
        ..context
    };
    let prepared = f
        .state
        .prepare_test_session_client_http(&raw, &context, UnixMillis::new(12_000))
        .unwrap();
    let receipt =
        verify_http_possession(prepared, UnixMillis::new(12_000), &mut StrictCrypto).unwrap();
    f.state
        .charge_test_session_control_http_attempt(
            ID,
            SessionHttpAdmission {
                ingress_attempt_id: "historical-retry-ingress",
                possession: &receipt,
            },
            UnixMillis::new(12_000),
        )
        .unwrap();
    let paid = f.state.clone();
    // The exact paid admission cannot be substituted by some other successfully paid ID.
    let mut bad = raw.clone();
    bad.nonce_hint = Some(URL_SAFE_NO_PAD.encode([99; 32]));
    assert!(
        f.state
            .prepare_test_session_client_http(&bad, &context, UnixMillis::new(12_000))
            .is_err()
    );
    assert_eq!(f.state, paid);
}

// RED-only: the current purpose API has no trusted ingress ID argument. These
// tests first prove that the registered current key/token and original signed
// bytes authenticate and charge successfully, then expose its conflation of a
// second actual ingress with the same signed proof. The old behavior REJECTS
// the second charge; it does not grant free downstream I/O.
//
// Real ingress identity is supplied by the trusted service, separate from signed bytes.
fn purpose_identical_proof_two_ingresses(purpose: SessionPurposeV1) {
    let (mut f, execution, read) = claimed();
    let token = match purpose {
        SessionPurposeV1::Execution => &execution,
        SessionPurposeV1::ReconcileRead => &read,
        SessionPurposeV1::TerminationPlan => panic!("cleanup never has a client token"),
    };
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: "purpose-ingress-replay-proof".into(),
        purpose,
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: "purpose-replayed-logical".into(),
        operation_request_digest: "ab".repeat(32),
    };
    let body = serde_json::to_vec(&q).unwrap();
    let proof = http(
        &f,
        &body,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &q.request_id,
        None,
        Some(token),
        12_000,
    );
    let remaining = |state: &AccountProvisionApprovalState| {
        state
            .test_session_progress(ID, UnixMillis::new(12_000))
            .unwrap()
            .purposes
            .into_iter()
            .find(|p| p.purpose == purpose)
            .unwrap()
            .transport_remaining
            .0
    };
    let before = remaining(&f.state);
    assert_eq!(
        before,
        match purpose {
            SessionPurposeV1::Execution => 400,
            SessionPurposeV1::ReconcileRead => 300,
            SessionPurposeV1::TerminationPlan => unreachable!(),
        }
    );
    f.state
        .charge_test_session_purpose_http_attempt(
            ID,
            purpose,
            SessionHttpAdmission {
                ingress_attempt_id: "purpose-ingress-A",
                possession: &proof,
            },
            UnixMillis::new(12_000),
        )
        .unwrap();
    assert_eq!(
        before - remaining(&f.state),
        1,
        "first authentic ingress is a real paid precondition"
    );
    let second_actual_ingress = f.state.charge_test_session_purpose_http_attempt(
        ID,
        purpose,
        SessionHttpAdmission {
            ingress_attempt_id: "purpose-ingress-B",
            possession: &proof,
        },
        UnixMillis::new(12_001),
    );
    assert!(
        second_actual_ingress.is_ok(),
        "second authentic purpose HTTP ingress needs its own paid admission; signed-proof identity is not ingress identity"
    );
    assert_eq!(before - remaining(&f.state), 2);
    let paid = f.state.clone();
    f.state
        .charge_test_session_purpose_http_attempt(
            ID,
            purpose,
            SessionHttpAdmission {
                ingress_attempt_id: "purpose-ingress-A",
                possession: &proof,
            },
            UnixMillis::new(12_002),
        )
        .unwrap();
    assert_eq!(
        paid, f.state,
        "same authenticated callback cannot charge twice"
    );
    let mut other_request = q.clone();
    other_request.request_id = "other-signed-purpose-request".into();
    let other_body = serde_json::to_vec(&other_request).unwrap();
    let other = http(
        &f,
        &other_body,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &other_request.request_id,
        None,
        Some(token),
        12_002,
    );
    assert!(
        f.state
            .charge_test_session_purpose_http_attempt(
                ID,
                purpose,
                SessionHttpAdmission {
                    ingress_attempt_id: "purpose-ingress-A",
                    possession: &other,
                },
                UnixMillis::new(12_002)
            )
            .is_err()
    );
    assert_eq!(
        paid, f.state,
        "same ingress cannot be rebound to different signed bytes"
    );
    let other_purpose = if purpose == SessionPurposeV1::Execution {
        SessionPurposeV1::ReconcileRead
    } else {
        SessionPurposeV1::Execution
    };
    let other_token = if other_purpose == SessionPurposeV1::Execution {
        &execution
    } else {
        &read
    };
    let mut cross = q.clone();
    cross.purpose = other_purpose;
    let cross_body = serde_json::to_vec(&cross).unwrap();
    let cross_proof = http(
        &f,
        &cross_body,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
        ),
        &cross.request_id,
        None,
        Some(other_token),
        12_002,
    );
    assert!(
        f.state
            .charge_test_session_purpose_http_attempt(
                ID,
                other_purpose,
                SessionHttpAdmission {
                    ingress_attempt_id: "purpose-ingress-A",
                    possession: &cross_proof,
                },
                UnixMillis::new(12_002)
            )
            .is_err()
    );
    assert_eq!(
        paid, f.state,
        "paid purpose cannot be replaced by another root"
    );
    assert_eq!(
        paid.test_sessions()[ID].control_transports,
        f.state.test_sessions()[ID].control_transports
    );
    let original = serde_json::to_value(&paid).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(original.clone()).unwrap(),
        paid
    );
    for mutation in 0..11 {
        let mut bad = original.clone();
        let session = &mut bad["test_sessions"][ID];
        match mutation {
            0 => {
                session
                    .as_object_mut()
                    .unwrap()
                    .remove("purpose_http_admissions");
            }
            1 => {
                session["purpose_http_admissions"]
                    .as_object_mut()
                    .unwrap()
                    .remove("purpose-ingress-A");
            }
            2 => {
                session["purpose_http_admissions"]["purpose-ingress-A"]["signed_attempt_digest"] =
                    "invalid".into()
            }
            3 => session["purpose_http_admissions"]["purpose-ingress-A"]["hop"] = "identity".into(),
            4 => {
                session["purpose_http_admissions"]["purpose-ingress-A"]["purpose"] =
                    "termination_plan".into()
            }
            5 => session["purpose_http_admissions"]["purpose-ingress-A"]["charged_at"] = "0".into(),
            6 => {
                session["purpose_http_admissions"]["purpose-ingress-A"]["charged_at"] =
                    "18446744073709551615".into()
            }
            7 => {
                session["purpose_http_admissions"]["purpose-ingress-A"]["purpose"] =
                    if purpose == SessionPurposeV1::Execution {
                        "reconcile_read"
                    } else {
                        "execution"
                    }
                    .into()
            }
            8 => {
                let row = session["purpose_http_admissions"]["purpose-ingress-A"].clone();
                session["purpose_http_admissions"]["orphan-http-ingress"] = row;
            }
            9 => {
                let row = session["purpose_http_admissions"]["purpose-ingress-A"].clone();
                session["purpose_http_admissions"]["claim-one"] = row;
            }
            _ => {
                let row = session["purpose_http_admissions"]["purpose-ingress-A"].clone();
                for n in 0..701 {
                    session["purpose_http_admissions"][format!("overflow-{n}")] = row.clone();
                }
            }
        }
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
            "corrupt purpose HTTP persistence mutation {mutation}"
        );
    }
    // The live challenge mutator must require this exact paid ingress, not any
    // paid signed-digest/purpose pair from another actual request.
    let life = funded_source(
        &mut f,
        purpose.source_funding(),
        purpose,
        "purpose-paid-source",
        &"c1".repeat(32),
        12_003,
    );
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    let ns = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &current,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "purpose-paid-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(12_005),
    };
    let before_challenge = f.state.clone();
    for (ingress, bytes, possession) in [
        ("never-paid-ingress", body.as_slice(), &proof),
        ("purpose-ingress-A", other_body.as_slice(), &other),
    ] {
        assert!(
            f.state
                .issue_test_session_challenge(
                    SessionProofRequest {
                        ingress_attempt_id: ingress,
                        original_bytes: bytes,
                        possession,
                    },
                    &URL_SAFE_NO_PAD.encode([8; 32]),
                    &live,
                    UnixMillis::new(12_006),
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(
            before_challenge, f.state,
            "unpaid or changed proof cannot borrow an existing ingress"
        );
    }
    f.state
        .issue_test_session_challenge(
            SessionProofRequest {
                ingress_attempt_id: "purpose-ingress-A",
                original_bytes: &body,
                possession: &proof,
            },
            &URL_SAFE_NO_PAD.encode([8; 32]),
            &live,
            UnixMillis::new(12_006),
            &mut StrictCrypto,
        )
        .unwrap();
}
#[test]
fn test_session_execution_identical_signed_proof_two_actual_ingresses_need_two_paid_admissions() {
    purpose_identical_proof_two_ingresses(SessionPurposeV1::Execution);
}
#[test]
fn test_session_read_identical_signed_proof_two_actual_ingresses_need_two_paid_admissions() {
    purpose_identical_proof_two_ingresses(SessionPurposeV1::ReconcileRead);
}

#[path = "test_session_purpose_collision_tests.rs"]
mod purpose_collisions;
