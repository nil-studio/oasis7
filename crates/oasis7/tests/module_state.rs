#![cfg(any(feature = "test_tier_required", feature = "test_tier_full"))]

mod common;

use oasis7::GeoPos;
use oasis7::runtime::{
    Action, Manifest, ModuleAbiContract, ModuleActivation, ModuleChangeSet, ModuleKind,
    ModuleLimits, ModuleManifest, ModuleRole, ModuleSubscription, ModuleSubscriptionStage,
    PolicySet, ProposalDecision, World, WorldEventBody,
};
use oasis7_wasm_abi::{
    ModuleCallFailure, ModuleCallInput, ModuleCallRequest, ModuleOutput, ModuleSandbox,
};
use sha2::{Digest, Sha256};

#[cfg(feature = "test_tier_full")]
use oasis7::runtime::WorldError;
#[cfg(feature = "test_tier_full")]
use oasis7_wasm_abi::ModuleCallErrorCode;
#[cfg(feature = "test_tier_full")]
use oasis7_wasm_executor::FixedSandbox;

fn pos(x: i64, y: i64) -> GeoPos {
    GeoPos {
        x_cm: x,
        y_cm: y,
        z_cm: 0,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

struct StateCaptureSandbox {
    seen_states: Vec<Option<Vec<u8>>>,
    next_state: Vec<u8>,
}

impl StateCaptureSandbox {
    fn new(next_state: Vec<u8>) -> Self {
        Self {
            seen_states: Vec::new(),
            next_state,
        }
    }
}

impl ModuleSandbox for StateCaptureSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        let decoded: ModuleCallInput = serde_cbor::from_slice(&request.input).unwrap();
        self.seen_states.push(decoded.state);
        Ok(ModuleOutput {
            new_state: Some(self.next_state.clone()),
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        })
    }
}

fn apply_module_manifest(world: &mut World, module_manifest: ModuleManifest) {
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

#[cfg(feature = "test_tier_required")]
#[test]
fn reducer_state_updates_and_is_reused() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-state";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.state".to_string(),
        name: "State".to_string(),
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
            max_emits: 0,
        },
    };

    apply_module_manifest(&mut world, module_manifest);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let event = world.journal().events.last().unwrap().clone();
    let mut sandbox = StateCaptureSandbox::new(b"state-1".to_vec());

    world.route_event_to_modules(&event, &mut sandbox).unwrap();
    world.route_event_to_modules(&event, &mut sandbox).unwrap();

    assert_eq!(sandbox.seen_states.len(), 2);
    assert_eq!(sandbox.seen_states[0], Some(Vec::new()));
    assert_eq!(sandbox.seen_states[1], Some(b"state-1".to_vec()));

    let has_state_event = world
        .journal()
        .events
        .iter()
        .any(|event| matches!(event.body, WorldEventBody::ModuleStateUpdated(_)));
    assert!(has_state_event);

    assert_eq!(
        world.state().module_states.get("m.state"),
        Some(&b"state-1".to_vec())
    );

    let snapshot = world.snapshot();
    let journal = world.journal().clone();
    let restored = oasis7::runtime_host::world::from_snapshot(snapshot, journal).unwrap();
    assert_eq!(
        restored.state().module_states.get("m.state"),
        Some(&b"state-1".to_vec())
    );
}

#[cfg(feature = "test_tier_full")]
#[test]
fn pure_module_new_state_is_rejected() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-state-pure";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.pure".to_string(),
        name: "Pure".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
    };

    apply_module_manifest(&mut world, module_manifest);

    let output = ModuleOutput {
        new_state: Some(b"bad".to_vec()),
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    let err = world
        .execute_module_call("m.pure", "trace-1", Vec::new(), &mut sandbox)
        .unwrap_err();
    assert!(matches!(
        err,
        WorldError::ModuleCallFailed {
            code: ModuleCallErrorCode::InvalidOutput,
            ..
        }
    ));

    let has_state_event = world
        .journal()
        .events
        .iter()
        .any(|event| matches!(event.body, WorldEventBody::ModuleStateUpdated(_)));
    assert!(!has_state_event);
}
