use super::*;
fn legitimate_dot_with_native_metadata_proof() -> (
    SessionFixture,
    Vec<u8>,
    SessionClientHttpRequestV1,
    HttpPossessionReceipt,
) {
    // Profile selection precedes invitation, actual Enrollment PoP, independently
    // signed MFA intent, consume, preflight, activation, and private claim.
    let (f, token, _) = claimed_from(activated_for_profile(
        8005,
        HostTransportProfileV1::DotMcpCustodyV1,
    ));
    let session = &f.state.test_sessions()[ID];
    assert_eq!(
        f.fact.registration.request.descriptor.transport_profile,
        HostTransportProfileV1::DotMcpCustodyV1
    );
    let CustodyIntentV1::TestSession {
        client_binding,
        client_pairing,
        control_budget,
        ..
    } = &session.input.request.intent
    else {
        panic!("fixture TestSession intent");
    };
    let ApprovedClientBindingV1::DotMcpCustodyV1 {
        client_key,
        connector_key,
    } = client_binding
    else {
        panic!("fixture Dot binding");
    };
    assert_ne!(client_key, connector_key);
    assert_eq!(
        client_key,
        &f.fact.registration.request.descriptor.client_public_key
    );
    assert_eq!(
        client_pairing
            .as_ref()
            .unwrap()
            .connector_public_key
            .as_ref(),
        Some(connector_key)
    );
    assert_eq!(control_budget.transport_hops, 2);
    assert!(session.connector_transport.is_some());
    assert_eq!(
        f.state
            .test_session_progress(ID, UnixMillis::new(12_000))
            .unwrap()
            .stage,
        TestSessionPhaseV1::Active
    );
    restore(&f.state);
    let q = request(SessionPurposeV1::Execution, "legitimate-dot");
    let body = serde_json::to_vec(&q).unwrap();
    let raw = raw_request(
        &f,
        &body,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
            &q.request_id,
            None,
            Some(&token),
            12_000,
        ),
    );
    let target = CanonicalHttpTarget::new(
        &f.fact.registration.public_origin,
        HttpPopRoute::ScopedClient {
            provision_id: f.state.draft().entry.provision_id.clone(),
            custody_account_id: f.state.draft().entry.custody_account_id.clone(),
            route: Box::new(HttpPopRoute::ProofChallenges),
        },
    )
    .unwrap();
    assert_eq!(raw.target_uri, target.uri());
    let headers = raw
        .headers
        .iter()
        .map(|h| HttpHeader {
            name: &h.name,
            value: &h.value,
        })
        .collect::<Vec<_>>();
    let token_digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    let prepared = prepare_http_pop(
        &HttpPopMessage {
            method: &raw.method,
            target_uri: &raw.target_uri,
            headers: &headers,
            representation: HttpHeaderRepresentation::RawInstances,
            body: raw.body.as_bytes(),
        },
        &HttpPopBinding {
            profile: HttpPopProfileV1::CustodyChallenge,
            target: &target,
            key_id: &f.fact.registration.key_id,
            public_key_hex: client_key,
            binding_id: ID,
            request_id: &q.request_id,
            nonce: None,
            credential_digest: &token_digest,
            valid_until: session.roots[&q.purpose].deadline,
        },
        UnixMillis::new(12_000),
    )
    .unwrap();
    let receipt =
        verify_http_possession(prepared, UnixMillis::new(12_000), &mut StrictCrypto).unwrap();
    (f, body, raw, receipt)
}
#[test]
fn tool_nonce_native_prepare_rejects_legitimate_dot_session() {
    let (f, _, raw, _) = legitimate_dot_with_native_metadata_proof();
    let context = SessionClientHttpContext {
        public_origin: &f.fact.registration.public_origin,
        custody_account_id: &f.state.draft().entry.custody_account_id,
        provision_id: &f.state.draft().entry.provision_id,
        operation: SessionClientOperation::ToolNonce,
        representation: HttpHeaderRepresentation::RawInstances,
    };
    let before = f.state.clone();
    assert!(
        f.state
            .prepare_test_session_client_http(&raw, &context, UnixMillis::new(12_000))
            .is_err(),
        "a valid Dot session and PoP cannot enter the Native-only metadata preparer"
    );
    assert_eq!(f.state, before);
}
#[test]
fn tool_nonce_native_fee_rejects_legitimate_dot_session() {
    let (mut f, body, _, receipt) = legitimate_dot_with_native_metadata_proof();
    let before = f.state.clone();
    assert!(
        f.state
            .charge_test_session_tool_nonce_http_attempt(
                proof("dot-metadata-A", &body, &receipt),
                UnixMillis::new(12_000)
            )
            .is_err(),
        "Native metadata three-fee admission must reject Dot before any fee or slot"
    );
    assert_eq!(f.state, before);
}

#[test]
fn tool_nonce_restore_rejects_native_metadata_facts_on_a_fully_signed_dot_registration() {
    let (dot, body, _, dot_receipt) = legitimate_dot_with_native_metadata_proof();
    let q: SessionChallengeRequestV1 = serde_json::from_slice(&body).unwrap();
    let (mut native, native_token, _) = claimed();
    let (native_body, native_receipt) = signed(&native, &q, &native_token, 12_000);
    assert_eq!(native_body, body);
    assert!(
        native_receipt.transport_attempt_digest() == dot_receipt.transport_attempt_digest(),
        "the raw proof itself does not distinguish these independently approved transport profiles"
    );
    native
        .state
        .charge_test_session_tool_nonce_http_attempt(
            proof("registration-guard-A", &native_body, &native_receipt),
            UnixMillis::new(12_000),
        )
        .unwrap();
    native
        .state
        .issue_test_session_tool_nonce(
            proof("registration-guard-A", &native_body, &native_receipt),
            &URL_SAFE_NO_PAD.encode([135; 32]),
            UnixMillis::new(12_001),
        )
        .unwrap();
    restore(&native.state);
    restore(&dot.state);
    // Keep the Dot invitation, Enrollment fact, complete signed MFA intent,
    // connector and two-hop ledgers intact. Import only the genuine new Native
    // metadata facts and their exact three paid transports/balance. This leaves
    // no unrelated invalid profile/signature or deliberately unbalanced budget.
    let native_value = serde_json::to_value(&native.state).unwrap();
    let mut dot_value = serde_json::to_value(&dot.state).unwrap();
    let from = &native_value["test_sessions"][ID];
    let to = &mut dot_value["test_sessions"][ID];
    for field in [
        "tool_nonce_ingresses",
        "tool_nonce_links",
        "purpose_http_admissions",
        "purpose_transports",
    ] {
        to[field] = from[field].clone();
    }
    to["challenges"][&q.request_id] = from["challenges"][&q.request_id].clone();
    to["roots"]["execution"]["budgets"]["transport"] =
        from["roots"]["execution"]["budgets"]["transport"].clone();
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(dot_value).is_err(),
        "new Native metadata history requires the same Native registered and signed profile on restore"
    );
}
