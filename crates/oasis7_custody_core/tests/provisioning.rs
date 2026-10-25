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
