use super::*;
fn labels() -> CborPreflightWorkV1 {
    CborPreflightWorkV1 {
        bytes_remaining: 1 << 20,
        values_remaining: 100_000,
        max_depth: 64,
    }
}
#[test]
fn result_label_probe_dispatches_only_the_actual_closed_envelope() {
    let memory = MemoryBudget::new(4 << 20, 0).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    for (schema, expected) in [
        ("snapshot-record.v1", 0),
        ("tick-consensus-record.v1", 1),
        ("tick-consensus-rejection-audit.v1", 2),
    ] {
        let raw=oasis7_wasm_abi::encode_canonical_cbor(&serde_json::json!({"domain":"oasis7.runtime.history.original.v1","schema":schema,"value":{"nested":[null,true,17,"payload"]}})).unwrap();
        let kind = runtime_label(&raw, &mut labels(), &mut || false)
            .unwrap()
            .unwrap();
        let actual = match kind {
            RuntimeHistoryDecodeKindV1::Snapshot => 0,
            RuntimeHistoryDecodeKindV1::Tick => 1,
            RuntimeHistoryDecodeKindV1::Audit => 2,
        };
        assert_eq!(actual, expected);
        let mut work = labels();
        work.values_remaining = 1;
        assert!(matches!(
            runtime_label(&raw, &mut work, &mut || false),
            Err(ValueTreeError::ResourceLimited(_))
        ));
        assert!(matches!(
            runtime_label(&raw, &mut labels(), &mut || true),
            Err(ValueTreeError::Cancelled)
        ));
    }
    for (domain, schema) in [
        ("other", "snapshot-record.v1"),
        ("oasis7.runtime.history.original.v1", "future-record.v1"),
    ] {
        let raw = oasis7_wasm_abi::encode_canonical_cbor(
            &serde_json::json!({"domain":domain,"schema":schema,"value":null}),
        )
        .unwrap();
        assert!(
            runtime_label(&raw, &mut labels(), &mut || false)
                .unwrap()
                .is_none()
        );
    }
}
#[test]
fn result_sink_obeys_partial_writes_cancellation_and_unwind_poison() {
    use oasis7_formal_protocol::incremental_value::VALUE_CHUNK_BYTES;
    use std::{
        cell::Cell,
        panic::{AssertUnwindSafe, catch_unwind},
    };
    let memory = MemoryBudget::new(4 << 20, 0).unwrap();
    let size = VALUE_CHUNK_BYTES * 3;
    let _input = memory.reserve(size, MemoryClass::Ordinary).unwrap();
    let bytes = vec![7; size];
    let _output = memory.reserve(size + 1024, MemoryClass::Ordinary).unwrap();
    let mut output = Vec::with_capacity(size);
    let mut remaining = size as u64;
    let calls = Cell::new(0);
    let mut cancel = || {
        let n = calls.get() + 1;
        calls.set(n);
        n > 1
    };
    let mut sink = Sink {
        bytes: &mut output,
        expected: size,
        hash: blake3::Hasher::new(),
        remaining: &mut remaining,
        cancelled: &mut cancel,
        stopped: None,
    };
    assert_eq!(sink.write(&bytes).unwrap(), VALUE_CHUNK_BYTES);
    assert!(sink.write(&bytes[VALUE_CHUNK_BYTES..]).is_err());
    assert!(matches!(sink.stopped, Some(ValueTreeError::Cancelled)));
    assert!(sink.write(&[]).is_err());
    assert_eq!(sink.bytes.len(), VALUE_CHUNK_BYTES);
    assert_eq!(*sink.remaining, (size - VALUE_CHUNK_BYTES) as u64);
    output.clear();
    let mut remaining = size as u64;
    let mut panic_cancel = || -> bool { panic!("injected callback unwind") };
    let mut sink = Sink {
        bytes: &mut output,
        expected: size,
        hash: blake3::Hasher::new(),
        remaining: &mut remaining,
        cancelled: &mut panic_cancel,
        stopped: None,
    };
    assert!(catch_unwind(AssertUnwindSafe(|| sink.write(&bytes))).is_err());
    assert!(sink.write(&bytes).is_err());
    assert!(sink.bytes.is_empty());
}
