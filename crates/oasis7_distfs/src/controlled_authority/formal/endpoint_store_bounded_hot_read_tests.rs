use super::super::super::tests::{
    NativeDraftFixtureV1, with_history_read_fixture, with_native_draft_fixture,
};
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(8_000_000, 512 << 20).unwrap()
}
fn key() -> String {
    hex::encode(SigningKey::from_bytes(&[5; 32]).verifying_key().to_bytes())
}
fn compare(store: &Store) {
    let before = store.checkpoint_bytes().unwrap();
    let old = Store::verify_public(&before, &key()).unwrap();
    let memory = memory();
    let meter = work();
    let paid = store
        .read_hot_bounded(&key(), &memory, &meter, &mut || false)
        .unwrap();
    assert_eq!(paid.bytes(), before);
    assert_eq!(paid.body(), &old);
    assert!(paid.uses_budget(&memory));
    assert_eq!(paid.reserved_bytes(), memory.used());
    assert_eq!(store.checkpoint_bytes().unwrap(), before);
    assert_eq!(store.body, old);
    drop(paid);
    assert_eq!(memory.used(), 0);
}
#[test]
fn actual_plain_qualified_and_sealed_disk_hot_match_original_verifier() {
    for mode in [NativeDraftFixtureV1::Plain, NativeDraftFixtureV1::Sealed] {
        with_native_draft_fixture(mode, |store, _| compare(store));
    }
    with_history_read_fixture(|store, _, _| compare(store));
}
#[test]
fn old_sequence_unknown_fields_and_nonminimal_hot_wire_remain_accepted() {
    with_native_draft_fixture(NativeDraftFixtureV1::Plain, |store, _| {
        let original = store.checkpoint_bytes().unwrap();
        let hot: Hot = decode(&original).unwrap();
        let sequence = serde_cbor::to_vec(&(&hot.body, &hot.signature)).unwrap();
        let mut unknown = serde_cbor::value::to_value(&hot).unwrap();
        let serde_cbor::Value::Map(ref mut map) = unknown else {
            panic!("original Hot map")
        };
        map.insert(
            serde_cbor::Value::Text("unused_future_field".into()),
            serde_cbor::Value::Array(vec![serde_cbor::Value::Text(
                "ignored original-compatible data".into(),
            )]),
        );
        let unknown = serde_cbor::to_vec(&unknown).unwrap();
        // Definite root map count uses an intentionally nonminimal uint8 header.
        assert_eq!(original[0], 0xa2);
        let mut nonminimal = vec![0xb8, 2];
        nonminimal.extend_from_slice(&original[1..]);
        for bytes in [sequence, unknown, nonminimal] {
            let expected = Store::verify_public(&bytes, &key())
                .expect("old codec accepts this representation");
            fs::write(store.path.join(HOT), &bytes).unwrap();
            let memory = memory();
            let paid = store
                .read_hot_bounded(&key(), &memory, &work(), &mut || false)
                .unwrap();
            assert_eq!(paid.body(), &expected);
            assert_eq!(paid.bytes(), bytes);
            drop(paid);
            assert_eq!(memory.used(), 0);
        }
        fs::write(store.path.join(HOT), original).unwrap();
    });
}
#[test]
fn malformed_wire_and_crypto_first_error_match_old_public_verifier() {
    with_native_draft_fixture(NativeDraftFixtureV1::Plain, |store, _| {
        let original = store.checkpoint_bytes().unwrap();
        let mut bad: Hot = decode(&original).unwrap();
        bad.signature = "invalid".into();
        let mut trailing = original.clone();
        trailing.push(0);
        let cases = [
            (vec![0xff], "bad".to_string()),
            (trailing, key()),
            (encode(&bad).unwrap(), "bad".to_string()),
            (encode(&bad).unwrap(), key()),
        ];
        for (bytes, public) in cases {
            let old = Store::verify_public(&bytes, &public).unwrap_err();
            fs::write(store.path.join(HOT), &bytes).unwrap();
            let memory = memory();
            let new = store
                .read_hot_bounded(&public, &memory, &work(), &mut || false)
                .unwrap_err();
            assert_eq!(std::mem::discriminant(&new), std::mem::discriminant(&old));
            if bytes.first() != Some(&0xff) && bytes.last() != Some(&0) {
                assert_eq!(new.to_string(), old.to_string());
            }
            assert_eq!(memory.used(), 0);
        }
        fs::write(store.path.join(HOT), original).unwrap();
    });
}
#[test]
fn zero_memory_steps_and_cancelled_disk_reads_never_return_a_paid_owner() {
    with_native_draft_fixture(NativeDraftFixtureV1::Plain, |store, _| {
        let original = store.checkpoint_bytes().unwrap();
        let zero = MemoryBudget::new(0, 0).unwrap();
        assert!(
            store
                .read_hot_bounded(&key(), &zero, &work(), &mut || false)
                .is_err()
        );
        assert_eq!(zero.used(), 0);
        let memory = memory();
        let meter = FormalArchiveReadWorkV1::new(0, 512 << 20).unwrap();
        let error = store
            .read_hot_bounded(&key(), &memory, &meter, &mut || false)
            .unwrap_err();
        assert!(error.to_string().contains("steps"));
        let error = store
            .read_hot_bounded(&key(), &memory, &work(), &mut || true)
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert_eq!(memory.used(), 0);
        assert_eq!(store.checkpoint_bytes().unwrap(), original);
    });
}
#[cfg(unix)]
#[test]
fn symlink_and_fifo_hot_are_rejected_without_following_or_blocking() {
    use std::os::unix::{ffi::OsStrExt, fs::symlink};
    with_native_draft_fixture(NativeDraftFixtureV1::Plain, |store, _| {
        let path = store.path.join(HOT);
        let original = fs::read(&path).unwrap();
        let target = store.path.join("unselected-hot-data");
        fs::write(&target, &original).unwrap();
        fs::remove_file(&path).unwrap();
        symlink(&target, &path).unwrap();
        let memory = memory();
        assert!(
            store
                .read_hot_bounded(&key(), &memory, &work(), &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
        fs::remove_file(&path).unwrap();
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(
            store
                .read_hot_bounded(&key(), &memory, &work(), &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
        fs::remove_file(&path).unwrap();
        fs::write(path, original).unwrap();
    });
}
