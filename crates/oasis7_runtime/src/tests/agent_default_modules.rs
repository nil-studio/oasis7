#![cfg(feature = "wasmtime")]

use super::super::*;
use super::pos;
use crate::agent_state::{CargoEntityEntry, CargoEntityKind};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_executor::WasmExecutor;

fn default_module_sandbox() -> WasmExecutor {
    WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor")
}

fn scenario_module_sandbox() -> WasmExecutor {
    WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor")
}

fn setup_world_with_default_modules() -> (World, WasmExecutor) {
    let mut world = World::new();
    world
        .install_m1_agent_default_modules("bootstrap")
        .expect("install default modules");

    (world, default_module_sandbox())
}

fn setup_world_with_scenario_modules(config: M1ScenarioBootstrapConfig) -> (World, WasmExecutor) {
    let mut world = World::new();
    world
        .install_m1_scenario_bootstrap_modules("bootstrap", config)
        .expect("install scenario bootstrap modules");

    (world, scenario_module_sandbox())
}

fn register_m1_builtin_wasm_artifact(world: &mut World, module_id: &str) -> String {
    super::super::register_m1_builtin_wasm_module_artifact(world, module_id)
        .expect("register embedded m1 builtin wasm module artifact")
}

fn install_builtin_module(
    world: &mut World,
    module_id: &str,
    name: &str,
    role: ModuleRole,
    subscriptions: Vec<ModuleSubscription>,
    max_output_bytes: u64,
) {
    let wasm_hash = register_m1_builtin_wasm_artifact(world, module_id);
    let module_manifest = ModuleManifest {
        module_id: module_id.to_string(),
        name: name.to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions,
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024 * 1024,
            max_gas: 2_000_000,
            max_call_rate: 16,
            max_output_bytes,
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
        serde_json::to_value(&changes).expect("serialize module changes"),
    );
    let manifest = Manifest {
        version: world.manifest().version.saturating_add(1),
        content: serde_json::Value::Object(content),
    };
    let proposal_id = world
        .propose_manifest_update(manifest, "bootstrap")
        .expect("propose module install");
    world.shadow_proposal(proposal_id).expect("shadow proposal");
    world
        .approve_proposal(proposal_id, "bootstrap", ProposalDecision::Approve)
        .expect("approve proposal");
    world.apply_proposal(proposal_id).expect("apply proposal");
}

fn install_m1_transfer_rule(world: &mut World) {
    install_builtin_module(
        world,
        M1_TRANSFER_RULE_MODULE_ID,
        "M1TransferRule",
        ModuleRole::Rule,
        vec![
            ModuleSubscription {
                event_kinds: vec![
                    "domain.agent_registered".to_string(),
                    "domain.agent_moved".to_string(),
                ],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            },
            ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.transfer_resource".to_string()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            },
        ],
        4096,
    );
}

fn install_m1_body_module(world: &mut World) {
    install_builtin_module(
        world,
        M1_BODY_MODULE_ID,
        "M1BodyModule",
        ModuleRole::Body,
        vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.body_action".to_string()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: None,
        }],
        4096,
    );
}

fn last_module_state(world: &World, module_id: &str) -> Option<Vec<u8>> {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::ModuleStateUpdated(update) if update.module_id == module_id => {
                Some(update.state.clone())
            }
            _ => None,
        })
}

fn last_domain_event(world: &World) -> Option<&DomainEvent> {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(domain) => Some(domain),
            _ => None,
        })
}

#[test]
fn default_sensor_module_emits_observation() {
    let (mut world, mut sandbox) = setup_world_with_default_modules();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register agent step");

    world.submit_action(Action::QueryObservation {
        agent_id: "agent-1".to_string(),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("query observation step");

    let last = last_domain_event(&world).expect("last domain event");
    match last {
        DomainEvent::Observation { observation } => {
            assert_eq!(observation.agent_id, "agent-1");
        }
        other => panic!("unexpected domain event: {other:?}"),
    }
}

#[test]
fn default_mobility_module_rejects_zero_distance_move() {
    let (mut world, mut sandbox) = setup_world_with_default_modules();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register agent step");

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(0, 0),
    });
    world.step_with_modules(&mut sandbox).expect("move step");

    let last = last_domain_event(&world).expect("last domain event");
    match last {
        DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
            ..
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("equals current position"))
            );
        }
        other => panic!("unexpected domain event: {other:?}"),
    }
}

#[test]
fn default_memory_module_records_domain_events() {
    let (mut world, mut sandbox) = setup_world_with_default_modules();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register agent step");

    world.submit_action(Action::QueryObservation {
        agent_id: "agent-1".to_string(),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("observation step");

    let state = last_module_state(&world, M1_MEMORY_MODULE_ID).expect("memory state update");
    let decoded: serde_json::Value =
        serde_cbor::from_slice(&state).expect("decode memory state as cbor value");
    let entries = decoded
        .get("entries")
        .and_then(|value| value.as_array())
        .expect("memory entries array");
    assert!(entries.iter().any(|entry| {
        entry.get("kind")
            == Some(&serde_json::Value::String(
                "domain.agent_registered".to_string(),
            ))
    }));
    assert!(entries.iter().any(|entry| {
        entry.get("kind") == Some(&serde_json::Value::String("domain.observation".to_string()))
    }));
}

#[test]
fn default_storage_cargo_module_tracks_expand_events() {
    let (mut world, mut sandbox) = setup_world_with_default_modules();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register agent step");

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
        .expect("seed cargo entry");

    world.submit_action(Action::ExpandBodyInterface {
        agent_id: "agent-1".to_string(),
        interface_module_item_id: "iface-kit-1".to_string(),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("expand body interface step");

    let state =
        last_module_state(&world, M1_STORAGE_CARGO_MODULE_ID).expect("cargo module state update");
    let decoded: serde_json::Value =
        serde_cbor::from_slice(&state).expect("decode cargo state as cbor value");
    let consumed = decoded
        .get("consumed_interface_items")
        .and_then(|value| value.as_object())
        .expect("consumed item map");
    assert_eq!(
        consumed.get("iface-kit-1"),
        Some(&serde_json::Value::Number(1_u64.into()))
    );
}

#[test]
fn scenario_modules_limit_mobility_before_sensor_when_power_low() {
    let (mut world, mut sandbox) =
        setup_world_with_scenario_modules(M1ScenarioBootstrapConfig::default());

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register agent step");

    let mut rejected_by_storage = false;
    for idx in 0..8 {
        world.submit_action(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: pos((idx as i64 + 1) * 100_000, 0),
        });
        world
            .step_with_modules(&mut sandbox)
            .expect("move step with scenario modules");

        let Some(DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
            ..
        }) = last_domain_event(&world)
        else {
            continue;
        };
        if notes
            .iter()
            .any(|note| note.contains("storage insufficient"))
        {
            rejected_by_storage = true;
            break;
        }
    }
    assert!(rejected_by_storage);

    world.submit_action(Action::QueryObservation {
        agent_id: "agent-1".to_string(),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("query observation after low power move rejection");

    let last = last_domain_event(&world).expect("last domain event");
    assert!(matches!(last, DomainEvent::Observation { .. }));
}

#[test]
fn scenario_modules_replay_keeps_state_consistent() {
    let (mut world, mut sandbox) =
        setup_world_with_scenario_modules(M1ScenarioBootstrapConfig::default());

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register agent step");

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
        .expect("seed cargo entry");

    world.submit_action(Action::ExpandBodyInterface {
        agent_id: "agent-1".to_string(),
        interface_module_item_id: "iface-kit-1".to_string(),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("expand body interface step");

    world.submit_action(Action::QueryObservation {
        agent_id: "agent-1".to_string(),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("query observation step");

    let snapshot = world.snapshot();
    let journal = world.journal().clone();
    let restored = World::from_snapshot(snapshot, journal).expect("restore world");

    assert_eq!(restored.state(), world.state());
    assert_eq!(restored.module_registry(), world.module_registry());

    assert_eq!(last_domain_event(&restored), last_domain_event(&world));
}

#[test]
fn scenario_modules_with_transfer_and_body_keep_wasm_closed_loop_consistent() {
    let (mut world, mut sandbox) =
        setup_world_with_scenario_modules(M1ScenarioBootstrapConfig::default());
    install_m1_transfer_rule(&mut world);
    install_m1_body_module(&mut world);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register agent-1 step");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-2".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register agent-2 step");
    world.set_resource_balance(ResourceKind::Electricity, 256);
    world
        .set_agent_resource_balance("agent-1", ResourceKind::Electricity, 32)
        .expect("seed agent-1 electricity");
    world
        .set_agent_resource_balance("agent-2", ResourceKind::Electricity, 32)
        .expect("seed agent-2 electricity");

    world.submit_action(Action::QueryObservation {
        agent_id: "agent-1".to_string(),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("query observation step");
    match last_domain_event(&world).expect("observation event") {
        DomainEvent::Observation { observation } => {
            assert_eq!(observation.agent_id, "agent-1");
            assert!(
                observation
                    .visible_agents
                    .iter()
                    .any(|agent| agent.agent_id == "agent-2")
            );
        }
        other => panic!("unexpected domain event after observation: {other:?}"),
    }

    world.submit_action(Action::TransferResource {
        from_agent_id: "agent-1".to_string(),
        to_agent_id: "agent-2".to_string(),
        kind: ResourceKind::Electricity,
        amount: 2,
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("transfer resource step");
    match last_domain_event(&world).expect("resource transfer event") {
        DomainEvent::ResourceTransferred {
            from_agent_id,
            to_agent_id,
            kind,
            amount,
        } => {
            assert_eq!(from_agent_id, "agent-1");
            assert_eq!(to_agent_id, "agent-2");
            assert_eq!(kind, &ResourceKind::Electricity);
            assert_eq!(*amount, 2);
        }
        other => panic!("unexpected domain event after transfer: {other:?}"),
    }

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(100_000, 0),
    });
    world.step_with_modules(&mut sandbox).expect("move step");
    match last_domain_event(&world).expect("move domain event") {
        DomainEvent::AgentMoved { agent_id, .. } => assert_eq!(agent_id, "agent-1"),
        other => panic!("unexpected domain event after move: {other:?}"),
    }

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
        .expect("seed cargo item");
    world.submit_action(Action::ExpandBodyInterface {
        agent_id: "agent-1".to_string(),
        interface_module_item_id: "iface-kit-1".to_string(),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("expand body interface step");
    assert!(matches!(
        last_domain_event(&world),
        Some(DomainEvent::BodyInterfaceExpanded { .. })
    ));

    world.submit_action(Action::BodyAction {
        agent_id: "agent-1".to_string(),
        kind: "bootstrap".to_string(),
        payload: serde_json::json!({
            "mass_kg": 120_u64,
            "radius_cm": 80_u64,
            "thrust_limit": 200_u64,
            "cross_section_cm2": 4000_u64,
        }),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("body action step");
    match last_domain_event(&world).expect("body update event") {
        DomainEvent::BodyAttributesUpdated {
            agent_id, reason, ..
        } => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(reason, "body.bootstrap");
        }
        other => panic!("unexpected domain event after body action: {other:?}"),
    }

    let memory_state = last_module_state(&world, M1_MEMORY_MODULE_ID).expect("memory state");
    let memory_value: serde_json::Value =
        serde_cbor::from_slice(&memory_state).expect("decode memory state");
    let memory_entries = memory_value
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .expect("memory entries");
    let memory_kinds: Vec<&str> = memory_entries
        .iter()
        .filter_map(|entry| entry.get("kind").and_then(serde_json::Value::as_str))
        .collect();
    assert!(memory_kinds.contains(&"domain.agent_registered"));
    assert!(memory_kinds.contains(&"domain.agent_moved"));
    assert!(memory_kinds.contains(&"domain.observation"));
    assert!(memory_kinds.contains(&"domain.resource_transferred"));
    assert!(memory_kinds.contains(&"domain.body_interface_expanded"));
    assert!(memory_kinds.contains(&"domain.body_attributes_updated"));

    let cargo_state = last_module_state(&world, M1_STORAGE_CARGO_MODULE_ID).expect("cargo state");
    let cargo_value: serde_json::Value =
        serde_cbor::from_slice(&cargo_state).expect("decode cargo state");
    assert_eq!(
        cargo_value
            .get("consumed_interface_items")
            .and_then(|value| value.get("iface-kit-1"))
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );

    let power_state = last_module_state(&world, M1_STORAGE_POWER_MODULE_ID).expect("power state");
    let power_value: serde_json::Value =
        serde_cbor::from_slice(&power_state).expect("decode power state");
    assert_eq!(
        power_value
            .get("agents")
            .and_then(|agents| agents.get("agent-1"))
            .and_then(|agent| agent.get("pos"))
            .and_then(|pos| pos.get("x_cm"))
            .and_then(serde_json::Value::as_i64),
        Some(100_000)
    );
}
