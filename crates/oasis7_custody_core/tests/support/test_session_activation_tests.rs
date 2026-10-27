use super::*;
use oasis7_custody_core::http::Role;
fn fresh_access(
    s: &AccountProvisionApprovalState,
    d: &TestSessionPreflightDescriptorV1,
    now: u64,
) -> OwnerSetupAccessEvidenceV1 {
    super::super::super::super::fresh_access_with_nonces(s, d.access_nonces.clone(), now)
}
fn lifetime(
    s: &AccountProvisionApprovalState,
    nonce: String,
    now: u64,
) -> SignedManagedNativeSessionLifetimeV1 {
    let status = fixture::root_session(s, nonce, now).status.status;
    let lifetime = oasis7_custody_core::native_binding::ManagedNativeSessionLifetimeV1 {
        api_version: 1,
        status,
        session_expires_at: DecimalU64(90_000),
        session_idle_expires_at: DecimalU64(90_000),
    };
    SignedManagedNativeSessionLifetimeV1 {
        signature: URL_SAFE_NO_PAD
            .encode(key().sign(&lifetime.signing_bytes().unwrap()).to_bytes()),
        key_id: "native-key".into(),
        lifetime,
    }
}
fn source_current(f: &mut SessionFixture, d: &TestSessionPreflightDescriptorV1, now: u64) {
    let p = Policy::new(f.state.draft());
    let c = p.current(&f.context);
    f.state
        .reserve_test_session_source_read(
            "session-one",
            &d.source_operation_id,
            UnixMillis::new(now),
        )
        .unwrap();
    let request = f
        .state
        .claim_test_session_source(
            "session-one",
            &d.source_operation_id,
            &c,
            UnixMillis::new(now + 1),
        )
        .unwrap();
    assert_eq!(request.verify_nonce, d.source_nonce);
    let mut v = verification(
        f.state.test_session_original("session-one").unwrap(),
        &f.context,
    )
    .verification;
    v.purpose = VerificationPurposeV1::InspectOrigin;
    v.verify_nonce = d.source_nonce.clone();
    v.observed_at = DecimalU64(now + 2);
    v.proof_state = if now + 2 >= 38_000 {
        ProofStateV1::Expired
    } else {
        ProofStateV1::Active
    };
    f.state
        .complete_test_session_source(
            "session-one",
            &d.source_operation_id,
            sign_verification(v),
            &c,
            UnixMillis::new(now + 2),
            &mut StrictCrypto,
        )
        .unwrap();
}
fn activated(start: u64) -> SessionFixture {
    let mut f = pending_fixture();
    let d = f
        .state
        .claim_test_session_preflight("session-one", UnixMillis::new(start))
        .unwrap();
    let snapshot = f.state.clone();
    assert!(
        f.state
            .claim_test_session_preflight("session-one", UnixMillis::new(start))
            .is_err()
    );
    assert_eq!(f.state, snapshot);
    source_current(&mut f, &d, start);
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let ns = scope(&f.pin);
    macro_rules! current {
        ($source:expr,$ticket:expr,$now:expr) => {
            TestSessionPreflightCurrent {
                provision: &c,
                native_scope: &ns,
                source_operation_id: &d.source_operation_id,
                source_ticket: $source,
                oracle_ticket: $ticket,
                observed_at: UnixMillis::new($now),
            }
        };
    }
    let before = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_preflight_read(
                "session-one",
                OwnerSetupReadPurposeV1::Sign,
                Role::Owner,
                UnixMillis::new(start + 3)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    for ticket in 1..=2 {
        assert_eq!(
            f.state
                .reserve_test_session_source_read(
                    "session-one",
                    &d.source_operation_id,
                    UnixMillis::new(start + 3 + u64::from(ticket))
                )
                .unwrap(),
            ticket + 1
        );
        assert_eq!(
            f.state
                .reserve_test_session_preflight_read(
                    "session-one",
                    OwnerSetupReadPurposeV1::Session,
                    Role::Identity,
                    UnixMillis::new(start + 3 + u64::from(ticket))
                )
                .unwrap(),
            ticket
        );
        f.state
            .test_session_preflight_permit(
                "session-one",
                Role::Identity,
                OwnerSetupReadPurposeV1::Session,
                &current!(ticket + 1, ticket, start + 3 + u64::from(ticket)),
                UnixMillis::new(start + 3 + u64::from(ticket)),
                &mut StrictCrypto,
            )
            .unwrap();
    }
    f.state
        .claim_test_session_preflight_step(
            "session-one",
            SessionPreflightStepV1::FreshAccessPair,
            UnixMillis::new(start + 6),
        )
        .unwrap();
    let access = fresh_access(&f.state, &d, start + 7);
    assert_eq!(
        f.state
            .reserve_test_session_source_read(
                "session-one",
                &d.source_operation_id,
                UnixMillis::new(start + 8)
            )
            .unwrap(),
        4
    );
    assert_eq!(
        f.state
            .reserve_test_session_preflight_read(
                "session-one",
                OwnerSetupReadPurposeV1::Sign,
                Role::Owner,
                UnixMillis::new(start + 8)
            )
            .unwrap(),
        3
    );
    let mut wrong = access.clone();
    wrong.policy_session.status.status.nonce = "bb".repeat(32);
    wrong.policy_session.status.signature = URL_SAFE_NO_PAD.encode(
        key()
            .sign(&wrong.policy_session.status.status.signing_bytes().unwrap())
            .to_bytes(),
    );
    let before = f.state.clone();
    assert!(
        f.state
            .record_test_session_preflight_access(
                "session-one",
                wrong,
                &current!(4, 3, start + 8),
                UnixMillis::new(start + 8),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    f.state
        .record_test_session_preflight_access(
            "session-one",
            access.clone(),
            &current!(4, 3, start + 8),
            UnixMillis::new(start + 8),
            &mut StrictCrypto,
        )
        .unwrap();
    let read = f
        .state
        .test_session_preflight_signing_request(
            "session-one",
            &current!(4, 3, start + 8),
            UnixMillis::new(start + 8),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(
        f.state
            .reserve_test_session_source_read(
                "session-one",
                &d.source_operation_id,
                UnixMillis::new(start + 9)
            )
            .unwrap(),
        5
    );
    assert_eq!(
        f.state
            .reserve_test_session_preflight_read(
                "session-one",
                OwnerSetupReadPurposeV1::Sign,
                Role::Owner,
                UnixMillis::new(start + 9)
            )
            .unwrap(),
        4
    );
    assert_eq!(
        f.state
            .test_session_preflight_signing_request(
                "session-one",
                &current!(5, 4, start + 9),
                UnixMillis::new(start + 9),
                &mut StrictCrypto
            )
            .unwrap(),
        read
    );
    let old = f
        .state
        .participant_original("participant-one")
        .unwrap()
        .bundle
        .as_ref()
        .unwrap();
    let original = ws::participant_discovery::ParticipantOwnerViewRequestV1 {
        owner: old.owner.clone(),
        admission: access.admission,
        read: ws::sign_read_request(ws::VIEW_JSON_PATH, read, &"3e".repeat(32)).unwrap(),
        response_profile: Some(ParticipantOwnerViewProfileV1::OwnerSetupV1),
    };
    assert_eq!(
        f.state
            .reserve_test_session_source_read(
                "session-one",
                &d.source_operation_id,
                UnixMillis::new(start + 10)
            )
            .unwrap(),
        6
    );
    assert_eq!(
        f.state
            .reserve_test_session_preflight_read(
                "session-one",
                OwnerSetupReadPurposeV1::Sign,
                Role::Owner,
                UnixMillis::new(start + 10)
            )
            .unwrap(),
        5
    );
    f.state
        .publish_test_session_preflight_read(
            "session-one",
            original.clone(),
            &current!(6, 5, start + 10),
            UnixMillis::new(start + 10),
            &mut StrictCrypto,
        )
        .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_preflight_read(
                "session-one",
                OwnerSetupReadPurposeV1::Sign,
                Role::Owner,
                UnixMillis::new(start + 10)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    assert!(f.state.test_sessions()["session-one"].roots.is_empty());
    f.state
        .claim_test_session_preflight_step(
            "session-one",
            SessionPreflightStepV1::Lifetime,
            UnixMillis::new(start + 11),
        )
        .unwrap();
    let life = lifetime(&f.state, d.lifetime_nonce.clone(), start + 12);
    assert_eq!(
        f.state
            .reserve_test_session_source_read(
                "session-one",
                &d.source_operation_id,
                UnixMillis::new(start + 13)
            )
            .unwrap(),
        7
    );
    let mut wrong = life.clone();
    wrong.lifetime.session_idle_expires_at = DecimalU64(60_000);
    wrong.signature = URL_SAFE_NO_PAD.encode(
        key()
            .sign(&wrong.lifetime.signing_bytes().unwrap())
            .to_bytes(),
    );
    let before = f.state.clone();
    assert!(
        f.state
            .record_test_session_preflight_lifetime(
                "session-one",
                wrong,
                &current!(7, 5, start + 13),
                UnixMillis::new(start + 13),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    f.state
        .record_test_session_preflight_lifetime(
            "session-one",
            life.clone(),
            &current!(7, 5, start + 13),
            UnixMillis::new(start + 13),
            &mut StrictCrypto,
        )
        .unwrap();
    f.state
        .claim_test_session_preflight_step(
            "session-one",
            SessionPreflightStepV1::OwnerRead,
            UnixMillis::new(start + 14),
        )
        .unwrap();
    let Some(OwnerSetupResultV1::FinalObservation { owner, .. }) = f
        .state
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
        .unwrap()
        .result
        .as_ref()
    else {
        unreachable!()
    };
    let response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &original).unwrap(),
        owner.payload.clone(),
        &"3f".repeat(32),
    )
    .unwrap();
    let world = OwnerSetupWorldEvidenceV1 {
        request: original,
        response,
    };
    assert_eq!(
        f.state
            .reserve_test_session_source_read(
                "session-one",
                &d.source_operation_id,
                UnixMillis::new(start + 15)
            )
            .unwrap(),
        8
    );
    let actual = ProvedEnrollmentObservation {
        actual_actor_id: "actor-one",
        expected_actor_id: "actor-one",
        actual_realm: "enrollment",
        expected_realm: "enrollment",
        expected_public_origin: "https://custody.example",
        fact: &f.fact,
    };
    let approval = TestSessionApprovalCurrent {
        provision: &c,
        native_scope: &ns,
        enrollment: &actual,
        deployments: &f.deployments,
        observed_at: UnixMillis::new(start + 15),
    };
    let current = TestSessionCurrent {
        approval,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        world: &world,
        source_operation_id: &d.source_operation_id,
    };
    f.state
        .activate_test_session(
            "session-one",
            &current,
            UnixMillis::new(start + 15),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(f.state.test_sessions()["session-one"].roots.len(), 3);
    assert_eq!(f.state.provision().stage(), ProvisionStage::Prepared);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .unwrap(),
        f.state
    );
    f
}
#[test]
fn test_session_strong_preflight_activates_only_exact_world_lifetime_and_three_siblings() {
    let f = activated(8005);
    assert_eq!(
        f.state.test_sessions()["session-one"].roots[&SessionPurposeV1::Execution].deadline,
        UnixMillis::new(50_000)
    );
    assert_eq!(
        f.state.test_sessions()["session-one"].roots[&SessionPurposeV1::ReconcileRead].deadline,
        UnixMillis::new(80_000)
    );
    assert_eq!(
        f.state.test_sessions()["session-one"].roots[&SessionPurposeV1::TerminationPlan].deadline,
        UnixMillis::new(60_000)
    );
}
#[test]
fn test_session_consumed_mfa_and_enrollment_http_ttl_do_not_replace_absolute_plan_deadline() {
    let f = activated(39_000);
    assert_eq!(
        f.state.test_sessions()["session-one"].consumed_at,
        Some(UnixMillis::new(8004))
    );
    assert_eq!(
        f.state.test_sessions()["session-one"].roots[&SessionPurposeV1::Execution].deadline,
        UnixMillis::new(50_000)
    );
}

#[path = "test_session_roots_tests.rs"]
mod roots;

#[path = "test_session_preflight_budget_tests.rs"]
mod preflight_budget;
