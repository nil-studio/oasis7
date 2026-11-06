use super::*;
fn work() -> RuntimeHistoryDecodeWorkV1 {
    RuntimeHistoryDecodeWorkV1 {
        structure: CborPreflightWorkV1 {
            bytes_remaining: 8 << 20,
            values_remaining: 100000,
            max_depth: 32,
        },
        canonical: CanonicalCborWorkV1 {
            visits_remaining: 100000,
            bytes_remaining: 8 << 20,
            max_depth: 32,
            max_map_keys: 1024,
            max_key_bytes: 1024,
        },
        max_original_bytes: 1 << 20,
        max_heap_bytes: 4 << 20,
    }
}
fn snapshot() -> SnapshotRecord {
    SnapshotRecord {
        snapshot_hash: "实际 original hash".into(),
        journal_len: usize::MAX,
        created_at: u64::MAX,
        manifest_hash: "actual manifest".into(),
    }
}
fn encode_snapshot(value: SnapshotRecord) -> Vec<u8> {
    oasis7_wasm_abi::encode_canonical_cbor(&Envelope {
        domain: "oasis7.runtime.history.original.v1".into(),
        schema: "snapshot-record.v1".into(),
        value,
    })
    .unwrap()
}
#[test]
fn owned_history_decoder_roundtrips_exact_snapshot_with_retained_class_budget() {
    let memory = MemoryBudget::new(16 << 20, 8 << 20).unwrap();
    let _input = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let raw = encode_snapshot(snapshot());
    let baseline = memory.used();
    for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
        let value = decode_runtime_history_original_v1(
            RuntimeHistoryDecodeKindV1::Snapshot,
            &raw,
            &memory,
            class,
            &mut work(),
            || false,
        )
        .unwrap();
        assert!(memory.used() > baseline);
        match &*value {
            DecodedRuntimeHistoryOriginalV1::Snapshot(value) => assert_eq!(value, &snapshot()),
            _ => panic!("wrong closed type"),
        };
        drop(value);
        assert_eq!(memory.used(), baseline);
    }
}
#[test]
fn owned_history_decoder_rejects_unknown_fields_wrong_profile_and_huge_hints_before_dto() {
    let memory = MemoryBudget::new(16 << 20, 8 << 20).unwrap();
    let _input = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let mut value = serde_json::json!({"domain":"oasis7.runtime.history.original.v1","schema":"snapshot-record.v1","value":snapshot()});
    value["value"]["unknown_ignored_field"] = serde_json::json!(true);
    let unknown = oasis7_wasm_abi::encode_canonical_cbor(&value).unwrap();
    let before = memory.used();
    assert!(
        decode_runtime_history_original_v1(
            RuntimeHistoryDecodeKindV1::Snapshot,
            &unknown,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        )
        .is_err()
    );
    assert_eq!(memory.used(), before);
    let raw = encode_snapshot(snapshot());
    assert!(
        decode_runtime_history_original_v1(
            RuntimeHistoryDecodeKindV1::Audit,
            &raw,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        )
        .is_err()
    );
    assert_eq!(memory.used(), before);
    let huge = [0x9b, 255, 255, 255, 255, 255, 255, 255, 255];
    assert!(matches!(
        decode_runtime_history_original_v1(
            RuntimeHistoryDecodeKindV1::Snapshot,
            &huge,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        ),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert_eq!(memory.used(), before);
}
#[test]
fn owned_history_decoder_preserves_complete_input_on_admission_cancel_or_noncanonical_bytes() {
    let memory = MemoryBudget::new(16 << 20, 8 << 20).unwrap();
    let _input = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let raw = encode_snapshot(snapshot());
    let original = raw.clone();
    let baseline = memory.used();
    let mut limit = work();
    limit.max_heap_bytes = 1;
    assert!(matches!(
        decode_runtime_history_original_v1(
            RuntimeHistoryDecodeKindV1::Snapshot,
            &raw,
            &memory,
            MemoryClass::Ordinary,
            &mut limit,
            || false
        ),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(matches!(
        decode_runtime_history_original_v1(
            RuntimeHistoryDecodeKindV1::Snapshot,
            &raw,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || true
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(raw, original);
    assert_eq!(memory.used(), baseline);
    // A valid serde representation with declaration-order maps is not canonical.
    let noncanonical = serde_cbor::to_vec(&Envelope {
        domain: "oasis7.runtime.history.original.v1".into(),
        schema: "snapshot-record.v1".into(),
        value: snapshot(),
    })
    .unwrap();
    assert_ne!(noncanonical, raw);
    assert!(
        decode_runtime_history_original_v1(
            RuntimeHistoryDecodeKindV1::Snapshot,
            &noncanonical,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        )
        .is_err()
    );
    assert_eq!(memory.used(), baseline);
}

#[test]
fn owned_history_tick_and_audit_preserve_extremes_and_reject_default_elision() {
    let memory = MemoryBudget::new(16 << 20, 8 << 20).unwrap();
    let _input = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let tick:TickConsensusRecord=serde_json::from_value(serde_json::json!({"block":{"header":{"epoch":0,"tick":1,"parent_hash":"原 parent","events_hash":"events","state_root":"state","executor_version":"v","randomness_seed":"seed"},"ordered_action_ids":[0,u64::MAX],"ordered_event_ids":[0,u64::MAX],"event_count":2,"execution_digest":{"action_batch_hash":"actions","domain_events_hash":"domain","state_projection_hash":"projection"}},"certificate":{"block_hash":"block","consensus_height":1,"threshold":u16::MAX,"authority_source":"actual","submission_role":"authority","signatures":{"原 key":"actual signature"}}})).unwrap();
    let envelope = Envelope {
        domain: "oasis7.runtime.history.original.v1".into(),
        schema: "tick-consensus-record.v1".into(),
        value: tick.clone(),
    };
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&envelope).unwrap();
    let value = decode_runtime_history_original_v1(
        RuntimeHistoryDecodeKindV1::Tick,
        &raw,
        &memory,
        MemoryClass::Ordinary,
        &mut work(),
        || false,
    )
    .unwrap();
    match &*value {
        DecodedRuntimeHistoryOriginalV1::Tick(actual) => assert_eq!(actual, &tick),
        _ => panic!("type"),
    };
    drop(value);
    let mut omitted = serde_json::to_value(envelope).unwrap();
    omitted["value"]["block"]["header"]
        .as_object_mut()
        .unwrap()
        .remove("schema_version");
    let omitted = oasis7_wasm_abi::encode_canonical_cbor(&omitted).unwrap();
    assert!(
        decode_runtime_history_original_v1(
            RuntimeHistoryDecodeKindV1::Tick,
            &omitted,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        )
        .is_err()
    );
    let audit:TickConsensusRejectionAuditEvent=serde_json::from_value(serde_json::json!({"recorded_at_tick":u64::MAX,"tick":u64::MAX,"consensus_height":u64::MAX,"attempted_source":"original","attempted_role":"propagation","existing_source":null,"existing_role":null,"reason":"原 reason"})).unwrap();
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&Envelope {
        domain: "oasis7.runtime.history.original.v1".into(),
        schema: "tick-consensus-rejection-audit.v1".into(),
        value: audit.clone(),
    })
    .unwrap();
    let value = decode_runtime_history_original_v1(
        RuntimeHistoryDecodeKindV1::Audit,
        &raw,
        &memory,
        MemoryClass::Recovery,
        &mut work(),
        || false,
    )
    .unwrap();
    match &*value {
        DecodedRuntimeHistoryOriginalV1::Audit(actual) => assert_eq!(actual, &audit),
        _ => panic!("type"),
    };
}

#[test]
fn owned_history_unknown_role_error_escape_is_admitted_before_serde_formatting() {
    let memory = MemoryBudget::new(16 << 20, 8 << 20).unwrap();
    let _input = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let value = serde_json::json!({"domain":"oasis7.runtime.history.original.v1","schema":"tick-consensus-rejection-audit.v1","value":{"recorded_at_tick":0,"tick":0,"consensus_height":0,"attempted_source":"source","attempted_role":"\u{1}".repeat(8192),"reason":"reason"}});
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&value).unwrap();
    let original = raw.clone();
    let baseline = memory.used();
    let mut limits = work();
    limits.max_heap_bytes = 64 << 10;
    assert!(matches!(
        decode_runtime_history_original_v1(
            RuntimeHistoryDecodeKindV1::Audit,
            &raw,
            &memory,
            MemoryClass::Ordinary,
            &mut limits,
            || false
        ),
        Err(ValueTreeError::ResourceLimited(
            "Runtime History heap allowance"
        ))
    ));
    assert_eq!(raw, original);
    assert_eq!(memory.used(), baseline);
}
