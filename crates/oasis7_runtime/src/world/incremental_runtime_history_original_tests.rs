use super::*;
use oasis7_proto::memory_budget::MemoryClass;
use original::AgentEventOriginalStreamV1;
fn work() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 1 << 30,
        max_depth: 64,
        max_map_keys: 10000,
        max_key_bytes: 1024,
    }
}
#[test]
fn runtime_history_snapshot_original_uses_result_domain_and_actual_canonical_value() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let snapshot = SnapshotRecord {
        snapshot_hash: "原 snapshot".into(),
        journal_len: usize::MAX,
        created_at: u64::MAX,
        manifest_hash: "original manifest".into(),
    };
    let expected = oasis7_wasm_abi::encode_canonical_cbor(&serde_json::json!({"domain":"oasis7.runtime.history.original.v1","schema":"snapshot-record.v1","value":snapshot}))
    .unwrap();
    let binding = measure_runtime_history_original_v1(
        RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
        &memory,
        &mut work(),
        &mut || false,
        1 << 20,
    )
    .unwrap();
    assert_eq!(binding.original_id(), *blake3::hash(&expected).as_bytes());
    assert_eq!(binding.original_bytes(), expected.len() as u64);
    let mut actual = Vec::new();
    let _out = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    with_runtime_history_original_v1(
        RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
        &memory,
        &mut work(),
        &mut || false,
        1 << 20,
        |producer| {
            producer.persist_to(
                &mut |stream: &mut dyn AgentEventOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
                    stream.write_to(&mut actual)
                },
            )
        },
    )
    .unwrap();
    assert_eq!(actual, expected);
}
#[test]
fn runtime_history_original_rejects_skipped_persistence_and_cancelled_measure() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let snapshot = SnapshotRecord {
        snapshot_hash: String::new(),
        journal_len: 0,
        created_at: 0,
        manifest_hash: String::new(),
    };
    assert!(
        with_runtime_history_original_v1(
            RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
            &memory,
            &mut work(),
            &mut || false,
            1 << 20,
            |_| Ok(())
        )
        .is_err()
    );
    assert!(
        measure_runtime_history_original_v1(
            RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
            &memory,
            &mut work(),
            &mut || true,
            1 << 20
        )
        .is_err()
    );
}
#[test]
fn runtime_history_large_snapshot_string_streams_without_encoded_copy() {
    let input = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let _fixture = input.reserve(40 << 20, MemoryClass::Ordinary).unwrap();
    let snapshot = SnapshotRecord {
        snapshot_hash: "x".repeat(32 << 20),
        journal_len: 7,
        created_at: 9,
        manifest_hash: "m".into(),
    };
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let baseline = memory.used();
    let binding = measure_runtime_history_original_v1(
        RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
        &memory,
        &mut work(),
        &mut || false,
        40 << 20,
    )
    .unwrap();
    let mut hash = blake3::Hasher::new();
    struct Output<'a>(&'a mut blake3::Hasher);
    impl std::io::Write for Output<'_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    with_runtime_history_original_v1(
        RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
        &memory,
        &mut work(),
        &mut || false,
        40 << 20,
        |producer| {
            producer.persist_to(
                &mut |stream: &mut dyn AgentEventOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
                    stream.write_to(&mut Output(&mut hash))
                },
            )
        },
    )
    .unwrap();
    assert_eq!(*hash.finalize().as_bytes(), binding.original_id());
    drop(binding);
    assert_eq!(memory.used(), baseline);
}
