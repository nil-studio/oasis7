use super::*;
pub(super) fn fixture(approval: &SignedCustodyApprovalV1) -> Value {
    let setup: OwnerSetupPrepareRequestV1 = serde_json::from_value(
        owner_setup::fixture(approval)["OwnerSetupPrepareRequestV1"][0].clone(),
    )
    .unwrap();
    let p = OwnerSetupReconcilePlanV1 {
        api_version: 1,
        reconcile_id: "reconcile-one".into(),
        setup_id: setup.plan.setup_id.clone(),
        provision_id: setup.plan.provision_id.clone(),
        stage: OwnerSetupStageV1::CreateTask,
        setup_plan_digest: setup.plan.digest().unwrap(),
        original_digest: "ad".repeat(32),
        correlation_digest: "ae".repeat(32),
        world_identity: setup.plan.world_identity.clone(),
        owner_key_ref: setup.plan.owner_key_ref.clone(),
        executor_key_ref: setup.plan.executor_key_ref.clone(),
        original_deadline: setup.plan.deadline,
        deadline: setup.plan.deadline,
        attempt_limit: 1,
        transport_limit: DecimalU64(112),
    };
    let request = ApprovalOptionsRequestV1 {
        approval_request_id: "reconcile-approval-one".into(),
        context: context(),
        intent: p.intent().unwrap(),
    };
    request.validate(1000).unwrap();
    let mut signed = approval.clone();
    signed.proof.context = request.context.clone();
    signed.proof.approval_request_id = request.approval_request_id.clone();
    signed.proof.intent_digest = request.intent_digest().unwrap();
    let basis = OwnerSetupReconcileBasisV1 {
        api_version: 1,
        custody_account_id: "custody-one".into(),
        provision_id: p.provision_id.clone(),
        setup_id: p.setup_id.clone(),
        stage: p.stage,
        setup_plan_digest: p.setup_plan_digest.clone(),
        original_digest: p.original_digest.clone(),
        correlation_digest: p.correlation_digest.clone(),
        world_identity: p.world_identity.clone(),
        owner_key_ref: p.owner_key_ref.clone(),
        executor_key_ref: p.executor_key_ref.clone(),
        original_deadline: p.original_deadline,
        original_transport_remaining: DecimalU64(488),
        security_epoch: context().security_version,
        service_epoch: context().service_epoch,
    };
    json!({"OwnerSetupReconcileBasisRequestV1":[OwnerSetupReconcileBasisRequestV1{api_version:1,setup_id:p.setup_id.clone(),stage:p.stage}],"OwnerSetupReconcileBasisV1":[basis],"OwnerSetupReconcilePrepareRequestV1":[OwnerSetupReconcilePrepareRequestV1{api_version:1,approval_request_id:request.approval_request_id.clone(),plan:p.clone()}],"OwnerSetupReconcilePrepareResponseV1":[request.clone()],"OwnerSetupReconcileApproveRequestV1":[OwnerSetupReconcileApproveRequestV1{api_version:1,input:crate::approval_admission::ApprovalAdmissionInput{operation_id:p.reconcile_id.clone(),request,approval:signed},plan:p.clone()}],"OwnerSetupReconcileAttemptRequestV1":[OwnerSetupReconcileAttemptRequestV1{api_version:1,reconcile_id:p.reconcile_id.clone(),attempt_id:"attempt-one".into()}],"OwnerSetupReconcileProgressV1":[OwnerSetupReconcileProgressV1{api_version:1,custody_account_id:"custody-one".into(),provision_id:p.provision_id,setup_id:p.setup_id,reconcile_id:p.reconcile_id,stage:p.stage,pending_attempts:vec!["attempt-one".into()],completed_attempts:vec![],transport_remaining:DecimalU64(0),original_transport_remaining:DecimalU64(488)}]})
}
