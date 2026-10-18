#![cfg(all(feature = "wasmtime", feature = "test_tier_full"))]

mod common;

use oasis7::GeoPos;
use oasis7::runtime::{
    Action, Manifest, ModuleAbiContract, ModuleActivation, ModuleChangeSet, ModuleKind,
    ModuleLimits, ModuleManifest, ModuleRole, ModuleSubscription, ModuleSubscriptionStage,
    PolicySet, ProposalDecision, World, WorldEventBody,
};
use oasis7_wasm_executor::{WasmExecutor, WasmExecutorConfig};
use sha2::{Digest, Sha256};

const WASM_EXECUTOR_TEST_MODULE: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x0d, 0x02, 0x60, 0x01, 0x7f, 0x01, 0x7f,
    0x60, 0x02, 0x7f, 0x7f, 0x02, 0x7f, 0x7f, 0x03, 0x03, 0x02, 0x00, 0x01, 0x05, 0x03, 0x01, 0x00,
    0x01, 0x07, 0x19, 0x03, 0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00, 0x05, 0x61, 0x6c,
    0x6c, 0x6f, 0x63, 0x00, 0x00, 0x04, 0x63, 0x61, 0x6c, 0x6c, 0x00, 0x01, 0x0a, 0x0f, 0x02, 0x05,
    0x00, 0x41, 0x80, 0x01, 0x0b, 0x07, 0x00, 0x41, 0x00, 0x41, 0xc7, 0x00, 0x0b, 0x0b, 0x4d, 0x01,
    0x00, 0x41, 0x00, 0x0b, 0x47, 0xa4, 0x65, 0x65, 0x6d, 0x69, 0x74, 0x73, 0x81, 0xa2, 0x64, 0x6b,
    0x69, 0x6e, 0x64, 0x69, 0x77, 0x61, 0x73, 0x6d, 0x2e, 0x74, 0x65, 0x73, 0x74, 0x67, 0x70, 0x61,
    0x79, 0x6c, 0x6f, 0x61, 0x64, 0xa1, 0x62, 0x6f, 0x6b, 0xf5, 0x67, 0x65, 0x66, 0x66, 0x65, 0x63,
    0x74, 0x73, 0x80, 0x69, 0x6e, 0x65, 0x77, 0x5f, 0x73, 0x74, 0x61, 0x74, 0x65, 0xf6, 0x6c, 0x6f,
    0x75, 0x74, 0x70, 0x75, 0x74, 0x5f, 0x62, 0x79, 0x74, 0x65, 0x73, 0x00,
];

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

fn wasm_executor_module_manifest(wasm_hash: String) -> ModuleManifest {
    ModuleManifest {
        module_id: "m.wasm".to_string(),
        name: "Wasm".to_string(),
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
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 2,
        },
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

#[test]
fn wasm_executor_runs_module_and_emits_event() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_hash = sha256_hex(WASM_EXECUTOR_TEST_MODULE);
    world
        .register_module_artifact(wasm_hash.clone(), WASM_EXECUTOR_TEST_MODULE)
        .unwrap();
    let module_manifest = wasm_executor_module_manifest(wasm_hash);
    apply_module_manifest(&mut world, module_manifest);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });

    let mut sandbox =
        WasmExecutor::new(WasmExecutorConfig::default()).expect("initialize wasm executor");
    world.step_with_modules(&mut sandbox).unwrap();

    let has_emit = world
        .journal()
        .events
        .iter()
        .any(|event| matches!(event.body, WorldEventBody::ModuleEmitted(_)));
    assert!(has_emit);
}

#[test]
fn wasm_executor_replay_preserves_module_emits() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_hash = sha256_hex(WASM_EXECUTOR_TEST_MODULE);
    world
        .register_module_artifact(wasm_hash.clone(), WASM_EXECUTOR_TEST_MODULE)
        .unwrap();
    let module_manifest = wasm_executor_module_manifest(wasm_hash);
    apply_module_manifest(&mut world, module_manifest);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });

    let mut sandbox =
        WasmExecutor::new(WasmExecutorConfig::default()).expect("initialize wasm executor");
    world.step_with_modules(&mut sandbox).unwrap();

    let snapshot = world.snapshot();
    let journal = world.journal().clone();
    let restored = oasis7::runtime_host::world::from_snapshot(snapshot, journal).unwrap();

    let original_emit = world
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::ModuleEmitted(emit) => Some(emit),
            _ => None,
        })
        .unwrap();
    let restored_emit = restored
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::ModuleEmitted(emit) => Some(emit),
            _ => None,
        })
        .unwrap();
    assert_eq!(original_emit.kind, restored_emit.kind);
    assert_eq!(original_emit.payload, restored_emit.payload);
}
