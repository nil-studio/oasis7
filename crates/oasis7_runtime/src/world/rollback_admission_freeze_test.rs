use super::*;

fn rolled_back() -> (World, ModuleManifest) {
    let (mut world, removed) = fixture(2);
    world.state.product_profiles.insert(
        "retained-product".into(),
        ProductProfileV1 {
            product_id: "retained-product".into(),
            role_tag: "scale".into(),
            maintenance_sink: vec![],
            tradable: true,
            unlock_stage: "scale_out".into(),
        },
    );
    let baseline = world.snapshot();
    let profiles = world.state.product_profiles.clone();
    let materials = world.state.materials.clone();
    let historical_events = world.journal().events.clone();
    dispatch(
        &mut world,
        Action::RollbackModuleInstance {
            operator_agent_id: "payer".into(),
            instance_id: "m.governed-atomic#1".into(),
            target_module_version: "1.0.0".into(),
        },
    )
    .unwrap();
    assert_eq!(
        world.state.module_instances["m.governed-atomic#1"].module_version,
        "1.0.0"
    );
    assert_eq!(world.state.product_profiles, profiles);
    assert_eq!(world.state.materials, materials);
    assert_eq!(
        &world.journal().events[..historical_events.len()],
        historical_events.as_slice()
    );
    let replayed = World::from_snapshot(baseline, world.journal().clone()).unwrap();
    assert_eq!(replayed.state, world.state);
    assert_eq!(
        replayed.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );
    (world, removed)
}

#[test]
fn rollback_admission_freeze_receipt_identifies_removed_artifact() {
    assert!(
        serde_json::to_value(&World::new().state)
            .unwrap()
            .get("module_admission_freezes")
            .is_none(),
        "empty new state field must preserve legacy bytes"
    );
    let (world, removed) = rolled_back();
    let event = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(event @ DomainEvent::ModuleRollbackApplied { .. }) => {
                Some(event)
            }
            _ => None,
        })
        .unwrap();
    let value = serde_json::to_value(event).unwrap();
    let receipt = &value["data"];
    assert_eq!(
        receipt["from_wasm_hash"], removed.wasm_hash,
        "rollback receipt must fence FROM artifact separately from restored TO hash"
    );
    assert_ne!(
        receipt["wasm_hash"], receipt["from_wasm_hash"],
        "restored artifact must not be frozen"
    );
    assert!(
        serde_json::to_value(&world.state).unwrap()["module_admission_freezes"]
            .as_object()
            .is_some_and(|markers| !markers.is_empty())
    );
}

#[test]
fn rollback_admission_freeze_blocks_removed_install_and_upgrade() {
    for install_again in [true, false] {
        let (mut world, removed) = rolled_back();
        let before_instances = world.state.module_instances.clone();
        let before_balance = world
            .agent_resource_balance("payer", ResourceKind::Electricity)
            .unwrap();
        let action = if install_again {
            install(removed)
        } else {
            upgrade(removed)
        };
        dispatch(&mut world, action).unwrap();
        assert_eq!(
            world.state.module_instances, before_instances,
            "rolled-back artifact must not regain admission via direct install/upgrade"
        );
        assert_eq!(
            world
                .agent_resource_balance("payer", ResourceKind::Electricity)
                .unwrap(),
            before_balance
        );
    }
}

#[test]
fn rollback_admission_freeze_blocks_generic_activation_and_preserves_good_target() {
    let (mut world, removed) = rolled_back();
    assert!(
        world
            .validate_module_changes(&crate::ModuleChangeSet {
                register: vec![removed.clone()],
                ..Default::default()
            })
            .is_err()
    );
    let error = world
        .validate_module_changes(&crate::ModuleChangeSet {
            activate: vec![crate::ModuleActivation {
                module_id: removed.module_id.clone(),
                version: removed.version.clone(),
            }],
            ..Default::default()
        })
        .unwrap_err();
    assert!(format!("{error:?}").contains("admission frozen"));
    let good = manifest(&mut world, "1.0.0");
    assert!(world.ensure_module_admission_allowed(&good).is_ok());
    assert!(world.ensure_module_admission_allowed(&removed).is_err());
    let error = world
        .validate_module_changes(&crate::ModuleChangeSet {
            upgrade: vec![crate::ModuleUpgrade {
                module_id: removed.module_id.clone(),
                from_version: "1.0.0".into(),
                to_version: removed.version.clone(),
                wasm_hash: removed.wasm_hash.clone(),
                manifest: removed.clone(),
            }],
            ..Default::default()
        })
        .unwrap_err();
    assert!(format!("{error:?}").contains("admission frozen"));
    let before = world.state.module_instances.len();
    dispatch(&mut world, install(good)).unwrap();
    assert_eq!(world.state.module_instances.len(), before + 1);
}

#[test]
fn rollback_admission_freeze_supports_legacy_snapshot_missing_instance_hash() {
    let (mut world, removed) = fixture(2);
    world
        .mutate_state_and_refresh_tick_consensus_for_test(|state| {
            state
                .module_instances
                .get_mut("m.governed-atomic#1")
                .unwrap()
                .wasm_hash
                .clear();
        })
        .unwrap();
    let mut snapshot = serde_json::to_value(world.snapshot()).unwrap();
    snapshot["state"]["module_instances"]["m.governed-atomic#1"]
        .as_object_mut()
        .unwrap()
        .remove("wasm_hash");
    let baseline = serde_json::from_value(snapshot).unwrap();
    let mut restored = World::from_snapshot(baseline, world.journal().clone()).unwrap();
    assert!(
        restored.state.module_instances["m.governed-atomic#1"]
            .wasm_hash
            .is_empty()
    );
    let before = restored.snapshot();
    dispatch(
        &mut restored,
        Action::RollbackModuleInstance {
            operator_agent_id: "payer".into(),
            instance_id: "m.governed-atomic#1".into(),
            target_module_version: "1.0.0".into(),
        },
    )
    .unwrap();
    assert!(restored.ensure_module_admission_allowed(&removed).is_err());
    let replayed = World::from_snapshot(before, restored.journal().clone()).unwrap();
    assert_eq!(replayed.state, restored.state);
    assert_eq!(
        replayed.current_state_root_hash().unwrap(),
        restored.current_state_root_hash().unwrap()
    );
}

#[test]
fn rollback_admission_freeze_rejects_frozen_rollback_target_without_partial_effects() {
    let (mut world, _) = rolled_back();
    let third = manifest(&mut world, "3.0.0");
    dispatch(&mut world, upgrade(third)).unwrap();
    assert_eq!(
        world.state.module_instances["m.governed-atomic#1"].module_version,
        "3.0.0"
    );
    let state = world.state.clone();
    let registry = world.module_registry.clone();
    dispatch(
        &mut world,
        Action::RollbackModuleInstance {
            operator_agent_id: "payer".into(),
            instance_id: "m.governed-atomic#1".into(),
            target_module_version: "2.0.0".into(),
        },
    )
    .unwrap();
    assert_eq!(world.state, state);
    assert_eq!(world.module_registry, registry);
    assert!(matches!(
        &world.journal().events.last().unwrap().body,
        WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
    ));
}
