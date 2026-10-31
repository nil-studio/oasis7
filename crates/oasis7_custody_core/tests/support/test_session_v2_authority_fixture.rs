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
    let plan = TestSessionPlanV2 {
        api_version: 2,
        model_budget: None,
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
    input.approval.proof.issued_at = DecimalU64(8000);
    input.approval.proof.expires_at = DecimalU64(38_000);
    input.approval.proof.model_budget_summary_contract = input
        .request
        .expected_model_budget_summary_contract()
        .unwrap();
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
    wrong.approval.proof.model_budget_summary_contract = wrong
        .request
        .expected_model_budget_summary_contract()
        .unwrap();
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
