use super::*;
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-legacy-bounded-{}",
            fencing::fresh_challenge().unwrap()
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn memory() -> MemoryBudget {
    MemoryBudget::new(16 << 20, 0).unwrap()
}
fn work() -> LegacyReadWorkV1 {
    LegacyReadWorkV1 {
        remaining_steps: 1_000_000,
        remaining_read_bytes: 64 << 20,
    }
}
fn raw(f: &Fixture, bytes: &[u8]) -> (String, std::path::PathBuf) {
    let id = blake3::hash(bytes).to_hex().to_string();
    let path = f.0.join(&id);
    fs::write(&path, bytes).unwrap();
    (id, path)
}
fn apparent_pack() -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    let index = HEADER as u64 + 3;
    let total = index + INDEX_ENTRY;
    bytes.extend_from_slice(&index.to_be_bytes());
    bytes.extend_from_slice(&total.to_be_bytes());
    bytes.extend_from_slice(b"abc");
    bytes.extend_from_slice(&[0; 32]);
    bytes.extend_from_slice(&(HEADER as u64).to_be_bytes());
    bytes.extend_from_slice(&3u32.to_be_bytes());
    bytes
}
#[test]
fn raw_magic_truncated_and_apparent_pack_keep_original_wire_hash_and_fee() {
    for bytes in [
        Vec::new(),
        b"ordinary raw bytes".to_vec(),
        MAGIC.to_vec(),
        [MAGIC, b"truncated"].concat(),
        apparent_pack(),
    ] {
        let f = Fixture::new();
        let (id, path) = raw(&f, &bytes);
        let memory = memory();
        let mut w = work();
        let out = read_legacy_unknown_length_durable_v1(
            &path,
            &id,
            bytes.len(),
            &memory,
            &mut w,
            &mut || false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(&**out, &bytes);
        assert_eq!(blake3::hash(&out).to_hex().as_str(), id);
        assert!(out.uses_budget(&memory));
        assert_eq!(memory.used(), out.reserved_bytes());
        drop(out);
        assert_eq!(memory.used(), 0);
        if !bytes.is_empty() {
            assert!(
                read_legacy_unknown_length_durable_v1(
                    &path,
                    &id,
                    bytes.len() - 1,
                    &memory,
                    &mut work(),
                    &mut || false
                )
                .is_err()
            );
            assert_eq!(memory.used(), 0);
        }
    }
}
#[test]
fn actual_pack_small_member_is_not_rejected_by_whole_pack_size() {
    let f = Fixture::new();
    let mut writes = [b"tiny selected member".to_vec(), vec![89; 200_000]]
        .into_iter()
        .map(|bytes| BusinessIndexNodeWriteV1 {
            digest: blake3::hash(&bytes).to_hex().to_string(),
            bytes,
        })
        .collect::<Vec<_>>();
    writes.sort_by(|a, b| a.digest.cmp(&b.digest));
    persist(&f.0, &writes).unwrap();
    let selected = writes.iter().find(|v| v.bytes.len() < 100).unwrap();
    let path = f.0.join(&selected.digest);
    assert!(fs::metadata(&path).unwrap().len() > selected.bytes.len() as u64);
    let memory = memory();
    let mut w = work();
    let out = read_legacy_unknown_length_durable_v1(
        &path,
        &selected.digest,
        selected.bytes.len(),
        &memory,
        &mut w,
        &mut || false,
    )
    .unwrap()
    .unwrap();
    assert_eq!(&**out, &selected.bytes);
    assert!(
        64 * 1024 * 1024 - w.remaining_read_bytes < 4096,
        "binary lookup must not read unrelated payload"
    );
    drop(out);
    assert_eq!(memory.used(), 0);
    assert!(
        read_legacy_unknown_length_durable_v1(
            &path,
            &selected.digest,
            selected.bytes.len() - 1,
            &memory,
            &mut work(),
            &mut || false
        )
        .is_err()
    );
}
#[test]
fn budget_and_cancel_first_causes_cannot_be_swallowed_by_raw_fallback() {
    let f = Fixture::new();
    let (id, path) = raw(&f, MAGIC);
    let memory = memory();
    // Prefix succeeds, the next header attempt fails on host IO budget. It must
    // not be treated as compatible header EOF and restart as a raw hash.
    let mut w = work();
    w.remaining_read_bytes = MAGIC.len() as u64;
    let error = read_legacy_unknown_length_durable_v1(
        &path,
        &id,
        MAGIC.len(),
        &memory,
        &mut w,
        &mut || false,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("bounded legacy IO bytes"),
        "{error}"
    );
    assert_eq!(memory.used(), 0);
    let mut w = work();
    let mut seen = 0;
    let error = read_legacy_unknown_length_durable_v1(
        &path,
        &id,
        MAGIC.len(),
        &memory,
        &mut w,
        &mut || {
            seen += 1;
            seen >= 4
        },
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("bounded legacy read cancelled"),
        "{error}"
    );
    assert_eq!(memory.used(), 0);
}
#[test]
fn real_missing_is_none_and_present_corrupt_content_is_error_on_next_call() {
    let f = Fixture::new();
    let memory = memory();
    let missing = "00".repeat(32);
    assert!(
        read_legacy_unknown_length_durable_v1(
            &f.0.join(&missing),
            &missing,
            100,
            &memory,
            &mut work(),
            &mut || false
        )
        .unwrap()
        .is_none()
    );
    let (id, path) = raw(&f, b"original bytes");
    drop(
        read_legacy_unknown_length_durable_v1(&path, &id, 100, &memory, &mut work(), &mut || false)
            .unwrap()
            .unwrap(),
    );
    fs::write(&path, b"corrupt! bytes").unwrap();
    assert!(
        read_legacy_unknown_length_durable_v1(&path, &id, 100, &memory, &mut work(), &mut || false)
            .is_err()
    );
    assert_eq!(memory.used(), 0);
}
#[test]
fn actual_pack_payload_and_metadata_corruption_are_not_absence() {
    for metadata in [false, true] {
        let f = Fixture::new();
        let bytes = b"actual selected payload".to_vec();
        let id = blake3::hash(&bytes).to_hex().to_string();
        persist(
            &f.0,
            &[BusinessIndexNodeWriteV1 {
                digest: id.clone(),
                bytes: bytes.clone(),
            }],
        )
        .unwrap();
        let path = f.0.join(&id);
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(if metadata {
            MAGIC.len() as u64
        } else {
            HEADER as u64
        }))
        .unwrap();
        file.write_all(&[255]).unwrap();
        file.sync_all().unwrap();
        let memory = memory();
        assert!(
            read_legacy_unknown_length_durable_v1(
                &path,
                &id,
                bytes.len(),
                &memory,
                &mut work(),
                &mut || false
            )
            .is_err()
        );
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn memory_and_continuous_work_are_admitted_and_released() {
    let f = Fixture::new();
    let (id, path) = raw(&f, b"bounded data");
    for mode in 0..3 {
        let memory = if mode == 2 {
            MemoryBudget::new(0, 0).unwrap()
        } else {
            memory()
        };
        let mut w = work();
        if mode == 0 {
            w.remaining_steps = 0;
        }
        if mode == 1 {
            w.remaining_read_bytes = 0;
        }
        assert!(
            read_legacy_unknown_length_durable_v1(&path, &id, 100, &memory, &mut w, &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
    }
    let memory = memory();
    let mut w = work();
    let before = w.remaining_read_bytes;
    drop(
        read_legacy_unknown_length_durable_v1(&path, &id, 100, &memory, &mut w, &mut || false)
            .unwrap()
            .unwrap(),
    );
    let spent = before - w.remaining_read_bytes;
    assert!(spent > 0);
    w.remaining_read_bytes = spent - 1;
    assert!(
        read_legacy_unknown_length_durable_v1(&path, &id, 100, &memory, &mut w, &mut || false)
            .is_err()
    );
    assert_eq!(w.remaining_read_bytes, 0);
    assert_eq!(memory.used(), 0);
}
