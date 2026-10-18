use super::super::*;
use super::World;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-module-store-load-{label}-{unique}"))
}

fn manifest(module_id: &str, wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.into(),
        name: format!("Store Load {module_id}"),
        version: "1.0.0".into(),
        kind: ModuleKind::Pure,
        role: ModuleRole::AgentInternal,
        wasm_hash: wasm_hash.into(),
        interface_version: "wasm-1".into(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".into()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(crate::tests::signed_test_artifact_identity(
            wasm_hash,
        )),
        limits: ModuleLimits::default(),
    }
}

fn install(world: &mut World, module_id: &str, bytes: &[u8]) -> String {
    let wasm_hash = crate::util::sha256_hex(bytes);
    world
        .register_module_artifact(wasm_hash.clone(), bytes)
        .expect("register fixture artifact");
    world
        .apply_module_event(
            &ModuleEvent {
                proposal_id: 0,
                kind: ModuleEventKind::RegisterModule {
                    module: manifest(module_id, &wasm_hash),
                    registered_by: "fixture".into(),
                },
            },
            world.state.time,
        )
        .expect("register fixture module");
    wasm_hash
}

fn persisted_two_record_store(label: &str) -> (PathBuf, String) {
    let mut persisted = World::new();
    install(&mut persisted, "m.store.a-valid", b"persisted-first-valid");
    let later_hash = install(
        &mut persisted,
        "m.store.z-invalid",
        b"persisted-second-invalid",
    );
    let dir = temp_dir(label);
    persisted
        .save_module_store_to_dir(&dir)
        .expect("save fixture module store");
    (dir, later_hash)
}

fn live_world() -> World {
    let mut world = World::new();
    let hash = install(&mut world, "m.live.distinct", b"live-distinct-artifact");
    world.set_module_cache_max(7);
    world
        .load_module(&hash)
        .expect("populate live module cache");
    world
}

fn assert_world_unchanged(world: &World, expected: &World) {
    assert_eq!(world.snapshot(), expected.snapshot());
    assert_eq!(world.manifest, expected.manifest);
    assert_eq!(world.module_registry, expected.module_registry);
    assert_eq!(world.module_artifacts, expected.module_artifacts);
    assert_eq!(world.module_artifact_bytes, expected.module_artifact_bytes);
    assert_eq!(world.module_cache, expected.module_cache);
    assert_eq!(world.journal, expected.journal);
    assert_eq!(world.next_event_id, expected.next_event_id);
    assert_eq!(world.next_event_id_era, expected.next_event_id_era);
    assert_eq!(
        world.tick_consensus_records(),
        expected.tick_consensus_records()
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected.runtime_backpressure_stats()
    );
}

fn assert_failed_load_preserves_live_world(dir: &Path, expected_error: impl FnOnce(&WorldError)) {
    let mut world = live_world();
    let before = world.clone();
    let error = world
        .load_module_store_from_dir(dir)
        .expect_err("later persisted record must be rejected");
    expected_error(&error);
    assert_world_unchanged(&world, &before);
}

#[test]
fn later_metadata_mismatch_preserves_existing_live_world() {
    let (dir, later_hash) = persisted_two_record_store("meta-mismatch");
    let store = ModuleStore::new(&dir);
    let mut mismatched = store.read_meta(&later_hash).expect("read later metadata");
    mismatched.name.push_str(" tampered");
    store
        .write_meta(&mismatched)
        .expect("tamper later metadata");

    assert_failed_load_preserves_live_world(&dir, |error| {
        assert!(matches!(
            error,
            WorldError::ModuleStoreManifestMismatch { wasm_hash } if wasm_hash == &later_hash
        ));
    });
    fs::remove_dir_all(dir).expect("remove fixture directory");
}

#[test]
fn later_missing_artifact_preserves_existing_live_world() {
    let (dir, later_hash) = persisted_two_record_store("missing-artifact");
    fs::remove_file(dir.join("modules").join(format!("{later_hash}.wasm")))
        .expect("remove later artifact");

    assert_failed_load_preserves_live_world(&dir, |error| {
        assert!(
            matches!(error, WorldError::Io(_)),
            "unexpected error: {error:?}"
        );
    });
    fs::remove_dir_all(dir).expect("remove fixture directory");
}

#[test]
fn later_artifact_hash_mismatch_preserves_existing_live_world() {
    let (dir, later_hash) = persisted_two_record_store("hash-mismatch");
    fs::write(
        dir.join("modules").join(format!("{later_hash}.wasm")),
        b"tampered later artifact",
    )
    .expect("tamper later artifact");

    assert_failed_load_preserves_live_world(&dir, |error| {
        assert!(matches!(
            error,
            WorldError::ModuleStoreManifestMismatch { wasm_hash } if wasm_hash == &later_hash
        ));
    });
    fs::remove_dir_all(dir).expect("remove fixture directory");
}

#[test]
fn later_invalid_identity_preserves_existing_live_world() {
    let (dir, later_hash) = persisted_two_record_store("invalid-identity");
    let store = ModuleStore::new(&dir);
    let mut registry = store.load_registry().expect("load fixture registry");
    let record = registry
        .records
        .values_mut()
        .find(|record| record.manifest.wasm_hash == later_hash)
        .expect("later registry record");
    let identity = record
        .manifest
        .artifact_identity
        .as_mut()
        .expect("fixture identity");
    identity.artifact_signature.push('0');
    store
        .write_meta(&record.manifest)
        .expect("write matching invalid metadata");
    store
        .save_registry(&registry)
        .expect("write matching invalid registry");

    assert_failed_load_preserves_live_world(&dir, |error| {
        assert!(
            matches!(
                error,
                WorldError::ModuleChangeInvalid { reason }
                    if reason == "module artifact signature must be valid hex"
            ),
            "unexpected error: {error:?}"
        );
    });
    fs::remove_dir_all(dir).expect("remove fixture directory");
}

#[test]
fn successful_load_replaces_store_authority_and_preserves_cache_policy() {
    let (dir, later_hash) = persisted_two_record_store("success");
    let store = ModuleStore::new(&dir);
    let expected_registry = store.load_registry().expect("load expected registry");
    let expected_artifacts = expected_registry
        .records
        .values()
        .map(|record| record.manifest.wasm_hash.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut world = live_world();
    let cache_before = world.module_cache.clone();

    world
        .load_module_store_from_dir(&dir)
        .expect("load valid module store");

    assert_eq!(world.module_registry, expected_registry);
    assert_eq!(world.module_artifacts, expected_artifacts);
    assert_eq!(world.module_artifact_bytes.len(), 2);
    assert_eq!(
        world.module_artifact_bytes[&later_hash].as_ref(),
        b"persisted-second-invalid"
    );
    assert_eq!(world.module_cache, cache_before);
    fs::remove_dir_all(dir).expect("remove fixture directory");
}

#[test]
fn absent_module_store_is_a_strict_no_op() {
    let dir = temp_dir("absent");
    let mut world = live_world();
    let before = world.clone();

    world
        .load_module_store_from_dir(&dir)
        .expect("absent module store remains compatible");

    assert_world_unchanged(&world, &before);
}

#[test]
fn sorted_first_invalid_record_preserves_error_priority_and_live_world() {
    let (dir, later_hash) = persisted_two_record_store("error-order");
    let store = ModuleStore::new(&dir);
    let registry = store.load_registry().expect("load fixture registry");
    let first_hash = registry
        .records
        .values()
        .next()
        .expect("first sorted record")
        .manifest
        .wasm_hash
        .clone();
    let mut first_meta = store.read_meta(&first_hash).expect("read first metadata");
    first_meta.name.push_str(" first-error");
    store
        .write_meta(&first_meta)
        .expect("invalidate first metadata");
    fs::remove_file(dir.join("modules").join(format!("{later_hash}.wasm")))
        .expect("also invalidate later artifact");

    assert_failed_load_preserves_live_world(&dir, |error| {
        assert!(matches!(
            error,
            WorldError::ModuleStoreManifestMismatch { wasm_hash } if wasm_hash == &first_hash
        ));
    });
    fs::remove_dir_all(dir).expect("remove fixture directory");
}
