//! Child of the genuine V2 credential fixture: no hand-filled Root or credential.
use super::*;
struct NoNativeChecks;
impl oasis7_custody_core::approval_admission::ApprovalSignatureVerifier for NoNativeChecks {
    fn verify(
        &mut self,
        _: &oasis7_custody_core::approval_admission::ExactSignatureChecks,
    ) -> std::result::Result<(), oasis7_custody_core::approval_admission::ApprovalAdmissionError>
    {
        panic!("pure tightening must not request a fresh Native signature check")
    }
}
fn claimed() -> SessionFixture {
    let mut f = activation_tests::activated(9000);
    let mut q = request(&f, "control-initial-claim", None);
    q.purposes = vec![TestSessionPurposeV1::Execute, TestSessionPurposeV1::Read];
    let mut read = allocation(32);
    read.purpose = SessionPurposeV1::ReconcileRead;
    assert!(
        claim(&mut f, &q, &[allocation(31), read], 41, 12_000)
            .unwrap()
            .newly_claimed
    );
    f
}
fn control_pop(
    f: &SessionFixture,
    body: &[u8],
    route_profile: (HttpPopRoute, HttpPopProfileV1),
    request_id: &str,
    nonce: Option<&SessionChallengeV1>,
    purpose: SessionPurposeV1,
    now: u64,
) -> HttpPossessionReceipt {
    let (route, profile) = route_profile;
    let root = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()
        .get(&purpose)
        .unwrap();
    let until = nonce.map_or(root.deadline.get(), |n| {
        root.deadline.get().min(n.expires_at.get())
    });
    let token = URL_SAFE_NO_PAD.encode(
        [if purpose == SessionPurposeV1::Execution {
            31
        } else {
            32
        }; 32],
    );
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
        nonce: nonce.map(|v| v.nonce.as_str()),
        token: Some(&token),
        created,
        expires: UnixSeconds::new((created.get() + 20).min(until / 1000)),
        body,
    })
    .unwrap();
    let headers = material.signed_headers(
        &SigningKey::from_bytes(&[8; 32])
            .sign(material.signature_base())
            .to_bytes(),
    );
    let q = SessionClientHttpRequestV1 {
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
        nonce_hint: nonce.map(|v| v.nonce.clone()),
    };
    let operation = if profile == HttpPopProfileV1::CustodyChallenge {
        SessionClientOperation::Challenge
    } else {
        let r: SessionControlRequestV1 = serde_json::from_slice(body).unwrap();
        SessionClientOperation::Control(r.kind)
    };
    let c = SessionClientHttpContext {
        public_origin: "https://custody.example",
        custody_account_id: &f.state.draft().entry.custody_account_id,
        provision_id: &f.state.draft().entry.provision_id,
        operation,
        representation: HttpHeaderRepresentation::RawInstances,
    };
    let prepared = f
        .state
        .prepare_test_session_v2_control_http(&q, &c, UnixMillis::new(now))
        .unwrap();
    verify_http_possession(prepared, UnixMillis::new(now), &mut StrictCrypto).unwrap()
}
fn control_paid(f: &mut SessionFixture, pop: &HttpPossessionReceipt, ingress: &str, now: u64) {
    f.state
        .charge_test_session_v2_control_http_attempt(
            &f.session_id,
            SessionHttpAdmission {
                ingress_attempt_id: ingress,
                possession: pop,
            },
            UnixMillis::new(now),
        )
        .unwrap();
}
fn control_source(
    f: &mut SessionFixture,
    pop: &HttpPossessionReceipt,
    ingress: &str,
    id: &str,
    now: u64,
) -> SignedManagedNativeSessionLifetimeV1 {
    let nonce = oasis7_custody_core::approval::bytes_digest(id.as_bytes());
    f.state
        .reserve_test_session_v2_source(
            &f.session_id,
            SessionSourceReservation {
                operation_id: id,
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
            .reserve_test_session_v2_source_read(&f.session_id, id, UnixMillis::new(now))
            .unwrap(),
        1
    );
    f.state
        .claim_test_session_v2_source(&f.session_id, id, &c, UnixMillis::new(now))
        .unwrap();
    let mut v = verification(
        f.state.test_session_v2_original(&f.session_id).unwrap(),
        &f.context,
    )
    .verification;
    v.purpose = VerificationPurposeV1::InspectOrigin;
    v.verify_nonce = nonce;
    v.observed_at = DecimalU64(now);
    // The fixture emulates the actual Native origin inspector after the one
    // consumed human MFA expires; the original proof and deadline stay exact.
    v.proof_state = if now
        >= f.state
            .test_session_v2_original(&f.session_id)
            .unwrap()
            .input()
            .approval
            .proof
            .expires_at
            .0
    {
        ProofStateV1::Expired
    } else {
        ProofStateV1::Active
    };
    f.state
        .complete_test_session_v2_source(
            &f.session_id,
            id,
            sign_verification(v),
            &c,
            UnixMillis::new(now),
            &mut StrictCrypto,
        )
        .unwrap();
    for ticket in [2, 3] {
        assert_eq!(
            f.state
                .reserve_test_session_v2_source_read(&f.session_id, id, UnixMillis::new(now + 1))
                .unwrap(),
            ticket
        );
    }
    let paid = || SessionHttpAdmission {
        ingress_attempt_id: ingress,
        possession: pop,
    };
    let nonce = f
        .state
        .claim_test_session_v2_control_lifetime(
            &f.session_id,
            id,
            paid(),
            &c,
            UnixMillis::new(now + 1),
            &mut StrictCrypto,
        )
        .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .claim_test_session_v2_control_lifetime(
                &f.session_id,
                id,
                paid(),
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
    let life = oasis7_custody_core::native_binding::ManagedNativeSessionLifetimeV1 {
        api_version: 1,
        status,
        session_expires_at: DecimalU64(90_000),
        session_idle_expires_at: DecimalU64(90_000),
    };
    let life = SignedManagedNativeSessionLifetimeV1 {
        signature: URL_SAFE_NO_PAD.encode(key().sign(&life.signing_bytes().unwrap()).to_bytes()),
        key_id: "native-key".into(),
        lifetime: life,
    };
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(&f.session_id, id, UnixMillis::new(now + 2))
            .unwrap(),
        4
    );
    life
}
fn challenge(
    f: &mut SessionFixture,
    q: &SessionControlRequestV1,
    purpose: SessionPurposeV1,
    salt: u8,
    now: u64,
) -> SessionChallengeV1 {
    let r = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: format!("ctrl-challenge-{salt}"),
        purpose,
        kind: SessionChallengeKindV1::Control(q.kind),
        logical_request_id: q.request_id.clone(),
        operation_request_digest: q.digest().unwrap(),
    };
    let bytes = serde_json::to_vec(&r).unwrap();
    let pop = control_pop(
        f,
        &bytes,
        (
            HttpPopRoute::ProofChallenges,
            HttpPopProfileV1::CustodyChallenge,
        ),
        &r.request_id,
        None,
        purpose,
        now,
    );
    let ingress = format!("ctrl-challenge-http-{salt}");
    control_paid(f, &pop, &ingress, now);
    let source = format!("ctrl-challenge-source-{salt}");
    let life = control_source(f, &pop, &ingress, &source, now);
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = TestSessionLiveCurrent {
        provision: &c,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: &source,
        source_ticket: 4,
        observed_at: UnixMillis::new(now),
    };
    f.state
        .issue_test_session_v2_control_challenge(
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
        .unwrap()
}
fn call(
    f: &mut SessionFixture,
    q: &SessionControlRequestV1,
    purpose: SessionPurposeV1,
    salt: u8,
    now: u64,
) -> std::result::Result<SessionControlAckV1, ProvisionApprovalError> {
    let nonce = challenge(f, q, purpose, salt, now);
    let bytes = serde_json::to_vec(q).unwrap();
    let route = match q.kind {
        SessionControlKindV1::Initialize => HttpPopRoute::Initialize(f.session_id.clone()),
        SessionControlKindV1::ToolsList => HttpPopRoute::ToolsList(f.session_id.clone()),
        SessionControlKindV1::Heartbeat => HttpPopRoute::Heartbeat(f.session_id.clone()),
        SessionControlKindV1::Finish => HttpPopRoute::Finish(f.session_id.clone()),
        SessionControlKindV1::Abort => HttpPopRoute::Abort(f.session_id.clone()),
    };
    let pop = control_pop(
        f,
        &bytes,
        (route, HttpPopProfileV1::Custody),
        &q.request_id,
        Some(&nonce),
        purpose,
        now + 3,
    );
    let ingress = format!("ctrl-http-{salt}");
    control_paid(f, &pop, &ingress, now + 3);
    let pure = matches!(
        q.kind,
        SessionControlKindV1::Finish | SessionControlKindV1::Abort
    );
    let source = format!("ctrl-source-{salt}");
    let life = if pure {
        None
    } else {
        Some(control_source(f, &pop, &ingress, &source, now + 3))
    };
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = life.as_ref().map(|life| TestSessionLiveCurrent {
        provision: &c,
        native_scope: &ns,
        lifetime: life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: &source,
        source_ticket: 4,
        observed_at: UnixMillis::new(now + 3),
    });
    let before = f.state.clone();
    let proof = SessionProofRequest {
        ingress_attempt_id: &ingress,
        original_bytes: &bytes,
        possession: &pop,
    };
    let out = if pure {
        f.state
            .control_test_session_v2(proof, None, UnixMillis::new(now + 5), &mut NoNativeChecks)
    } else {
        f.state.control_test_session_v2(
            proof,
            current.as_ref(),
            UnixMillis::new(now + 5),
            &mut StrictCrypto,
        )
    };
    if out.is_err() {
        assert_eq!(f.state, before);
    }
    out
}
fn q(f: &SessionFixture, id: &str, kind: SessionControlKindV1) -> SessionControlRequestV1 {
    SessionControlRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: id.into(),
        kind,
    }
}
#[test]
fn session_v2_control_five_methods_keep_business_off_and_closing_only() {
    let mut f = claimed();
    let original = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()
        .iter()
        .map(|(p, r)| (*p, r.budgets.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for (kind, id, salt, now) in [
        (SessionControlKindV1::Initialize, "initialize", 51, 13_000),
        (SessionControlKindV1::ToolsList, "tools", 52, 14_000),
        (SessionControlKindV1::Heartbeat, "heartbeat", 53, 15_000),
    ] {
        let r = q(&f, id, kind);
        let ack = call(&mut f, &r, SessionPurposeV1::Execution, salt, now).unwrap();
        assert_eq!(ack.kind, kind);
        assert!(ack.lease_until.get() <= 30_000);
    }
    let early = q(&f, "early-heartbeat", SessionControlKindV1::Heartbeat);
    assert!(call(&mut f, &early, SessionPurposeV1::Execution, 54, 16_000).is_err());
    let finish = q(&f, "finish", SessionControlKindV1::Finish);
    call(&mut f, &finish, SessionPurposeV1::ReconcileRead, 55, 17_000).unwrap();
    let abort = q(&f, "abort-after-finish", SessionControlKindV1::Abort);
    call(&mut f, &abort, SessionPurposeV1::ReconcileRead, 56, 18_000).unwrap();
    let value = serde_json::to_value(&f.state).unwrap();
    let s = &value["test_sessions_v2"][&f.session_id];
    assert_eq!(s["closing"][0], "finished");
    assert_eq!(s["last_heartbeat"], "15005");
    assert!(s["execution_book"].is_null());
    assert!(s["context"].is_null());
    assert!(s["actions"].as_object().unwrap().is_empty());
    assert!(s["termination_state"]["stop"].is_null());
    assert!(s["termination_state"]["close"].is_null());
    for (p, r) in f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()
    {
        assert_eq!(r.budgets, original[p]);
    }
    let bytes = serde_json::to_vec(&f.state).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, f.state);
    let mut bad = value.clone();
    bad["test_sessions_v2"][&f.session_id]["controls"]["heartbeat"]["lease_until"] =
        serde_json::json!("30001");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
    let mut bad = value;
    bad["test_sessions_v2"][&f.session_id]
        .as_object_mut()
        .unwrap()
        .remove("control_records");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
}

#[test]
fn session_v2_control_ack_history_needs_a_new_paid_http_but_no_native_or_lease_refresh() {
    let mut f = claimed();
    let request = q(&f, "history-initialize", SessionControlKindV1::Initialize);
    let ack = call(&mut f, &request, SessionPurposeV1::Execution, 61, 13_000).unwrap();
    let raw = serde_json::to_value(&f.state).unwrap();
    let challenge: SessionChallengeV1 = serde_json::from_value(
        raw["test_sessions_v2"][&f.session_id]["challenges"]["ctrl-challenge-61"].clone(),
    )
    .unwrap();
    let bytes = serde_json::to_vec(&request).unwrap();
    let pop = control_pop(
        &f,
        &bytes,
        (
            HttpPopRoute::Initialize(f.session_id.clone()),
            HttpPopProfileV1::Custody,
        ),
        &request.request_id,
        Some(&challenge),
        SessionPurposeV1::Execution,
        14_000,
    );
    control_paid(&mut f, &pop, "control-historical-http", 14_000);
    let paid = f.state.clone();
    let history = f
        .state
        .control_test_session_v2(
            SessionProofRequest {
                ingress_attempt_id: "control-historical-http",
                original_bytes: &bytes,
                possession: &pop,
            },
            None,
            UnixMillis::new(14_000),
            &mut NoNativeChecks,
        )
        .unwrap();
    assert_eq!(history, ack);
    assert_eq!(f.state, paid);
}

#[test]
fn session_v2_control_expiry_is_durable_local_closing_without_cleanup_effects() {
    let mut f = claimed();
    let hb = q(&f, "expiry-heartbeat", SessionControlKindV1::Heartbeat);
    let ack = call(&mut f, &hb, SessionPurposeV1::Execution, 71, 13_000).unwrap();
    let write = f
        .state
        .test_session_v2_header(&f.session_id)
        .unwrap()
        .0
        .write_deadline
        .0;
    assert_eq!(ack.lease_until.get(), write);
    assert_eq!(
        f.state
            .expire_test_sessions_v2(UnixMillis::new(write))
            .unwrap(),
        vec![f.session_id.clone()]
    );
    let data = serde_json::to_value(&f.state).unwrap();
    let v = &data["test_sessions_v2"][&f.session_id];
    assert_eq!(v["closing"][0], "deadline");
    assert!(v["termination_state"]["stop"].is_null());
    assert!(v["termination_state"]["close"].is_null());
    let bytes = serde_json::to_vec(&f.state).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, f.state);
}

#[path = "test_session_v2_control_pending_restore_tests.rs"]
mod pending_restore_tests;

#[path = "test_session_v2_control_budget_cause_tests.rs"]
mod budget_cause_tests;
