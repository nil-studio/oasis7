use super::*;
fn work() -> DomainEventDecodeWorkV1 {
    DomainEventDecodeWorkV1 {
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
        max_heap_bytes: 16 << 20,
    }
}
fn event() -> DomainEvent {
    DomainEvent::BodyAttributesRejected {
        agent_id: "原 agent".into(),
        reason: "actual reason".into(),
    }
}
#[test]
fn owned_domain_decoder_actual_adjacent_data_before_tag_roundtrips_both_classes() {
    let memory = MemoryBudget::new(64 << 20, 32 << 20).unwrap();
    let _input = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&event()).unwrap();
    let before = memory.used();
    for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
        let decoded =
            decode_domain_event_original_v1(&raw, &memory, class, &mut work(), || false).unwrap();
        assert_eq!(decoded.allocation_class(), class);
        match decoded.event() {
            DomainEvent::BodyAttributesRejected { agent_id, reason } => {
                assert_eq!(agent_id, "原 agent");
                assert_eq!(reason, "actual reason");
            }
            _ => panic!("wrong bare DTO"),
        };
        assert!(memory.used() > before);
        drop(decoded);
        assert_eq!(memory.used(), before);
    }
}
#[test]
fn owned_domain_decoder_rejects_world_wrapper_unknown_fields_and_bad_tag_canonical_identity() {
    let memory = MemoryBudget::new(64 << 20, 32 << 20).unwrap();
    let _input = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let value = serde_json::to_value(event()).unwrap();
    let mut unknown = value.clone();
    unknown["data"]["unknown_field"] = serde_json::json!([1, 2, 3]);
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&unknown).unwrap();
    assert!(
        decode_domain_event_original_v1(&raw, &memory, MemoryClass::Ordinary, &mut work(), || {
            false
        })
        .is_err()
    );
    let wrapper = serde_json::json!({"id":1,"time":0,"caused_by":null,"body":{"Domain":value}});
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&wrapper).unwrap();
    assert!(
        decode_domain_event_original_v1(&raw, &memory, MemoryClass::Ordinary, &mut work(), || {
            false
        })
        .is_err()
    );
    let unknown = serde_json::json!({"type":"\u{1}".repeat(8192),"data":{}});
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&unknown).unwrap();
    let mut limited = work();
    limited.max_heap_bytes = 64 << 10;
    assert!(matches!(
        decode_domain_event_original_v1(&raw, &memory, MemoryClass::Ordinary, &mut limited, || {
            false
        }),
        Err(ValueTreeError::ResourceLimited(
            "DomainEvent heap allowance"
        ))
    ));
}
#[test]
fn owned_domain_decoder_admission_and_cancel_leave_raw_and_budget_intact() {
    let memory = MemoryBudget::new(64 << 20, 32 << 20).unwrap();
    let _input = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&event()).unwrap();
    let original = raw.clone();
    let before = memory.used();
    let mut limited = work();
    limited.max_heap_bytes = 1;
    assert!(matches!(
        decode_domain_event_original_v1(&raw, &memory, MemoryClass::Ordinary, &mut limited, || {
            false
        }),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(matches!(
        decode_domain_event_original_v1(&raw, &memory, MemoryClass::Ordinary, &mut work(), || true),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(raw, original);
    assert_eq!(memory.used(), before);
    let huge = [0x9b, 255, 255, 255, 255, 255, 255, 255, 255];
    assert!(
        decode_domain_event_original_v1(&huge, &memory, MemoryClass::Ordinary, &mut work(), || {
            false
        })
        .is_err()
    );
}
