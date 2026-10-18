#![cfg(any(feature = "test_tier_required", feature = "test_tier_full"))]

mod common;

use oasis7::runtime::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "test_tier_full")]
use serde_json::json;

fn wasm_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(feature = "test_tier_required")]
#[test]
fn module_store_roundtrip() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("oasis7-store-{unique}"));

    let store = ModuleStore::new(&dir);
    let wasm_bytes = b"store-bytes";
    let hash = wasm_hash(wasm_bytes);

    store.write_artifact(&hash, wasm_bytes).unwrap();
    let loaded_bytes = store.read_artifact(&hash).unwrap();
    assert_eq!(loaded_bytes, wasm_bytes.to_vec());

    let manifest = ModuleManifest {
        module_id: "m.store".to_string(),
        name: "Store".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(hash.as_str())),
        limits: ModuleLimits::unbounded(),
    };

    store.write_meta(&manifest).unwrap();
    let loaded_manifest = store.read_meta(&hash).unwrap();
    assert_eq!(loaded_manifest, manifest);

    let mut registry = ModuleRegistry::default();
    let key = ModuleRegistry::record_key("m.store", "0.1.0");
    registry.records.insert(
        key,
        ModuleRecord {
            manifest,
            registered_at: 1,
            registered_by: "tester".to_string(),
            audit_event_id: None,
        },
    );
    registry
        .active
        .insert("m.store".to_string(), "0.1.0".to_string());

    store.save_registry(&registry).unwrap();
    let loaded_registry = store.load_registry().unwrap();
    assert_eq!(loaded_registry, registry);

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_store_rejects_version_mismatch() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("oasis7-store-bad-{unique}"));

    let store = ModuleStore::new(&dir);
    fs::create_dir_all(store.root()).unwrap();
    let bad = json!({
        "version": 2,
        "updated_at": 0,
        "records": {},
        "active": {}
    });
    let data = serde_json::to_vec_pretty(&bad).unwrap();
    fs::write(store.registry_path(), data).unwrap();

    let err = store.load_registry().unwrap_err();
    assert!(matches!(err, WorldError::ModuleStoreVersionMismatch { .. }));

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn world_module_store_roundtrip() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("oasis7-store-world-{unique}"));

    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    let wasm_bytes = b"world-store-bytes";
    let hash = wasm_hash(wasm_bytes);
    world
        .register_module_artifact(hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.world".to_string(),
        name: "WorldStore".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(hash.as_str())),
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

    world.save_module_store_to_dir(&dir).unwrap();

    let mut restored = oasis7::runtime_host::world::new();
    restored.load_module_store_from_dir(&dir).unwrap();

    let key = ModuleRegistry::record_key("m.world", "0.1.0");
    assert!(restored.module_registry().records.contains_key(&key));
    let artifact = restored.load_module(&hash).unwrap();
    assert_eq!(artifact.bytes.as_ref(), wasm_bytes);

    let _ = fs::remove_dir_all(&dir);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn world_save_to_dir_with_modules_roundtrip() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("oasis7-store-full-{unique}"));

    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    let wasm_bytes = b"world-store-full";
    let hash = wasm_hash(wasm_bytes);
    world
        .register_module_artifact(hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.full".to_string(),
        name: "WorldFullStore".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(hash.as_str())),
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

    world.save_to_dir_with_modules(&dir).unwrap();

    let mut restored = oasis7::runtime_host::world::load_from_dir_with_modules(&dir).unwrap();
    let key = ModuleRegistry::record_key("m.full", "0.1.0");
    assert!(restored.module_registry().records.contains_key(&key));
    let artifact = restored.load_module(&hash).unwrap();
    assert_eq!(artifact.bytes.as_ref(), wasm_bytes);

    let _ = fs::remove_dir_all(&dir);
}
