use super::*;
fn work() -> WorldEventDecodeWorkV1 {
    WorldEventDecodeWorkV1 {
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
        max_heap_bytes: 32 << 20,
    }
}
fn domain() -> WorldEvent {
    WorldEvent {
        id: u64::MAX,
        time: u64::MAX,
        caused_by: None,
        body: crate::WorldEventBody::Domain(crate::DomainEvent::BodyAttributesRejected {
            agent_id: "原 agent".into(),
            reason: "actual reason".into(),
        }),
    }
}
#[test]
fn owned_world_decoder_roundtrips_actual_domain_and_flattened_module_event() {
    let memory = MemoryBudget::new(128 << 20, 64 << 20).unwrap();
    let _input = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let module = WorldEvent {
        id: 7,
        time: 9,
        caused_by: None,
        body: crate::WorldEventBody::ModuleEvent(oasis7_wasm_abi::ModuleEvent {
            proposal_id: u64::MAX,
            kind: oasis7_wasm_abi::ModuleEventKind::DeactivateModule {
                module_id: "原 module".into(),
                reason: "actual reason".into(),
                deactivated_by: "operator".into(),
            },
        }),
    };
    for event in [domain(), module] {
        let raw = oasis7_wasm_abi::encode_canonical_cbor(&event).unwrap();
        let baseline = memory.used();
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let decoded =
                decode_world_event_original_v1(&raw, &memory, class, &mut work(), || false)
                    .unwrap();
            assert_eq!(decoded.allocation_class(), class);
            assert_eq!(decoded.event(), &event);
            drop(decoded);
            assert_eq!(memory.used(), baseline);
        }
    }
}
#[test]
fn owned_world_decoder_rejects_bare_domain_unknown_field_and_preserves_raw_on_limits() {
    let memory = MemoryBudget::new(128 << 20, 64 << 20).unwrap();
    let _input = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let bare =
        oasis7_wasm_abi::encode_canonical_cbor(&crate::DomainEvent::BodyAttributesRejected {
            agent_id: "a".into(),
            reason: "r".into(),
        })
        .unwrap();
    assert!(
        decode_world_event_original_v1(&bare, &memory, MemoryClass::Ordinary, &mut work(), || {
            false
        })
        .is_err()
    );
    let mut value = serde_json::to_value(domain()).unwrap();
    value["unknown_field"] = serde_json::json!(true);
    let unknown = oasis7_wasm_abi::encode_canonical_cbor(&value).unwrap();
    assert!(
        decode_world_event_original_v1(
            &unknown,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        )
        .is_err()
    );
    let raw = oasis7_wasm_abi::encode_canonical_cbor(&domain()).unwrap();
    let original = raw.clone();
    let baseline = memory.used();
    let mut limited = work();
    limited.max_heap_bytes = 1;
    assert!(matches!(
        decode_world_event_original_v1(&raw, &memory, MemoryClass::Ordinary, &mut limited, || {
            false
        }),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(matches!(
        decode_world_event_original_v1(&raw, &memory, MemoryClass::Ordinary, &mut work(), || true),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(raw, original);
    assert_eq!(memory.used(), baseline);
}
