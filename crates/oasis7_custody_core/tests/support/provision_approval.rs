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

fn attempt_approved() -> (AccountProvisionApprovalState, ApprovalAdmissionContext) {
    let (mut draft, mut input, context) = fixture();
    draft.plan.transport_limit = DecimalU64(128);
    draft.entry.plan_digest = draft.plan.digest().unwrap();
    if let CustodyIntentV1::Provision {
        plan_digest,
        transport_limit,
        ..
    } = &mut input.request.intent
    {
        *plan_digest = draft.entry.plan_digest.clone();
        *transport_limit = draft.plan.transport_limit;
    }
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval = sign_proof(input.approval.proof);
    let mut state = AccountProvisionApprovalState::new(draft).unwrap();
    let pending = begin(&mut state, input, &context);
    let signed = verification(&pending, &context);
    let policy = Policy::new(state.draft());
    state
        .commit(
            "operation-one",
            signed,
            &policy.current(&context),
            1200,
            &mut StrictCrypto,
        )
        .unwrap();
    (state, context)
}
fn inspect_signed(
    state: &AccountProvisionApprovalState,
    context: &ApprovalAdmissionContext,
    id: &str,
    now: u64,
) -> SignedApprovalVerificationV1 {
    let mut value = verification(state.accepted_approval().unwrap(), context).verification;
    value.purpose = VerificationPurposeV1::InspectOrigin;
    value.verify_nonce = state.origin_inspections()[id].verify_nonce.clone();
    value.observed_at = DecimalU64(now);
    value.proof_state = if now >= value.proof_expires_at.0 {
        ProofStateV1::Expired
    } else {
        ProofStateV1::Active
    };
    sign_verification(value)
}
#[test]
fn origin_inspection_persists_nonce_charges_one_shot_fixed_hops_and_accepts_expired_proof_only_for_origin()
 {
    let (mut state, context) = attempt_approved();
    let policy = Policy::new(state.draft());
    let current = policy.current(&context);
    let original = state
        .begin_origin_inspection("inspect-one", &"ab".repeat(32), &current, 40_000)
        .unwrap();
    assert_eq!(
        state
            .begin_origin_inspection("inspect-one", &"cd".repeat(32), &current, 40_001)
            .unwrap(),
        original
    );
    let request = state
        .claim_origin_inspection("inspect-one", &current, 40_001)
        .unwrap();
    assert_eq!(request.purpose, VerificationPurposeV1::InspectOrigin);
    assert_eq!(request.verify_nonce, original.verify_nonce);
    assert_eq!(
        state.provision().transport_remaining().unwrap(),
        128 - PROVISION_ORIGIN_TRANSPORT_COST
    );
    let claimed = state.clone();
    assert!(
        state
            .claim_origin_inspection("inspect-one", &current, 40_002)
            .is_err()
    );
    assert_eq!(
        state, claimed,
        "unknown first send cannot silently send or charge twice"
    );
    state
        .begin_origin_inspection("inspect-two", &"cd".repeat(32), &current, 40_002)
        .unwrap();
    state
        .claim_origin_inspection("inspect-two", &current, 40_003)
        .unwrap();
    assert_eq!(
        state.provision().transport_remaining().unwrap(),
        128 - 2 * PROVISION_ORIGIN_TRANSPORT_COST
    );
    let signed = inspect_signed(&state, &context, "inspect-one", 40_004);
    assert_eq!(signed.verification.proof_state, ProofStateV1::Expired);
    let checks = state
        .origin_inspection_checks("inspect-one", &signed, &current, 40_005)
        .unwrap();
    state
        .complete_origin_inspection(
            "inspect-one",
            signed.clone(),
            &current,
            40_005,
            &mut ExactCache::verified(checks),
        )
        .unwrap();
    for field in 0..6 {
        let mut wrong = inspect_signed(&state, &context, "inspect-two", 40_004);
        match field {
            0 => wrong.verification.purpose = VerificationPurposeV1::ConsumeApproval,
            1 => wrong.verification.origin_state = OriginStateV1::Revoked,
            2 => wrong.verification.verify_nonce = "wrong".into(),
            3 => wrong.verification.manager_security_version = DecimalU64(99),
            4 => wrong.verification.proof_digest = "00".repeat(32),
            _ => wrong.verification.observed_at = DecimalU64(99_999),
        }
        wrong = sign_verification(wrong.verification);
        assert!(
            state
                .origin_inspection_checks("inspect-two", &wrong, &current, 40_005)
                .is_err()
        );
    }
    assert!(
        state
            .claim_role_key_dispatch(
                "stale",
                Role::Owner,
                "inspect-one",
                &current,
                45_005,
                &mut StrictCrypto
            )
            .is_err()
    );
    let saved = serde_json::to_value(&state).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(saved.clone()).unwrap(),
        state
    );
    let mut corrupt = saved.clone();
    corrupt["origin_inspections"]["inspect-one"]["claimed_at"] = Value::Null;
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(corrupt).is_err());
    let mut corrupt = saved;
    corrupt["provision"]["transport"]["spent"] = json!("0");
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(corrupt).is_err(),
        "restoring cannot erase pre-reserved transport cost"
    );
}
#[test]
fn role_permit_binds_canonical_request_static_role_three_reads_and_fixed_dispatch_budget() {
    let (mut state, context) = attempt_approved();
    let policy = Policy::new(state.draft());
    let current = policy.current(&context);
    state
        .begin_origin_inspection("inspect-role", &"ef".repeat(32), &current, 40_000)
        .unwrap();
    state
        .claim_origin_inspection("inspect-role", &current, 40_001)
        .unwrap();
    let signed = inspect_signed(&state, &context, "inspect-role", 40_002);
    state
        .complete_origin_inspection("inspect-role", signed, &current, 40_003, &mut StrictCrypto)
        .unwrap();
    let mut corrupted = serde_json::to_value(&state).unwrap();
    corrupted["origin_inspections"]["inspect-role"]["verification"]["signature"] =
        json!("A".repeat(86));
    let mut restored: AccountProvisionApprovalState = serde_json::from_value(corrupted).unwrap();
    let before = restored.clone();
    assert!(
        restored
            .claim_role_key_dispatch(
                "tampered-dispatch",
                Role::Owner,
                "inspect-role",
                &current,
                40_004,
                &mut StrictCrypto
            )
            .is_err(),
        "stored evidence must pass cryptography again before producing a permit"
    );
    assert_eq!(restored, before);
    let mut unknown_pin = context.clone();
    unknown_pin.native_key_pins.clear();
    assert!(
        state
            .role_key_dispatch_checks(
                Role::Owner,
                "inspect-role",
                &policy.current(&unknown_pin),
                40_004
            )
            .is_err()
    );
    let permit = state
        .claim_role_key_dispatch(
            "dispatch-owner",
            Role::Owner,
            "inspect-role",
            &current,
            40_004,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(
        state.provision().transport_remaining().unwrap(),
        128 - PROVISION_ORIGIN_TRANSPORT_COST - PROVISION_ROLE_TRANSPORT_COST
    );
    let request = ProvisionRoleKeyRequestV1 {
        api_version: 1,
        custody_account_id: permit.scope.context.identity.account_id.clone(),
        provision_id: permit.scope.provision_id.clone(),
        task_operation_id: permit.scope.operation_id.clone(),
        permit_id: permit.permit_id.clone(),
    };
    permit.validate_for(&request, Role::Owner).unwrap();
    permit.validate_at(40_004).unwrap();
    assert_eq!(permit.request_digest, request.digest().unwrap());
    let dispatch = &state.role_key_dispatches()["dispatch-owner"];
    assert_eq!(
        serde_json::from_str::<ProvisionRoleKeyRequestV1>(&dispatch.request_bytes).unwrap(),
        request
    );
    assert_eq!(dispatch.request_digest, request.digest().unwrap());
    let mut corrupted = serde_json::to_value(&state).unwrap();
    corrupted["role_key_dispatches"]["dispatch-owner"]["request_bytes"] = json!("{}");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(corrupted).is_err());
    let mut corrupted = serde_json::to_value(&state).unwrap();
    corrupted["role_key_dispatches"]["dispatch-owner"]["permit_reads"] = json!(4);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(corrupted).is_err());

    assert!(permit.validate_for(&request, Role::Executor).is_err());
    let mut wrong = request.clone();
    wrong.task_operation_id = "other-task".into();
    assert!(permit.validate_for(&wrong, Role::Owner).is_err());
    assert!(
        state
            .claim_role_key_dispatch(
                "dispatch-owner",
                Role::Owner,
                "inspect-role",
                &current,
                40_004,
                &mut StrictCrypto,
            )
            .is_err()
    );
    assert!(
        state
            .claim_role_key_permit_read(
                &permit.permit_id,
                Role::Executor,
                &current,
                40_004,
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(
        state
            .reserve_role_key_permit_read(&permit.permit_id, Role::Owner, 40_005)
            .unwrap(),
        1
    );
    for _ in 1..MAX_PROVISION_ROLE_PERMIT_READS {
        let observed = state
            .claim_role_key_permit_read(
                &permit.permit_id,
                Role::Owner,
                &current,
                40_005,
                &mut StrictCrypto,
            )
            .unwrap();
        assert_eq!(observed.authority_revision, permit.authority_revision);
        assert_eq!(observed.expires_at, permit.expires_at);
    }
    let before = state.clone();
    assert!(
        state
            .claim_role_key_permit_read(
                &permit.permit_id,
                Role::Owner,
                &current,
                40_006,
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(state, before);
    let result = ProvisionRoleKeyResultV1 {
        scope: permit.scope,
        public_ref: PublicRoleKey {
            key_id: request.custody_account_id,
            key_version: DecimalU64(1),
            public_key: "13".repeat(32),
        },
    };
    assert!(
        state
            .record_dispatched_role_key_result("dispatch-owner", result, &current, 40_006)
            .is_err()
    );
    let result = role_result(&state, "owner", "13");
    let original = state
        .record_dispatched_role_key_result("dispatch-owner", result.clone(), &current, 40_006)
        .unwrap();
    state.freeze(1).unwrap();
    let frozen = state.clone();
    assert_eq!(
        state
            .record_dispatched_role_key_result("dispatch-owner", result, &current, 90_000)
            .unwrap(),
        original
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
fn provision_transport_limit_matches_fixed_call_graph_budget_and_preserves_old_approval_bounds() {
    for (limit, valid) in [(0, false), (1, true), (24, true), (400, true), (401, false)] {
        let (mut draft, mut input, _) = fixture();
        draft.plan.transport_limit = DecimalU64(limit);
        if let CustodyIntentV1::Provision {
            transport_limit, ..
        } = &mut input.request.intent
        {
            *transport_limit = DecimalU64(limit);
        }
        let policy = Policy::new(&draft);
        assert_eq!(
            input.request.validate(1000).is_ok(),
            valid,
            "native approval bound {limit}"
        );
        assert_eq!(
            draft.plan.validate(1000, &policy.routes).is_ok(),
            valid,
            "draft plan bound {limit}"
        );
    }
}

#[test]
fn local_preflight_reservation_spends_before_io_and_never_reissues_an_allowance() {
    let (mut state, context) = attempt_approved();
    let policy = Policy::new(state.draft());
    let reserved = state
        .reserve_origin_inspection("local-preflight", &"12".repeat(32), 40_000)
        .unwrap();
    assert_eq!(
        state.provision().transport_remaining().unwrap(),
        128 - PROVISION_ORIGIN_TRANSPORT_COST
    );
    assert!(
        reserved.claimed_at.is_none()
            && reserved.request_bytes.is_none()
            && reserved.verification.is_none()
    );
    assert!(
        state
            .origin_inspection_request_bytes("local-preflight")
            .is_err(),
        "budget reservation cannot issue native bytes"
    );
    let before = state.clone();
    assert!(
        state
            .reserve_origin_inspection("local-preflight", &"34".repeat(32), 40_001)
            .is_err(),
        "a retry cannot mint unlimited preflight reads from one reservation"
    );
    assert_eq!(state, before);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&state).unwrap()
        )
        .unwrap(),
        state,
        "unclaimed reservations retain their full cost after restart"
    );
    state.freeze(1).unwrap();
    assert!(
        state
            .claim_origin_inspection("local-preflight", &policy.current(&context), 40_002)
            .is_err()
    );
    assert!(
        state
            .reserve_origin_inspection("another-after-freeze", &"56".repeat(32), 40_002)
            .is_err()
    );
    assert_eq!(
        state.provision().transport_remaining().unwrap(),
        128 - PROVISION_ORIGIN_TRANSPORT_COST,
        "freeze cannot refund an interrupted reservation"
    );
}

#[test]
fn pending_nonce_window_does_not_extend_native_observation_or_role_permit_freshness() {
    let (mut state, context) = attempt_approved();
    let policy = Policy::new(state.draft());
    let current = policy.current(&context);
    let pending = state
        .reserve_origin_inspection("slow-native", &"78".repeat(32), 40_000)
        .unwrap();
    assert_eq!(pending.expires_at.0, 40_000 + PROVISION_ORIGIN_PENDING_MS);
    assert!(pending.expires_at.0 > 40_000 + PROVISION_ORIGIN_FRESHNESS_MS);
    state
        .claim_origin_inspection("slow-native", &current, 40_001)
        .unwrap();
    let old = inspect_signed(&state, &context, "slow-native", 40_002);
    assert!(
        state
            .complete_origin_inspection("slow-native", old, &current, 50_001, &mut StrictCrypto)
            .is_err(),
        "pending nonce does not bless a stale native observation"
    );
    let fresh = inspect_signed(&state, &context, "slow-native", 50_000);
    state
        .complete_origin_inspection("slow-native", fresh, &current, 50_001, &mut StrictCrypto)
        .unwrap();
    let permit = state
        .claim_role_key_dispatch(
            "slow-native-owner",
            Role::Owner,
            "slow-native",
            &current,
            50_002,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(permit.expires_at.0, 50_000 + PROVISION_ORIGIN_FRESHNESS_MS);
    for now in [50_100, 50_200, 50_300] {
        let view = state
            .claim_role_key_permit_read(
                &permit.permit_id,
                Role::Owner,
                &current,
                now,
                &mut StrictCrypto,
            )
            .unwrap();
        assert_eq!(view.observed_at.0, now);
        assert_eq!(
            view.expires_at, permit.expires_at,
            "fresh oracle response cannot renew native evidence"
        );
    }
    assert!(
        state
            .validate_role_key_permit(&permit.permit_id, Role::Owner, &current, 55_000)
            .is_err()
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&state).unwrap()
        )
        .unwrap(),
        state
    );
}
