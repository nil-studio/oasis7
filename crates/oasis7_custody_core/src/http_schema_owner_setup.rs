use super::*;
pub(super) fn fixture(approval: &SignedCustodyApprovalV1) -> Value {
    let world = ApprovedWorldV1 {
        world_id: "world-one".into(),
        genesis_digest: "genesis-one".into(),
        branch_id: "branch-one".into(),
        reorg_epoch: DecimalU64(1),
        service_public_key: "aa".repeat(32),
    };
    let plan = OwnerSetupPlanV1 {
        api_version: 1,
        setup_id: "setup-one".into(),
        provision_id: "provision-one".into(),
        participant_receipt_digest: "ab".repeat(32),
        world_identity: world.clone(),
        owner_key_ref: "owner-one".into(),
        executor_key_ref: "executor-one".into(),
        bootstrap_config_version: DecimalU64(1),
        goal_content: oasis7_agent_api::game::GoalContentV1 {
            goal_text: "Private structural fixture goal".into(),
            soft_constraints: vec![],
        },
        create_operation_id: "setup-create".into(),
        prepare_operation_id: "setup-prepare".into(),
        prepare_nonce: "ac".repeat(32),
        publish_operation_id: "setup-publish".into(),
        delegation_operation_id: "setup-delegate".into(),
        bind_operation_id: "setup-bind".into(),
        executor_id: "executor-one".into(),
        executor_public_key: "cc".repeat(32),
        delegation_generation: DecimalU64(1),
        delegation_nonce: DecimalU64(1),
        resource_refs: vec![],
        deadline: DecimalU64(100_000),
        request_limit: DecimalU64(6),
        transport_limit: DecimalU64(600),
        write_limit: DecimalU64(4),
    };
    let prepare = OwnerSetupPrepareRequestV1 {
        api_version: 1,
        approval_request_id: "setup-approval-one".into(),
        plan: plan.clone(),
    };
    prepare.validate().unwrap();
    let options = ApprovalOptionsRequestV1 {
        approval_request_id: prepare.approval_request_id.clone(),
        context: context(),
        intent: plan.intent().unwrap(),
    };
    options.validate(1000).unwrap();
    let mut signed = approval.clone();
    signed.proof.context = options.context.clone();
    signed.proof.approval_request_id = options.approval_request_id.clone();
    signed.proof.intent_digest = options.intent_digest().unwrap();
    let approve = OwnerSetupApproveRequestV1 {
        api_version: 1,
        input: crate::approval_admission::ApprovalAdmissionInput {
            operation_id: plan.setup_id.clone(),
            request: options.clone(),
            approval: signed,
        },
        plan: plan.clone(),
    };
    approve.validate().unwrap();
    let basis = OwnerSetupBasisV1 {
        api_version: 1,
        custody_account_id: context().custody_account_id,
        provision_id: plan.provision_id.clone(),
        participant_receipt_digest: plan.participant_receipt_digest.clone(),
        world_identity: world,
        owner_key: PublicRoleKey {
            key_id: plan.owner_key_ref.clone(),
            key_version: DecimalU64(1),
            public_key: "bb".repeat(32),
        },
        executor_key: PublicRoleKey {
            key_id: plan.executor_key_ref.clone(),
            key_version: DecimalU64(1),
            public_key: plan.executor_public_key.clone(),
        },
        native_account_id: "native-one".into(),
        native_identity_namespace: "identity:test".into(),
        identity_instance_epoch: DecimalU64(3),
        bootstrap_config_version: DecimalU64(1),
        security_epoch: context().security_version,
        service_epoch: context().service_epoch,
    };
    json!({"OwnerSetupApproveRequestV1":[approve],"OwnerSetupDriveRequestV1":[OwnerSetupDriveRequestV1{api_version:1,custody_account_id:"custody-one".into(),provision_id:plan.provision_id.clone(),setup_id:plan.setup_id.clone()}],"OwnerSetupProgressV1":[OwnerSetupProgressV1{api_version:1,custody_account_id:"custody-one".into(),provision_id:plan.provision_id.clone(),setup_id:plan.setup_id.clone(),stage:ProvisionStage::ParticipantBound,pending_operations:vec![],blocked_operations:vec![],transport_remaining:DecimalU64(600)}],"OwnerSetupBasisRequestV1":[OwnerSetupBasisRequestV1{api_version:1}],"OwnerSetupBasisV1":[basis],"OwnerSetupPrepareRequestV1":[prepare],"OwnerSetupPrepareResponseV1":[options]})
}
