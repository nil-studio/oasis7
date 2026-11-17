use super::*;
use oasis7_client_api::world_service_full_sync::{
    FormalFullSyncArtifactRoleV1 as Role, FormalFullSyncArtifactRootV1,
    FormalFullSyncObjectDescriptorV1,
};
use std::time::Instant;

fn manifest_for(
    anchor: &VerifiedFormalReadAnchorV1,
    material: &ClosedRecord,
) -> FormalFullSyncManifestV1 {
    let roles = [
        Role::Input,
        Role::Result,
        Role::Snapshot,
        Role::Journal,
        Role::NonceIndex,
        Role::EffectOutbox,
        Role::ExecutionManifest,
        Role::Wasm,
        Role::Rules,
        Role::NativeExecutionEnvironment,
    ];
    FormalFullSyncManifestV1 {
        schema_version: if material
            .roots
            .contains_key(&ArtifactRole::NativeExecutionEnvironment)
        {
            2
        } else {
            1
        },
        cut: cut(anchor),
        artifact_roots: roles
            .into_iter()
            .filter_map(|role| {
                material
                    .roots
                    .get(&native_role(role))
                    .map(|hash| FormalFullSyncArtifactRootV1 {
                        role,
                        content_hash: hash.clone(),
                    })
            })
            .collect(),
        objects: material
            .objects
            .iter()
            .map(|object| FormalFullSyncObjectDescriptorV1 {
                content_hash: object.content_hash.clone(),
                size_bytes: object.bytes.len() as u64,
                references: object.references.clone(),
            })
            .collect(),
        retained_history_root_digest: None,
    }
}

#[test]
fn local_record_readback_is_exact_without_pin_or_directory_mutation() {
    let (_, _, anchor, material, _) = full_sync_fixture();
    let manifest = manifest_for(&anchor, &material);
    manifest.validate().unwrap();
    let directory = TestPrivateDir::new();
    let store = LocalCasStore::new(directory.0.join("cas"));
    let read = || {
        local_closure::read_record(
            &store,
            &anchor,
            &manifest,
            MAX_FULL_SYNC_RECORD_BYTES as u64,
            Instant::now() + Duration::from_secs(10),
        )
    };
    assert!(read().is_err());
    assert!(!store.root().exists());
    for object in &material.objects {
        store.put(&object.content_hash, &object.bytes).unwrap();
    }
    let restored = read().unwrap();
    assert_eq!(restored.roots, material.roots);
    assert_eq!(restored.objects.len(), material.objects.len());
    for (actual, expected) in restored.objects.iter().zip(&material.objects) {
        assert_eq!(actual.bytes, expected.bytes);
        assert_eq!(actual.references, expected.references);
    }
    assert!(!store.pins_path().exists());
    assert!(!store.root().join("pin_scopes").exists());
    let first = &material.objects[0];
    let path = store
        .blobs_dir()
        .join(format!("{}.blob", first.content_hash));
    let original = std::fs::read(&path).unwrap();
    std::fs::write(&path, b"corrupt original").unwrap();
    assert!(read().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt original");
    std::fs::write(&path, original).unwrap();
    let mut wrong = manifest.clone();
    wrong.cut.record_id = "ab".repeat(32);
    assert!(
        local_closure::read_record(
            &store,
            &anchor,
            &wrong,
            MAX_FULL_SYNC_RECORD_BYTES as u64,
            Instant::now() + Duration::from_secs(10)
        )
        .is_err()
    );
    assert!(
        local_closure::read_record(
            &store,
            &anchor,
            &manifest,
            0,
            Instant::now() + Duration::from_secs(10)
        )
        .is_err()
    );
    assert!(
        local_closure::read_record(
            &store,
            &anchor,
            &manifest,
            MAX_FULL_SYNC_RECORD_BYTES as u64,
            Instant::now()
        )
        .is_err()
    );
    assert!(!store.pins_path().exists());
    assert!(!store.root().join("pin_scopes").exists());
}

#[test]
fn local_record_readback_bypasses_warm_compressed_cache_and_bounds_decode() {
    let (_, _, anchor, material, _) = full_sync_fixture_with_result(vec![b'x'; 4096]);
    let manifest = manifest_for(&anchor, &material);
    let directory = TestPrivateDir::new();
    let store = LocalCasStore::new(directory.0.join("cas"));
    for object in &material.objects {
        store.put(&object.content_hash, &object.bytes).unwrap();
    }
    let read = || {
        local_closure::read_record(
            &store,
            &anchor,
            &manifest,
            MAX_FULL_SYNC_RECORD_BYTES as u64,
            Instant::now() + Duration::from_secs(10),
        )
    };
    read().unwrap();
    let hash = &material.roots[&ArtifactRole::Result];
    let path = store.blobs_dir().join(format!("{hash}.blob"));
    let original = std::fs::read(&path).unwrap();
    assert!(original.starts_with(b"O7CBLOB1"));
    store.get_range(hash, 0, 4097).unwrap(); // Populate the unrelated range cache.
    let mut corrupt = original[..80].to_vec();
    corrupt.extend_from_slice(b"not a zstd stream");
    std::fs::write(&path, &corrupt).unwrap();
    assert!(read().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), corrupt);
    let oversized = vec![b'y'; 1024 * 1024];
    let oversized_hash = blake3_hex(&oversized);
    store.put(&oversized_hash, &oversized).unwrap();
    let encoded = std::fs::read(store.blobs_dir().join(format!("{oversized_hash}.blob"))).unwrap();
    assert!(encoded.starts_with(b"O7CBLOB1"));
    // Preserve the original small header but supply a much larger decoded body.
    let mut forged = original[..80].to_vec();
    forged.extend_from_slice(&encoded[80..]);
    std::fs::write(&path, &forged).unwrap();
    assert!(read().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), forged);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(read().is_err());
    assert!(path.is_dir());
    assert!(!store.pins_path().exists());
    assert!(!store.root().join("pin_scopes").exists());
}
