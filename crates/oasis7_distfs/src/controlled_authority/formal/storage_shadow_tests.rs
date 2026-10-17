use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: std::path::PathBuf,
    source: std::path::PathBuf,
    target: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "o7-shadow-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let source = root.join("endpoint");
        fs::create_dir_all(source.join("formal-objects-v1")).unwrap();
        File::create(source.join(".formal-endpoint.lock")).unwrap();
        Self {
            target: root.join("shadow"),
            root,
            source,
        }
    }
    fn raw(&self, bytes: &[u8]) -> String {
        let id = blake3::hash(bytes).to_hex().to_string();
        fs::write(self.source.join("formal-objects-v1").join(&id), bytes).unwrap();
        id
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn shadow_packs_small_objects_with_exact_bytes_and_logical_ranges() {
    let fixture = Fixture::new();
    let mut expected = Vec::new();
    for value in 0..128u64 {
        let bytes = serde_cbor::to_vec(&(value, "unchanged formal metadata")).unwrap();
        let id = fixture.raw(&bytes);
        expected.push((id, bytes));
    }
    // Existing source packs must be resolved, not copied as raw object bytes.
    let bytes = b"already packed object".to_vec();
    let id = blake3::hash(&bytes).to_hex().to_string();
    business_pack::persist(
        &fixture.source.join("formal-objects-v1"),
        &[BusinessIndexNodeWriteV1 {
            digest: id.clone(),
            bytes: bytes.clone(),
        }],
    )
    .unwrap();
    expected.push((id, bytes));
    let report = build_formal_storage_shadow(&fixture.source, &fixture.target).unwrap();
    assert_eq!(report.verified_objects, expected.len());
    assert_eq!(
        report.logical_bytes,
        expected
            .iter()
            .map(|(_, bytes)| bytes.len() as u64)
            .sum::<u64>()
    );
    assert!(report.shadow_objects.unique_files < report.source_objects.unique_files);
    eprintln!(
        "shadow fixture: {}",
        serde_json::to_string(&report).unwrap()
    );
    for (id, bytes) in expected {
        let source = fixture.source.join("formal-objects-v1").join(&id);
        let target = fixture.target.join(&id);
        assert_eq!(
            business_pack::read_object(&source, &id).unwrap().unwrap(),
            bytes
        );
        assert_eq!(
            business_pack::read_object(&target, &id).unwrap().unwrap(),
            bytes
        );
        let (length, part) = business_pack::read_range(&target, &id, 1, 3)
            .unwrap()
            .unwrap();
        assert_eq!(length, bytes.len() as u64);
        assert_eq!(part, bytes[1..4]);
    }
    assert!(fixture.target.join(SHADOW_MARKER).is_file());
    assert!(!fixture.target.join("formal-checkpoint-v1.cbor").exists());
}
#[test]
fn shadow_preserves_empty_and_large_raw_objects() {
    let fixture = Fixture::new();
    let empty = fixture.raw(&[]);
    let bytes = vec![7; business_pack::MAX_PACK_NODE as usize + 1];
    let large = fixture.raw(&bytes);
    let report = build_formal_storage_shadow(&fixture.source, &fixture.target).unwrap();
    assert_eq!(report.verified_objects, 2);
    assert_eq!(
        fs::read(fixture.target.join(empty)).unwrap(),
        Vec::<u8>::new()
    );
    assert_eq!(fs::read(fixture.target.join(large)).unwrap(), bytes);
}
#[test]
fn shadow_refuses_live_source_nested_output_and_existing_output() {
    let fixture = Fixture::new();
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.source.join(".formal-endpoint.lock"))
        .unwrap();
    lease.try_lock().unwrap();
    assert!(matches!(
        build_formal_storage_shadow(&fixture.source, &fixture.target),
        Err(FormalError::Conflict(_))
    ));
    assert!(!fixture.target.exists());
    drop(lease);
    let nested = fixture.source.join("shadow");
    assert!(build_formal_storage_shadow(&fixture.source, &nested).is_err());
    assert!(!nested.exists());
    fs::create_dir(&fixture.target).unwrap();
    fs::write(fixture.target.join("sentinel"), b"keep").unwrap();
    assert!(build_formal_storage_shadow(&fixture.source, &fixture.target).is_err());
    assert_eq!(fs::read(fixture.target.join("sentinel")).unwrap(), b"keep");
}
#[test]
fn shadow_budget_and_corruption_leave_source_and_marked_partial_output() {
    let fixture = Fixture::new();
    let bytes = b"original immutable bytes";
    let id = fixture.raw(bytes);
    assert!(build_with_limit(&fixture.source, &fixture.target, 1).is_err());
    assert_eq!(
        fs::read(fixture.source.join("formal-objects-v1").join(&id)).unwrap(),
        bytes
    );
    assert!(fixture.target.join(SHADOW_MARKER).is_file());
    let second = fixture.root.join("second-shadow");
    fs::write(
        fixture.source.join("formal-objects-v1").join(&id),
        b"corruption",
    )
    .unwrap();
    assert!(build_formal_storage_shadow(&fixture.source, &second).is_err());
    assert_eq!(
        fs::read(fixture.source.join("formal-objects-v1").join(&id)).unwrap(),
        b"corruption"
    );
    assert!(second.join(SHADOW_MARKER).is_file());
}
#[test]
fn shadow_rejects_unknown_or_pending_material_without_removing_it() {
    let fixture = Fixture::new();
    let staging = fixture
        .source
        .join("formal-objects-v1/.business-pack-staging-v1");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("pending"), b"unique pending bytes").unwrap();
    assert!(build_formal_storage_shadow(&fixture.source, &fixture.target).is_err());
    assert!(!fixture.target.exists());
    assert_eq!(
        fs::read(staging.join("pending")).unwrap(),
        b"unique pending bytes"
    );
    fs::remove_file(staging.join("pending")).unwrap();
    fs::write(fixture.source.join("formal-objects-v1/unknown"), b"keep").unwrap();
    assert!(build_formal_storage_shadow(&fixture.source, &fixture.target).is_err());
    assert!(!fixture.target.exists());
}

#[test]
fn shadow_bounded_batches_keep_all_objects_across_multiple_packs() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    let mut ids = Vec::new();
    for value in 0..3u8 {
        let bytes = vec![value; 3 * 1024 * 1024];
        ids.push((fixture.raw(&bytes), bytes));
    }
    let report = build_formal_storage_shadow(&fixture.source, &fixture.target).unwrap();
    assert_eq!(report.verified_objects, 3);
    let mut inodes = std::collections::BTreeSet::new();
    for (id, original) in ids {
        let path = fixture.target.join(&id);
        let metadata = fs::metadata(&path).unwrap();
        assert!(metadata.len() <= BATCH_BYTES as u64 + 1024);
        inodes.insert((metadata.dev(), metadata.ino()));
        assert_eq!(
            business_pack::read_object(&path, &id).unwrap().unwrap(),
            original
        );
    }
    assert_eq!(inodes.len(), 2);
}

#[test]
fn streamed_shadow_codecs_preserve_bytes_source_and_shadow_refusal_marker() {
    for optimized in [false, true] {
        let fixture = Fixture::new();
        let mut expected = Vec::new();
        for value in 0u64..1100 {
            let mut bytes = vec![7; 5000];
            bytes[..8].copy_from_slice(&value.to_be_bytes());
            expected.push((fixture.raw(&bytes), bytes));
        }
        let report = build_formal_storage_shadow_with_options(
            &fixture.source,
            &fixture.target,
            &FormalStorageShadowOptions {
                optimized,
                max_logical_bytes: 8 * 1024 * 1024,
            },
        )
        .unwrap();
        assert_eq!(report.verified_objects, 1100);
        assert!(fixture.target.join(SHADOW_MARKER).is_file());
        for (id, bytes) in expected {
            assert_eq!(
                business_pack::read_object(
                    &fixture.source.join("formal-objects-v1").join(&id),
                    &id
                )
                .unwrap()
                .unwrap(),
                bytes
            );
            assert_eq!(
                business_pack::read_object(&fixture.target.join(id.clone()), &id)
                    .unwrap()
                    .unwrap(),
                bytes
            );
        }
    }
}

#[test]
fn shadow_resumes_bounded_steps_and_rejects_source_changes() {
    let fixture = Fixture::new();
    for id in 0u8..5 {
        fixture.raw(&vec![id; 10000]);
    }
    let options = FormalStorageShadowOptions {
        optimized: true,
        max_logical_bytes: 1,
    };
    let first = advance_formal_storage_shadow(&fixture.source, &fixture.target, &options).unwrap();
    assert!(!first.complete);
    assert_eq!(first.processed_records, 1);
    let mut result = None;
    // Separate copy and independent verification passes each advance one
    // object under this deliberately tiny step budget.
    for _ in 0..20 {
        let progress =
            advance_formal_storage_shadow(&fixture.source, &fixture.target, &options).unwrap();
        if progress.complete {
            result = progress.report;
            break;
        }
    }
    assert_eq!(result.unwrap().verified_objects, 5);
    assert!(
        advance_formal_storage_shadow(&fixture.source, &fixture.target, &options)
            .unwrap()
            .complete
    );
    let changed = Fixture::new();
    for id in 0u8..5 {
        changed.raw(&vec![id; 10000]);
    }
    assert!(
        !advance_formal_storage_shadow(&changed.source, &changed.target, &options)
            .unwrap()
            .complete
    );
    let extra = changed.raw(b"new object after captured immutable source cut");
    assert!(advance_formal_storage_shadow(&changed.source, &changed.target, &options).is_err());
    assert!(
        changed
            .source
            .join("formal-objects-v1")
            .join(extra)
            .exists()
    );
}
