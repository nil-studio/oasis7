//! Genuine V2 roots/claim and actual Rust-issued metadata nonce. No dispatch.
use super::*;
fn claimed_execution() -> SessionFixture {
    claimed_execution_from(activation_tests::activated(9000))
}
fn claimed_execution_from(mut f: SessionFixture) -> SessionFixture {
    let mut q = request(&f, "execution-fee-claim", None);
    q.purposes = vec![TestSessionPurposeV1::Execute, TestSessionPurposeV1::Read];
    let mut read = allocation(32);
    read.purpose = SessionPurposeV1::ReconcileRead;
    assert!(
        claim(&mut f, &q, &[allocation(31), read], 63, 12_000)
            .unwrap()
            .newly_claimed
    );
    f
}
fn signed_http(
    f: &SessionFixture,
    body: &[u8],
    id: &str,
    route_profile: (HttpPopRoute, HttpPopProfileV1),
    nonce: Option<&str>,
    purpose: SessionPurposeV1,
    now: u64,
) -> SessionClientHttpRequestV1 {
    let (route, profile) = route_profile;
    let saved = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    let CustodyIntentV1::TestSessionV2 { scope, .. } = &saved.input().request.intent else {
        unreachable!()
    };
    let host = SigningKey::from_bytes(&[8; 32]);
    let public: String = host
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert!(public == scope.client_pairing.client_public_key);
    let root = &saved.roots()[&purpose];
    let byte = if purpose == SessionPurposeV1::Execution {
        31
    } else {
        32
    };
    let token = URL_SAFE_NO_PAD.encode([byte; 32]);
    let token_digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    assert!(
        root.credential.as_ref().unwrap().digest
            == token_digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
    );
    let target = CanonicalHttpTarget::new(
        &f.fact.registration.public_origin,
        HttpPopRoute::ScopedClient {
            provision_id: f.state.draft().entry.provision_id.clone(),
            custody_account_id: f.state.draft().entry.custody_account_id.clone(),
            route: Box::new(route),
        },
    )
    .unwrap();
    let created = UnixMillis::new(now).floor_seconds();
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile,
        target: &target,
        key_id: &scope.client_pairing.client_key_id,
        binding_id: &f.session_id,
        request_id: id,
        nonce,
        token: Some(&token),
        created,
        expires: UnixSeconds::new((created.get() + 20).min(root.deadline.get() / 1000)),
        body,
    })
    .unwrap();
    let headers = material.signed_headers(&host.sign(material.signature_base()).to_bytes());
    SessionClientHttpRequestV1 {
        session_id: f.session_id.clone(),
        method: target.method().into(),
        target_uri: target.uri().into(),
        headers: headers
            .into_iter()
            .map(|h| SessionClientHeaderV1 {
                name: h.name,
                value: h.value,
            })
            .collect(),
        body: std::str::from_utf8(body).unwrap().into(),
        nonce_hint: nonce.map(str::to_owned),
    }
}
fn action_pop(
    f: &mut SessionFixture,
    id: &str,
    now: u64,
    wrong_target: bool,
) -> (Vec<u8>, HttpPossessionReceipt) {
    action_pop_for(f, id, now, wrong_target, SessionPurposeV1::Execution)
}
fn action_pop_for(
    f: &mut SessionFixture,
    id: &str,
    now: u64,
    wrong_target: bool,
    purpose: SessionPurposeV1,
) -> (Vec<u8>, HttpPossessionReceipt) {
    let plan = f.state.test_session_v2_header(&f.session_id).unwrap().0;
    let selected = if purpose == SessionPurposeV1::Execution {
        SessionToolActionV1::Execute(plan.actions[0].clone())
    } else {
        assert_eq!(purpose, SessionPurposeV1::ReconcileRead);
        SessionToolActionV1::ReadCloseState
    };
    let action = SessionActionRequestV2 {
        api_version: 2,
        session_id: f.session_id.clone(),
        request_id: id.into(),
        action: selected,
    };
    let metadata = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: format!("metadata-{id}"),
        purpose: action.purpose(),
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: id.into(),
        operation_request_digest: action.digest().unwrap(),
    };
    let body = serde_json::to_vec(&metadata).unwrap();
    let http = signed_http(
        f,
        &body,
        &metadata.request_id,
        (
            HttpPopRoute::ProofChallenges,
            HttpPopProfileV1::CustodyChallenge,
        ),
        None,
        action.purpose(),
        now,
    );
    let context = SessionClientHttpContext {
        public_origin: &f.fact.registration.public_origin,
        custody_account_id: &f.state.draft().entry.custody_account_id,
        provision_id: &f.state.draft().entry.provision_id,
        operation: SessionClientOperation::ToolNonce,
        representation: HttpHeaderRepresentation::RawInstances,
    };
    let prepared = f
        .state
        .prepare_test_session_v2_tool_nonce_http(&http, &context, UnixMillis::new(now))
        .unwrap();
    let proof = verify_http_possession(prepared, UnixMillis::new(now), &mut StrictCrypto).unwrap();
    let ingress = format!("metadata-ingress-{id}");
    let admission = || SessionProofRequest {
        ingress_attempt_id: &ingress,
        original_bytes: &body,
        possession: &proof,
    };
    f.state
        .charge_test_session_v2_tool_nonce_http_attempt(admission(), UnixMillis::new(now))
        .unwrap();
    let nonce = URL_SAFE_NO_PAD.encode(Sha256::digest(
        format!("fixture-public-metadata-v2:{id}").as_bytes(),
    ));
    let challenge = f
        .state
        .issue_test_session_v2_tool_nonce(admission(), &nonce, UnixMillis::new(now + 1))
        .unwrap();
    assert_eq!(challenge.kind, SessionChallengeKindV1::ToolMetadata);
    let bytes = serde_json::to_vec(&action).unwrap();
    let http = signed_http(
        f,
        &bytes,
        id,
        (
            if wrong_target {
                HttpPopRoute::Heartbeat(f.session_id.clone())
            } else {
                HttpPopRoute::Actions
            },
            HttpPopProfileV1::Custody,
        ),
        Some(&challenge.nonce),
        action.purpose(),
        now + 2,
    );
    let prepared = if wrong_target {
        // Genuine wrong-route Host PoP, for the fee consumer's defensive check.
        let target = CanonicalHttpTarget::new(
            &f.fact.registration.public_origin,
            HttpPopRoute::ScopedClient {
                provision_id: f.state.draft().entry.provision_id.clone(),
                custody_account_id: f.state.draft().entry.custody_account_id.clone(),
                route: Box::new(HttpPopRoute::Heartbeat(f.session_id.clone())),
            },
        )
        .unwrap();
        let saved = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
        let CustodyIntentV1::TestSessionV2 { scope, .. } = &saved.input().request.intent else {
            unreachable!()
        };
        let digest: [u8; 32] = Sha256::digest(URL_SAFE_NO_PAD.encode([31; 32]).as_bytes()).into();
        let headers: Vec<_> = http
            .headers
            .iter()
            .map(|h| HttpHeader {
                name: &h.name,
                value: &h.value,
            })
            .collect();
        prepare_http_pop(
            &HttpPopMessage {
                method: &http.method,
                target_uri: &http.target_uri,
                headers: &headers,
                representation: HttpHeaderRepresentation::RawInstances,
                body: &bytes,
            },
            &HttpPopBinding {
                profile: HttpPopProfileV1::Custody,
                target: &target,
                key_id: &scope.client_pairing.client_key_id,
                public_key_hex: &scope.client_pairing.client_public_key,
                binding_id: &f.session_id,
                request_id: id,
                nonce: Some(&challenge.nonce),
                credential_digest: &digest,
                valid_until: saved.roots()[&action.purpose()].deadline,
            },
            UnixMillis::new(now + 2),
        )
        .unwrap()
    } else {
        f.state
            .prepare_test_session_v2_action_http(
                &http,
                HttpHeaderRepresentation::RawInstances,
                UnixMillis::new(now + 2),
            )
            .unwrap()
    };
    (
        bytes,
        verify_http_possession(prepared, UnixMillis::new(now + 2), &mut StrictCrypto).unwrap(),
    )
}
fn charge(
    f: &mut SessionFixture,
    body: &[u8],
    pop: &HttpPossessionReceipt,
    ingress: &str,
    purpose: SessionPurposeV1,
    now: u64,
) -> std::result::Result<(), ProvisionApprovalError> {
    f.state.charge_test_session_v2_purpose_http_attempt(
        &f.session_id,
        purpose,
        SessionProofRequest {
            possession: pop,
            ingress_attempt_id: ingress,
            original_bytes: body,
        },
        UnixMillis::new(now),
    )
}
#[test]
fn session_v2_actual_actions_ingress_is_paid_once_but_does_not_create_execution_book() {
    let mut f = claimed_execution();
    let (body, pop) = action_pop(&mut f, "execution-fee-one", 13_000, false);
    let before = serde_json::to_value(&f.state).unwrap();
    let saved = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    let execution = saved.roots()[&SessionPurposeV1::Execution].clone();
    let read = saved.roots()[&SessionPurposeV1::ReconcileRead].clone();
    charge(
        &mut f,
        &body,
        &pop,
        "actual-actions-ingress-one",
        SessionPurposeV1::Execution,
        13_002,
    )
    .unwrap();
    let snapshot = f.state.clone();
    charge(
        &mut f,
        &body,
        &pop,
        "actual-actions-ingress-one",
        SessionPurposeV1::Execution,
        13_003,
    )
    .unwrap();
    assert_eq!(f.state, snapshot);
    let saved = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    assert_eq!(
        saved.roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap(),
        execution.budgets.transport.remaining().unwrap() - 1
    );
    assert_eq!(saved.roots()[&SessionPurposeV1::ReconcileRead], read);
    let after = serde_json::to_value(&f.state).unwrap();
    let a = &after["test_sessions_v2"][&f.session_id];
    let b = &before["test_sessions_v2"][&f.session_id];
    assert_eq!(a["purpose_http_admissions"].as_object().unwrap().len(), 2);
    assert!(a["execution_book"].is_null());
    assert!(a["actions"].as_object().unwrap().is_empty());
    assert_eq!(a["control"], b["control"]);
    assert_eq!(a["identity_transport"], b["identity_transport"]);
    assert_eq!(a["sources"], b["sources"]);
    let restored: AccountProvisionApprovalState = serde_json::from_value(after).unwrap();
    assert_eq!(restored, f.state);
}
#[test]
fn session_v2_execution_fee_charges_new_ingress_and_rejects_wrong_target_or_cross_lane() {
    let mut f = claimed_execution();
    let (wrong_body, wrong) = action_pop(&mut f, "wrong-control-target", 13_000, true);
    let before = f.state.clone();
    assert!(
        charge(
            &mut f,
            &wrong_body,
            &wrong,
            "wrong-target-ingress",
            SessionPurposeV1::Execution,
            13_002
        )
        .is_err()
    );
    assert_eq!(f.state, before);
    let (body, pop) = action_pop(&mut f, "execution-fee-one", 14_000, false);
    charge(
        &mut f,
        &body,
        &pop,
        "actual-actions-ingress-one",
        SessionPurposeV1::Execution,
        14_002,
    )
    .unwrap();
    // A second physical ingress is another paid HTTP, even for the same
    // signed business original. Business once/unknown is enforced separately.
    charge(
        &mut f,
        &body,
        &pop,
        "actual-actions-ingress-two",
        SessionPurposeV1::Execution,
        14_003,
    )
    .unwrap();
    let before = f.state.clone();
    assert!(
        charge(
            &mut f,
            &body,
            &pop,
            "actual-actions-ingress-one",
            SessionPurposeV1::ReconcileRead,
            14_003
        )
        .is_err()
    );
    assert_eq!(f.state, before);
    let mut bad = serde_json::to_value(&f.state).unwrap();
    bad["test_sessions_v2"][&f.session_id]["purpose_http_admissions"]["actual-actions-ingress-one"]
        ["purpose"] = serde_json::json!("termination_plan");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
}

#[test]
fn session_v2_read_actions_use_only_read_transport_and_no_execution_authority() {
    let mut f = claimed_execution();
    let initial = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    let execution = initial.roots()[&SessionPurposeV1::Execution].clone();
    let read_remaining = initial.roots()[&SessionPurposeV1::ReconcileRead]
        .budgets
        .transport
        .remaining()
        .unwrap();
    let (body, proof) = action_pop_for(
        &mut f,
        "read-close-fact",
        13_000,
        false,
        SessionPurposeV1::ReconcileRead,
    );
    charge(
        &mut f,
        &body,
        &proof,
        "read-actual-ingress",
        SessionPurposeV1::ReconcileRead,
        13_002,
    )
    .unwrap();
    let saved = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    assert_eq!(saved.roots()[&SessionPurposeV1::Execution], execution);
    assert_eq!(
        saved.roots()[&SessionPurposeV1::ReconcileRead]
            .budgets
            .transport
            .remaining()
            .unwrap(),
        read_remaining - 4
    );
    let snapshot = f.state.clone();
    charge(
        &mut f,
        &body,
        &proof,
        "read-actual-ingress",
        SessionPurposeV1::ReconcileRead,
        13_003,
    )
    .unwrap();
    assert_eq!(snapshot, f.state);
    let json = serde_json::to_value(&f.state).unwrap();
    assert!(json["test_sessions_v2"][&f.session_id]["execution_book"].is_null());
    let restored: AccountProvisionApprovalState = serde_json::from_value(json).unwrap();
    assert_eq!(restored, f.state);
}

#[test]
fn session_v2_read_fee_and_metadata_survive_actual_execution_lease_closing() {
    let mut f = claimed_execution();
    let before = serde_json::to_value(&f.state).unwrap();
    let lease = serde_json::from_value::<UnixMillis>(
        before["test_sessions_v2"][&f.session_id]["lease_until"].clone(),
    )
    .unwrap()
    .get();
    let read_deadline = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::ReconcileRead]
        .deadline
        .get();
    assert!(lease + 3_000 < read_deadline);
    let expired = f
        .state
        .expire_test_sessions_v2(UnixMillis::new(lease))
        .unwrap();
    assert_eq!(expired, vec![f.session_id.clone()]);
    let closed = serde_json::to_value(&f.state).unwrap();
    assert!(!closed["test_sessions_v2"][&f.session_id]["closing"].is_null());
    let execution = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::Execution]
        .clone();
    let (body, proof) = action_pop_for(
        &mut f,
        "read-after-lease",
        lease + 1_000,
        false,
        SessionPurposeV1::ReconcileRead,
    );
    charge(
        &mut f,
        &body,
        &proof,
        "read-after-lease-ingress",
        SessionPurposeV1::ReconcileRead,
        lease + 1_002,
    )
    .unwrap();
    let saved = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    assert_eq!(saved.roots()[&SessionPurposeV1::Execution], execution);
    let after = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        after["test_sessions_v2"][&f.session_id]["closing"],
        closed["test_sessions_v2"][&f.session_id]["closing"]
    );
    assert!(after["test_sessions_v2"][&f.session_id]["execution_book"].is_null());
    let restored: AccountProvisionApprovalState = serde_json::from_value(after).unwrap();
    assert_eq!(restored, f.state);
}

#[path = "test_session_v2_execution_nonce_tests.rs"]
mod nonce_tests;

#[path = "test_session_v2_execution_budget_tests.rs"]
mod budget_tests;

#[path = "test_session_v2_tool_nonce_capacity_tests.rs"]
mod capacity_tests;

#[path = "test_session_v2_execution_book_tests.rs"]
mod book_tests;

#[path = "test_session_v2_execution_nonce_five_tests.rs"]
mod execution_nonce_five_tests;
