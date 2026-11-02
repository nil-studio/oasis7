//! Real Native consent; fixture Account issuer is not a production payer.
use super::*;
fn world_hash<T: Serialize>(path: &str, q: &T) -> String {
    ws::request_digest(path, q)
        .unwrap()
        .strip_prefix("blake3:")
        .unwrap()
        .to_owned()
}
#[derive(Serialize)]
struct NativeBody<'a> {
    api_version: u8,
    input: &'a ApprovalAdmissionInput,
    participant: &'a ParticipantDescriptorV1,
    request: &'a ws::entry_control::EntryOwnerControlRequestV1,
}
pub(super) fn frame(
    f: &mut native_fixture::Fixture,
    data: &world_fixture::Data,
) -> OwnerSetupResourceFinalHttpV2 {
    let now = now_ms();
    let mut request: ApprovalOptionsRequestV1 =
        serde_json::from_value(session_options(now)).unwrap();
    let world = serde_json::json!({"world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"3","service_public_key":f.f.service.participant_entry.as_ref().unwrap().trust.service_public_key.clone()});
    let plan:OwnerSetupPlanV2=serde_json::from_value(json!({"api_version":2,"model_budget":null,"setup_id":"resource-setup","provision_id":"provision","participant_receipt_digest":"11".repeat(32),"world_identity":world,"owner_key_ref":"owner","executor_key_ref":"executor","bootstrap_config_version":"1","goal_content":{"goal_text":"private component goal","soft_constraints":["no production approval"]},"create_operation_id":"create","prepare_operation_id":"prepare","prepare_nonce":"ab".repeat(32),"publish_operation_id":"publish","delegation_operation_id":"delegate","bind_operation_id":"bind","executor_id":"executor","executor_public_key":data.state.signer_delegation.as_ref().unwrap().delegate_public_key,"delegation_generation":"1","delegation_nonce":"1","selection":{"kind":"build","site":"site","factory":"factory","factory_kind":"factory.smelter.mk1"},"deadline":(now+60000).to_string(),"request_limit":"6","transport_limit":"600","write_limit":"4"})).unwrap();
    plan.validate().unwrap();
    request.approval_request_id = "resource-native-new-uv".into();
    request.intent = plan.intent().unwrap();
    let (input, _) = approve_and_consume(f, request);
    let namespace = oasis7_agent_api::game::GameIdentityNamespaceV1 {
        service_id: f.f.service.config.namespace.identity_service_id.clone(),
        environment_id: f.f.service.config.namespace.environment_id.clone(),
        instance_epoch: DecimalU64(f.f.service.config.rp.identity_instance_epoch),
    };
    let mut access = data.goal_request().access;
    let mut register = access.owner.request.clone();
    register.account_id = f.binding.native_account_id.clone();
    register.identity_namespace = namespace.clone();
    access.owner = ws::sign_read_request(
        oasis7_agent_api::game::GAME_PARTICIPANT_REGISTER_DOMAIN,
        register,
        &data.owner,
    )
    .unwrap();
    let mut admission = access.admission.request.clone();
    admission.account_id = f.binding.native_account_id.clone();
    admission.identity_namespace = namespace;
    admission.business_digest =
        oasis7_client_api::world_service_correlation::participant_business_digest(
            &access.owner.request,
        )
        .unwrap();
    access.admission = ws::sign_read_request(
        oasis7_agent_api::game::GAME_PARTICIPANT_ADMISSION_DOMAIN,
        admission,
        &data.issuer,
    )
    .unwrap();
    let mut q = match data.goal_request().operation {
        ws::entry_control::EntryOwnerOperationV1::ReadResourceGoal(q) => q,
        _ => unreachable!(),
    };
    q.request.setup_id = plan.setup_id.clone();
    q.request.provision_id = plan.provision_id.clone();
    q.request.plan_digest = plan.digest().unwrap();
    let q = resource::SignedResourceReadV1::from(
        ws::sign_read_request(resource::RESOURCE_GOAL_PATH_V2, q.request, &data.owner).unwrap(),
    );
    let original = ws::entry_control::EntryOwnerControlRequestV1 {
        access: access.clone(),
        operation: ws::entry_control::EntryOwnerOperationV1::ReadResourceGoal(q.clone()),
    };
    let participant = ParticipantDescriptorV1 {
        world: plan.world_identity.clone(),
        native_binding: f.binding.clone(),
        session: ParticipantSessionRefV1 {
            scope: NativeHistoryScopeV1 {
                role_scope: role_scope(Role::Identity, "identity-key"),
                identity_namespace: f.binding.identity_namespace.clone(),
                identity_instance_epoch: f.binding.identity_instance_epoch,
                registration_operation_id: f.registration.clone(),
                initial_login_operation_id: "initial-login".into(),
            },
            phase: NativeHistoryPhaseV1::InitialLogin,
            source_permit_id: "resource-source".into(),
            native_operation_id: "native-login".into(),
            session_ref: f.managed.session_ref.clone(),
            private_sid_generation: DecimalU64(1),
            native_binding_generation: f.managed.generation,
        },
        owner_scope: role_scope(Role::Owner, "owner"),
        owner_key: PublicRoleKey {
            key_id: "owner".into(),
            key_version: DecimalU64(1),
            public_key: q.subject_public_key.clone(),
        },
        identity_scope: role_scope(Role::Identity, "identity-key"),
        register: access.owner.request.clone(),
        owner_read: access.read.request.clone(),
        policy_nonce: "12".repeat(32),
        owner_view_nonce: "13".repeat(32),
    };
    let body = NativeBody {
        api_version: 2,
        input: &input,
        participant: &participant,
        request: &original,
    };
    let hashes =
        OwnerSetupResourceFinalHttpV2::expected_body_digests(&input, &participant, &original)
            .unwrap();
    let issued = now_ms();
    let payload = PaidResourceReadPayloadV1 {
        api_version: 1,
        issuer_namespace: "account".into(),
        issuer_service_id: "account-service".into(),
        issuer_environment: "test".into(),
        issuer_key_id: "account-key".into(),
        issuer_epoch: DecimalU64(1),
        issuer_config_generation: DecimalU64(1),
        native_audience_origin: "https://localhost".into(),
        world_audience: plan.world_identity.clone(),
        native_path: "/entry-owner/control".into(),
        world_path: resource::RESOURCE_GOAL_PATH_V2.into(),
        stage: PaidResourceReadStageV1::OwnerSetupFinal,
        custody_account_id: "custody-a".into(),
        manager_native_account_id: input.approval.proof.manager_account_id.clone(),
        participant_native_account_id: f.binding.native_account_id.clone(),
        provision_id: plan.provision_id.clone(),
        setup_id: plan.setup_id.clone(),
        test_session_id: None,
        approval_operation_id: input.operation_id.clone(),
        approval_input_digest: hashes.approval_input,
        plan_digest: plan.digest().unwrap(),
        prepared_resource_basis_digest: None,
        selection_digest: hashes.selection,
        owner_public_key: q.subject_public_key.clone(),
        owner_binding: q.request.expected_fence.owner_binding.clone(),
        task_fence_digest: hashes.task_fence,
        signer_record_digest: hashes.signer_record,
        native_request_digest: hashes.native_body,
        world_request_digest: world_hash(resource::RESOURCE_GOAL_PATH_V2, &q),
        preflight_world_request_digest: Some(world_hash(
            ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
            &access,
        )),
        source_operation_id: "resource-source".into(),
        source_record_digest: "a4".repeat(32),
        source_security_epoch: input.request.context.security_version,
        service_epoch: input.request.context.service_epoch,
        directory_generation: DecimalU64(1),
        resource_flag_generation: DecimalU64(1),
        paid_attempt_id: "resource-physical-once".into(),
        paid_record_digest: "a5".repeat(32),
        native_http_units: DecimalU64(1),
        world_http_units: DecimalU64(2),
        issued_at: DecimalU64(issued),
        expires_at: DecimalU64(issued + 5000),
    };
    let proof = PaidResourceReadProofV1 {
        signature_hex: hex::encode(
            SigningKey::from_bytes(&[81; 32])
                .sign(&payload.signing_bytes().unwrap())
                .to_bytes(),
        ),
        payload,
    };
    OwnerSetupResourceFinalHttpV2::decode_http(
        &serde_json::to_vec(&body).unwrap(),
        &URL_SAFE_NO_PAD.encode(serde_json::to_vec(&proof).unwrap()),
    )
    .unwrap()
}
