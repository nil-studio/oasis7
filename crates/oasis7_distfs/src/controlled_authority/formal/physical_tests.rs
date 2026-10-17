use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "oasis7-physical-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        fs::write(p.join("formal-checkpoint-v1.cbor"), b"checkpoint").unwrap();
        let objects = p.join("formal-objects-v1");
        fs::create_dir(&objects).unwrap();
        initialize(&objects).unwrap();
        Self(objects)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0.parent().unwrap());
    }
}
pub(super) fn object(bytes: Vec<u8>) -> (String, Vec<u8>) {
    (blake3::hash(&bytes).to_hex().to_string(), bytes)
}
#[test]
fn exact_bytes_and_cross_frame_range_survive_restart() {
    let f = Fixture::new();
    let o = object((0..200000).map(|n| (n % 251) as u8).collect());
    assert!(persist(&f.0, std::slice::from_ref(&o)).unwrap());
    assert_eq!(
        read_object(&f.0.join(&o.0), &o.0, true).unwrap().unwrap(),
        o.1
    );
    for (offset, limit) in [(0, 1), (65520, 120), (180000, 65536), (200000, 1)] {
        let (len, range) = read_range(&f.0.join(&o.0), &o.0, offset, limit)
            .unwrap()
            .unwrap();
        assert_eq!(len, 200000);
        assert_eq!(
            range,
            o.1[offset as usize..(offset as usize + limit as usize).min(o.1.len())]
        );
    }
    assert!(read_range(&f.0.join(&o.0), &o.0, 200001, 1).is_err());
}
#[test]
fn pack_aliases_share_recipe_and_frames_and_corruption_never_falls_back() {
    let f = Fixture::new();
    let mut objects = (0..128)
        .map(|i| object(format!("{i:05}{}", "x".repeat(1024)).into_bytes()))
        .collect::<Vec<_>>();
    objects.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(persist(&f.0, &objects).unwrap());
    for (id, bytes) in &objects {
        assert_eq!(
            super::super::business_pack::read_object(&f.0.join(id), id)
                .unwrap()
                .as_ref(),
            Some(bytes)
        );
    }
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(f.0.join(SELECTORS).join(&objects[0].0))
            .unwrap()
            .ino(),
        fs::metadata(f.0.join(SELECTORS).join(&objects[1].0))
            .unwrap()
            .ino()
    );
    let (_, recipe) = selector(&f.0.join(&objects[0].0), &objects[0].0)
        .unwrap()
        .unwrap();
    fs::write(f.0.join(FRAMES).join(&recipe.frames[0].stored), b"broken").unwrap();
    assert!(read_object(&f.0.join(&objects[0].0), &objects[0].0, false).is_err());
}
#[test]
fn explicit_selector_avoids_raw_magic_ambiguity_and_rejects_alias_substitution() {
    let f = Fixture::new();
    let raw = object(b"oasis7.formal.physical.v1\nnot metadata".to_vec());
    fs::write(f.0.join(&raw.0), &raw.1).unwrap();
    assert_eq!(
        super::super::business_pack::read_object(&f.0.join(&raw.0), &raw.0)
            .unwrap()
            .unwrap(),
        raw.1
    );
    let o = object(vec![2; 100000]);
    assert!(persist(&f.0, std::slice::from_ref(&o)).unwrap());
    assert!(!f.0.join(&o.0).exists());
    fs::write(f.0.join(&o.0), &o.1).unwrap();
    assert!(read_object(&f.0.join(&o.0), &o.0, false).is_err());
}
#[test]
fn small_raw_and_duplicate_frames_do_not_grow_redundant_payload() {
    let f = Fixture::new();
    let small = object(vec![4; 100]);
    assert!(!persist(&f.0, &[small]).unwrap());
    let a = object(vec![7; 200000]);
    assert!(persist(&f.0, &[a]).unwrap());
    let before = fs::read_dir(f.0.join(FRAMES)).unwrap().count();
    let mut b = vec![7; 200000];
    b.extend_from_slice(b"tail");
    let b = object(b);
    assert!(persist(&f.0, std::slice::from_ref(&b)).unwrap());
    assert!(fs::read_dir(f.0.join(FRAMES)).unwrap().count() <= before + 1);
    assert_eq!(
        read_object(&f.0.join(&b.0), &b.0, false).unwrap().unwrap(),
        b.1
    );
}

#[test]
fn private_frame_selector_and_alias_interruptions_recover_exact_batch_on_reopen_retry() {
    for stage in 1..=3 {
        let fixture = Fixture::new();
        let _surviving_reader = pin(&fixture.0, false).unwrap();
        let mut objects = vec![object(vec![31; 100000]), object(vec![32; 100000])];
        objects.sort_by(|a, b| a.0.cmp(&b.0));
        publication::fail_at(stage);
        assert!(persist(&fixture.0, &objects).is_err());
        initialize(&fixture.0).unwrap();
        // Retry can be a strict subset of the interrupted batch.
        assert!(persist(&fixture.0, &objects[..1]).unwrap());
        if stage == 1 {
            assert!(persist(&fixture.0, &objects[1..]).unwrap());
        }
        for (id, bytes) in &objects {
            assert_eq!(
                super::super::business_pack::read_object(&fixture.0.join(id), id)
                    .unwrap()
                    .as_ref(),
                Some(bytes)
            );
        }
    }
}

#[test]
fn physical_recipe_descriptor_bitrot_rejects_range_without_full_logical_decode() {
    let f = Fixture::new();
    let value = object(vec![73; 200000]);
    assert!(persist(&f.0, std::slice::from_ref(&value)).unwrap());
    let selected = f.0.join(SELECTORS).join(&value.0);
    let mut bytes = fs::read(&selected).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&selected, bytes).unwrap();
    assert!(read_range(&f.0.join(&value.0), &value.0, 0, 1).is_err());
    assert!(read_object(&f.0.join(&value.0), &value.0, false).is_err());
}

#[test]
fn enabled_store_never_recreates_missing_fixed_fence_or_retention_head() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.0.join(GUARD)).unwrap();
    assert!(initialize(&fixture.0).is_err());
    assert!(!fixture.0.join(GUARD).exists());
    let fixture = Fixture::new();
    fs::remove_file(fixture.0.join(PUBLICATION_GUARD)).unwrap();
    assert!(initialize(&fixture.0).is_err());
    assert!(!fixture.0.join(PUBLICATION_GUARD).exists());
    let fixture = Fixture::new();
    fs::remove_file(fixture.0.join(".retained-logical-v1").join("head.cbor")).unwrap();
    assert!(initialize(&fixture.0).is_err());
    assert!(
        !fixture
            .0
            .join(".retained-logical-v1")
            .join("head.cbor")
            .exists()
    );
}

#[test]
fn independent_readback_amortizes_recipe_sync_but_never_caches_logical_verification() {
    let fixture = Fixture::new();
    let mut values = vec![object(vec![91; 100000]), object(vec![92; 100000])];
    values.sort_by(|a, b| a.0.cmp(&b.0));
    publication::reset_recipe_syncs();
    persist(&fixture.0, &values).unwrap();
    assert_eq!(
        publication::recipe_syncs(),
        1,
        "persist syncs the shared recipe once while reading every object"
    );
    let markers = fixture.0.parent().unwrap().join("sync-markers");
    fs::create_dir(&markers).unwrap();
    for (id, bytes) in &values {
        assert_eq!(
            super::super::business_pack::read_object_durable_once(
                &fixture.0.join(id),
                id,
                &markers
            )
            .unwrap()
            .as_ref(),
            Some(bytes)
        );
    }
    assert_eq!(fs::read_dir(&markers).unwrap().count(), 1);
    let (_, recipe) = selector(&fixture.0.join(&values[0].0), &values[0].0)
        .unwrap()
        .unwrap();
    fs::write(
        fixture.0.join(FRAMES).join(&recipe.frames[0].stored),
        b"corrupt",
    )
    .unwrap();
    assert!(
        super::super::business_pack::read_object_durable_once(
            &fixture.0.join(&values[0].0),
            &values[0].0,
            &markers
        )
        .is_err()
    );
    assert!(
        persist(&fixture.0, &values).is_err(),
        "new publication cannot reuse prior readback evidence after corruption"
    );
}

#[test]
fn long_reader_allows_serialized_immutable_append_but_fences_retirement() {
    let fixture = Fixture::new();
    let reader = pin(&fixture.0, false).unwrap();
    let value = object(vec![101; 100000]);
    let writer = publication_guard(&fixture.0).unwrap();
    assert!(
        publication_guard(&fixture.0).is_err(),
        "two publishers must not overlap"
    );
    assert!(pin(&fixture.0, true).is_err());
    assert!(persist_guarded(&fixture.0, std::slice::from_ref(&value), &writer).unwrap());
    drop(writer);
    // The legacy/raw fallback and retention ledger use the SAME writer fence.
    let raw = object(vec![102; 100]);
    super::super::business_pack::persist(
        &fixture.0,
        &[super::super::business_index::BusinessIndexNodeWriteV1 {
            digest: raw.0.clone(),
            bytes: raw.1.clone(),
        }],
    )
    .unwrap();
    assert_eq!(
        super::super::business_pack::read_object(&fixture.0.join(&raw.0), &raw.0).unwrap(),
        Some(raw.1)
    );
    initialize(&fixture.0).unwrap();
    assert!(pin(&fixture.0, true).is_err());
    drop(reader);
    assert!(pin(&fixture.0, true).is_ok());
}

#[test]
fn parsed_recipe_cache_rechecks_current_bytes_and_touched_frames_after_same_inode_mutation() {
    let fixture = Fixture::new();
    let mut values = vec![object(vec![71; 100000]), object(vec![72; 100000])];
    values.sort_by(|a, b| a.0.cmp(&b.0));
    persist(&fixture.0, &values).unwrap();
    for (id, bytes) in &values {
        assert_eq!(
            read_object(&fixture.0.join(id), id, false)
                .unwrap()
                .as_ref(),
            Some(bytes)
        );
    }
    let selected = fixture.0.join(SELECTORS).join(&values[0].0);
    let mut encoded = read(&selected, META_LIMIT).unwrap();
    let original = encoded.clone();
    let index = encoded.len() - 1;
    encoded[index] ^= 1;
    let mut file = OpenOptions::new().write(true).open(&selected).unwrap();
    file.write_all(&encoded).unwrap();
    file.sync_all().unwrap();
    assert!(read_object(&fixture.0.join(&values[0].0), &values[0].0, false).is_err());
    drop(file);
    fs::write(&selected, &original).unwrap();
    let (_, parsed) = selector(&fixture.0.join(&values[0].0), &values[0].0)
        .unwrap()
        .unwrap();
    let frame = fixture.0.join(FRAMES).join(&parsed.frames[0].stored);
    let mut bytes = fs::read(&frame).unwrap();
    bytes[0] ^= 1;
    fs::write(frame, bytes).unwrap();
    assert!(read_object(&fixture.0.join(&values[0].0), &values[0].0, false).is_err());
}
