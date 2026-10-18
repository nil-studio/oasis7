#![cfg(any(feature = "test_tier_required", feature = "test_tier_full"))]

mod common;

use oasis7::runtime::*;
use sha2::{Digest, Sha256};

#[cfg(feature = "test_tier_full")]
use oasis7::GeoPos;
#[cfg(feature = "test_tier_full")]
use oasis7_wasm_abi::{ModuleEmit, ModuleOutput};
#[cfg(feature = "test_tier_full")]
use oasis7_wasm_executor::FixedSandbox;
#[cfg(feature = "test_tier_full")]
use serde_json::json;

fn manifest_with_changes(changes: ModuleChangeSet) -> Manifest {
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    }
}

fn wasm_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(feature = "test_tier_required")]
#[test]
fn governance_module_happy_path_updates_registry() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-lifecycle-happy";
    let wasm_hash = wasm_hash(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.lifecycle".to_string(),
        name: "Lifecycle".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits::unbounded(),
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: module_manifest.module_id.clone(),
            version: module_manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };

    let manifest = manifest_with_changes(changes);
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();

    let key = ModuleRegistry::record_key(&module_manifest.module_id, &module_manifest.version);
    let record = world.module_registry().records.get(&key).unwrap();
    assert_eq!(record.manifest, module_manifest);
    assert_eq!(
        world.module_registry().active.get("m.lifecycle").cloned(),
        Some("0.1.0".to_string())
    );

    let module_events = world
        .journal()
        .events
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::ModuleEvent(module_event) => Some(module_event),
            _ => None,
        })
        .count();
    assert_eq!(module_events, 2);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn shadow_failure_blocks_apply() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let module_manifest = ModuleManifest {
        module_id: "m.lifecycle".to_string(),
        name: "Lifecycle".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: "missing-hash".to_string(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity("missing-hash")),
        limits: ModuleLimits::unbounded(),
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest],
        ..ModuleChangeSet::default()
    };

    let manifest = manifest_with_changes(changes);
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    let err = world.shadow_proposal(proposal_id).unwrap_err();
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));

    let has_module_events = world
        .journal()
        .events
        .iter()
        .any(|event| matches!(event.body, WorldEventBody::ModuleEvent(_)));
    assert!(!has_module_events);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_routing_emits_event() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-routing";
    let wasm_hash = wasm_hash(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.route".to_string(),
        name: "Route".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
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

    let manifest = manifest_with_changes(changes);
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "AgentRegistered".to_string(),
            payload: json!({"ok": true}),
        }],
        tick_lifecycle: None,
        output_bytes: 64,
    };

    let mut sandbox = FixedSandbox::succeed(output);
    world.step_with_modules(&mut sandbox).unwrap();

    let has_emit = world
        .journal()
        .events
        .iter()
        .any(|event| matches!(event.body, WorldEventBody::ModuleEmitted(_)));
    assert!(has_emit);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn replay_preserves_module_events() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-replay";
    let wasm_hash = wasm_hash(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.replay".to_string(),
        name: "Replay".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
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

    let manifest = manifest_with_changes(changes);
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();

    let snapshot = world.snapshot();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "AgentRegistered".to_string(),
            payload: json!({"ok": true}),
        }],
        tick_lifecycle: None,
        output_bytes: 64,
    };

    let mut sandbox = FixedSandbox::succeed(output);
    world.step_with_modules(&mut sandbox).unwrap();

    let journal = world.journal().clone();
    let restored = oasis7::runtime_host::world::from_snapshot(snapshot, journal).unwrap();

    let has_emit = restored
        .journal()
        .events
        .iter()
        .any(|event| matches!(event.body, WorldEventBody::ModuleEmitted(_)));
    assert!(has_emit);
}
