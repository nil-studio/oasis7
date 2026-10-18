#![cfg(any(feature = "test_tier_required", feature = "test_tier_full"))]

mod common;

use oasis7::GeoPos;
use oasis7::runtime::{
    Action, CapabilityGrant, Manifest, ModuleAbiContract, ModuleActivation, ModuleChangeSet,
    ModuleKind, ModuleLimits, ModuleManifest, ModuleRole, ModuleSubscription, PolicySet,
    ProposalDecision, World, WorldEventBody,
};
use oasis7_wasm_abi::{ModuleEmit, ModuleOutput};
use oasis7_wasm_executor::FixedSandbox;
use serde_json::json;
use sha2::{Digest, Sha256};

#[cfg(feature = "test_tier_full")]
use oasis7::runtime::{ModuleSubscriptionStage, WorldError};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn apply_module_changes(world: &mut World, changes: ModuleChangeSet) {
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

fn pos(x: i64, y: i64) -> GeoPos {
    GeoPos {
        x_cm: x,
        y_cm: y,
        z_cm: 0,
    }
}

#[cfg(feature = "test_tier_required")]
#[test]
fn module_subscription_event_filters_by_agent_id() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    world.add_capability(CapabilityGrant::allow_all("cap.module"));

    let wasm_bytes = b"module-filter-event";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.filter.event".to_string(),
        name: "FilterEvent".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: None,
            filters: Some(json!({
                "event": [
                    {"path": "/body/payload/data/agent_id", "eq": "agent-keep"}
                ]
            })),
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
    apply_module_changes(&mut world, changes);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-keep".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-skip".to_string(),
        pos: pos(1, 1),
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "Hit".to_string(),
            payload: json!({"agent": "agent-keep"}),
        }],
        tick_lifecycle: None,
        output_bytes: 64,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world.step_with_modules(&mut sandbox).unwrap();

    let emit_count = world
        .journal()
        .events
        .iter()
        .filter(|event| matches!(event.body, WorldEventBody::ModuleEmitted(_)))
        .count();
    assert_eq!(emit_count, 1);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_subscription_action_filters_by_agent_id() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    world.add_capability(CapabilityGrant::allow_all("cap.module"));

    let wasm_bytes = b"module-filter-action";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.filter.action".to_string(),
        name: "FilterAction".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.register_agent".to_string()],
            stage: None,
            filters: Some(json!({
                "action": [
                    {"path": "/action/data/agent_id", "eq": "agent-keep"}
                ]
            })),
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
    apply_module_changes(&mut world, changes);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-keep".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-skip".to_string(),
        pos: pos(1, 1),
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "ActionSeen".to_string(),
            payload: json!({"agent": "agent-keep"}),
        }],
        tick_lifecycle: None,
        output_bytes: 64,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world.step_with_modules(&mut sandbox).unwrap();

    let emit_count = world
        .journal()
        .events
        .iter()
        .filter(|event| matches!(event.body, WorldEventBody::ModuleEmitted(_)))
        .count();
    assert_eq!(emit_count, 1);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_subscription_invalid_filter_is_rejected() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    world.add_capability(CapabilityGrant::allow_all("cap.module"));

    let wasm_bytes = b"module-filter-invalid";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.filter.invalid".to_string(),
        name: "FilterInvalid".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: Some(json!({
                "event": [
                    {"path": "body/payload/data/agent_id", "eq": "agent-1"}
                ]
            })),
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

    let err = world.shadow_proposal(proposal_id).unwrap_err();
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_subscription_rejects_mixed_kinds_without_stage() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    world.add_capability(CapabilityGrant::allow_all("cap.module"));

    let wasm_bytes = b"module-filter-mixed";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.filter.mixed".to_string(),
        name: "FilterMixed".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: vec!["action.register_agent".to_string()],
            stage: None,
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

    let err = world.shadow_proposal(proposal_id).unwrap_err();
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_subscription_rejects_action_kinds_in_post_event_stage() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    world.add_capability(CapabilityGrant::allow_all("cap.module"));

    let wasm_bytes = b"module-filter-action-post-event";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.filter.action.post".to_string(),
        name: "FilterActionPost".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.register_agent".to_string()],
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

    let err = world.shadow_proposal(proposal_id).unwrap_err();
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_subscription_any_matches() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    world.add_capability(CapabilityGrant::allow_all("cap.module"));

    let wasm_bytes = b"module-filter-any";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.filter.any".to_string(),
        name: "FilterAny".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: Some(json!({
                "event": {
                    "any": [
                        {"path": "/body/payload/data/agent_id", "eq": "agent-keep"}
                    ]
                }
            })),
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
    apply_module_changes(&mut world, changes);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-keep".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-skip".to_string(),
        pos: pos(1, 1),
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "Hit".to_string(),
            payload: json!({"agent": "agent-keep"}),
        }],
        tick_lifecycle: None,
        output_bytes: 64,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world.step_with_modules(&mut sandbox).unwrap();

    let emit_count = world
        .journal()
        .events
        .iter()
        .filter(|event| matches!(event.body, WorldEventBody::ModuleEmitted(_)))
        .count();
    assert_eq!(emit_count, 1);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_subscription_numeric_range_matches() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    world.add_capability(CapabilityGrant::allow_all("cap.module"));

    let wasm_bytes = b"module-filter-range";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.filter.range".to_string(),
        name: "FilterRange".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: Some(json!({
                "event": {
                    "all": [
                        {"path": "/body/payload/data/pos/x_cm", "gte": 0},
                        {"path": "/body/payload/data/pos/x_cm", "lt": 10}
                    ]
                }
            })),
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
    apply_module_changes(&mut world, changes);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-in".to_string(),
        pos: pos(5, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-out".to_string(),
        pos: pos(-5, 0),
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "Hit".to_string(),
            payload: json!({"agent": "agent-in"}),
        }],
        tick_lifecycle: None,
        output_bytes: 64,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world.step_with_modules(&mut sandbox).unwrap();

    let emit_count = world
        .journal()
        .events
        .iter()
        .filter(|event| matches!(event.body, WorldEventBody::ModuleEmitted(_)))
        .count();
    assert_eq!(emit_count, 1);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_subscription_regex_matches() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    world.add_capability(CapabilityGrant::allow_all("cap.module"));

    let wasm_bytes = b"module-filter-regex";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.filter.regex".to_string(),
        name: "FilterRegex".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.register_agent".to_string()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: Some(json!({
                "action": {
                    "all": [
                        {"path": "/action/data/agent_id", "re": "^agent-[0-9]+$"}
                    ]
                }
            })),
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
    apply_module_changes(&mut world, changes);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-123".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-x".to_string(),
        pos: pos(1, 1),
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "ActionSeen".to_string(),
            payload: json!({"agent": "agent-123"}),
        }],
        tick_lifecycle: None,
        output_bytes: 64,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world.step_with_modules(&mut sandbox).unwrap();

    let emit_count = world
        .journal()
        .events
        .iter()
        .filter(|event| matches!(event.body, WorldEventBody::ModuleEmitted(_)))
        .count();
    assert_eq!(emit_count, 1);
}
