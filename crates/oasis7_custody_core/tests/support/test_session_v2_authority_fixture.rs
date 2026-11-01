//! Real Core V2 authority transitions with signed fixture services, not OS/live acceptance.
use super::super::test_session_authority_tests::{enrollment, metadata};
use super::*;
use oasis7_custody_core::{client_enrollment::*, time_units::*};
struct SessionFixture {
    session_id: String,
    state: AccountProvisionApprovalState,
    context: ApprovalAdmissionContext,
    pin: NativeBindingPinV1,
    fact: ProvedEnrollmentFactV1,
    deployments: HostDeploymentAllowlist,
}
fn pending_fixture() -> SessionFixture {
    pending_fixture_with_control_requests(80)
}
fn pending_fixture_with_control_requests(control_requests: u64) -> SessionFixture {
    pending_fixture_with_model_budget(control_requests, None)
}
fn pending_fixture_with_model_budget(
    control_requests: u64,
    model_budget: Option<ModelInferenceBudgetV1>,
) -> SessionFixture {
    pending_fixture_with_model_budget_and_profile(control_requests, model_budget, None)
}
fn pending_fixture_with_profile(
    control_requests: u64,
    cleanup_transport_policy: Option<CleanupTransportPolicyV2>,
) -> SessionFixture {
    pending_fixture_with_model_budget_and_profile(control_requests, None, cleanup_transport_policy)
}
fn pending_fixture_with_model_budget_and_profile(
    control_requests: u64,
    mut model_budget: Option<ModelInferenceBudgetV1>,
    cleanup_transport_policy: Option<CleanupTransportPolicyV2>,
) -> SessionFixture {
    let (mut s, mut c, pin) = prepared_move0_v2_fixture();
    c.handoff_until = DecimalU64(80_000);
    let policy = Policy::new(s.draft());
    let manager = metadata(&c);
    let world = s.draft().world_identity.clone();
    let native = fixture::root_session(&s, "ab".repeat(32), 8000)
        .status
        .status
        .receipt;
    let management = OwnerSetupManagementCurrent {
        manager: &manager,
        authority: &c.authority,
        policy: policy.current(&c).policy,
        world_identity: &world,
        native_identity_namespace: &native.identity_namespace,
        fence: &c.fence,
        observed_at: 8000,
    };
    let invite = s
        .create_pairing_invite(
            PairingInviteCreateRequestV1 {
                api_version: 1,
                request_id: "v2-invite".into(),
                transport_profile: HostTransportProfileV1::NativeBrokerCustodyV1,
                tool_profile: "custody-native-v1".into(),
            },
            "v2-invite",
            &management,
            UnixMillis::new(8000),
        )
        .unwrap();
    let fact = enrollment(
        invite.target().unwrap(),
        s.draft().entry.environment.clone(),
        HostTransportProfileV1::NativeBrokerCustodyV1,
    );
    let d = &fact.registration.request.descriptor;
    let deployments = HostDeploymentAllowlist::from_fixed_config(vec![HostDeploymentPinV1 {
        environment: d.environment.clone(),
        host_instance_id: d.host_instance_id.clone(),
        descriptor_digest: d.digest().unwrap(),
        client_public_key: d.client_public_key.clone(),
    }])
    .unwrap();
    let ns = scope(&pin);
    let actual_enrollment = ProvedEnrollmentObservation {
        actual_actor_id: "actor-one",
        expected_actor_id: "actor-one",
        actual_realm: "enrollment",
        expected_realm: "enrollment",
        expected_public_origin: "https://custody.example",
        fact: &fact,
    };
    let basis = s
        .test_session_basis_v2(
            &TestSessionBasisRequestV2 {
                api_version: 2,
                request_id: "v2-session-request".into(),
                enrollment_id: fact.registration.enrollment_id.clone(),
                setup_id: "setup-one".into(),
                cleanup_until: UnixMillis::new(60_000),
                cleanup_transport_policy,
            },
            &TestSessionManagementCurrent {
                management: &management,
                native_scope: &ns,
                enrollment: &actual_enrollment,
                deployments: &deployments,
            },
            UnixMillis::new(8002),
        )
        .unwrap();
    let session_scope = basis.scope;
    let session_id = session_scope.session_id.clone();
    let term = basis.termination;
    let owner = s.owner_setup_v2_plan().unwrap();
    if let Some(budget) = &mut model_budget {
        budget.native_account_id = c.source.manager_account_id.clone();
        budget.native_public_key = c.native_key_pins.values().next().unwrap().clone();
    }
    let plan = TestSessionPlanV2 {
        api_version: 2,
        model_budget,
        termination: Box::new(term),
        request_id: "v2-session-request".into(),
        enrollment_id: fact.registration.enrollment_id.clone(),
        basis_digest: s
            .test_session_v2_basis_digest(&session_scope, &c.authority)
            .unwrap(),
        owner_setup_plan_digest: owner.digest().unwrap(),
        prepared_resource_basis_digest: None,
        selection: owner.selection.clone(),
        actions: vec![owner.selection.approved_action()],
        allowed_resource_refs: vec![],
        write_deadline: DecimalU64(30_000),
        read_until: DecimalU64(90_000),
        cleanup_until: DecimalU64(60_000),
        request_limit: DecimalU64(100),
        transport_limit: DecimalU64(400),
        business_limit: DecimalU64(0),
        read_budget: PurposeBudgetV1 {
            requests: DecimalU64(100),
            transport: DecimalU64(300),
            business: DecimalU64(0),
        },
        cleanup_budget: PurposeBudgetV1 {
            requests: DecimalU64(2),
            transport: DecimalU64(24),
            business: DecimalU64(0),
        },
        control_budget: ControlBudgetV1 {
            requests: DecimalU64(control_requests),
            transport_per_hop: DecimalU64(240),
            identity_transport: DecimalU64(160),
            transport_hops: 1,
        },
    };
    let options = s
        .test_session_prepare_request_v2(
            &TestSessionPrepareRequestV2 {
                api_version: 2,
                approval_request_id: "v2-session-new-UV".into(),
                plan: plan.clone(),
                scope: session_scope.clone(),
            },
            &TestSessionManagementCurrent {
                management: &management,
                native_scope: &ns,
                enrollment: &actual_enrollment,
                deployments: &deployments,
            },
            UnixMillis::new(8002),
        )
        .unwrap();
    let mut input = s.accepted_approval().unwrap().input().clone();
    input.operation_id = session_id.clone();
    input.request.approval_request_id = "v2-session-new-UV".into();
    input.request.context = c.authority.clone();
    input.request = options;
    input.approval.proof.context = input.request.context.clone();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.nonce = "v2-session-new-MFA-nonce".into();
    input.approval.proof.approval_id = "v2-session-proof".into();
    input.approval.proof.approval_request_id = input.request.approval_request_id.clone();
    if cleanup_transport_policy == Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2) {
        // The new profile uses one actual Native root for consent and managed
        // facts. Configure the public pin before issuing this new ceremony;
        // never relabel or re-sign an already-consumed approval.
        let configured = scope(&pin);
        c.native_key_pins.insert(
            configured.native_root_key_id.into(),
            configured.native_root_public_key.into(),
        );
        input.approval.proof.key_id = configured.native_root_key_id.into();
    }
    input.approval.proof.issued_at = DecimalU64(8000);
    input.approval.proof.expires_at = DecimalU64(38_000);
    input.approval.proof.model_budget_summary_contract =
        native_model_budget_tests::summary_contract(&input, &c);
    input.approval = sign_proof(input.approval.proof);
    let policy = Policy::new(s.draft());
    let p = policy.current(&c);
    let ns = scope(&pin);
    let actual = ProvedEnrollmentObservation {
        actual_actor_id: "actor-one",
        expected_actor_id: "actor-one",
        actual_realm: "enrollment",
        expected_realm: "enrollment",
        expected_public_origin: "https://custody.example",
        fact: &fact,
    };
    let current = TestSessionApprovalCurrent {
        provision: &p,
        native_scope: &ns,
        enrollment: &actual,
        deployments: &deployments,
        observed_at: UnixMillis::new(8002),
    };
    let before = s.clone();
    let mut wrong = input.clone();
    if let CustodyIntentV1::TestSessionV2 { plan, scope } = &mut wrong.request.intent {
        plan.termination.original_delegation.nonce += 1;
        scope.termination_plan_digest = plan.termination.digest().unwrap();
        plan.basis_digest = s.test_session_v2_basis_digest(scope, &c.authority).unwrap();
        plan.validate_scope(scope, 8002).unwrap();
    }
    wrong.approval.proof.intent_digest = wrong.request.intent_digest().unwrap();
    wrong.approval.proof.model_budget_summary_contract =
        native_model_budget_tests::summary_contract(&wrong, &c);
    wrong.approval = sign_proof(wrong.approval.proof);
    StrictCrypto
        .verify(
            &ApprovalAdmissionBook::default()
                .begin_checks(&wrong, &"e1".repeat(32), &c, 8002)
                .unwrap(),
        )
        .unwrap();
    assert!(
        s.begin_test_session_v2(
            TestSessionAdmissionV2 {
                input: wrong,
                verify_nonce: "e1".repeat(32)
            },
            &current,
            UnixMillis::new(8002),
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(s, before);
    let record = s
        .begin_test_session_v2(
            TestSessionAdmissionV2 {
                input,
                verify_nonce: "e1".repeat(32),
            },
            &current,
            UnixMillis::new(8002),
            &mut StrictCrypto,
        )
        .unwrap();
    assert!(
        s.test_session_v2_snapshot(&session_id)
            .unwrap()
            .roots()
            .is_empty()
    );
    assert!(s.test_sessions().is_empty());
    s.claim_test_session_v2_approval_verification(&session_id, UnixMillis::new(8003))
        .unwrap();
    let mut v = verification(&record, &c).verification;
    v.key_id = record
        .native_verify_request()
        .unwrap()
        .approval
        .proof
        .key_id;
    v.observed_at = DecimalU64(8003);
    s.consume_test_session_v2_approval(
        CommitTestSessionV2 {
            session_id: session_id.clone(),
            signed: sign_verification(v),
            source_nonce: "e2".repeat(32),
        },
        &current,
        UnixMillis::new(8004),
        &mut StrictCrypto,
    )
    .unwrap();
    assert!(
        s.test_session_v2_snapshot(&session_id)
            .unwrap()
            .roots()
            .is_empty()
    );
    SessionFixture {
        session_id,
        state: s,
        context: c,
        pin,
        fact,
        deployments,
    }
}
#[path = "test_session_v2_activation_tests.rs"]
mod activation_tests;

#[test]
fn new_v2_source_cannot_reuse_another_source_nonce_or_restore_it() {
    let mut f = pending_fixture();
    f.state
        .reserve_test_session_v2_source(
            &f.session_id,
            SessionSourceReservation {
                operation_id: "first-source",
                nonce: &"a1".repeat(32),
                funding: SessionSourceFundingV1::Control,
            },
            UnixMillis::new(8005),
        )
        .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_v2_source(
                &f.session_id,
                SessionSourceReservation {
                    operation_id: "second-source",
                    nonce: &"a1".repeat(32),
                    funding: SessionSourceFundingV1::Control,
                },
                UnixMillis::new(8006)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    f.state
        .reserve_test_session_v2_source(
            &f.session_id,
            SessionSourceReservation {
                operation_id: "second-source",
                nonce: &"a2".repeat(32),
                funding: SessionSourceFundingV1::Control,
            },
            UnixMillis::new(8006),
        )
        .unwrap();
    // Preserve all paid counters while changing only the duplicate nonce.
    let mut changed = serde_json::to_value(&f.state).unwrap();
    changed["test_sessions_v2"][&f.session_id]["sources"]["second-source"]["source"]["verify_nonce"] =
        serde_json::json!("a1".repeat(32));
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(changed).is_err());
}

#[path = "test_session_v2_credential_fixture.rs"]
mod credential_tests;

#[path = "test_session_v2_machine_tests.rs"]
mod machine_tests;
#[path = "test_session_v2_native_model_budget_tests.rs"]
mod native_model_budget_tests;

#[test]
fn world3_explicit_basis_produces_new_full_consumed_approval_without_execution_book_or_roots() {
    let old = pending_fixture();
    let new = pending_fixture_with_profile(
        80,
        Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2),
    );
    let original = old.state.test_session_v2_snapshot(&old.session_id).unwrap();
    let fresh = new.state.test_session_v2_snapshot(&new.session_id).unwrap();
    let CustodyIntentV1::TestSessionV2 { plan: old_plan, .. } = &original.input().request.intent
    else {
        panic!("actual old V2");
    };
    let CustodyIntentV1::TestSessionV2 { plan: new_plan, .. } = &fresh.input().request.intent
    else {
        panic!("actual new V2");
    };
    assert_eq!(
        old_plan.termination.cleanup_transport_policy,
        CleanupTransportPolicyV2::TermWideFiveSecondOwnedV2
    );
    assert_eq!(
        new_plan.termination.cleanup_transport_policy,
        CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2
    );
    assert_ne!(old_plan.digest().unwrap(), new_plan.digest().unwrap());
    assert_ne!(original.input().approval, fresh.input().approval);
    assert_eq!(
        new.state
            .test_session_v2_original(&new.session_id)
            .unwrap()
            .stage(),
        oasis7_custody_core::approval_admission::ApprovalAdmissionStage::Consumed
    );
    assert!(fresh.roots().is_empty());
    let json = serde_json::to_value(&new.state).unwrap();
    assert!(json["test_sessions_v2"][&new.session_id]["execution_book"].is_null());
    assert!(new.state.test_sessions().is_empty());
}
#[test]
fn world3_metadata_selector_preserves_none_wire_and_rejects_redundant_old_variant() {
    let mut q = TestSessionBasisRequestV2 {
        api_version: 2,
        request_id: "r".into(),
        enrollment_id: "e".into(),
        setup_id: "s".into(),
        cleanup_until: UnixMillis::new(60000),
        cleanup_transport_policy: None,
    };
    let raw = serde_json::to_value(&q).unwrap();
    assert_eq!(raw.as_object().unwrap().len(), 5);
    assert_eq!(
        serde_json::from_value::<TestSessionBasisRequestV2>(raw.clone()).unwrap(),
        q
    );
    assert!(raw.get("cleanup_transport_policy").is_none());
    q.cleanup_transport_policy = Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedV2);
    assert!(q.validate().is_err());
    q.cleanup_transport_policy = Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2);
    q.validate().unwrap();
    let fresh = serde_json::to_value(&q).unwrap();
    assert_eq!(
        fresh["cleanup_transport_policy"],
        "term_wide_five_second_owned_world3_v2"
    );
    assert_eq!(
        serde_json::from_value::<TestSessionBasisRequestV2>(fresh).unwrap(),
        q
    );
}

#[path = "test_session_v2_preflight_world_tests.rs"]
mod preflight_world_tests;
