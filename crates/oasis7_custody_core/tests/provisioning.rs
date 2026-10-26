use oasis7_custody_core::{DecimalU64, http::Role, provisioning::*};
fn manager(id: &str) -> ManagerPrincipal {
    ManagerPrincipal {
        identity_namespace: "identity.staging".into(),
        native_account_id: id.into(),
    }
}
fn plan() -> ProvisionPlan {
    ProvisionPlan {
        environment: "staging".into(),
        identity_route: "native-identity".into(),
        world_route: "prepared-world".into(),
        world_identity: "world/genesis/branch".into(),
        purpose: "bounded industrial self-test".into(),
        expires_at: DecimalU64(1000),
        transport_limit: DecimalU64(2),
    }
}
fn routes() -> ProvisionRoutes {
    ProvisionRoutes {
        environment: "staging".into(),
        identity_route: "native-identity".into(),
        world_route: "prepared-world".into(),
        world_identity: "world/genesis/branch".into(),
    }
}
fn allow() -> Vec<BootstrapManager> {
    vec![BootstrapManager {
        principal: manager("manager"),
        environments: vec!["staging".into()],
    }]
}
fn reserve(
    index: &mut ManagerAccountIndex,
    id: &str,
    plan: &ProvisionPlan,
) -> Result<DirectoryEntry, ProvisionError> {
    index.reserve(
        &manager("manager"),
        id,
        plan,
        &ProvisionPolicy {
            routes: &routes(),
            managers: &allow(),
            version: 1,
        },
        100,
    )
}
fn record() -> ProvisionRecord {
    let mut index = ManagerAccountIndex::new(manager("manager")).unwrap();
    ProvisionRecord::draft(reserve(&mut index, "request", &plan()).unwrap(), plan()).unwrap()
}
fn approve(record: &mut ProvisionRecord) {
    record
        .approve(&plan().digest().unwrap(), "proof-nonce", 0, 100)
        .unwrap();
}
fn key(role: Role) -> PublicRoleKey {
    let name = format!("{role:?}");
    PublicRoleKey {
        key_id: format!("key-{name}"),
        key_version: DecimalU64(1),
        public_key: format!("public-{name}"),
    }
}
fn ready_keys(record: &mut ProvisionRecord) {
    approve(record);
    for role in [Role::Identity, Role::Owner, Role::Executor] {
        record.record_key(role, key(role), 0, 100).unwrap();
    }
}
#[test]
fn stable_provision_identity_is_scoped_and_body_bound() {
    let mut index = ManagerAccountIndex::new(manager("manager")).unwrap();
    let original = reserve(&mut index, "request", &plan()).unwrap();
    assert_eq!(reserve(&mut index, "request", &plan()).unwrap(), original);
    assert_eq!(index.entries().count(), 1);
    assert_ne!(
        reserve(&mut index, "different", &plan())
            .unwrap()
            .custody_account_id,
        original.custody_account_id
    );
    let mut changed = plan();
    changed.purpose = "other purpose".into();
    assert_eq!(
        reserve(&mut index, "request", &changed),
        Err(ProvisionError::Conflict)
    );
    let mut other = ManagerAccountIndex::new(manager("other")).unwrap();
    let mut list = allow();
    list.push(BootstrapManager {
        principal: manager("other"),
        environments: vec!["staging".into()],
    });
    let second = other
        .reserve(
            &manager("other"),
            "request",
            &plan(),
            &ProvisionPolicy {
                routes: &routes(),
                managers: &list,
                version: 1,
            },
            100,
        )
        .unwrap();
    assert_ne!(second.custody_account_id, original.custody_account_id);
}
#[test]
fn allowlist_and_fixed_routes_cannot_be_replaced_by_caller() {
    let mut index = ManagerAccountIndex::new(manager("manager")).unwrap();
    assert_eq!(
        index.reserve(
            &manager("other"),
            "request",
            &plan(),
            &ProvisionPolicy {
                routes: &routes(),
                managers: &allow(),
                version: 1
            },
            100
        ),
        Err(ProvisionError::Forbidden)
    );
    assert_eq!(
        index.reserve(
            &manager("manager"),
            "request",
            &plan(),
            &ProvisionPolicy {
                routes: &routes(),
                managers: &[],
                version: 1
            },
            100
        ),
        Err(ProvisionError::Forbidden)
    );
    let mut changed = plan();
    changed.identity_route = "https://attacker.example".into();
    assert_eq!(
        reserve(&mut index, "request", &changed),
        Err(ProvisionError::Forbidden)
    );
    assert_eq!(index.entries().count(), 0);
}
#[test]
fn directory_freeze_and_account_tombstone_win_over_late_initialization() {
    let mut index = ManagerAccountIndex::new(manager("manager")).unwrap();
    let original = reserve(&mut index, "request", &plan()).unwrap();
    index.freeze();
    assert_eq!(
        reserve(&mut index, "request", &plan()),
        Err(ProvisionError::Frozen)
    );
    assert_eq!(
        reserve(&mut index, "new", &plan()),
        Err(ProvisionError::Frozen)
    );
    let mut account = ProvisionRecord::draft(original, plan()).unwrap();
    account.freeze(1).unwrap();
    assert_eq!(
        account.approve(&plan().digest().unwrap(), "old-proof", 1, 100),
        Err(ProvisionError::Frozen)
    );
    assert_eq!(
        account.record_key(Role::Identity, key(Role::Identity), 1, 100),
        Err(ProvisionError::Frozen)
    );
}
#[test]
fn partial_keys_are_immutable_and_cannot_alias_another_role() {
    let mut account = record();
    approve(&mut account);
    account
        .record_key(Role::Identity, key(Role::Identity), 0, 100)
        .unwrap();
    account
        .record_key(Role::Identity, key(Role::Identity), 0, 100)
        .unwrap();
    assert_eq!(
        account.record_key(Role::Identity, key(Role::Owner), 0, 100),
        Err(ProvisionError::Conflict)
    );
    assert_eq!(
        account.record_key(Role::Owner, key(Role::Identity), 0, 100),
        Err(ProvisionError::Conflict)
    );
    account
        .record_key(Role::Owner, key(Role::Owner), 0, 100)
        .unwrap();
    account
        .record_key(Role::Executor, key(Role::Executor), 0, 100)
        .unwrap();
    assert_eq!(account.stage(), ProvisionStage::RoleKeysReady);
    assert_eq!(account.role_key(Role::Identity), Some(&key(Role::Identity)));
}
#[test]
fn registration_unknown_resumes_same_identity_and_never_implies_session_or_prepared() {
    let mut account = record();
    ready_keys(&mut account);
    let original = PhaseOriginal {
        operation_id: "registration-original".into(),
        body_digest: "digest".into(),
        receipt_digest: None,
    };
    account
        .save_original(ProvisionStage::NativeRegistered, original.clone(), 0, 100)
        .unwrap();
    account.charge_transport(0, 100).unwrap();
    let bytes = serde_json::to_vec(&account).unwrap();
    let mut recovered: ProvisionRecord = serde_json::from_slice(&bytes).unwrap();
    recovered
        .save_original(ProvisionStage::NativeRegistered, original.clone(), 0, 100)
        .unwrap();
    assert_eq!(
        recovered.save_original(
            ProvisionStage::NativeRegistered,
            PhaseOriginal {
                operation_id: "replacement".into(),
                ..original
            },
            0,
            100
        ),
        Err(ProvisionError::Conflict)
    );
    assert_eq!(recovered.native_account_id(), None);
    assert_eq!(recovered.stage(), ProvisionStage::RoleKeysReady);
    recovered
        .confirm_phase(
            ProvisionStage::NativeRegistered,
            "registration-original",
            "receipt",
            Some("native-account"),
            0,
            100,
        )
        .unwrap();
    recovered
        .confirm_phase(
            ProvisionStage::NativeRegistered,
            "registration-original",
            "receipt",
            Some("native-account"),
            0,
            100,
        )
        .unwrap();
    assert_eq!(recovered.native_account_id(), Some("native-account"));
    assert_eq!(recovered.stage(), ProvisionStage::NativeRegistered);
    assert_eq!(
        recovered.confirm_phase(
            ProvisionStage::NativeRegistered,
            "registration-original",
            "receipt",
            Some("different-account"),
            0,
            100
        ),
        Err(ProvisionError::Conflict)
    );
    assert_eq!(
        recovered.save_original(
            ProvisionStage::Prepared,
            PhaseOriginal {
                operation_id: "setup".into(),
                body_digest: "setup-digest".into(),
                receipt_digest: None
            },
            0,
            100
        ),
        Err(ProvisionError::InvalidTransition)
    );
}
#[test]
fn transport_budget_and_expiry_do_not_reset_on_restart_or_retry() {
    let mut account = record();
    approve(&mut account);
    account.charge_transport(0, 100).unwrap();
    let mut account: ProvisionRecord =
        serde_json::from_slice(&serde_json::to_vec(&account).unwrap()).unwrap();
    account.charge_transport(0, 100).unwrap();
    assert_eq!(
        account.charge_transport(0, 100),
        Err(ProvisionError::Exhausted)
    );
    assert_eq!(account.check_current(0, 1000), Err(ProvisionError::Expired));
    assert_eq!(account.check_current(1, 999), Err(ProvisionError::Conflict));
}

#[test]
fn expired_same_body_draft_retry_returns_original_without_new_authority() {
    let mut index = ManagerAccountIndex::new(manager("manager")).unwrap();
    let original = reserve(&mut index, "request", &plan()).unwrap();
    assert_eq!(
        index.reserve(
            &manager("manager"),
            "request",
            &plan(),
            &ProvisionPolicy {
                routes: &routes(),
                managers: &allow(),
                version: 1
            },
            1001
        ),
        Ok(original)
    );
    assert_eq!(
        index.reserve(
            &manager("manager"),
            "new-request",
            &plan(),
            &ProvisionPolicy {
                routes: &routes(),
                managers: &allow(),
                version: 1
            },
            1001
        ),
        Err(ProvisionError::Expired)
    );
    assert_eq!(index.entries().count(), 1);
}

fn revocation_notice() -> ManagerRevocationNotice {
    ManagerRevocationNotice {
        operation_id: "native-revocation-one".into(),
        manager: manager("manager"),
        environment: "staging".into(),
        identity_instance_epoch: DecimalU64(3),
        source_security_version: DecimalU64(9_007_199_254_740_993),
        source_credential_ref: Some("credential-one".into()),
        source_credential_version: Some(DecimalU64(u64::MAX)),
    }
}
fn freeze_receipt(target: AccountFreezeTarget, operations: &[&str]) -> AccountFreezeReceipt {
    AccountFreezeReceipt {
        frozen_security_epoch: target.minimum_security_epoch,
        target,
        authorization_closure: AccountAuthorizationClosure::AllManagerRootsAndChildrenRevoked {
            inventory_digest: "a".repeat(64),
        },
        dispatch_barrier: AccountDispatchBarrier::AllUnclaimedBlocked {
            barrier_digest: "b".repeat(64),
        },
        receipt_digest: "c".repeat(64),
        claimed_operation_ids: operations.iter().map(|id| (*id).into()).collect(),
    }
}
#[test]
fn complete_directory_freezes_pending_and_initialized_before_late_initialization() {
    let mut index =
        ManagerAccountIndex::new_for_environment(manager("manager"), "staging".into()).unwrap();
    let pending = reserve(&mut index, "pending", &plan()).unwrap();
    let ready = reserve(&mut index, "ready", &plan()).unwrap();
    index
        .mark_initialized(&ready.custody_account_id, 9_007_199_254_740_993)
        .unwrap();
    let notice = revocation_notice();
    index.begin_revocation(notice.clone()).unwrap();
    assert_eq!(index.begin_revocation(notice), Ok(()));
    assert_eq!(index.closure_status(), ManagerClosureStatus::Pending);
    assert_eq!(
        reserve(&mut index, "new", &plan()),
        Err(ProvisionError::Frozen)
    );
    assert_eq!(
        index.mark_initialized(&pending.custody_account_id, 0),
        Err(ProvisionError::Frozen)
    );
    let targets: Vec<_> = index.freeze_targets().cloned().collect();
    assert_eq!(targets.len(), 2);
    assert_eq!(
        targets
            .iter()
            .find(|t| t.custody_account_id == pending.custody_account_id)
            .unwrap()
            .minimum_security_epoch
            .0,
        1
    );
    assert_eq!(
        targets
            .iter()
            .find(|t| t.custody_account_id == ready.custody_account_id)
            .unwrap()
            .minimum_security_epoch
            .0,
        9_007_199_254_740_994
    );
    for target in targets {
        index
            .record_freeze_receipt(freeze_receipt(target, &[]))
            .unwrap();
    }
    assert_eq!(index.closure_status(), ManagerClosureStatus::Closed);
    let restored: ManagerAccountIndex =
        serde_json::from_slice(&serde_json::to_vec(&index).unwrap()).unwrap();
    assert_eq!(restored, index);
    assert_eq!(restored.closure_status(), ManagerClosureStatus::Closed);
}
#[test]
fn fanout_confirmation_does_not_hide_claimed_unknown_operations() {
    let mut index = ManagerAccountIndex::new(manager("manager")).unwrap();
    reserve(&mut index, "one", &plan()).unwrap();
    index.begin_revocation(revocation_notice()).unwrap();
    let target = index.freeze_targets().next().unwrap().clone();
    let receipt = freeze_receipt(target.clone(), &["claimed-one", "claimed-two"]);
    index.record_freeze_receipt(receipt.clone()).unwrap();
    index.record_freeze_receipt(receipt).unwrap();
    assert_eq!(index.closure_status(), ManagerClosureStatus::Pending);
    let applied = InFlightResult {
        outcome: ConfirmedOriginalOutcome::Applied,
        receipt_digest: "d".repeat(64),
    };
    index
        .record_inflight_result(
            &target.revocation_digest,
            &target.custody_account_id,
            "claimed-one",
            applied.clone(),
        )
        .unwrap();
    index
        .record_inflight_result(
            &target.revocation_digest,
            &target.custody_account_id,
            "claimed-one",
            applied,
        )
        .unwrap();
    assert_eq!(index.closure_status(), ManagerClosureStatus::Pending);
    let mut restored: ManagerAccountIndex =
        serde_json::from_slice(&serde_json::to_vec(&index).unwrap()).unwrap();
    restored
        .record_inflight_result(
            &target.revocation_digest,
            &target.custody_account_id,
            "claimed-two",
            InFlightResult {
                outcome: ConfirmedOriginalOutcome::NotApplied,
                receipt_digest: "e".repeat(64),
            },
        )
        .unwrap();
    assert_eq!(restored.closure_status(), ManagerClosureStatus::Closed);
}
#[test]
fn closure_requires_exact_source_target_epoch_and_immutable_proofs() {
    let mut index = ManagerAccountIndex::new(manager("manager")).unwrap();
    reserve(&mut index, "one", &plan()).unwrap();
    let before = index.clone();
    let mut wrong = revocation_notice();
    wrong.environment = "other".into();
    assert_eq!(
        index.begin_revocation(wrong),
        Err(ProvisionError::Forbidden)
    );
    assert_eq!(index, before);
    index.begin_revocation(revocation_notice()).unwrap();
    let target = index.freeze_targets().next().unwrap().clone();
    let before = index.clone();
    let mut invalid = freeze_receipt(target.clone(), &[]);
    invalid.frozen_security_epoch = DecimalU64(0);
    assert_eq!(
        index.record_freeze_receipt(invalid),
        Err(ProvisionError::Invalid)
    );
    let mut wrong = freeze_receipt(target.clone(), &[]);
    wrong.target.environment = "other".into();
    assert_eq!(
        index.record_freeze_receipt(wrong),
        Err(ProvisionError::Conflict)
    );
    assert_eq!(index, before);
    let receipt = freeze_receipt(target.clone(), &["claimed"]);
    index.record_freeze_receipt(receipt.clone()).unwrap();
    let before = index.clone();
    let mut changed = receipt;
    changed.claimed_operation_ids.clear();
    assert_eq!(
        index.record_freeze_receipt(changed),
        Err(ProvisionError::Conflict)
    );
    assert_eq!(
        index.record_inflight_result(
            &target.revocation_digest,
            &target.custody_account_id,
            "unregistered",
            InFlightResult {
                outcome: ConfirmedOriginalOutcome::Applied,
                receipt_digest: "d".repeat(64)
            }
        ),
        Err(ProvisionError::Forbidden)
    );
    assert_eq!(index, before);
}
#[test]
fn restored_directory_rejects_missing_associations_targets_and_false_closure() {
    let mut index = ManagerAccountIndex::new(manager("manager")).unwrap();
    reserve(&mut index, "one", &plan()).unwrap();
    index.begin_revocation(revocation_notice()).unwrap();
    let original = serde_json::to_value(&index).unwrap();
    for field in ["entries", "accounts"] {
        let mut wrong = original.clone();
        wrong[field] = serde_json::json!({});
        assert!(serde_json::from_value::<ManagerAccountIndex>(wrong).is_err());
    }
    let mut wrong = original.clone();
    wrong["revocation"]["accounts"] = serde_json::json!({});
    assert!(serde_json::from_value::<ManagerAccountIndex>(wrong).is_err());
    let mut wrong = original.clone();
    wrong["frozen"] = false.into();
    assert!(serde_json::from_value::<ManagerAccountIndex>(wrong).is_err());
    let mut wrong = original;
    wrong["revocation"]["notice"]["source_security_version"] =
        serde_json::json!(9007199254740993u64);
    assert!(serde_json::from_value::<ManagerAccountIndex>(wrong).is_err());
    let old = serde_json::json!({"manager": manager("manager"), "frozen":true,"entries":{}});
    assert!(serde_json::from_value::<ManagerAccountIndex>(old).is_err());
    let mut empty =
        ManagerAccountIndex::new_for_environment(manager("manager"), "staging".into()).unwrap();
    empty.freeze();
    assert_eq!(empty.closure_status(), ManagerClosureStatus::Pending);
}
#[test]
fn directory_capacity_and_environment_pin_preserve_stable_originals() {
    let mut index =
        ManagerAccountIndex::new_for_environment(manager("manager"), "other".into()).unwrap();
    assert_eq!(
        reserve(&mut index, "one", &plan()),
        Err(ProvisionError::Forbidden)
    );
    let mut index = ManagerAccountIndex::new(manager("manager")).unwrap();
    for n in 0..MAX_MANAGER_ASSOCIATIONS {
        reserve(&mut index, &format!("request-{n}"), &plan()).unwrap();
    }
    assert_eq!(
        reserve(&mut index, "overflow", &plan()),
        Err(ProvisionError::Exhausted)
    );
    assert!(reserve(&mut index, "request-0", &plan()).is_ok());
    let account = index.entries().next().unwrap().custody_account_id.clone();
    let before = index.clone();
    assert_eq!(
        index.mark_initialized(&account, u64::MAX),
        Err(ProvisionError::Overflow)
    );
    assert_eq!(index, before);
    let restored: ManagerAccountIndex =
        serde_json::from_slice(&serde_json::to_vec(&index).unwrap()).unwrap();
    assert_eq!(restored, index);
}
