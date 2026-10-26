use super::*;
use oasis7_custody_core::{http::Role, provisioning::*};

pub(super) fn fixture() -> (
    AccountDraftV1,
    ApprovalAdmissionInput,
    ApprovalAdmissionContext,
) {
    let (mut input, mut context) = super::fixture();
    context.authority.security_version = DecimalU64(0);
    input.request.context = context.authority.clone();
    let world = ApprovedWorldV1 {
        world_id: "world-one".into(),
        genesis_digest: "genesis-one".into(),
        branch_id: "branch-one".into(),
        reorg_epoch: DecimalU64(0),
        service_public_key: "aa".repeat(32),
    };
    let plan = ProvisionPlan {
        environment: "test".into(),
        identity_route: "native-main".into(),
        world_route: "world-main".into(),
        world_identity: world.world_id.clone(),
        purpose: "controlled-test-account".into(),
        expires_at: DecimalU64(80_000),
        transport_limit: DecimalU64(24),
    };
    let draft = AccountDraftV1 {
        entry: DirectoryEntry {
            provision_id: "provision-one".into(),
            custody_account_id: context.authority.custody_account_id.clone(),
            manager: ManagerPrincipal {
                identity_namespace: context.identity_namespace.clone(),
                native_account_id: context.source.manager_account_id.clone(),
            },
            client_request_id: "create-one".into(),
            environment: plan.environment.clone(),
            plan_digest: plan.digest().unwrap(),
            bootstrap_config_version: DecimalU64(1),
        },
        plan: plan.clone(),
        world_identity: world.clone(),
    };
    input.request.intent = CustodyIntentV1::Provision {
        provision_id: draft.entry.provision_id.clone(),
        purpose: plan.purpose.clone(),
        plan_digest: plan.digest().unwrap(),
        bootstrap_config_version: DecimalU64(1),
        world_identity: world,
        identity_route: plan.identity_route,
        world_route: plan.world_route,
        native_identity_namespace: context.identity_namespace.clone(),
        register_request_id: "register-one".into(),
        login_request_id: "login-one".into(),
        deadline: plan.expires_at,
        transport_limit: plan.transport_limit,
    };
    input.approval.proof.context = input.request.context.clone();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval = sign_proof(input.approval.proof);
    (draft, input, context)
}
struct Policy {
    routes: ProvisionRoutes,
    managers: Vec<BootstrapManager>,
    world: ApprovedWorldV1,
    namespace: String,
}
impl Policy {
    fn new(draft: &AccountDraftV1) -> Self {
        Self {
            routes: ProvisionRoutes {
                environment: draft.plan.environment.clone(),
                identity_route: draft.plan.identity_route.clone(),
                world_route: draft.plan.world_route.clone(),
                world_identity: draft.plan.world_identity.clone(),
            },
            managers: vec![BootstrapManager {
                principal: draft.entry.manager.clone(),
                environments: vec![draft.plan.environment.clone()],
            }],
            world: draft.world_identity.clone(),
            namespace: draft.entry.manager.identity_namespace.clone(),
        }
    }
    fn current<'a>(
        &'a self,
        approval: &'a ApprovalAdmissionContext,
    ) -> ProvisionApprovalContext<'a> {
        ProvisionApprovalContext {
            approval,
            policy: ProvisionPolicy {
                routes: &self.routes,
                managers: &self.managers,
                version: 1,
            },
            world_identity: &self.world,
            native_identity_namespace: &self.namespace,
        }
    }
}
fn begin(
    state: &mut AccountProvisionApprovalState,
    input: ApprovalAdmissionInput,
    context: &ApprovalAdmissionContext,
) -> ApprovalAdmissionRecord {
    let policy = Policy::new(state.draft());
    let current = policy.current(context);
    let checks = state
        .begin_checks(&input, "owner-nonce", &current, 1000)
        .unwrap();
    state
        .begin(
            input,
            "owner-nonce".into(),
            &current,
            1000,
            &mut ExactCache::verified(checks),
        )
        .unwrap()
}
fn approved() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    ApprovalAdmissionRecord,
    SignedApprovalVerificationV1,
) {
    let (draft, input, context) = fixture();
    let mut state = AccountProvisionApprovalState::new(draft).unwrap();
    let pending = begin(&mut state, input, &context);
    let signed = verification(&pending, &context);
    let policy = Policy::new(state.draft());
    let current = policy.current(&context);
    let checks = state
        .commit_checks("operation-one", &signed, &current, 1200)
        .unwrap();
    let record = state
        .commit(
            "operation-one",
            signed.clone(),
            &current,
            1200,
            &mut ExactCache::verified(checks),
        )
        .unwrap();
    (state, context, record, signed)
}
#[test]
fn consumed_proof_atomically_installs_exact_plan_and_three_fixed_inert_tasks() {
    let (state, context, record, signed) = approved();
    assert_eq!(state.provision().stage(), ProvisionStage::Approved);
    assert_eq!(record.stage(), ApprovalAdmissionStage::Consumed);
    assert_eq!(state.role_key_tasks().len(), 3);
    for (name, role) in [
        ("identity", Role::Identity),
        ("owner", Role::Owner),
        ("executor", Role::Executor),
    ] {
        let task = &state.role_key_tasks()[name];
        assert_eq!(task.role, role);
        assert_eq!(task.key_version.0, 1);
        assert_eq!(task.state, ProvisionRoleKeyTaskStateV1::Pending);
        assert_eq!(task.custody_account_id, "custody-one");
        assert_eq!(task.provision_id, "provision-one");
    }
    assert_ne!(
        state.role_key_tasks()["owner"].key_id,
        state.role_key_tasks()["executor"].key_id
    );
    let json = serde_json::to_string(&state).unwrap();
    let mut restored: AccountProvisionApprovalState = serde_json::from_str(&json).unwrap();
    let policy = Policy::new(restored.draft());
    let current = policy.current(&context);
    let checks = restored
        .commit_checks("operation-one", &signed, &current, 90_000)
        .unwrap();
    assert_eq!(
        restored
            .commit(
                "operation-one",
                signed,
                &current,
                90_000,
                &mut ExactCache::verified(checks)
            )
            .unwrap(),
        record
    );
    assert_eq!(restored, state);
}
#[test]
fn purpose_plan_pins_principal_and_current_policy_are_not_interchangeable_digests() {
    let (draft, input, context) = fixture();
    let state = AccountProvisionApprovalState::new(draft.clone()).unwrap();
    let policy = Policy::new(&draft);
    for (field, value) in [
        ("purpose", json!("other")),
        ("plan_digest", json!(input.request.intent_digest().unwrap())),
        ("provision_id", json!("another")),
        ("identity_route", json!("other")),
        ("world_route", json!("other")),
        ("bootstrap_config_version", json!("2")),
        ("deadline", json!("79000")),
        ("transport_limit", json!("23")),
        ("native_identity_namespace", json!("identity:other")),
    ] {
        let mut raw = serde_json::to_value(&input).unwrap();
        raw["request"]["intent"][field] = value;
        let mut other: ApprovalAdmissionInput = serde_json::from_value(raw).unwrap();
        other.approval.proof.intent_digest = other.request.intent_digest().unwrap();
        other.approval = sign_proof(other.approval.proof);
        assert!(
            state
                .begin_checks(&other, "owner-nonce", &policy.current(&context), 1000)
                .is_err(),
            "{field}"
        );
    }
    let mut other = input.clone();
    if let CustodyIntentV1::Provision { world_identity, .. } = &mut other.request.intent {
        world_identity.service_public_key = "bb".repeat(32);
    }
    other.approval.proof.intent_digest = other.request.intent_digest().unwrap();
    other.approval = sign_proof(other.approval.proof);
    assert!(
        state
            .begin_checks(&other, "owner-nonce", &policy.current(&context), 1000)
            .is_err()
    );
    let mut no_manager = Policy::new(&draft);
    no_manager.managers.clear();
    assert!(
        state
            .begin_checks(&input, "n", &no_manager.current(&context), 1000)
            .is_err()
    );
    let mut stale = context.clone();
    stale.source.manager_account_id = "other".into();
    assert!(
        state
            .begin_checks(&input, "n", &policy.current(&stale), 1000)
            .is_err()
    );
    let mut legacy = serde_json::to_value(&input.request).unwrap();
    legacy["intent"].as_object_mut().unwrap().remove("purpose");
    assert!(serde_json::from_value::<ApprovalOptionsRequestV1>(legacy).is_err());
}
#[test]
fn failure_after_book_consumption_rolls_back_entire_pure_transition() {
    let (draft, mut input, context) = fixture();
    // Native proof validates this bounded nonce, but ProvisionRecord requires a stable ID.
    input.approval.proof.nonce = "not a stable id".into();
    input.approval = sign_proof(input.approval.proof);
    let mut state = AccountProvisionApprovalState::new(draft).unwrap();
    let pending = begin(&mut state, input, &context);
    let before = state.clone();
    let signed = verification(&pending, &context);
    let policy = Policy::new(state.draft());
    let checks = state
        .commit_checks("operation-one", &signed, &policy.current(&context), 1200)
        .unwrap();
    assert!(
        state
            .commit(
                "operation-one",
                signed,
                &policy.current(&context),
                1200,
                &mut ExactCache::verified(checks)
            )
            .is_err()
    );
    assert_eq!(state, before);
    assert!(state.role_key_tasks().is_empty());
    assert_eq!(
        state.original("operation-one").unwrap().stage(),
        ApprovalAdmissionStage::Pending
    );
}
#[test]
fn cancellation_and_freeze_never_recreate_tasks_or_consume_a_fresh_proof() {
    let (draft, input, context) = fixture();
    let mut pending = AccountProvisionApprovalState::new(draft).unwrap();
    let record = begin(&mut pending, input, &context);
    let signed = verification(&record, &context);
    pending.cancel("cancel-one", 1, 2000).unwrap();
    let before = pending.clone();
    let policy = Policy::new(pending.draft());
    assert!(
        pending
            .commit_checks("operation-one", &signed, &policy.current(&context), 2001)
            .is_err()
    );
    assert_eq!(pending, before);
    assert!(pending.role_key_tasks().is_empty());
    assert!(pending.cancel("cancel-other", 1, 2001).is_err());
    let (mut state, mut context, record, signed) = approved();
    state.freeze(1).unwrap();
    context.frozen = true;
    let before = state.clone();
    let policy = Policy::new(state.draft());
    let current = policy.current(&context);
    let checks = state
        .commit_checks("operation-one", &signed, &current, 999_999)
        .unwrap();
    assert_eq!(
        state
            .commit(
                "operation-one",
                signed.clone(),
                &current,
                999_999,
                &mut ExactCache::verified(checks)
            )
            .unwrap(),
        record
    );
    assert_eq!(state, before);
    assert!(
        state
            .role_key_tasks()
            .values()
            .all(|t| t.state == ProvisionRoleKeyTaskStateV1::Cancelled)
    );
    let checks = state
        .begin_checks(record.input(), "never-refresh", &current, 999_999)
        .unwrap();
    assert_eq!(
        state
            .begin(
                record.input().clone(),
                "never-refresh".into(),
                &current,
                999_999,
                &mut ExactCache::verified(checks)
            )
            .unwrap(),
        record
    );
    assert_eq!(state, before);
    let mut other = signed;
    other.verification.verify_nonce = "changed".into();
    other = sign_verification(other.verification);
    assert!(
        state
            .commit_checks("operation-one", &other, &current, 999_999)
            .is_err()
    );
}
#[test]
fn restore_rejects_fabricated_approval_or_mismatched_fixed_outbox() {
    let (state, _, _, _) = approved();
    let original = serde_json::to_value(&state).unwrap();
    for field in ["accepted_operation_id", "role_key_tasks"] {
        let mut bad = original.clone();
        bad[field] = if field == "role_key_tasks" {
            json!({})
        } else {
            Value::Null
        };
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
    }
    for (field, value) in [
        ("role", json!("recovery-fence")),
        ("key_version", json!("2")),
        ("state", json!("cancelled")),
        ("custody_account_id", json!("other")),
    ] {
        let mut bad = original.clone();
        bad["role_key_tasks"]["owner"][field] = value;
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
    }
    let mut bad = original;
    bad["provision"]["approval_nonce"] = json!("another");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
}

#[test]
fn zero_target_generation_is_only_for_a_pristine_bound_provision_draft() {
    let (draft, input, context) = fixture();
    input.request.validate(1000).unwrap();
    let (mut login, _) = super::fixture();
    login.request.context.security_version = DecimalU64(0);
    assert!(login.request.validate(1000).is_err());
    let state = AccountProvisionApprovalState::new(draft.clone()).unwrap();
    let policy = Policy::new(&draft);
    let mut wrong = context;
    wrong.authority.security_version = DecimalU64(1);
    assert!(
        state
            .begin_checks(&input, "nonce", &policy.current(&wrong), 1000)
            .is_err()
    );
    let mut raw = serde_json::to_value(input.request).unwrap();
    raw["intent"].as_object_mut().unwrap().remove("plan_digest");
    assert!(serde_json::from_value::<ApprovalOptionsRequestV1>(raw).is_err());
}

#[test]
fn pending_body_conflicts_and_current_source_failures_leave_plan_unapproved() {
    let (draft, input, context) = fixture();
    let mut state = AccountProvisionApprovalState::new(draft).unwrap();
    let pending = begin(&mut state, input.clone(), &context);
    let before = state.clone();
    let policy = Policy::new(state.draft());
    let mut other = input;
    if let CustodyIntentV1::Provision {
        register_request_id,
        ..
    } = &mut other.request.intent
    {
        *register_request_id = "another-register".into();
    }
    other.approval.proof.intent_digest = other.request.intent_digest().unwrap();
    other.approval = sign_proof(other.approval.proof);
    assert!(
        state
            .begin_checks(&other, "different", &policy.current(&context), 1001)
            .is_err()
    );
    let signed = verification(&pending, &context);
    for change in 0..5 {
        let mut current = context.clone();
        match change {
            0 => current.source.manager_security_version = DecimalU64(6),
            1 => current.source.origin_credential_version = DecimalU64(1),
            2 => current.source.passkey_credential_version = DecimalU64(1),
            3 => current.fence = RecoveryFence::new(3, true),
            _ => current.frozen = true,
        }
        assert!(
            state
                .commit_checks("operation-one", &signed, &policy.current(&current), 1200)
                .is_err()
        );
    }
    assert!(
        state
            .commit_checks("operation-one", &signed, &policy.current(&context), 31_001)
            .is_err()
    );
    let mut bad = signed;
    bad.signature = URL_SAFE_NO_PAD.encode([0_u8; 64]);
    assert!(
        state
            .commit(
                "operation-one",
                bad,
                &policy.current(&context),
                1200,
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(state, before);
    assert_eq!(state.provision().stage(), ProvisionStage::Draft);
}

#[test]
fn historical_consumption_is_not_a_cross_principal_or_epoch_replay() {
    let (state, context, _, signed) = approved();
    let policy = Policy::new(state.draft());
    for changed in 0..5 {
        let mut current = context.clone();
        match changed {
            0 => current.authority.custody_account_id = "other".into(),
            1 => current.source.manager_account_id = "other".into(),
            2 => current.identity_namespace = "other".into(),
            3 => current.identity_instance_epoch = DecimalU64(7),
            _ => current.authority.service_epoch = DecimalU64(4),
        }
        assert!(
            state
                .commit_checks("operation-one", &signed, &policy.current(&current), 999_999)
                .is_err()
        );
    }
}

fn role_result(
    state: &AccountProvisionApprovalState,
    name: &str,
    byte: &str,
) -> ProvisionRoleKeyResultV1 {
    let task = &state.role_key_tasks()[name];
    ProvisionRoleKeyResultV1 {
        scope: oasis7_custody_core::role_key_slot::RoleKeySlotScopeV1::from_task(task).unwrap(),
        public_ref: PublicRoleKey {
            key_id: task.key_id.clone(),
            key_version: task.key_version,
            public_key: byte.repeat(32),
        },
    }
}

#[test]
fn role_results_require_three_exact_current_receipts_and_restore() {
    let (mut state, context, _, _) = approved();
    let policy = Policy::new(state.draft());
    for (index, (role, byte)) in [("identity", "11"), ("owner", "22"), ("executor", "33")]
        .into_iter()
        .enumerate()
    {
        let result = role_result(&state, role, byte);
        // The native handoff proof expired at 31000; current source and accepted plan remain valid.
        state
            .record_role_key_result(result.clone(), &policy.current(&context), 40_000)
            .unwrap();
        assert_eq!(
            state.provision().stage(),
            if index == 2 {
                ProvisionStage::RoleKeysReady
            } else {
                ProvisionStage::Approved
            }
        );
        let restored: AccountProvisionApprovalState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        assert_eq!(state, restored);
        let before = state.clone();
        state
            .record_role_key_result(result, &policy.current(&context), 999_999)
            .unwrap();
        assert_eq!(state, before);
    }
    let mut missing = serde_json::to_value(&state).unwrap();
    missing.as_object_mut().unwrap().remove("role_key_results");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
}

#[test]
fn role_results_reject_replacement_scope_alias_and_stale_source_atomically() {
    let (mut state, context, _, _) = approved();
    let policy = Policy::new(state.draft());
    let identity = role_result(&state, "identity", "11");
    state
        .record_role_key_result(identity.clone(), &policy.current(&context), 1201)
        .unwrap();
    let before = state.clone();
    let mut changed = identity.clone();
    changed.public_ref.public_key = "44".repeat(32);
    assert!(
        state
            .record_role_key_result(changed, &policy.current(&context), 1202)
            .is_err()
    );
    let owner = role_result(&state, "owner", "11");
    assert!(
        state
            .record_role_key_result(owner, &policy.current(&context), 1202)
            .is_err()
    );
    let mut wrong = role_result(&state, "owner", "22");
    wrong.scope.operation_id = "other-operation".into();
    assert!(
        state
            .record_role_key_result(wrong, &policy.current(&context), 1202)
            .is_err()
    );
    for kind in 0..5 {
        let mut current = context.clone();
        match kind {
            0 => current.source.manager_security_version = DecimalU64(99),
            1 => current.source.origin_credential_version = DecimalU64(99),
            2 => current.source.passkey_credential_version = DecimalU64(99),
            3 => current.fence = RecoveryFence::new(3, true),
            _ => current.frozen = true,
        }
        let result = role_result(&state, "owner", "22");
        assert!(
            state
                .record_role_key_result(result, &policy.current(&current), 1202)
                .is_err()
        );
    }
    assert_eq!(state, before);
    state.freeze(1).unwrap();
    let frozen = state.clone();
    state
        .record_role_key_result(identity, &policy.current(&context), 999_999)
        .unwrap();
    assert_eq!(state, frozen);
    let result = role_result(&state, "owner", "22");
    assert!(
        state
            .record_role_key_result(result, &policy.current(&context), 1202)
            .is_err()
    );
    assert_eq!(state, frozen);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&state).unwrap()
        )
        .unwrap(),
        state
    );
}

#[test]
fn legacy_approved_snapshot_defaults_only_empty_results() {
    let (state, _, _, _) = approved();
    let mut json = serde_json::to_value(&state).unwrap();
    json.as_object_mut().unwrap().remove("role_key_results");
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(json).unwrap(),
        state
    );
    let (_, input, _) = fixture();
    let request = ProvisionApproveRequestV1 {
        api_version: 1,
        input,
    };
    request.validate().unwrap();
    let mut json = serde_json::to_value(request).unwrap();
    json["api_version"] = serde_json::json!(2);
    assert!(serde_json::from_value::<ProvisionApproveRequestV1>(json).is_err());
}
