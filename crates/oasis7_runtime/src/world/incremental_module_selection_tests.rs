use super::*;
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_world_state::incremental_module_selection_fields::{
    ModuleSelectionFieldV1 as SF, ModuleSelectionKeyV1 as SK, ModuleSelectionScalarRefV1 as SS,
};
fn record(id: &str, version: &str) -> oasis7_wasm_abi::ModuleRecord {
    use oasis7_wasm_abi::*;
    ModuleRecord {
        manifest: ModuleManifest {
            module_id: id.into(),
            name: id.into(),
            version: version.into(),
            kind: ModuleKind::Reducer,
            role: ModuleRole::Domain,
            wasm_hash: "lookup-only fixture".into(),
            interface_version: "wasm-1".into(),
            exports: vec![],
            subscriptions: vec![],
            required_caps: vec![],
            abi_contract: ModuleAbiContract::default(),
            artifact_identity: None,
            limits: ModuleLimits::unbounded(),
        },
        registered_at: 0,
        registered_by: "fixture".into(),
        audit_event_id: None,
    }
}
fn selection_setup(memory: &MemoryBudget) -> (World, ExecutionParent, Pages, Values) {
    let (mut world, mut parent, mut pages, mut values) = setup(memory);
    world
        .state
        .module_instances
        .insert("map-key".into(), instance());
    let mut active = instance();
    active.active = true;
    world
        .state
        .module_instances
        .insert("active-key".into(), active);
    for (id, version) in [
        ("module", "2.0"),
        ("global", "v @2"),
        ("self", ""),
        ("active-key", "shadow"),
        ("map-key", "shadow"),
    ] {
        world
            .module_registry
            .active
            .insert(id.into(), version.into());
        world.module_registry.records.insert(
            oasis7_wasm_abi::ModuleRegistry::record_key(id, version),
            record(id, version),
        );
    }
    world.state.installed_module_targets.insert(
        "global".into(),
        ModuleInstallTarget::LocationInfrastructure {
            location_id: " 空 place ".into(),
        },
    );
    seed_instances(&world, &mut parent, &mut pages, &mut values, memory);
    let root = world
        .write_iws_module_selection_genesis(
            parent.roots.state[5].clone(),
            &mut BodyMutationContext {
                sink: &mut values,
                memory,
                io: &mut genesis_io(),
                cancelled: || false,
            },
            &mut 100,
            4096,
        )
        .unwrap();
    parent.roots.state[5] = (*root).clone();
    for (id, bytes) in values.pages.borrow().iter() {
        if bytes.starts_with(b"oasis7.iws.state-page.v1\0") {
            pages.0.insert(hex::encode(id), bytes.to_vec());
        }
    }
    (world, parent, pages, values)
}
fn selection_limits() -> oasis7_formal_protocol::incremental_execution::TransactionLimits {
    let mut bounds = limits();
    bounds.max_reads = 64;
    bounds.max_writes = 64;
    bounds
}
#[test]
fn lookup_content_matches_legacy_branch_and_target_after_all_selection_caches_clear() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (mut world, parent, pages, values) = selection_setup(&memory);
    let ids = [
        "map-key",
        "active-key",
        "module",
        "global",
        "self",
        "missing",
    ];
    let expected: Vec<_> = ids
        .iter()
        .map(|id| world.active_module_invocation_for_id(id).unwrap())
        .collect();
    world.state.module_instances.clear();
    world.module_registry.active.clear();
    world.module_registry.records.clear();
    world.state.installed_module_targets.clear();
    for (id, expected) in ids.into_iter().zip(expected) {
        let live = parent.fence().unwrap();
        let mut txn = ExecutionTransaction::begin(
            parent.clone(),
            [4; 32],
            &pages,
            &memory,
            selection_limits(),
        )
        .unwrap();
        let mut quota = genesis_io();
        let actual = ModuleReadContext {
            values: &values,
            memory: &memory,
            io: &mut quota,
            max_value_bytes: 4096,
            cancelled: || false,
        }
        .select_module_content(id, &mut txn)
        .unwrap();
        assert_eq!(actual.is_some(), expected.is_some(), "{id}");
        if let (Some(actual), Some(expected)) = (actual, expected) {
            assert!(actual.uses_budget(&memory));
            assert_eq!(actual.invocation_id(), id);
            assert_eq!(actual.instance_id(), expected.instance_id);
            assert_eq!(actual.module_id(), expected.module_id);
            assert_eq!(actual.record_version(), expected.manifest.version);
            let target = match &expected.install_target {
                ModuleInstallTarget::SelfAgent => T::SelfAgent,
                ModuleInstallTarget::LocationInfrastructure { location_id } => {
                    T::LocationInfrastructure(location_id)
                }
            };
            assert_eq!(actual.target(), target);
            assert_eq!(actual.is_instance(), id == "active-key");
        }
        let guard = txn.guard();
        txn.seal(&guard, &live, 0).unwrap();
    }
}
#[test]
fn suppressed_branches_never_read_corrupt_global_fields_and_overlay_changes_are_atomic() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, mut values) = selection_setup(&memory);
    for id in ["module", "map-key", "active-key"] {
        let key = SK::new(id, SF::ActiveVersion).unwrap();
        let value = parent.roots.state[5]
            .get(key.as_bytes(), &pages)
            .unwrap()
            .unwrap();
        values.pages.borrow_mut().remove(&value);
    }
    let source = values.clone();
    for id in ["module", "map-key", "active-key"] {
        let mut txn = ExecutionTransaction::begin(
            parent.clone(),
            [4; 32],
            &pages,
            &memory,
            selection_limits(),
        )
        .unwrap();
        let mut quota = genesis_io();
        let result = ModuleReadContext {
            values: &source,
            memory: &memory,
            io: &mut quota,
            max_value_bytes: 4096,
            cancelled: || false,
        }
        .select_module_content(id, &mut txn)
        .unwrap();
        assert_eq!(result.is_some(), id == "active-key");
    }
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, selection_limits()).unwrap();
    let mut quota = genesis_io();
    let old = ModuleReadContext {
        values: &source,
        memory: &memory,
        io: &mut quota,
        max_value_bytes: 4096,
        cancelled: || false,
    }
    .select_module_content("global", &mut txn)
    .unwrap()
    .unwrap();
    for scalar in [
        SS::ActiveVersion("changed @ literal"),
        SS::Target(T::SelfAgent),
    ] {
        crate::contribute_module_selection_field(
            "global",
            scalar.field(),
            Some(scalar),
            &mut txn,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut quota,
                cancelled: || false,
            },
            4096,
        )
        .unwrap();
    }
    let new = ModuleReadContext {
        values: &source,
        memory: &memory,
        io: &mut quota,
        max_value_bytes: 4096,
        cancelled: || false,
    }
    .select_module_content("global", &mut txn)
    .unwrap()
    .unwrap();
    assert_eq!(old.record_version(), "v @2");
    assert_eq!(old.target(), T::LocationInfrastructure(" 空 place "));
    assert_eq!(new.record_version(), "changed @ literal");
    assert_eq!(new.target(), T::SelfAgent);
    drop(old);
    drop(new);
    for field in [SF::ActiveVersion, SF::Target] {
        crate::contribute_module_selection_field(
            "global",
            field,
            None,
            &mut txn,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut quota,
                cancelled: || false,
            },
            4096,
        )
        .unwrap();
    }
    assert!(
        ModuleReadContext {
            values: &source,
            memory: &memory,
            io: &mut quota,
            max_value_bytes: 4096,
            cancelled: || false
        }
        .select_module_content("global", &mut txn)
        .unwrap()
        .is_none()
    );

    let guard = txn.guard();
    assert_eq!(txn.seal(&guard, &live, 0).unwrap().record.writes.len(), 2);
}
#[test]
fn selected_field_failure_cancel_limit_pool_and_unwind_poison_prior_business() {
    for case in 0..7 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let foreign = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (_, parent, pages, mut values) = selection_setup(&memory);
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, selection_limits())
                .unwrap();
        txn.write(Ns::Business, b"outer", Some([7; 32])).unwrap();
        let selected_key = SK::new("global", SF::ActiveVersion).unwrap();
        if case == 5 {
            let id = txn
                .read(Ns::Artifacts, selected_key.as_bytes())
                .unwrap()
                .unwrap();
            values.pages.borrow_mut().remove(&id);
        } else if case == 6 {
            let wrong = crate::world::incremental_module_selection::write_selection_scalar(
                SS::Target(T::SelfAgent),
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut genesis_io(),
                    cancelled: || false,
                },
                4096,
            )
            .unwrap();
            txn.write(Ns::Artifacts, selected_key.as_bytes(), Some(wrong))
                .unwrap();
        }

        let mut quota = genesis_io();
        if case == 1 {
            quota.nodes_remaining = 0;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ModuleReadContext {
                values: &values,
                memory: if case == 3 { &foreign } else { &memory },
                io: &mut quota,
                max_value_bytes: if case == 0 { 1 } else { 4096 },
                cancelled: || {
                    if case == 4 {
                        panic!("selection cancel unwind");
                    }
                    case == 2
                },
            }
            .select_module_content("global", &mut txn)
        }));
        if case == 4 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err());
        }
        let guard = txn.guard();
        assert!(txn.seal(&guard, &live, 0).is_err());
    }
}

#[test]
fn selection_genesis_bounds_and_warm_writer_failures_never_publish_partial_success() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (world, parent, pages, mut values) = selection_setup(&memory);
    for case in 0..4 {
        let mut quota = genesis_io();
        if case == 3 {
            quota.write_bytes_remaining = 0;
        }
        let mut entries = if case == 0 { 0 } else { 100 };
        let result = world.write_iws_module_selection_genesis(
            oasis7_formal_protocol::incremental_state::StateRootV1::empty(Ns::Artifacts).unwrap(),
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut quota,
                cancelled: || case == 2,
            },
            &mut entries,
            if case == 1 { 1 } else { 4096 },
        );
        assert!(result.is_err(), "case {case}");
    }
    for after in [
        Some(SS::ActiveVersion("oversized version")),
        Some(SS::Target(T::SelfAgent)),
    ] {
        let live = parent.fence().unwrap();
        let mut txn = ExecutionTransaction::begin(
            parent.clone(),
            [4; 32],
            &pages,
            &memory,
            selection_limits(),
        )
        .unwrap();
        txn.write(Ns::Business, b"outer", Some([7; 32])).unwrap();
        assert!(
            crate::contribute_module_selection_field(
                "global",
                SF::ActiveVersion,
                after,
                &mut txn,
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut genesis_io(),
                    cancelled: || false
                },
                1
            )
            .is_err()
        );
        let guard = txn.guard();
        assert!(txn.seal(&guard, &live, 0).is_err());
    }
}
