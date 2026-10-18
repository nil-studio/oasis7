use super::super::{Action, DomainEvent, WorldEventBody};
use super::{World, WorldError};
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_agent_api::world_action::ResourceKind;

fn install(active: bool) -> DomainEvent {
    DomainEvent::ModuleInstalled {
        installer_agent_id: "payer".into(),
        instance_id: "instance-1".into(),
        module_id: "m.instance".into(),
        module_version: "1.0.0".into(),
        wasm_hash: "old-hash".into(),
        install_target: ModuleInstallTarget::SelfAgent,
        active,
        proposal_id: 1,
        manifest_hash: "manifest".into(),
        fee_kind: ResourceKind::Data,
        fee_amount: 3,
    }
}

fn upgrade(active: bool) -> DomainEvent {
    DomainEvent::ModuleUpgraded {
        upgrader_agent_id: "payer".into(),
        instance_id: "instance-1".into(),
        module_id: "m.instance".into(),
        from_module_version: "1.0.0".into(),
        to_module_version: "2.0.0".into(),
        wasm_hash: "new-hash".into(),
        install_target: ModuleInstallTarget::SelfAgent,
        active,
        proposal_id: 2,
        manifest_hash: "manifest-new".into(),
        fee_kind: ResourceKind::Data,
        fee_amount: 3,
    }
}

fn fixture(with_instance: bool) -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "payer".into(),
        pos: crate::tests::pos(0, 0),
    });
    world.step().unwrap();
    world
        .set_agent_resource_balance("payer", ResourceKind::Data, 30)
        .unwrap();
    if with_instance {
        world
            .append_event(WorldEventBody::Domain(install(false)), None)
            .unwrap();
    }
    world.module_tick_schedule.insert("instance-1".into(), 77);
    world
}

fn assert_world_unchanged(world: &World, before: &World) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(world.journal(), before.journal());
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        (before.next_event_id, before.next_event_id_era)
    );
    assert_eq!(
        world.tick_consensus_records(),
        before.tick_consensus_records()
    );
    assert_eq!(world.module_tick_schedule, before.module_tick_schedule);
}

#[test]
fn missing_upgrade_instance_rejects_without_fee_or_legacy_material_migration() {
    let mut world = fixture(false);
    world.state.material_ledgers.clear();
    world.state.materials.insert("iron_ingot".into(), 7);
    let before = world.state.clone();
    let error = world
        .state
        .apply_domain_event(&upgrade(false), 99)
        .unwrap_err();
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("module instance missing for upgrade"))
    );
    assert_eq!(world.state, before);
}

#[test]
fn mismatched_upgrade_instance_rejects_without_fee_or_instance_mutation() {
    for mismatch in ["owner", "module_id", "from_version"] {
        let mut world = fixture(true);
        let instance = world.state.module_instances.get_mut("instance-1").unwrap();
        match mismatch {
            "owner" => instance.owner_agent_id = "someone-else".into(),
            "module_id" => instance.module_id = "m.other".into(),
            _ => instance.module_version = "0.9.0".into(),
        }
        let before = world.state.clone();
        let error = world
            .state
            .apply_domain_event(&upgrade(false), 99)
            .unwrap_err();
        assert!(
            matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains(&format!("{mismatch} mismatch for upgrade")))
        );
        assert_eq!(world.state, before, "{mismatch} rejection must be atomic");
    }
}

#[test]
fn active_install_missing_registry_rejects_without_world_side_effects() {
    let mut world = fixture(false);
    let before = world.clone();
    let error = world
        .append_event(WorldEventBody::Domain(install(true)), None)
        .unwrap_err();
    assert!(
        matches!(error, WorldError::ModuleChangeInvalid { reason } if reason.contains("module record missing"))
    );
    assert_world_unchanged(&world, &before);
}

#[test]
fn active_upgrade_missing_registry_rejects_without_world_side_effects() {
    let mut world = fixture(true);
    let before = world.clone();
    let error = world
        .append_event(WorldEventBody::Domain(upgrade(true)), None)
        .unwrap_err();
    assert!(
        matches!(error, WorldError::ModuleChangeInvalid { reason } if reason.contains("module record missing"))
    );
    assert_world_unchanged(&world, &before);
}

#[test]
fn install_post_prepare_failure_publishes_no_instance_fee_or_event() {
    let mut world = fixture(false);
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event(WorldEventBody::Domain(install(false)), None)
        .expect_err("install must honor publication failpoint");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("injected append_event failure"))
    );
    assert_world_unchanged(&world, &before);
}

#[test]
fn upgrade_post_prepare_failure_publishes_no_instance_fee_or_event() {
    let mut world = fixture(true);
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event(WorldEventBody::Domain(upgrade(false)), None)
        .expect_err("upgrade must honor publication failpoint");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("injected append_event failure"))
    );
    assert_world_unchanged(&world, &before);
}

fn registry_fixture(world: &mut World, version: &str, tick: bool) {
    use oasis7_wasm_abi::*;
    let manifest = ModuleManifest {
        module_id: "m.instance".into(),
        name: "InstanceCompat".into(),
        version: version.into(),
        kind: ModuleKind::Pure,
        role: ModuleRole::AgentInternal,
        wasm_hash: "fixture-hash".into(),
        interface_version: "wasm-1".into(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".into()],
        subscriptions: if tick {
            vec![ModuleSubscription {
                event_kinds: vec![],
                action_kinds: vec![],
                stage: Some(ModuleSubscriptionStage::Tick),
                filters: None,
            }]
        } else {
            vec![]
        },
        required_caps: vec![],
        artifact_identity: None,
        limits: ModuleLimits::default(),
    };
    world.module_registry.records.insert(
        ModuleRegistry::record_key("m.instance", version),
        ModuleRecord {
            manifest,
            registered_at: 0,
            registered_by: "fixture".into(),
            audit_event_id: None,
        },
    );
}

#[test]
fn install_preserves_raw_schedule_key_and_trimmed_state_key_compatibility() {
    for raw in ["", "  ", " padded-instance "] {
        for (active, tick) in [(true, true), (true, false), (false, false)] {
            let mut world = fixture(false);
            if active {
                registry_fixture(&mut world, "1.0.0", tick);
            }
            let state_key = if raw.trim().is_empty() {
                "m.instance"
            } else {
                raw.trim()
            };
            let schedule_key = if raw.trim().is_empty() {
                "m.instance"
            } else {
                raw
            };
            world.module_tick_schedule.insert(schedule_key.into(), 88);
            let mut event = install(active);
            if let DomainEvent::ModuleInstalled { instance_id, .. } = &mut event {
                *instance_id = raw.into();
            }
            world
                .append_event(WorldEventBody::Domain(event), None)
                .unwrap();
            assert!(world.state.module_instances.contains_key(state_key));
            assert_eq!(
                world.module_tick_schedule.get(schedule_key).copied(),
                if active && tick {
                    Some(world.state.time)
                } else {
                    None
                }
            );
            assert_eq!(world.state.module_instances[state_key].active, active);
        }
    }
}

#[test]
fn upgrade_retains_exact_instance_lookup_and_fee_validation_priority() {
    for (fee, payer, instance, expected) in [
        (
            -1,
            "missing-payer",
            "missing",
            "module action fee must be >= 0",
        ),
        (99, "payer", "missing", "module action fee debit failed"),
        (
            0,
            "payer",
            " instance-1 ",
            "module instance missing for upgrade",
        ),
    ] {
        let mut world = fixture(true);
        let mut event = upgrade(false);
        if let DomainEvent::ModuleUpgraded {
            fee_amount,
            upgrader_agent_id,
            instance_id,
            ..
        } = &mut event
        {
            *fee_amount = fee;
            *upgrader_agent_id = payer.into();
            *instance_id = instance.into();
        }
        let before = world.snapshot();
        let error = world
            .append_event(WorldEventBody::Domain(event), None)
            .unwrap_err();
        assert!(
            matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains(expected))
        );
        assert_eq!(world.snapshot(), before);
    }
}

#[test]
fn instance_install_upgrade_success_preserves_published_and_replayed_roots() {
    let mut world = fixture(false);
    registry_fixture(&mut world, "1.0.0", true);
    registry_fixture(&mut world, "2.0.0", false);
    let baseline = world.snapshot();
    for event in [install(true), upgrade(true)] {
        world
            .append_event(WorldEventBody::Domain(event), None)
            .unwrap();
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
    }
    assert_eq!(
        world.state.module_instances["instance-1"].module_version,
        "2.0.0"
    );
    assert!(!world.module_tick_schedule.contains_key("instance-1"));
    assert_eq!(
        world
            .agent_resource_balance("payer", ResourceKind::Data)
            .unwrap(),
        24
    );
    let replayed = World::from_snapshot(baseline, world.journal().clone()).unwrap();
    assert_eq!(
        replayed.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
    assert_eq!(replayed.module_tick_schedule, world.module_tick_schedule);
    assert_eq!(
        replayed.state.module_instances,
        world.state.module_instances
    );
}

#[test]
fn successful_install_normalizes_legacy_materials_without_losing_other_ledgers() {
    use crate::MaterialLedgerId;
    use std::collections::BTreeMap;
    for ledger_case in ["absent", "empty", "authoritative"] {
        let mut world = fixture(false);
        let world_key = MaterialLedgerId::world();
        let other_key = MaterialLedgerId::site("unrelated-site");
        let other_materials = BTreeMap::from([("copper".to_string(), 11)]);
        world.state.materials = BTreeMap::from([("iron_ingot".to_string(), 7)]);
        world.state.material_ledgers.clear();
        world
            .state
            .material_ledgers
            .insert(other_key.clone(), other_materials.clone());
        let expected_materials = if ledger_case == "authoritative" {
            BTreeMap::from([("steel".to_string(), 13)])
        } else {
            world.state.materials.clone()
        };
        if ledger_case != "absent" {
            world.state.material_ledgers.insert(
                world_key.clone(),
                if ledger_case == "empty" {
                    BTreeMap::new()
                } else {
                    expected_materials.clone()
                },
            );
        }
        let event = install(false);
        let mut direct = world.state.clone();
        direct.apply_domain_event(&event, world.state.time).unwrap();
        direct.route_domain_event(&event);
        world
            .append_event(WorldEventBody::Domain(event), None)
            .unwrap();
        assert_eq!(world.state, direct);
        assert_eq!(world.state.materials, expected_materials);
        assert_eq!(world.state.material_ledgers[&world_key], expected_materials);
        assert_eq!(world.state.material_ledgers[&other_key], other_materials);
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
    }
}

#[test]
fn instance_overwrite_saturates_counter_and_routes_zero_fee_events_exactly_once() {
    let mut world = fixture(true);
    world.state.next_module_instance_id = u64::MAX;
    world.state.time += 2;
    let baseline = world.snapshot();
    let balances = world.state.agents["payer"].state.resources.clone();
    let treasury = world.state.resources.clone();
    let mailbox_len = world.state.agents["payer"].mailbox.len();
    let mut replacement = install(false);
    if let DomainEvent::ModuleInstalled {
        fee_amount,
        wasm_hash,
        install_target,
        ..
    } = &mut replacement
    {
        *fee_amount = 0;
        *wasm_hash = "replacement-hash".into();
        *install_target = ModuleInstallTarget::LocationInfrastructure {
            location_id: "location-compat".into(),
        };
    }
    let mut upgraded = upgrade(false);
    if let DomainEvent::ModuleUpgraded { fee_amount, .. } = &mut upgraded {
        *fee_amount = 0;
    }
    for (index, event) in [replacement, upgraded].into_iter().enumerate() {
        world
            .append_event(WorldEventBody::Domain(event.clone()), None)
            .unwrap();
        assert_eq!(world.state.next_module_instance_id, u64::MAX);
        assert_eq!(world.state.module_instances.len(), 1);
        assert_eq!(
            world.state.agents["payer"].mailbox.len(),
            mailbox_len + index + 1
        );
        assert_eq!(world.state.agents["payer"].mailbox.back(), Some(&event));
        assert_eq!(world.state.agents["payer"].last_active, world.state.time);
        assert_eq!(world.state.agents["payer"].state.resources, balances);
        assert_eq!(world.state.resources, treasury);
        assert_eq!(
            world.state.module_instances["instance-1"].installed_at,
            world.state.time
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
        if index == 0 {
            assert_eq!(
                world.state.module_instances["instance-1"].wasm_hash,
                "replacement-hash"
            );
            assert!(matches!(
                world.state.installed_module_targets["m.instance"],
                ModuleInstallTarget::LocationInfrastructure { .. }
            ));
        }
    }
    assert_eq!(
        world.state.module_instances["instance-1"].wasm_hash,
        "new-hash"
    );
    assert_eq!(
        world.state.installed_module_targets["m.instance"],
        ModuleInstallTarget::SelfAgent
    );
    let replayed = World::from_snapshot(baseline, world.journal().clone()).unwrap();
    assert_eq!(replayed.state, world.state);
    assert_eq!(
        replayed.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
    assert_eq!(replayed.module_tick_schedule, world.module_tick_schedule);
}
