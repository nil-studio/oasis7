use super::super::*;
use super::pos;
use crate::agent_state::{
    BodyKernelView, BodySlotType, CargoEntityEntry, CargoEntityKind,
};
#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
use oasis7_agent_api::world_action::ResourceKind;
#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
use oasis7_wasm_executor::WasmExecutor;

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
fn install_m1_body_module(world: &mut World) {
    let wasm_hash =
        super::super::register_m1_builtin_wasm_module_artifact(world, M1_BODY_MODULE_ID)
            .expect("register embedded m1 builtin wasm module artifact");

    let module_manifest = ModuleManifest {
        module_id: M1_BODY_MODULE_ID.to_string(),
        name: "M1BodyModule".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Body,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.body_action".to_string()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024 * 1024,
            max_gas: 2_000_000,
            max_call_rate: 10,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 1,
        },
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: module_manifest.module_id.clone(),
            version: module_manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();
}

#[test]
fn record_body_attributes_updates_state() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let view = BodyKernelView {
        mass_kg: 120,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4000,
    };

    world
        .record_body_attributes_update(
            "agent-1",
            view.clone(),
            "boot".to_string(),
            Some(CausedBy::Action(1)),
        )
        .unwrap();

    let agent = world.state().agents.get("agent-1").unwrap();
    assert_eq!(agent.state.body_view, view);

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::BodyAttributesUpdated { agent_id, view, .. }) => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(view.mass_kg, 120);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn record_body_attributes_rejects() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    world
        .record_body_attributes_reject(
            "agent-1",
            "out_of_range".to_string(),
            Some(CausedBy::Action(2)),
        )
        .unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::BodyAttributesRejected { agent_id, .. }) => {
            assert_eq!(agent_id, "agent-1");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn record_body_attributes_update_rejects_out_of_range() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let invalid = BodyKernelView {
        mass_kg: 0,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4000,
    };

    world
        .record_body_attributes_update(
            "agent-1",
            invalid,
            "boot".to_string(),
            Some(CausedBy::Action(3)),
        )
        .unwrap();

    let agent = world.state().agents.get("agent-1").unwrap();
    assert_eq!(agent.state.body_view, BodyKernelView::default());

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::BodyAttributesRejected { agent_id, reason }) => {
            assert_eq!(agent_id, "agent-1");
            assert!(reason.contains("mass_kg"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn record_body_attributes_update_rejects_on_rate_violation() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let initial = BodyKernelView {
        mass_kg: 100,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4000,
    };

    world
        .record_body_attributes_update(
            "agent-1",
            initial.clone(),
            "boot".to_string(),
            Some(CausedBy::Action(4)),
        )
        .unwrap();

    let spike = BodyKernelView {
        mass_kg: 100_000,
        radius_cm: initial.radius_cm,
        thrust_limit: initial.thrust_limit,
        cross_section_cm2: initial.cross_section_cm2,
    };

    world
        .record_body_attributes_update(
            "agent-1",
            spike,
            "upgrade".to_string(),
            Some(CausedBy::Action(5)),
        )
        .unwrap();

    let agent = world.state().agents.get("agent-1").unwrap();
    assert_eq!(agent.state.body_view, initial);

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::BodyAttributesRejected { agent_id, reason }) => {
            assert_eq!(agent_id, "agent-1");
            assert!(reason.contains("rate violation"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn body_action_updates_view_and_costs_resources() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    install_m1_body_module(&mut world);
    world.set_resource_balance(ResourceKind::Electricity, 100);

    let view = BodyKernelView {
        mass_kg: 120,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4000,
    };

    world.submit_action(Action::BodyAction {
        agent_id: "agent-1".to_string(),
        kind: "boot".to_string(),
        payload: serde_json::to_value(view.clone()).unwrap(),
    });

    let mut sandbox =
        WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor");
    world.step_with_modules(&mut sandbox).unwrap();

    let agent = world.state().agents.get("agent-1").unwrap();
    assert_eq!(agent.state.body_view, view);
    assert_eq!(
        world.resource_balance(ResourceKind::Electricity),
        100 - M1_BODY_ACTION_COST_ELECTRICITY
    );
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn body_action_rejects_when_insufficient_resources() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    install_m1_body_module(&mut world);
    world.set_resource_balance(ResourceKind::Electricity, 0);

    let view = BodyKernelView {
        mass_kg: 120,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4000,
    };

    world.submit_action(Action::BodyAction {
        agent_id: "agent-1".to_string(),
        kind: "boot".to_string(),
        payload: serde_json::to_value(view).unwrap(),
    });

    let mut sandbox =
        WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor");
    world.step_with_modules(&mut sandbox).unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::InsufficientResources { .. } => {}
            other => panic!("unexpected reject reason: {other:?}"),
        },
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn body_update_replay_is_consistent() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let initial = BodyKernelView {
        mass_kg: 120,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4000,
    };
    world.submit_action(Action::EmitBodyAttributes {
        agent_id: "agent-1".to_string(),
        view: initial.clone(),
        reason: "init".to_string(),
    });
    world.step().unwrap();

    let snapshot = world.snapshot();

    let invalid = BodyKernelView {
        mass_kg: 0,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4000,
    };
    world.submit_action(Action::EmitBodyAttributes {
        agent_id: "agent-1".to_string(),
        view: invalid,
        reason: "invalid".to_string(),
    });
    world.step().unwrap();

    let updated = BodyKernelView {
        mass_kg: 150,
        radius_cm: 80,
        thrust_limit: 220,
        cross_section_cm2: 4000,
    };
    world.submit_action(Action::EmitBodyAttributes {
        agent_id: "agent-1".to_string(),
        view: updated,
        reason: "upgrade".to_string(),
    });
    world.step().unwrap();

    let journal = world.journal().clone();
    let restored = World::from_snapshot(snapshot, journal).unwrap();
    assert_eq!(restored.state(), world.state());
}

#[test]
fn expand_body_interface_consumes_item_and_adds_slot() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    world
        .add_agent_cargo_entity(
            "agent-1",
            CargoEntityEntry {
                entity_id: "iface-kit-1".to_string(),
                entity_kind: CargoEntityKind::InterfaceModuleItem,
                quantity: 2,
                size_per_unit: 1,
            },
        )
        .unwrap();

    world.submit_action(Action::ExpandBodyInterface {
        agent_id: "agent-1".to_string(),
        interface_module_item_id: "iface-kit-1".to_string(),
    });
    world.step().unwrap();

    let agent = world.state().agents.get("agent-1").unwrap();
    assert_eq!(agent.state.body_state.slot_capacity, 8);
    assert_eq!(agent.state.body_state.expansion_level, 1);
    assert!(
        agent
            .state
            .body_state
            .slots
            .iter()
            .any(|slot| slot.slot_id == "slot-8" && slot.slot_type == BodySlotType::Universal)
    );

    let item = agent
        .state
        .body_state
        .cargo_entries
        .iter()
        .find(|entry| entry.entity_id == "iface-kit-1")
        .unwrap();
    assert_eq!(item.quantity, 1);

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::BodyInterfaceExpanded {
            agent_id,
            consumed_item_id,
            ..
        }) => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(consumed_item_id, "iface-kit-1");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn expand_body_interface_rejects_when_item_missing() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    world.submit_action(Action::ExpandBodyInterface {
        agent_id: "agent-1".to_string(),
        interface_module_item_id: "iface-kit-missing".to_string(),
    });
    world.step().unwrap();

    let agent = world.state().agents.get("agent-1").unwrap();
    assert_eq!(agent.state.body_state.slot_capacity, 7);
    assert_eq!(agent.state.body_state.expansion_level, 0);

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::BodyInterfaceExpandRejected {
            agent_id,
            consumed_item_id,
            reason,
        }) => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(consumed_item_id, "iface-kit-missing");
            assert!(reason.contains("unavailable") || reason.contains("depleted"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn expand_body_interface_rejects_missing_agent() {
    let mut world = World::new();
    world.submit_action(Action::ExpandBodyInterface {
        agent_id: "missing-agent".to_string(),
        interface_module_item_id: "iface-kit-1".to_string(),
    });
    world.step().unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected {
            reason: RejectReason::AgentNotFound { agent_id },
            ..
        }) => {
            assert_eq!(agent_id, "missing-agent");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn expanded_body_state_persists_after_restore() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    world
        .add_agent_cargo_entity(
            "agent-1",
            CargoEntityEntry {
                entity_id: "iface-kit-1".to_string(),
                entity_kind: CargoEntityKind::InterfaceModuleItem,
                quantity: 1,
                size_per_unit: 1,
            },
        )
        .unwrap();

    world.submit_action(Action::ExpandBodyInterface {
        agent_id: "agent-1".to_string(),
        interface_module_item_id: "iface-kit-1".to_string(),
    });
    world.step().unwrap();

    let snapshot = world.snapshot();
    let restored = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    let agent = restored.state().agents.get("agent-1").unwrap();
    assert_eq!(agent.state.body_state.slot_capacity, 8);
    assert_eq!(agent.state.body_state.expansion_level, 1);
    assert!(
        agent
            .state
            .body_state
            .slots
            .iter()
            .any(|slot| slot.slot_id == "slot-8")
    );
    assert!(
        !agent
            .state
            .body_state
            .cargo_entries
            .iter()
            .any(|entry| entry.entity_id == "iface-kit-1")
    );
}
