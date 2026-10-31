//! Genuine activated/claimed V2 + actual purpose HTTP. No fake roots or token.
use super::*;
fn execution_source(
    f: &mut SessionFixture,
    id: &str,
    paid: (
        &[u8],
        &oasis7_custody_core::http_pop::HttpPossessionReceipt,
        &str,
    ),
    now: u64,
) {
    let (body, pop, ingress) = paid;
    let wrapper = f
        .state
        .reserve_test_session_v2_execution_source(
            &f.session_id,
            id,
            [93; 32],
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: body,
                possession: pop,
            },
            UnixMillis::new(now),
        )
        .unwrap();
    complete_execution_source(f, id, wrapper.source.verify_nonce, now);
}
fn complete_execution_source(f: &mut SessionFixture, id: &str, nonce: String, now: u64) {
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
    v.proof_state = ProofStateV1::Active;
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
}
fn lifetime(f: &SessionFixture, nonce: String, at: u64) -> SignedManagedNativeSessionLifetimeV1 {
    let status = fixture::root_session(&f.state, nonce, at).status.status;
    let lifetime = oasis7_custody_core::native_binding::ManagedNativeSessionLifetimeV1 {
        api_version: 1,
        status,
        session_expires_at: DecimalU64(90_000),
        session_idle_expires_at: DecimalU64(90_000),
    };
    SignedManagedNativeSessionLifetimeV1 {
        key_id: "native-key".into(),
        signature: URL_SAFE_NO_PAD
            .encode(key().sign(&lifetime.signing_bytes().unwrap()).to_bytes()),
        lifetime,
    }
}
#[test]
fn session_v2_execution_book_requires_actual_paid_nonce_source_and_lifetime_once() {
    let mut f = claimed_execution();
    let (body, pop) = action_pop(&mut f, "actual-book-move", 13_000, false);
    let ingress = "actual-book-ingress";
    charge(
        &mut f,
        &body,
        &pop,
        ingress,
        SessionPurposeV1::Execution,
        13_002,
    )
    .unwrap();
    let source = "execution-source-for-book";
    execution_source(&mut f, source, (&body, &pop, ingress), 14_000);
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let proof = || SessionProofRequest {
        ingress_attempt_id: ingress,
        original_bytes: &body,
        possession: &pop,
    };
    let before = f.state.clone();
    let nonce = f
        .state
        .claim_test_session_v2_execution_lifetime(
            &f.session_id,
            source,
            proof(),
            &c,
            UnixMillis::new(14_001),
            &mut StrictCrypto,
        )
        .unwrap();
    let pending = f.state.clone();
    assert!(
        f.state
            .claim_test_session_v2_execution_lifetime(
                &f.session_id,
                source,
                proof(),
                &c,
                UnixMillis::new(14_001),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(pending, f.state);
    let old_root = before
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::Execution]
        .clone();
    let new_root = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::Execution]
        .clone();
    assert_eq!(
        new_root.budgets.transport.remaining().unwrap(),
        old_root.budgets.transport.remaining().unwrap() - 1
    );
    assert_eq!(
        new_root.budgets.requests.remaining().unwrap(),
        old_root.budgets.requests.remaining().unwrap() - 1
    );
    let original = serde_json::to_value(&before).unwrap();
    let json = serde_json::to_value(&f.state).unwrap();
    let b = &json["test_sessions_v2"][&f.session_id]["execution_book"];
    assert!(b.is_object());
    assert!(b["stages"].as_object().unwrap().is_empty());
    assert!(b["context_slots"].as_object().unwrap().is_empty());
    assert_eq!(b["last_allocation_sequence"], serde_json::json!("0"));
    let native_old: oasis7_custody_core::budget::BudgetLedger = serde_json::from_value(
        original["test_sessions_v2"][&f.session_id]["identity_transport"].clone(),
    )
    .unwrap();
    let native_new: oasis7_custody_core::budget::BudgetLedger = serde_json::from_value(
        json["test_sessions_v2"][&f.session_id]["identity_transport"].clone(),
    )
    .unwrap();
    assert_eq!(native_new, native_old);
    let restored: AccountProvisionApprovalState = serde_json::from_value(json).unwrap();
    assert_eq!(restored, f.state);
    let fact = lifetime(&f, nonce, 14_002);
    let ns = scope(&f.pin);
    f.state
        .archive_test_session_v2_execution_lifetime(
            &f.session_id,
            &fact,
            &ns,
            UnixMillis::new(14_003),
            &mut StrictCrypto,
        )
        .unwrap();
    // Keep every genuine record and signature except this newly archived LF.
    // Its alternate-key signature is mathematically strong, but not the pin.
    let alternate = SigningKey::from_bytes(&[221; 32]);
    let alternate_public: String = alternate
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut wrong = fact.clone();
    wrong.signature = URL_SAFE_NO_PAD.encode(
        alternate
            .sign(&wrong.lifetime.signing_bytes().unwrap())
            .to_bytes(),
    );
    wrong.verify_root("native-key", &alternate_public).unwrap();
    let mut historical = serde_json::to_value(&f.state).unwrap();
    historical["test_sessions_v2"][&f.session_id]["execution_book"]["archived_lifetime"] =
        serde_json::to_value(&wrong).unwrap();
    let historical: AccountProvisionApprovalState = serde_json::from_value(historical).unwrap();
    assert!(
        historical
            .test_session_v2_native_restore_checks(&f.session_id, &ns)
            .is_err()
    );
    for checks in f
        .state
        .test_session_v2_native_restore_checks(&f.session_id, &ns)
        .unwrap()
    {
        StrictCrypto.verify(&checks).unwrap();
    }

    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(&f.session_id, source, UnixMillis::new(14_003))
            .unwrap(),
        4
    );
    let current = TestSessionLiveCurrent {
        provision: &c,
        native_scope: &ns,
        lifetime: &fact,
        private_sid_generation: DecimalU64(1),
        source_ticket: 4,
        source_operation_id: source,
        observed_at: UnixMillis::new(14_000),
    };
    f.state
        .admit_test_session_v2_execution_lifetime(
            &f.session_id,
            &current,
            UnixMillis::new(14_004),
            &mut StrictCrypto,
        )
        .unwrap();
    let admitted = f.state.clone();
    assert!(
        f.state
            .admit_test_session_v2_execution_lifetime(
                &f.session_id,
                &current,
                UnixMillis::new(14_004),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, admitted);
    let json = serde_json::to_value(&f.state).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_value(json).unwrap();
    assert_eq!(restored, f.state);
}
#[test]
fn session_v2_late_strong_execution_lifetime_archives_but_cannot_admit() {
    let mut f = claimed_execution();
    let (body, pop) = action_pop(&mut f, "late-book-move", 13_000, false);
    let ingress = "late-book-ingress";
    charge(
        &mut f,
        &body,
        &pop,
        ingress,
        SessionPurposeV1::Execution,
        13_002,
    )
    .unwrap();
    let source = "late-execution-source";
    execution_source(&mut f, source, (&body, &pop, ingress), 14_000);
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let nonce = f
        .state
        .claim_test_session_v2_execution_lifetime(
            &f.session_id,
            source,
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &body,
                possession: &pop,
            },
            &c,
            UnixMillis::new(14_001),
            &mut StrictCrypto,
        )
        .unwrap();
    let fact = lifetime(&f, nonce, 14_002);
    let ns = scope(&f.pin);
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(&f.session_id, source, UnixMillis::new(14_003))
            .unwrap(),
        4
    );
    f.state
        .archive_test_session_v2_execution_lifetime(
            &f.session_id,
            &fact,
            &ns,
            UnixMillis::new(20_100),
            &mut StrictCrypto,
        )
        .unwrap();
    let archived = f.state.clone();
    let current = TestSessionLiveCurrent {
        provision: &c,
        native_scope: &ns,
        lifetime: &fact,
        private_sid_generation: DecimalU64(1),
        source_ticket: 4,
        source_operation_id: source,
        observed_at: UnixMillis::new(14_000),
    };
    assert!(
        f.state
            .admit_test_session_v2_execution_lifetime(
                &f.session_id,
                &current,
                UnixMillis::new(20_100),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, archived);
}

#[path = "test_session_v2_execution_book_paid_binding_tests.rs"]
mod paid_binding_tests;

#[path = "test_session_v2_execution_source_binding_tests.rs"]
mod source_binding_tests;

#[path = "test_session_v2_execution_prepare_factory_tests.rs"]
mod prepare_factory_tests;
