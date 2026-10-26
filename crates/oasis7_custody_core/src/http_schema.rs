//! Explicit developer export from real HTTP DTOs. No handler/build.rs invokes this.
use super::*;
use crate::{DecimalU64, approval::*, provisioning::*};
use serde_json::{Value, json};

fn context() -> ApprovalContextV1 {
    ApprovalContextV1 {
        environment: "test".into(),
        custody_account_id: "custody-one".into(),
        audience: "account-authority:test".into(),
        policy_version: DecimalU64(1),
        security_version: DecimalU64(2),
        service_epoch: DecimalU64(3),
    }
}
fn options() -> Vec<ApprovalOptionsRequestV1> {
    let request = ApprovalOptionsRequestV1 {
        approval_request_id: "request-one".into(),
        context: context(),
        intent: CustodyIntentV1::ManagedLogin {
            native_account_id: "native-one".into(),
            login_key_ref: "login-one".into(),
            native_identity_namespace: "identity:test".into(),
            blocked_operation_ref: "blocked-one".into(),
            deadline: DecimalU64(100_000),
        },
    };
    let provision = ApprovalOptionsRequestV1 {
        intent: CustodyIntentV1::Provision {
            provision_id: "provision-one".into(),
            bootstrap_config_version: DecimalU64(1),
            world_identity: ApprovedWorldV1 {
                world_id: "world-one".into(),
                genesis_digest: "genesis-one".into(),
                branch_id: "branch-one".into(),
                reorg_epoch: DecimalU64(1),
                service_public_key: "aa".repeat(32),
            },
            identity_route: "identity-main".into(),
            world_route: "world-main".into(),
            native_identity_namespace: "identity:test".into(),
            register_request_id: "register-one".into(),
            login_request_id: "login-one".into(),
            deadline: DecimalU64(100_000),
            transport_limit: DecimalU64(24),
        },
        ..request.clone()
    };
    let plan = json!({"kind":"test_session","session_id":"session-one","native_account_id":"native-one",
        "world_identity":{"world_id":"world-one","genesis_digest":"genesis-one","branch_id":"branch-one","reorg_epoch":"1","service_public_key":"aa".repeat(32)},
        "task_fence":{"world_id":"world-one","genesis_digest":"genesis-one","branch_id":"branch-one","reorg_epoch":"1",
            "agent_id":"agent-one","owner_binding":"owner-one","owner_generation":"1","owner_public_key":"bb".repeat(32),
            "task_id":"task-one","task_revision":"1","task_status":"active","latest_goal_revision":"0","active_goal_revision":"0","active_goal_reference":null,
            "executor_epoch":"1","executor_binding":{"executor_id":"executor-one","executor_public_key":"cc".repeat(32),"delegation_generation":"1","executor_epoch":"1"}},
        "executor_key_ref":"executor-one","client_binding":{"profile":"native-broker-custody-v1","client_key":"dd".repeat(32)},
        "runtime_ref":"codex","skill_ref":"skill-one","source_ref":"source-one",
        "actions":[{"tool":"move","position_cm":["9007199254740993","-9223372036854775808","9223372036854775807"]},
            {"tool":"transfer","from":"agent:agent-one","to":"site:site-one","material":"iron_ore","amount":"1","distance_km":"0","priority":"standard","routing":{"kind":"direct"}},
            {"tool":"build","site":"site-one","factory":"factory-one","factory_kind":"factory.smelter.mk1"},
            {"tool":"schedule","factory":"factory-one","recipe":"recipe.smelter.iron_ingot","batches":1}],
        "allowed_resource_refs":["agent:agent-one","site:site-one","iron_ore","site-one","factory-one","recipe.smelter.iron_ingot"],"read_budget":{"requests":"100","transport":"300","business":"0"},
        "cleanup_budget":{"requests":"2","transport":"24","business":"0"},
        "control_budget":{"requests":"80","transport_per_hop":"240","identity_transport":"160","transport_hops":1},
        "write_deadline":"301000","read_until":"601000","cleanup_until":"601000","request_limit":"100","transport_limit":"400",
        "business_limit":"18446744073709551615","termination_plan_digest":"ee".repeat(32)});
    let session = ApprovalOptionsRequestV1 {
        intent: serde_json::from_value(plan).unwrap(),
        ..request.clone()
    };
    let requests = vec![request, provision, session];
    for request in &requests {
        request.validate(1000).unwrap();
    }
    requests
}
pub(super) fn export(directory: &std::path::Path) {
    let requests = options();
    let intent_digest = requests[0].intent_digest().unwrap();
    let proof = CustodyApprovalProofV1 {
        version: 1,
        key_id: "key-one".into(),
        identity_namespace: "identity:test".into(),
        approval_id: "approval-one".into(),
        approval_request_id: "request-one".into(),
        context: context(),
        intent_digest: intent_digest.clone(),
        manager_account_id: "manager-one".into(),
        manager_session_ref: "manager-session".into(),
        origin_credential_id: "origin-one".into(),
        origin_credential_version: DecimalU64(u64::MAX),
        passkey_credential_id: "passkey-one".into(),
        passkey_credential_version: DecimalU64(9_007_199_254_740_993),
        manager_policy_version: DecimalU64(1),
        manager_security_version: DecimalU64(2),
        identity_instance_epoch: DecimalU64(3),
        nonce: "proof-nonce".into(),
        issued_at: DecimalU64(1000),
        expires_at: DecimalU64(31000),
    };
    let approval = SignedCustodyApprovalV1 {
        proof,
        signature: "structural-fixture-only".into(),
    };
    let verify = ApprovalVerifyRequestV1 {
        approval: approval.clone(),
        proof_digest: approval.digest().unwrap(),
        expected_audience: context().audience,
        expected_intent_digest: intent_digest.clone(),
        purpose: VerificationPurposeV1::ConsumeApproval,
        verify_nonce: "owner-nonce".into(),
    };
    let response = SignedApprovalVerificationV1 {
        signature: "structural-fixture-only".into(),
        verification: ApprovalVerificationV1 {
            version: 1,
            key_id: "key-one".into(),
            identity_namespace: "identity:test".into(),
            purpose: verify.purpose,
            proof_digest: verify.proof_digest.clone(),
            approval_request_id: "request-one".into(),
            audience: context().audience,
            intent_digest: intent_digest.clone(),
            verify_nonce: verify.verify_nonce.clone(),
            manager_security_version: DecimalU64(2),
            manager_policy_version: DecimalU64(1),
            origin_credential_version: DecimalU64(u64::MAX),
            identity_instance_epoch: DecimalU64(3),
            credential_version: DecimalU64(9_007_199_254_740_993),
            observed_at: DecimalU64(2000),
            proof_expires_at: DecimalU64(31000),
            source_session_expires_at: DecimalU64(100000),
            source_session_idle_expires_at: DecimalU64(90000),
            proof_state: ProofStateV1::Active,
            origin_state: OriginStateV1::Active,
        },
    };
    let complete = ApprovalCompleteRequestV1 {
        approval_id: "approval-one".into(),
        ceremony_id: "approval-one".into(),
        operation_id: "approval-one".into(),
        intent_digest: intent_digest.clone(),
        response: json!({"type":"public-key"}),
    };
    let options_response = ApprovalOptionsResponseV1 {
        api_version: 1,
        phase: ApprovalPhaseV1::Pending,
        approval_id: "approval-one".into(),
        ceremony_id: "approval-one".into(),
        operation_id: "approval-one".into(),
        options: Some(json!({"publicKey":{"userVerification":"required"}})),
        intent_digest,
        audience: context().audience,
        ceremony_expires_at: DecimalU64(121000),
    };
    let complete_response = ApprovalCompleteResponseV1 {
        api_version: 1,
        phase: ApprovalPhaseV1::Completed,
        approval,
        proof_bytes: "structural-fixture-only".into(),
    };
    let status = CustodySessionStatusV1 {
        api_version: 1,
        identity_namespace: "identity:test".into(),
        identity_instance_epoch: DecimalU64(3),
        account_id: "native-one".into(),
        session_ref: "session-one".into(),
        credential_ref: "credential-one".into(),
        credential_version: DecimalU64(9_007_199_254_740_993),
        security_version: DecimalU64(2),
        policy_version: DecimalU64(1),
        server_time: DecimalU64(1000),
        expires_at: DecimalU64(u64::MAX),
        idle_expires_at: DecimalU64(100000),
    };
    let manager = ManagerPrincipal {
        identity_namespace: "identity:test".into(),
        native_account_id: "manager-one".into(),
    };
    let routes = ProvisionRoutes {
        environment: "test".into(),
        identity_route: "native-test".into(),
        world_route: "world-test".into(),
        world_identity: "world-one".into(),
    };
    let managers = vec![BootstrapManager {
        principal: manager.clone(),
        environments: vec!["test".into()],
    }];
    let plan = ProvisionPlan {
        environment: routes.environment.clone(),
        identity_route: routes.identity_route.clone(),
        world_route: routes.world_route.clone(),
        world_identity: routes.world_identity.clone(),
        purpose: "prepared_world".into(),
        expires_at: DecimalU64(100_000),
        transport_limit: DecimalU64(24),
    };
    let draft_request = ProvisionDraftRequestV1 {
        api_version: 1,
        client_request_id: "client-one".into(),
        plan: plan.clone(),
    };
    let mut index =
        ManagerAccountIndex::new_for_environment(manager.clone(), "test".into()).unwrap();
    let entry = index
        .reserve(
            &manager,
            "client-one",
            &plan,
            &ProvisionPolicy {
                routes: &routes,
                managers: &managers,
                version: 1,
            },
            1000,
        )
        .unwrap();
    let draft_view = ProvisionDraftViewV1 {
        api_version: 1,
        phase: DraftPhase::DraftInitialized,
        entry,
        plan,
        world_identity: ApprovedWorldV1 {
            world_id: "world-one".into(),
            genesis_digest: "genesis-one".into(),
            branch_id: "branch-one".into(),
            reorg_epoch: DecimalU64(0),
            service_public_key: "aa".repeat(32),
        },
        security_epoch: DecimalU64(0),
    };
    let freeze_request = ManagerFreezeRequestV1 {
        api_version: 1,
        operation_id: "freeze-one".into(),
    };
    let closure_view = ManagerClosureViewV1 {
        api_version: 1,
        status: ManagerClosureStatus::Open,
        associated_accounts: DecimalU64(1),
        pending_accounts: vec![],
        pending_operations: vec![],
    };
    let schemas = json!({
        "ApprovalOptionsRequestV1":schemars::schema_for!(ApprovalOptionsRequestV1),
        "ApprovalOptionsResponseV1":schemars::schema_for!(ApprovalOptionsResponseV1),
        "ApprovalCompleteRequestV1":schemars::schema_for!(ApprovalCompleteRequestV1),
        "ApprovalCompleteResponseV1":schemars::schema_for!(ApprovalCompleteResponseV1),
        "ApprovalVerifyRequestV1":schemars::schema_for!(ApprovalVerifyRequestV1),
        "SignedApprovalVerificationV1":schemars::schema_for!(SignedApprovalVerificationV1),
        "CustodySessionStatusV1":schemars::schema_for!(CustodySessionStatusV1),
        "ProvisionDraftRequestV1":schemars::schema_for!(ProvisionDraftRequestV1),
        "ProvisionDraftViewV1":schemars::schema_for!(ProvisionDraftViewV1),
        "ManagerFreezeRequestV1":schemars::schema_for!(ManagerFreezeRequestV1),
        "ManagerClosureViewV1":schemars::schema_for!(ManagerClosureViewV1),
    });
    let fixtures = json!({"metadata":([Role::Identity,Role::Owner,Role::Executor,Role::RecoveryFence].map(ServiceMetadata::disabled)),
        "error":([ErrorCode::NotFound,ErrorCode::NotReady,ErrorCode::MethodNotAllowed].map(ErrorResponse::new)),
        "ApprovalOptionsRequestV1":requests,"ApprovalOptionsResponseV1":[options_response],"ApprovalCompleteRequestV1":[complete],
        "ApprovalCompleteResponseV1":[complete_response],"ApprovalVerifyRequestV1":[verify],"SignedApprovalVerificationV1":[response],"CustodySessionStatusV1":[status],"ProvisionDraftRequestV1":[draft_request],"ProvisionDraftViewV1":[draft_view],"ManagerFreezeRequestV1":[freeze_request],"ManagerClosureViewV1":[closure_view]});
    let decimal_fixtures = [
        json!(0),
        json!(9007199254740992_u64),
        json!(""),
        json!("0"),
        json!("01"),
        json!("+1"),
        json!("-0"),
        json!("-1"),
        json!("1.0"),
        json!("1e3"),
        json!("9007199254740993"),
        json!("18446744073709551615"),
        json!("18446744073709551616"),
        json!("99999999999999999999"),
    ]
    .map(|value| {
        let accepted = serde_json::from_value::<DecimalU64>(value.clone()).is_ok();
        json!({"value":value,"accepted_by_rust":accepted})
    });
    let document: Value = json!({"metadata":schemars::schema_for!(ServiceMetadata),"error":schemars::schema_for!(ErrorResponse),
        "native":schemas,"fixtures":fixtures,"decimal_fixtures":decimal_fixtures});
    std::fs::write(
        directory.join("http.schema.json"),
        format!("{}\n", serde_json::to_string_pretty(&document).unwrap()),
    )
    .unwrap();
}
