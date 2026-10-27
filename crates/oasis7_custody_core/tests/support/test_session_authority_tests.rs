use super::*;
use oasis7_custody_core::{client_enrollment::*, http_pop::*, time_units::*};
use sha2::{Digest, Sha256};
fn enrollment(target: PairingInviteTargetRefV1, environment: String) -> ProvedEnrollmentFactV1 {
    let host = SigningKey::from_bytes(&[8; 32]);
    let public = host
        .verifying_key()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let create = ClientEnrollmentCreateRequestV1 {
        api_version: 1,
        request_id: "host-create".into(),
        target,
        descriptor: HostDeploymentDescriptorV1 {
            api_version: 1,
            environment,
            transport_profile: HostTransportProfileV1::NativeBrokerCustodyV1,
            tool_profile: "custody-native-v1".into(),
            host_instance_id: "synthetic-host-no-isolation-PASS".into(),
            client_public_key: public,
            build_digest: "bc".repeat(32),
            skill_manifest_digest: "cd".repeat(32),
            tool_manifest_digest: "de".repeat(32),
            https_origin: "https://host.example".into(),
            restricted_runtime_identity: "synthetic-principal-not-CT008".into(),
            isolation_mechanism: HostIsolationMechanismV1::SeparateOsPrincipal,
            isolation_acceptance_refs: vec!["synthetic-no-CT008-PASS".into()],
        },
    };
    let physical = EnrollmentPhysicalCurrent {
        actual_actor_id: "actor-one",
        expected_actor_id: "actor-one",
        actual_realm: "enrollment",
        expected_realm: "enrollment",
        public_origin: "https://custody.example",
    };
    let mut s = ClientEnrollmentStateV1::new(
        &serde_json::to_vec(&create).unwrap(),
        EnrollmentServerIds {
            enrollment_id: "enrollment-one",
            key_id: "client-one",
            nonce: &URL_SAFE_NO_PAD.encode([2; 32]),
        },
        &physical,
        UnixMillis::new(8000),
    )
    .unwrap();
    let r = s.registration();
    let prove = ClientEnrollmentProofRequestV1 {
        api_version: 1,
        enrollment_id: r.enrollment_id.clone(),
        request_id: "host-prove".into(),
        target: r.request.target.clone(),
        descriptor_digest: r.descriptor_digest.clone(),
        client_public_key: r.request.descriptor.client_public_key.clone(),
        server_nonce: r.server_nonce.clone(),
        challenge_expires_at: r.challenge_expires_at,
    };
    let body = serde_json::to_vec(&prove).unwrap();
    let target = CanonicalHttpTarget::new(
        &r.public_origin,
        HttpPopRoute::EnrollmentProve(r.enrollment_id.clone()),
    )
    .unwrap();
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile: HttpPopProfileV1::EnrollmentProof,
        target: &target,
        key_id: &r.key_id,
        binding_id: &r.enrollment_id,
        request_id: &prove.request_id,
        nonce: Some(&r.server_nonce),
        token: None,
        created: UnixSeconds::new(8),
        expires: UnixSeconds::new(37),
        body: &body,
    })
    .unwrap();
    let headers = material.signed_headers(&host.sign(material.signature_base()).to_bytes());
    let raw = headers
        .iter()
        .map(|h| HttpHeader {
            name: &h.name,
            value: &h.value,
        })
        .collect::<Vec<_>>();
    let empty: [u8; 32] = Sha256::digest(b"").into();
    let prepared = prepare_http_pop(
        &HttpPopMessage {
            method: "POST",
            target_uri: target.uri(),
            headers: &raw,
            representation: HttpHeaderRepresentation::RawInstances,
            body: &body,
        },
        &HttpPopBinding {
            profile: HttpPopProfileV1::EnrollmentProof,
            target: &target,
            key_id: &r.key_id,
            public_key_hex: &r.request.descriptor.client_public_key,
            binding_id: &r.enrollment_id,
            request_id: &prove.request_id,
            nonce: Some(&r.server_nonce),
            credential_digest: &empty,
            valid_until: r.challenge_expires_at,
        },
        UnixMillis::new(8001),
    )
    .unwrap();
    let receipt =
        verify_http_possession(prepared, UnixMillis::new(8001), &mut StrictCrypto).unwrap();
    s.prove(&body, receipt, &physical, UnixMillis::new(8001))
        .unwrap()
}
fn metadata(c: &ApprovalAdmissionContext) -> CustodySessionStatusV1 {
    CustodySessionStatusV1 {
        api_version: 1,
        identity_namespace: c.identity_namespace.clone(),
        identity_instance_epoch: c.identity_instance_epoch,
        account_id: c.source.manager_account_id.clone(),
        session_ref: c.source.manager_session_ref.clone(),
        credential_ref: c.source.origin_credential_id.clone(),
        credential_version: c.source.origin_credential_version,
        security_version: c.source.manager_security_version,
        policy_version: c.source.manager_policy_version,
        server_time: DecimalU64(8000),
        expires_at: c.source.session_expires_at,
        idle_expires_at: c.source.session_idle_expires_at,
    }
}
fn admission(
    s: &AccountProvisionApprovalState,
    c: &ApprovalAdmissionContext,
    f: &ProvedEnrollmentFactV1,
) -> TestSessionAdmissionV1 {
    let term = s
        .test_session_termination_basis("session-one", UnixMillis::new(60_000))
        .unwrap();
    let mut input = s.accepted_approval().unwrap().input().clone();
    input.operation_id = "session-one".into();
    input.request.approval_request_id = "session-UV".into();
    input.request.context = c.authority.clone();
    let d = &f.registration.request.descriptor;
    let native = fixture::root_session(s, "ab".repeat(32), 8000)
        .status
        .status
        .receipt
        .native_account_id;
    input.request.intent = CustodyIntentV1::TestSession {
        session_id: "session-one".into(),
        native_account_id: native,
        world_identity: Box::new(term.world.clone()),
        task_fence: Box::new(term.initial_fence.clone()),
        executor_key_ref: term.executor_key_ref.clone(),
        client_binding: ApprovedClientBindingV1::NativeBrokerCustodyV1 {
            client_key: d.client_public_key.clone(),
        },
        client_pairing: Some(Box::new(ClientPairingBindingV1 {
            api_version: 1,
            target: f.registration.request.target.clone(),
            enrollment_id: f.registration.enrollment_id.clone(),
            enrollment_digest: f.digest().unwrap(),
            host_instance_id: d.host_instance_id.clone(),
            descriptor_digest: f.registration.descriptor_digest.clone(),
            client_key_id: f.registration.key_id.clone(),
            client_public_key: d.client_public_key.clone(),
            connector_public_key: None,
        })),
        runtime_ref: d.restricted_runtime_identity.clone(),
        skill_ref: d.skill_manifest_digest.clone(),
        source_ref: d.build_digest.clone(),
        actions: vec![ApprovedActionV1::Move {
            position_cm: [DecimalI64(1), DecimalI64(2), DecimalI64(3)],
        }],
        allowed_resource_refs: vec![],
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
        write_deadline: DecimalU64(50_000),
        read_until: DecimalU64(80_000),
        cleanup_until: DecimalU64(60_000),
        request_limit: DecimalU64(100),
        transport_limit: DecimalU64(400),
        business_limit: DecimalU64(0),
        termination_plan_digest: term.digest().unwrap(),
    };
    input.approval.proof.context = input.request.context.clone();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.nonce = "session-fresh-UV-nonce".into();
    input.approval.proof.approval_id = "session-fresh-UV".into();
    input.approval.proof.approval_request_id = "session-UV".into();
    input.approval.proof.issued_at = DecimalU64(8000);
    input.approval.proof.expires_at = DecimalU64(38000);
    input.approval = sign_proof(input.approval.proof);
    TestSessionAdmissionV1 {
        input,
        termination: term,
        verify_nonce: "e1".repeat(32),
    }
}
struct SessionFixture {
    state: AccountProvisionApprovalState,
    context: ApprovalAdmissionContext,
    pin: NativeBindingPinV1,
    fact: ProvedEnrollmentFactV1,
    deployments: HostDeploymentAllowlist,
}
#[test]
fn test_session_real_mfa_reserves_pending_but_only_native_positive_consume_can_progress() {
    let _ = pending_fixture();
}
fn management_fixture() -> SessionFixture {
    let (mut s, mut c, pin, _, _) = super::tests::prepared_by_reconcile();
    c.handoff_until = DecimalU64(80_000);
    let policy = Policy::new(s.draft());
    let manager = metadata(&c);
    let world = s.draft().world_identity.clone();
    let native_namespace = fixture::root_session(&s, "ab".repeat(32), 8000)
        .status
        .status
        .receipt
        .identity_namespace;
    let management = OwnerSetupManagementCurrent {
        manager: &manager,
        authority: &c.authority,
        policy: policy.current(&c).policy,
        world_identity: &world,
        native_identity_namespace: &native_namespace,
        fence: &c.fence,
        observed_at: 8000,
    };
    let invite = s
        .create_pairing_invite(
            PairingInviteCreateRequestV1 {
                api_version: 1,
                request_id: "invite-request".into(),
                transport_profile: HostTransportProfileV1::NativeBrokerCustodyV1,
                tool_profile: "custody-native-v1".into(),
            },
            "invite-one",
            &management,
            UnixMillis::new(8000),
        )
        .unwrap();
    let fact = enrollment(
        invite.target().unwrap(),
        s.draft().entry.environment.clone(),
    );
    let d = &fact.registration.request.descriptor;
    let deployments = HostDeploymentAllowlist::from_fixed_config(vec![HostDeploymentPinV1 {
        environment: d.environment.clone(),
        host_instance_id: d.host_instance_id.clone(),
        descriptor_digest: d.digest().unwrap(),
        client_public_key: d.client_public_key.clone(),
    }])
    .unwrap();
    SessionFixture {
        state: s,
        context: c,
        pin,
        fact,
        deployments,
    }
}
fn pending_fixture() -> SessionFixture {
    session_fixture(true)
}
fn session_fixture(consume: bool) -> SessionFixture {
    let SessionFixture {
        state: mut s,
        context: c,
        pin,
        fact,
        deployments,
    } = management_fixture();
    let policy = Policy::new(s.draft());
    let ns = scope(&pin);
    let actual = ProvedEnrollmentObservation {
        actual_actor_id: "actor-one",
        expected_actor_id: "actor-one",
        actual_realm: "enrollment",
        expected_realm: "enrollment",
        expected_public_origin: "https://custody.example",
        fact: &fact,
    };
    let p = policy.current(&c);
    let current = TestSessionApprovalCurrent {
        provision: &p,
        native_scope: &ns,
        enrollment: &actual,
        deployments: &deployments,
        observed_at: UnixMillis::new(8002),
    };
    let a = admission(&s, &c, &fact);
    let before = s.clone();
    for field in 0..3 {
        let mut wrong = a.clone();
        if let CustodyIntentV1::TestSession {
            runtime_ref,
            skill_ref,
            source_ref,
            ..
        } = &mut wrong.input.request.intent
        {
            match field {
                0 => *runtime_ref = "different-runtime".into(),
                1 => *skill_ref = "bb".repeat(32),
                _ => *source_ref = "cc".repeat(32),
            }
        }
        wrong.input.approval.proof.intent_digest = wrong.input.request.intent_digest().unwrap();
        wrong.input.approval = sign_proof(wrong.input.approval.proof);
        assert!(
            s.begin_test_session(wrong, &current, UnixMillis::new(8002), &mut StrictCrypto)
                .is_err()
        );
        assert_eq!(s, before);
    }

    let empty = HostDeploymentAllowlist::default();
    let missing_deployment = TestSessionApprovalCurrent {
        deployments: &empty,
        ..current
    };
    assert!(
        s.begin_test_session(
            a.clone(),
            &missing_deployment,
            UnixMillis::new(8002),
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(s, before);
    let current = TestSessionApprovalCurrent {
        deployments: &deployments,
        ..missing_deployment
    };
    let mut legacy = a.clone();
    if let CustodyIntentV1::TestSession { client_pairing, .. } = &mut legacy.input.request.intent {
        *client_pairing = None;
    }
    legacy.input.approval.proof.intent_digest = legacy.input.request.intent_digest().unwrap();
    legacy.input.approval = sign_proof(legacy.input.approval.proof);
    assert!(
        s.begin_test_session(legacy, &current, UnixMillis::new(8002), &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(s, before);
    let record = s
        .begin_test_session(
            a.clone(),
            &current,
            UnixMillis::new(8002),
            &mut StrictCrypto,
        )
        .unwrap();
    assert!(s.test_sessions()["session-one"].roots.is_empty());
    assert!(s.test_sessions()["session-one"].consumed_at.is_none());
    let snapshot = s.clone();
    let mut reused = a.clone();
    reused.input.operation_id = "same-proof-second-book".into();
    assert!(
        s.begin_test_session(reused, &current, UnixMillis::new(8002), &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(s, snapshot);
    assert_eq!(
        s.begin_test_session(
            a.clone(),
            &current,
            UnixMillis::new(8002),
            &mut StrictCrypto
        )
        .unwrap(),
        record
    );
    assert_eq!(s, snapshot);
    if !consume {
        return SessionFixture {
            state: s,
            context: c,
            pin,
            fact,
            deployments,
        };
    }
    let request = s
        .claim_test_session_approval_verification("session-one", UnixMillis::new(8003))
        .unwrap();
    assert_eq!(request.verify_nonce, a.verify_nonce);
    let snapshot = s.clone();
    assert!(
        s.claim_test_session_approval_verification("session-one", UnixMillis::new(8003))
            .is_err()
    );
    assert_eq!(s, snapshot);
    let mut v = verification(&record, &c).verification;
    v.observed_at = DecimalU64(8004);
    let signed = sign_verification(v.clone());
    v.purpose = VerificationPurposeV1::InspectOrigin;
    let bad = CommitTestSessionV1 {
        session_id: "session-one".into(),
        signed: sign_verification(v),
        source_nonce: "e2".repeat(32),
    };
    assert!(
        s.consume_test_session_approval(bad, &current, UnixMillis::new(8004), &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(s, snapshot);
    s.consume_test_session_approval(
        CommitTestSessionV1 {
            session_id: "session-one".into(),
            signed,
            source_nonce: "e2".repeat(32),
        },
        &current,
        UnixMillis::new(8004),
        &mut StrictCrypto,
    )
    .unwrap();
    assert!(s.test_sessions()["session-one"].roots.is_empty());
    assert!(s.test_sessions()["session-one"].consumed_at.is_some());
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
    let mut corrupt = serde_json::to_value(&s).unwrap();
    corrupt["test_sessions"]["session-one"]["consumed_at"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(corrupt).is_err());
    SessionFixture {
        state: s,
        context: c,
        pin,
        fact,
        deployments,
    }
}

#[test]
fn test_session_termination_basis_is_account_prepared_fact_not_echo_or_free_signer() {
    let (s, _, _, _, _) = super::tests::prepared_by_reconcile();
    let p = s
        .test_session_termination_basis("session-one", UnixMillis::new(60_000))
        .unwrap();
    p.validate().unwrap();
    assert_eq!(p.request_limit, DecimalU64(2));
    assert_eq!(p.transport_limit, DecimalU64(24));
    assert_ne!(p.stop_operation_id, p.revoke_operation_id);
    let next = s
        .test_session_termination_basis("session-two", UnixMillis::new(60_000))
        .unwrap();
    assert_ne!(p.stop_operation_id, next.stop_operation_id);
    assert_ne!(p.revoke_operation_id, next.revoke_operation_id);
    let snapshot = s.clone();
    for mutation in 0..5 {
        let mut wrong = p.clone();
        match mutation {
            0 => wrong.stop_operation_id = next.stop_operation_id.clone(),
            1 => wrong.original_delegation.delegate_public_key = "ab".repeat(32),
            2 => wrong.original_delegation.revoked = true,
            3 => wrong.request_limit = DecimalU64(3),
            _ => wrong.transport_limit = DecimalU64(25),
        }
        assert!(wrong.validate().is_err());
    }
    let mut raw = serde_json::to_value(&p).unwrap();
    raw["arbitrary_raw_owner_payload"] = true.into();
    assert!(serde_json::from_value::<TerminationPlanV1>(raw).is_err());
    assert_eq!(s, snapshot);
}

#[path = "test_session_activation_tests.rs"]
mod activation;

#[path = "test_session_management_tests.rs"]
mod management;

#[path = "test_session_expiry_tests.rs"]
mod expiry;
