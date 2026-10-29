//! Structural public contract fixtures, never cryptographic admission evidence.
use super::*;
use crate::{client_enrollment::*, time_units::UnixMillis};
use base64::Engine;
pub(super) fn schemas() -> Value {
    json!({
        "PairingInviteCreateRequestV1":schemars::schema_for!(PairingInviteCreateRequestV1),
        "PairingInviteOriginalV1":schemars::schema_for!(PairingInviteOriginalV1),
        "PairingInviteTargetRefV1":schemars::schema_for!(PairingInviteTargetRefV1),
        "HostDeploymentDescriptorV1":schemars::schema_for!(HostDeploymentDescriptorV1),
        "ClientEnrollmentCreateRequestV1":schemars::schema_for!(ClientEnrollmentCreateRequestV1),
        "ClientEnrollmentCreateResponseV1":schemars::schema_for!(ClientEnrollmentCreateResponseV1),
        "ClientEnrollmentProofRequestV1":schemars::schema_for!(ClientEnrollmentProofRequestV1),
        "ClientEnrollmentProofResponseV1":schemars::schema_for!(ClientEnrollmentProofResponseV1),
        "ClientCredentialClaimRequestV1":schemars::schema_for!(ClientCredentialClaimRequestV1),
        "ClientPairingBindingV1":schemars::schema_for!(ClientPairingBindingV1),
        "TestSessionBasisRequestV1":schemars::schema_for!(TestSessionBasisRequestV1),
        "TestSessionBasisV1":schemars::schema_for!(TestSessionBasisV1),
        "TestSessionPrepareRequestV1":schemars::schema_for!(TestSessionPrepareRequestV1),
        "TestSessionPrepareResponseV1":schemars::schema_for!(TestSessionPrepareResponseV1),
        "TestSessionApproveRequestV1":schemars::schema_for!(TestSessionApproveRequestV1),
        "TestSessionContinueRequestV1":schemars::schema_for!(TestSessionContinueRequestV1),
        "TestSessionProgressV1":schemars::schema_for!(TestSessionProgressV1)
    })
}
fn typed<T: serde::de::DeserializeOwned + Serialize>(value: Value) -> T {
    let result: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&result).unwrap(), value);
    result
}
fn enrollment_fixture() -> (
    Value,
    HostDeploymentDescriptorV1,
    ClientEnrollmentCreateResponseV1,
    ClientPairingBindingV1,
) {
    let invite = PairingInviteOriginalV1 {
        api_version: 1,
        domain: "oasis7-custody-pairing-invite-v1".into(),
        environment: "test".into(),
        service_epoch: DecimalU64(3),
        custody_account_id: "custody-one".into(),
        provision_id: "provision-one".into(),
        invite_id: "invite-one".into(),
        issued_at: UnixMillis::new(1000),
        expires_at: UnixMillis::new(601000),
        transport_profile: HostTransportProfileV1::NativeBrokerCustodyV1,
        tool_profile: "custody-native-v1".into(),
    };
    let target = invite.target().unwrap();
    let descriptor = HostDeploymentDescriptorV1 {
        api_version: 1,
        environment: "test".into(),
        transport_profile: HostTransportProfileV1::NativeBrokerCustodyV1,
        tool_profile: "custody-native-v1".into(),
        host_instance_id: "host-one".into(),
        client_public_key: "dd".repeat(32),
        build_digest: "a1".repeat(32),
        skill_manifest_digest: "a2".repeat(32),
        tool_manifest_digest: "a3".repeat(32),
        https_origin: "https://custody.example".into(),
        restricted_runtime_identity: "isolated-runtime-one".into(),
        isolation_mechanism: HostIsolationMechanismV1::SeparateOsPrincipal,
        isolation_acceptance_refs: vec!["acceptance-pending-not-proof".into()],
    };
    descriptor.validate().unwrap();
    let create = ClientEnrollmentCreateRequestV1 {
        target: target.clone(),
        api_version: 1,
        request_id: "enroll-create-one".into(),
        descriptor: descriptor.clone(),
    };
    create.validate().unwrap();
    let created = ClientEnrollmentCreateResponseV1 {
        api_version: 1,
        enrollment_id: "enrollment-one".into(),
        key_id: "client-one".into(),
        server_nonce: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([7; 32]),
        challenge_expires_at: UnixMillis::new(31000),
        expires_at: invite.expires_at,
        descriptor_digest: descriptor.digest().unwrap(),
    };
    created.validate().unwrap();
    let prove = ClientEnrollmentProofRequestV1 {
        target: target.clone(),
        client_public_key: descriptor.client_public_key.clone(),
        server_nonce: created.server_nonce.clone(),
        challenge_expires_at: created.challenge_expires_at,
        api_version: 1,
        enrollment_id: created.enrollment_id.clone(),
        request_id: "prove-one".into(),
        descriptor_digest: created.descriptor_digest.clone(),
    };
    prove.validate().unwrap();
    let proved = ClientEnrollmentProofResponseV1 {
        api_version: 1,
        enrollment_id: created.enrollment_id.clone(),
        status: ClientEnrollmentProofStatusV1::Proved,
    };
    proved.validate().unwrap();
    let pairing = ClientPairingBindingV1 {
        api_version: 1,
        target: target.clone(),
        enrollment_id: created.enrollment_id.clone(),
        enrollment_digest: "ce".repeat(32),
        host_instance_id: descriptor.host_instance_id.clone(),
        descriptor_digest: created.descriptor_digest.clone(),
        client_key_id: created.key_id.clone(),
        client_public_key: descriptor.client_public_key.clone(),
        connector_public_key: None,
    };
    pairing.validate().unwrap();
    let fixtures = json!({"PairingInviteCreateRequestV1":[PairingInviteCreateRequestV1{api_version:1,request_id:"invite-request-one".into(),transport_profile:descriptor.transport_profile,tool_profile:descriptor.tool_profile.clone()}],"PairingInviteOriginalV1":[invite],"PairingInviteTargetRefV1":[target],"HostDeploymentDescriptorV1":[descriptor],"ClientEnrollmentCreateRequestV1":[create],"ClientEnrollmentCreateResponseV1":[created],"ClientEnrollmentProofRequestV1":[prove],"ClientEnrollmentProofResponseV1":[proved],"ClientPairingBindingV1":[pairing],
    "ClientCredentialClaimRequestV1":[ClientCredentialClaimRequestV1{api_version:1,enrollment_id:"enrollment-one".into(),test_session_id:"session-one".into(),request_id:"claim-one".into(),purposes:vec![TestSessionPurposeV1::Execute,TestSessionPurposeV1::Read],replace:false,expected_generation:None}]});
    (fixtures, descriptor, created, pairing)
}

fn termination_fixture(intent: &Value, fence: Value) -> TerminationPlanV1 {
    let prepared = "ee".repeat(32);
    let operation = |domain: &str| {
        crate::approval::bytes_digest(
            &oasis7_wasm_abi::encode_canonical_cbor(&(domain, "session-one", prepared.as_str()))
                .unwrap(),
        )
    };
    let termination: TerminationPlanV1 = typed(
        json!({"api_version":1,"session_id":"session-one","custody_account_id":"custody-one","provision_id":"provision-one","setup_id":"setup-one","prepared_result_digest":prepared,
        "world":intent["world_identity"],"owner_key_ref":"owner-one","executor_key_ref":"executor-one","initial_fence":fence,
        "original_delegation":{"world_id":"world-one","branch_id":"branch-one","agent_id":"agent-one","owner_binding":"owner-one","agent_identity_generation":"1","generation":"1","delegate_public_key":"cc".repeat(32),"revoked":false,"nonce":"1"},
        "stop_operation_id":operation("oasis7.custody.session-stop.v1"),"revoke_operation_id":operation("oasis7.custody.session-revoke.v1"),
        "cleanup_until":"601000","request_limit":"2","transport_limit":"24","trigger":"closing_only_v1","nonce_policy":"account_allocated_high_water_v1"}),
    );
    termination
}

fn session_plan_fixture(
    basis: &TestSessionBasisV1,
    created: &ClientEnrollmentCreateResponseV1,
    intent: &Value,
) -> TestSessionPlanV1 {
    let plan: TestSessionPlanV1 = typed(
        json!({"api_version":1,"request_id":basis.request_id,"enrollment_id":created.enrollment_id,"basis_digest":basis.digest().unwrap(),"actions":intent["actions"],"allowed_resource_refs":intent["allowed_resource_refs"],"write_deadline":intent["write_deadline"],"read_until":intent["read_until"],"cleanup_until":intent["cleanup_until"],"request_limit":intent["request_limit"],"transport_limit":intent["transport_limit"],"business_limit":intent["business_limit"],"read_budget":intent["read_budget"],"cleanup_budget":intent["cleanup_budget"],"control_budget":intent["control_budget"]}),
    );
    plan
}

pub(super) fn fixture(approval: &SignedCustodyApprovalV1) -> Value {
    let (mut fixtures, descriptor, created, pairing) = enrollment_fixture();
    let mut options = options().remove(2);
    let mut intent = serde_json::to_value(&options.intent).unwrap();
    intent["client_pairing"] = serde_json::to_value(&pairing).unwrap();
    intent["runtime_ref"] = descriptor.restricted_runtime_identity.clone().into();
    intent["skill_ref"] = descriptor.skill_manifest_digest.clone().into();
    intent["source_ref"] = descriptor.build_digest.clone().into();
    let mut fence = intent["task_fence"].clone();
    fence["latest_goal_revision"] = "1".into();
    fence["active_goal_revision"] = "1".into();
    fence["active_goal_reference"] = json!({"goal_revision":"1","goal_ref":"cf".repeat(32),"prepare_operation_id":"prepare-one","publish_operation_id":"publish-one","activation_committed_height":"9007199254740993"});
    let termination = termination_fixture(&intent, fence);
    termination.validate().unwrap();
    intent["task_fence"] = serde_json::to_value(&termination.initial_fence).unwrap();
    intent["termination_plan_digest"] = termination.digest().unwrap().into();
    options.intent = serde_json::from_value(intent.clone()).unwrap();
    options.validate(1000).unwrap();
    let basis = TestSessionBasisV1 {
        api_version: 1,
        request_id: "session-request-one".into(),
        session_id: "session-one".into(),
        custody_account_id: "custody-one".into(),
        provision_id: "provision-one".into(),
        authority: context(),
        termination,
        native_account_id: "native-one".into(),
        owner_key: PublicRoleKey {
            key_id: "owner-one".into(),
            key_version: DecimalU64(1),
            public_key: "bb".repeat(32),
        },
        executor_key: PublicRoleKey {
            key_id: "executor-one".into(),
            key_version: DecimalU64(1),
            public_key: "cc".repeat(32),
        },
        client_pairing: pairing.clone(),
        runtime_ref: descriptor.restricted_runtime_identity.clone(),
        skill_ref: descriptor.skill_manifest_digest.clone(),
        source_ref: descriptor.build_digest.clone(),
        allowed_resource_refs: serde_json::from_value(intent["allowed_resource_refs"].clone())
            .unwrap(),
    };
    let plan = session_plan_fixture(&basis, &created, &intent);
    plan.validate().unwrap();
    let prepare = TestSessionPrepareRequestV1 {
        api_version: 1,
        approval_request_id: options.approval_request_id.clone(),
        plan: plan.clone(),
    };
    prepare.validate().unwrap();
    let mut signed = approval.clone();
    signed.proof.context = options.context.clone();
    signed.proof.intent_digest = options.intent_digest().unwrap();
    let approve = TestSessionApproveRequestV1 {
        api_version: 1,
        input: crate::approval_admission::ApprovalAdmissionInput {
            operation_id: basis.session_id.clone(),
            request: options.clone(),
            approval: signed,
        },
        plan,
    };
    approve.validate().unwrap();
    let progress: TestSessionProgressV1 = typed(
        json!({"api_version":1,"session_id":"session-one","custody_account_id":"custody-one","provision_id":"provision-one","stage":"consumed_pending_preflight","write_closed":true,"closing_reason":null,"write_deadline":"301000","read_until":"601000","cleanup_until":"601000","lease_until":null,"purposes":[],"control_requests_remaining":"79","control_host_http_remaining":"240","control_connector_http_remaining":null,"identity_control_http_remaining":"159","stop_status":"not_allocated","revoke_status":"not_allocated","pending_operations":["test-session-preflight"]}),
    );
    let mut active = progress.clone();
    active.stage = TestSessionPhaseV1::Active;
    active.write_closed = false;
    active.lease_until = Some(UnixMillis::new(91000));
    active.pending_operations.clear();
    active.purposes = vec![TestSessionPurposeProgressV1 {
        purpose: SessionPurposeV1::Execution,
        deadline: active.write_deadline,
        requests_remaining: DecimalU64(100),
        transport_remaining: DecimalU64(400),
        business_remaining: DecimalU64(u64::MAX),
        revoked: false,
    }];
    let mut closing = active.clone();
    closing.stage = TestSessionPhaseV1::Closing;
    closing.write_closed = true;
    closing.closing_reason = Some(TestSessionClosingReasonV1::Finished);
    closing.stop_status = TestSessionTerminationStatusV1::Unknown;
    closing.pending_operations = vec!["stop-one".into()];
    fixtures.as_object_mut().unwrap().extend(
    json!({"TestSessionBasisRequestV1":[TestSessionBasisRequestV1{api_version:1,request_id:basis.request_id.clone(),enrollment_id:"enrollment-one".into(),cleanup_until:UnixMillis::new(601000)}],"TestSessionBasisV1":[basis],"TestSessionPrepareRequestV1":[prepare],"TestSessionPrepareResponseV1":[options],"TestSessionApproveRequestV1":[approve],"TestSessionContinueRequestV1":[TestSessionContinueRequestV1{api_version:1,session_id:"session-one".into()}],"TestSessionProgressV1":[progress,active,closing]}).as_object().unwrap().clone());
    fixtures
}

#[test]
fn public_test_session_and_enrollment_codecs_are_closed_and_decimal_exact() {
    let approval = SignedCustodyApprovalV1 {
        proof: CustodyApprovalProofV1 {
            version: 1,
            key_id: "fixture".into(),
            identity_namespace: "identity:test".into(),
            approval_id: "approval-one".into(),
            approval_request_id: "request-one".into(),
            context: context(),
            intent_digest: "aa".repeat(32),
            manager_account_id: "manager-one".into(),
            manager_session_ref: "manager-session".into(),
            origin_credential_id: "origin-one".into(),
            origin_credential_version: DecimalU64(1),
            passkey_credential_id: "passkey-one".into(),
            passkey_credential_version: DecimalU64(1),
            manager_policy_version: DecimalU64(1),
            manager_security_version: DecimalU64(2),
            identity_instance_epoch: DecimalU64(3),
            nonce: "fixture-nonce".into(),
            issued_at: DecimalU64(1000),
            expires_at: DecimalU64(31000),
        },
        signature: "structural-fixture-only".into(),
    };
    let f = fixture(&approval);
    let mut response = f["ClientEnrollmentCreateResponseV1"][0].clone();
    response["expires_at"] = json!(9007199254740992_u64);
    assert!(serde_json::from_value::<ClientEnrollmentCreateResponseV1>(response).is_err());
    for invalid in ["01", "18446744073709551616"] {
        let mut response = f["ClientEnrollmentCreateResponseV1"][0].clone();
        response["expires_at"] = invalid.into();
        assert!(serde_json::from_value::<ClientEnrollmentCreateResponseV1>(response).is_err());
    }
    let mut proof = f["ClientEnrollmentProofResponseV1"][0].clone();
    proof["status"] = "approved".into();
    assert!(serde_json::from_value::<ClientEnrollmentProofResponseV1>(proof).is_err());
    let mut create = f["ClientEnrollmentCreateRequestV1"][0].clone();
    create.as_object_mut().unwrap().remove("target");
    assert!(serde_json::from_value::<ClientEnrollmentCreateRequestV1>(create).is_err());
    for secret in [
        "raw_token",
        "private_sid",
        "proof",
        "credential_history",
        "goal_content",
    ] {
        let mut progress = f["TestSessionProgressV1"][0].clone();
        progress[secret] = "unexpected".into();
        assert!(serde_json::from_value::<TestSessionProgressV1>(progress).is_err());
    }
    let mut prepare = f["TestSessionPrepareRequestV1"][0].clone();
    prepare["verified"] = true.into();
    assert!(serde_json::from_value::<TestSessionPrepareRequestV1>(prepare).is_err());
    // Metadata has the full integer width; no JSON number or float conversion.
    let mut basis = f["TestSessionBasisRequestV1"][0].clone();
    basis["cleanup_until"] = "18446744073709551615".into();
    let decoded: TestSessionBasisRequestV1 = serde_json::from_value(basis).unwrap();
    assert_eq!(decoded.cleanup_until.get(), u64::MAX);
    let roots = schemas();
    assert!(roots.as_object().unwrap().keys().all(|name| {
        !["Authority", "History", "Current", "Permit"]
            .iter()
            .any(|private| name.contains(private))
    }));
}
