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

#[test]
fn local_native_closure_requires_original_chunks_and_combined_material_budget() {
    use oasis7_formal_protocol::native_environment::{
        NativeEnvironmentChunkV1, NativeEnvironmentComponentV1, NativeEnvironmentRole,
        NativeExecutionEnvironmentV1,
    };
    let (_, _, _, mut material, _) = full_sync_fixture();
    let executable = b"original fixture executable bytes";
    let metadata = b"original fixture build metadata";
    let component = |name: &str, role, bytes: &[u8]| NativeEnvironmentComponentV1 {
        role,
        name: name.into(),
        byte_len: bytes.len() as u64,
        content_digest: blake3_hex(bytes),
        chunks: vec![NativeEnvironmentChunkV1 {
            digest: blake3_hex(bytes),
            byte_len: bytes.len() as u32,
        }],
    };
    let environment = NativeExecutionEnvironmentV1 {
        schema_version: 1,
        target: "fixture-native".into(),
        os: "fixture".into(),
        arch: "fixture".into(),
        components: vec![
            component("runtime", NativeEnvironmentRole::Executable, executable),
            component("build", NativeEnvironmentRole::BuildMetadata, metadata),
        ],
    };
    let descriptor = environment.encode().unwrap();
    let digest = blake3_hex(&descriptor);
    material
        .roots
        .insert(ArtifactRole::NativeExecutionEnvironment, digest.clone());
    material.objects.push(ArtifactObject {
        content_hash: digest,
        bytes: descriptor,
        references: vec![],
    });
    let limit = material
        .objects
        .iter()
        .map(|o| o.bytes.len() as u64)
        .sum::<u64>()
        + executable.len() as u64
        + metadata.len() as u64;
    let directory = TestPrivateDir::new();
    let store = LocalCasStore::new(directory.0.join("cas"));
    for object in &material.objects {
        store.put(&object.content_hash, &object.bytes).unwrap();
    }
    let verify = |max| {
        local_closure::verify_native(
            &store,
            &material,
            max,
            Instant::now() + Duration::from_secs(10),
        )
    };
    assert!(verify(limit).is_err());
    for bytes in [executable.as_slice(), metadata.as_slice()] {
        store.put(&blake3_hex(bytes), bytes).unwrap();
    }
    verify(limit).unwrap();
    assert!(verify(limit - 1).is_err());
    assert!(local_closure::verify_native(&store, &material, limit, Instant::now()).is_err());
    let path = store
        .blobs_dir()
        .join(format!("{}.blob", blake3_hex(executable)));
    std::fs::write(&path, b"changed executable bytes").unwrap();
    assert!(verify(limit).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"changed executable bytes");
    assert!(!store.pins_path().exists());
    assert!(!store.root().join("pin_scopes").exists());
}

#[test]
fn local_retained_readback_checks_original_path_count_and_disk_bytes() {
    use oasis7_distfs::controlled_authority::formal::business_index::{
        BusinessIndexLeafKindV1, BusinessIndexLeafV1,
    };
    let directory = TestPrivateDir::new();
    let store = LocalCasStore::new(directory.0.join("cas"));
    let deadline = || Instant::now() + Duration::from_secs(10);
    let leaf = BusinessIndexNodeV1::leaf(BusinessIndexLeafV1 {
        key_digest: "00".repeat(32),
        kind: BusinessIndexLeafKindV1::Outcome,
        canonical_value: vec![0xf6],
    })
    .unwrap();
    let leaf_bytes = leaf.encode().unwrap();
    let leaf_digest = leaf.digest().unwrap();
    let mut digest = leaf_digest.clone();
    let mut total = leaf_bytes.len() as u64;
    for depth in (0..256).rev() {
        let node = BusinessIndexNodeV1::branch(
            depth,
            digest,
            BusinessIndexRootV1::empty_subtree_hash(depth + 1).unwrap(),
        )
        .unwrap();
        let bytes = node.encode().unwrap();
        digest = node.digest().unwrap();
        store.put(&digest, &bytes).unwrap();
        total += bytes.len() as u64;
    }
    let root = BusinessIndexRootV1 {
        node_root: digest,
        leaf_count: 1,
    };
    assert!(local_closure::verify_retained(&store, &root, total, deadline()).is_err());
    store.put(&leaf_digest, &leaf_bytes).unwrap();
    local_closure::verify_retained(&store, &root, total, deadline()).unwrap();
    assert!(local_closure::verify_retained(&store, &root, total - 1, deadline()).is_err());
    assert!(local_closure::verify_retained(&store, &root, total, Instant::now()).is_err());
    let wrong_count = BusinessIndexRootV1 {
        leaf_count: 2,
        ..root.clone()
    };
    assert!(local_closure::verify_retained(&store, &wrong_count, total, deadline()).is_err());
    let wrong_depth = BusinessIndexNodeV1::branch(
        1,
        root.node_root.clone(),
        BusinessIndexRootV1::empty_subtree_hash(2).unwrap(),
    )
    .unwrap();
    store
        .put(
            &wrong_depth.digest().unwrap(),
            &wrong_depth.encode().unwrap(),
        )
        .unwrap();
    let bad = BusinessIndexRootV1 {
        node_root: wrong_depth.digest().unwrap(),
        leaf_count: 1,
    };
    assert!(local_closure::verify_retained(&store, &bad, total, deadline()).is_err());
    let path = store.blobs_dir().join(format!("{leaf_digest}.blob"));
    std::fs::write(&path, b"corrupt retained original").unwrap();
    assert!(local_closure::verify_retained(&store, &root, total, deadline()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt retained original");
    assert!(!store.root().join("pins.json").exists());
    assert!(!store.root().join("pin_scopes").exists());
}
