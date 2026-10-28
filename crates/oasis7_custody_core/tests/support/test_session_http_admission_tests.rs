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
