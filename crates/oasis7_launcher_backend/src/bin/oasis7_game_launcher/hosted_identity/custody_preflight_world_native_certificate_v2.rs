//! Component-only Account issuer. Real production uses the opaque Core budget
//! factory; this fixture never asserts that an Account transport was prepaid.
use super::*;
use oasis7_client_api::world_service::{self as ws, sign_read_request};
use oasis7_custody_core::{
    approval_admission::*,
    http::Role,
    native_history::{NativeHistoryPhaseV1, NativeHistoryScopeV1},
    role_key_slot::RoleKeySlotScopeV1,
    time_units::UnixMillis,
};
pub(super) fn options_for(f: &native_fixture::Fixture, now: u64) -> ApprovalOptionsRequestV1 {
    let mut request: ApprovalOptionsRequestV1 =
        serde_json::from_value(session_options(now)).unwrap();
    request.approval_request_id = "world3-physical-ports".into();
    let CustodyIntentV1::TestSessionV2 { plan, scope } = &mut request.intent else {
        panic!("new typed plan")
    };
    plan.termination.cleanup_transport_policy =
        CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2;
    plan.termination.world.service_public_key =
        hex::encode(SigningKey::from_bytes(&[82; 32]).verifying_key().to_bytes());
    plan.termination.initial_fence.owner_public_key =
        hex::encode(SigningKey::from_bytes(&[11; 32]).verifying_key().to_bytes());
    plan.termination.stop_operation_id = plan.termination.expected_stop_operation_id().unwrap();
    plan.termination.close_operation_id = plan.termination.expected_close_operation_id().unwrap();
    scope.native_account_id = f.binding.native_account_id.clone();
    scope.world_identity = plan.termination.world.clone();
    scope.task_fence = plan.termination.initial_fence.clone();
    scope.termination_plan_digest = plan.termination.digest().unwrap();
    request
}
struct BookCrypto;
impl ApprovalSignatureVerifier for BookCrypto {
    fn verify(&mut self, checks: &ExactSignatureChecks) -> Result<(), ApprovalAdmissionError> {
        for job in checks.checks() {
            let key: [u8; 32] = hex::decode(job.public_key_hex())
                .unwrap()
                .try_into()
                .unwrap();
            let signature: [u8; 64] = URL_SAFE_NO_PAD
                .decode(job.signature_base64url())
                .unwrap()
                .try_into()
                .unwrap();
            ed25519_dalek::VerifyingKey::from_bytes(&key)
                .unwrap()
                .verify_strict(job.canonical_bytes(), &Signature::from_bytes(&signature))
                .unwrap();
        }
        Ok(())
    }
}
pub(super) fn approve_and_consume(
    f: &mut native_fixture::Fixture,
    request: ApprovalOptionsRequestV1,
) -> (ApprovalAdmissionInput, u64) {
    let started = now_ms();
    let options =
        f.f.service
            .custody_options(
                Some(&f.f.token),
                &serde_json::to_value(&request).unwrap(),
                "source",
                started,
            )
            .unwrap();
    if matches!(&request.intent, CustodyIntentV1::TestSessionV2 { .. }) {
        assert_eq!(
            options["custody_session_summary_v2"]["plan"],
            serde_json::to_value(&request.intent).unwrap()["plan"]
        );
    }
    let operation_id = match &request.intent {
        CustodyIntentV1::OwnerSetupV2 { plan } => plan.setup_id.clone(),
        _ => "session".into(),
    };
    let completed =
        f.f.service
            .custody_complete(
                Some(&f.f.token),
                &complete_payload(&options, &f.human_key, 0x05),
                "source",
                now_ms(),
            )
            .unwrap();
    let approval: SignedCustodyApprovalV1 =
        serde_json::from_value(completed["approval"].clone()).unwrap();
    let (_, manager) = f.f.service.session(Some(&f.f.token), now_ms()).unwrap();
    let proof = &approval.proof;
    let input = ApprovalAdmissionInput {
        operation_id: operation_id.clone(),
        request,
        approval: approval.clone(),
    };
    let context = ApprovalAdmissionContext {
        identity_namespace: proof.identity_namespace.clone(),
        identity_instance_epoch: proof.identity_instance_epoch,
        authority: proof.context.clone(),
        allowed_managers: [proof.manager_account_id.clone()].into(),
        source: ApprovalSourceVersions {
            manager_account_id: proof.manager_account_id.clone(),
            manager_session_ref: proof.manager_session_ref.clone(),
            origin_credential_id: proof.origin_credential_id.clone(),
            origin_credential_version: proof.origin_credential_version,
            passkey_credential_id: proof.passkey_credential_id.clone(),
            passkey_credential_version: proof.passkey_credential_version,
            manager_policy_version: proof.manager_policy_version,
            manager_security_version: proof.manager_security_version,
            session_expires_at: DecimalU64(manager.expires_at),
            session_idle_expires_at: DecimalU64(manager.idle_expires_at),
        },
        native_key_pins: [(
            proof.key_id.clone(),
            hex::encode(SigningKey::from_bytes(&[51; 32]).verifying_key().to_bytes()),
        )]
        .into(),
        fence: oasis7_custody_core::recovery::RecoveryFence::new(
            proof.context.service_epoch.0,
            false,
        ),
        frozen: false,
        handoff_until: proof.expires_at,
    };
    let mut book = ApprovalAdmissionBook::default();
    let record = book
        .begin(
            input.clone(),
            "77".repeat(32),
            &context,
            now_ms(),
            &mut BookCrypto,
        )
        .unwrap();
    let headers = format!(
        "POST / HTTP/1.1\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\n",
        hex::encode([52; 32])
    );
    let checked =
        f.f.service
            .custody_verify_route(
                "POST",
                &headers,
                &serde_json::to_value(record.native_verify_request().unwrap()).unwrap(),
                "source",
                now_ms(),
            )
            .unwrap();
    let consumed_at = now_ms();
    let consumed = book
        .commit(
            &operation_id,
            serde_json::from_value(checked).unwrap(),
            &context,
            consumed_at,
            &mut BookCrypto,
        )
        .unwrap();
    assert_eq!(consumed.stage(), ApprovalAdmissionStage::Consumed);
    (input, consumed_at)
}
fn role_scope(role: Role, key_id: &str) -> RoleKeySlotScopeV1 {
    RoleKeySlotScopeV1::from_task(&ProvisionRoleKeyTaskV1 {
        operation_id: format!("{key_id}-operation"),
        provision_id: "provision".into(),
        environment: "test".into(),
        custody_account_id: "custody-a".into(),
        role,
        key_id: key_id.into(),
        key_version: DecimalU64(1),
        algorithm: ProvisionRoleKeyAlgorithmV1::Ed25519,
        state: ProvisionRoleKeyTaskStateV1::Pending,
    })
    .unwrap()
}
pub(super) fn certificate(
    f: &native_fixture::Fixture,
    input: ApprovalAdmissionInput,
    consumed_at: u64,
) -> SignedAccountPreflightAccessV2 {
    let issued = now_ms();
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &input.request.intent else {
        panic!("V2")
    };
    let approved_input_digest = AccountPreflightAccessV2::expected_input_digest(&input).unwrap();
    let namespace = oasis7_agent_api::game::GameIdentityNamespaceV1 {
        service_id: f.f.service.config.namespace.identity_service_id.clone(),
        environment_id: f.f.service.config.namespace.environment_id.clone(),
        instance_epoch: DecimalU64(f.f.service.config.rp.identity_instance_epoch),
    };
    let participant_owner = sign_read_request(
        oasis7_agent_api::game::GAME_PARTICIPANT_REGISTER_DOMAIN,
        oasis7_agent_api::game::GameParticipantRegisterV1 {
            api_rev: 1,
            client_operation_id: "participant".into(),
            identity_namespace: namespace,
            account_id: f.binding.native_account_id.clone(),
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            owner_public_key: hex::encode(
                SigningKey::from_bytes(&[11; 32]).verifying_key().to_bytes(),
            ),
            owner_key_version: DecimalU64(1),
        },
        &hex::encode([11; 32]),
    )
    .unwrap();
    let d = TestSessionPreflightDescriptorV2 {
        api_version: 2,
        approved_input_digest: approved_input_digest.clone(),
        session_id: "session".into(),
        session: ParticipantSessionRefV1 {
            scope: NativeHistoryScopeV1 {
                role_scope: role_scope(Role::Identity, "identity-key"),
                identity_namespace: f.binding.identity_namespace.clone(),
                identity_instance_epoch: f.binding.identity_instance_epoch,
                registration_operation_id: f.registration.clone(),
                initial_login_operation_id: "initial-login".into(),
            },
            phase: NativeHistoryPhaseV1::InitialLogin,
            source_permit_id: "source-permit".into(),
            native_operation_id: "native-login".into(),
            session_ref: f.managed.session_ref.clone(),
            private_sid_generation: DecimalU64(1),
            native_binding_generation: f.managed.generation,
        },
        native_binding: f.binding.clone(),
        world: plan.termination.world.clone(),
        owner_key: PublicRoleKey {
            key_id: "owner".into(),
            key_version: DecimalU64(1),
            public_key: participant_owner.subject_public_key.clone(),
        },
        participant_owner,
        owner_scope: role_scope(Role::Owner, "owner"),
        identity_scope: role_scope(Role::Identity, "identity-key"),
        source_operation_id: "source-operation".into(),
        source_nonce: "b5".repeat(32),
        lifetime_nonce: "b4".repeat(32),
        access_nonces: OwnerSetupAccessNoncesV1 {
            policy: "b1".repeat(32),
            policy_session: "b2".repeat(32),
            authorize_session: "b3".repeat(32),
        },
        owner_read: ws::ReadWorldViewRequest {
            observation_nonce: Some("b6".repeat(32)),
            formal_anchor: None,
            contract_version: 1,
            world: ws::WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
            },
            scope_id: "owner".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        },
        expires_at: UnixMillis::new(plan.write_deadline.0),
        cleanup_transport_policy: Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2),
    };
    let ports = [
        NativePreflightWorldPortV2 {
            operation: NativePreflightWorldOperationV2::PolicyOriginal,
            physical_attempt_id: "a1".repeat(32),
            native_path: NATIVE_PREFLIGHT_POLICY_V2_PATH.into(),
            world_path: "/v1/world/participant-policy".into(),
            body_digest: AccountPreflightAccessV2::policy_body_digest(&PreflightPolicyBodyV2 {
                observation_nonce: d.access_nonces.policy.clone(),
            })
            .unwrap(),
        },
        NativePreflightWorldPortV2 {
            operation: NativePreflightWorldOperationV2::AuthorizePolicy,
            physical_attempt_id: "a2".repeat(32),
            native_path: NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH.into(),
            world_path: "/v1/world/participant-policy".into(),
            body_digest: AccountPreflightAccessV2::authorize_body_digest(&d.participant_owner)
                .unwrap(),
        },
    ];
    let certificate = AccountPreflightAccessV2 {
        api_version: 2,
        issuer: AccountMachineIssuerV2 {
            namespace: "account".into(),
            service_id: "account-service".into(),
            environment: "test".into(),
            key_id: "account-key".into(),
            issuer_epoch: DecimalU64(1),
            config_generation: DecimalU64(1),
            native_audience_origin: "https://localhost".into(),
        },
        certificate_id: "a3".repeat(32),
        input: Box::new(input),
        approved_input_digest,
        descriptor: Box::new(d),
        source_record_digest: "a4".repeat(32),
        fresh_access_claimed_at: DecimalU64(issued),
        consumed_at: DecimalU64(consumed_at),
        issued_at: DecimalU64(issued),
        expires_at: DecimalU64(issued + 5000),
        ports,
    };
    certificate.validate_shape().unwrap();
    let signature_hex = hex::encode(
        SigningKey::from_bytes(&[81; 32])
            .sign(&certificate.signing_bytes().unwrap())
            .to_bytes(),
    );
    SignedAccountPreflightAccessV2 {
        certificate,
        signature_hex,
    }
}

#[path = "custody_resource_final_native_tests_v2.rs"]
mod resource_final_tests;
