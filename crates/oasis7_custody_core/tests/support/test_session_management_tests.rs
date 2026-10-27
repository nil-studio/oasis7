use super::*;
fn plan(b: &TestSessionBasisV1) -> TestSessionPlanV1 {
    TestSessionPlanV1 {
        api_version: 1,
        request_id: b.request_id.clone(),
        enrollment_id: b.client_pairing.enrollment_id.clone(),
        basis_digest: b.digest().unwrap(),
        actions: vec![ApprovedActionV1::Move {
            position_cm: [DecimalI64(11), DecimalI64(22), DecimalI64(33)],
        }],
        allowed_resource_refs: b.allowed_resource_refs.clone(),
        write_deadline: UnixMillis::new(50_000),
        read_until: UnixMillis::new(80_000),
        cleanup_until: UnixMillis::new(60_000),
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
            requests: DecimalU64(80),
            transport_per_hop: DecimalU64(240),
            identity_transport: DecimalU64(160),
            transport_hops: 1,
        },
    }
}
#[test]
fn test_session_management_codec_is_exact_readonly_and_never_returns_or_consumes_grants() {
    let f = management_fixture();
    let policy = Policy::new(f.state.draft());
    let manager = metadata(&f.context);
    let world = f.state.draft().world_identity.clone();
    let namespace = fixture::root_session(&f.state, "ab".repeat(32), 8000)
        .status
        .status
        .receipt
        .identity_namespace;
    let ns = scope(&f.pin);
    let m = OwnerSetupManagementCurrent {
        manager: &manager,
        authority: &f.context.authority,
        policy: policy.current(&f.context).policy,
        world_identity: &world,
        native_identity_namespace: &namespace,
        fence: &f.context.fence,
        observed_at: 8000,
    };
    let actual = ProvedEnrollmentObservation {
        actual_actor_id: "actor-one",
        expected_actor_id: "actor-one",
        actual_realm: "enrollment",
        expected_realm: "enrollment",
        expected_public_origin: "https://custody.example",
        fact: &f.fact,
    };
    let current = TestSessionManagementCurrent {
        management: &m,
        native_scope: &ns,
        enrollment: &actual,
        deployments: &f.deployments,
    };
    let request = TestSessionBasisRequestV1 {
        api_version: 1,
        request_id: "management-plan-one".into(),
        enrollment_id: f.fact.registration.enrollment_id.clone(),
        cleanup_until: UnixMillis::new(60_000),
    };
    let before = f.state.clone();
    let basis = f
        .state
        .test_session_basis(&request, &current, UnixMillis::new(8001))
        .unwrap();
    assert_eq!(
        basis,
        f.state
            .test_session_basis(&request, &current, UnixMillis::new(8001))
            .unwrap()
    );
    assert_eq!(basis.session_id.len(), 64);
    assert_eq!(basis.termination.session_id, basis.session_id);
    assert_eq!(
        basis.runtime_ref,
        f.fact
            .registration
            .request
            .descriptor
            .restricted_runtime_identity
    );
    assert_eq!(
        basis.skill_ref,
        f.fact.registration.request.descriptor.skill_manifest_digest
    );
    assert_eq!(
        basis.source_ref,
        f.fact.registration.request.descriptor.build_digest
    );
    let prepared = TestSessionPrepareRequestV1 {
        api_version: 1,
        approval_request_id: "management-UV-one".into(),
        plan: plan(&basis),
    };
    let options = f
        .state
        .test_session_prepare_request(&prepared, &current, UnixMillis::new(8001))
        .unwrap();
    let CustodyIntentV1::TestSession {
        session_id,
        termination_plan_digest,
        client_pairing,
        ..
    } = &options.intent
    else {
        unreachable!()
    };
    assert_eq!(session_id, &basis.session_id);
    assert_eq!(
        termination_plan_digest,
        &basis.termination.digest().unwrap()
    );
    assert_eq!(client_pairing.as_deref(), Some(&basis.client_pairing));
    let mut input = f.state.accepted_approval().unwrap().input().clone();
    input.operation_id = basis.session_id.clone();
    input.request = options;
    input.approval.proof.context = input.request.context.clone();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.approval_request_id = input.request.approval_request_id.clone();
    input.approval.proof.approval_id = "management-new-MFA".into();
    input.approval.proof.nonce = "management-new-MFA-nonce".into();
    input.approval.proof.issued_at = DecimalU64(8001);
    input.approval.proof.expires_at = DecimalU64(38001);
    input.approval = sign_proof(input.approval.proof);
    let approve = TestSessionApproveRequestV1 {
        api_version: 1,
        input,
        plan: prepared.plan,
    };
    let admission = f
        .state
        .test_session_management_admission(
            &approve,
            &"b3".repeat(32),
            &current,
            UnixMillis::new(8001),
        )
        .unwrap();
    assert_eq!(admission.termination, basis.termination);
    assert_eq!(admission.input.operation_id, basis.session_id);
    assert!(f.state.test_sessions().is_empty());
    assert_eq!(f.state, before);
    let json = serde_json::to_value(&basis).unwrap();
    for forbidden in [
        "goal_content",
        "book",
        "proof",
        "native_lifetime",
        "session_ref",
        "token",
    ] {
        assert!(json.get(forbidden).is_none())
    }
    let mut unknown = serde_json::to_value(approve).unwrap();
    unknown["raw_owner_payload"] = true.into();
    assert!(serde_json::from_value::<TestSessionApproveRequestV1>(unknown).is_err());
}
#[test]
fn test_session_management_wrong_basis_resource_epoch_enrollment_and_refs_reject_readonly() {
    let f = management_fixture();
    let policy = Policy::new(f.state.draft());
    let manager = metadata(&f.context);
    let world = f.state.draft().world_identity.clone();
    let namespace = fixture::root_session(&f.state, "ab".repeat(32), 8000)
        .status
        .status
        .receipt
        .identity_namespace;
    let ns = scope(&f.pin);
    let m = OwnerSetupManagementCurrent {
        manager: &manager,
        authority: &f.context.authority,
        policy: policy.current(&f.context).policy,
        world_identity: &world,
        native_identity_namespace: &namespace,
        fence: &f.context.fence,
        observed_at: 8000,
    };
    let actual = ProvedEnrollmentObservation {
        actual_actor_id: "actor-one",
        expected_actor_id: "actor-one",
        actual_realm: "enrollment",
        expected_realm: "enrollment",
        expected_public_origin: "https://custody.example",
        fact: &f.fact,
    };
    let current = TestSessionManagementCurrent {
        management: &m,
        native_scope: &ns,
        enrollment: &actual,
        deployments: &f.deployments,
    };
    let request = TestSessionBasisRequestV1 {
        api_version: 1,
        request_id: "management-plan-one".into(),
        enrollment_id: f.fact.registration.enrollment_id.clone(),
        cleanup_until: UnixMillis::new(60_000),
    };
    let b = f
        .state
        .test_session_basis(&request, &current, UnixMillis::new(8001))
        .unwrap();
    let before = f.state.clone();
    for field in 0..4 {
        let mut bad = plan(&b);
        match field {
            0 => bad.basis_digest = "ff".repeat(32),
            1 => bad.allowed_resource_refs.push("unobserved-resource".into()),
            2 => bad.enrollment_id = "other-enrollment".into(),
            _ => bad.cleanup_until = UnixMillis::new(70_000),
        }
        let r = TestSessionPrepareRequestV1 {
            api_version: 1,
            approval_request_id: "new-UV".into(),
            plan: bad,
        };
        assert!(
            f.state
                .test_session_prepare_request(&r, &current, UnixMillis::new(8001))
                .is_err()
        );
        assert_eq!(f.state, before)
    }
    let mut authority = f.context.authority.clone();
    authority.security_version = DecimalU64(2);
    let wrong = OwnerSetupManagementCurrent {
        manager: &manager,
        authority: &authority,
        policy: policy.current(&f.context).policy,
        world_identity: &world,
        native_identity_namespace: &namespace,
        fence: &f.context.fence,
        observed_at: 8000,
    };
    let wrong = TestSessionManagementCurrent {
        management: &wrong,
        ..current
    };
    assert!(
        f.state
            .test_session_basis(&request, &wrong, UnixMillis::new(8001))
            .is_err()
    );
    assert_eq!(f.state, before);
}
