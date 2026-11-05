use super::*;
fn work() -> WorldEventDecodeWorkV1 {
    WorldEventDecodeWorkV1 {
        structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
            bytes_remaining: 2 << 20,
            values_remaining: 100000,
            max_depth: 64,
        },
        canonical: CanonicalCborWorkV1 {
            visits_remaining: 100000,
            bytes_remaining: 2 << 20,
            max_depth: 64,
            max_map_keys: 64,
            max_key_bytes: 1024,
        },
        max_original_bytes: 64 << 10,
        max_heap_bytes: 64 << 20,
    }
}
fn rejected() -> WorldEvent {
    WorldEvent {
        id: 1,
        time: 2,
        caused_by: Some(crate::CausedBy::Action(7)),
        body: crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
            action_id: 7,
            reason: crate::RejectReason::RuleDenied {
                notes: vec!["dynamic failure".repeat(20)],
            },
        }),
    }
}
#[test]
fn owned_event_admission_preserves_rejection_and_override_and_retains_fee() {
    let memory = MemoryBudget::new(128 << 20, 1 << 20).unwrap();
    let _caller = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let action = crate::Action::QueryObservation {
        agent_id: "agent".into(),
    };
    let events = [
        rejected(),
        WorldEvent {
            id: 2,
            time: 2,
            caused_by: Some(crate::CausedBy::Action(7)),
            body: crate::WorldEventBody::ActionOverridden(crate::ActionOverrideRecord {
                action_id: 7,
                original_action: action.clone(),
                override_action: action,
            }),
        },
    ];
    for event in events {
        let expected = serde_cbor::to_vec(&event).unwrap();
        let owned =
            retain_world_event_owned_v1(event, &memory, MemoryClass::Ordinary, &mut work(), || {
                false
            })
            .unwrap();
        assert_eq!(serde_cbor::to_vec(&*owned).unwrap(), expected);
        assert!(owned.uses_budget(&memory));
        assert!(owned.reserved_bytes() >= std::mem::size_of::<WorldEvent>());
    }
}
#[test]
fn owned_event_refuses_limits_class_and_cancellation() {
    let memory = MemoryBudget::new(128 << 20, 1 << 20).unwrap();
    let _caller = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    for mode in 0..5 {
        let mut work = work();
        if mode == 0 {
            work.max_original_bytes = 1;
        }
        if mode == 1 {
            work.max_heap_bytes = 1;
        }
        if mode == 2 {
            work.structure.values_remaining = 0;
        }
        let class = if mode == 3 {
            MemoryClass::Recovery
        } else {
            MemoryClass::Ordinary
        };
        let result =
            retain_world_event_owned_v1(rejected(), &memory, class, &mut work, || mode == 4);
        assert!(result.is_err());
    }
}

#[test]
fn owned_event_normalizes_large_spare_capacity_under_separate_input_fee() {
    let memory = MemoryBudget::new(128 << 20, 1 << 20).unwrap();
    let capacity = 100_000;
    let input_fee = memory
        .reserve(
            capacity * std::mem::size_of::<String>() + 4096,
            MemoryClass::Ordinary,
        )
        .unwrap();
    let mut notes = Vec::with_capacity(capacity);
    notes.push("one actual note".to_owned());
    assert!(notes.capacity() >= capacity);
    let event = WorldEvent {
        id: 1,
        time: 2,
        caused_by: None,
        body: crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
            action_id: 7,
            reason: crate::RejectReason::RuleDenied { notes },
        }),
    };
    let output =
        retain_world_event_owned_v1(event, &memory, MemoryClass::Ordinary, &mut work(), || false)
            .unwrap();
    let crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
        reason: crate::RejectReason::RuleDenied { notes },
        ..
    }) = &output.body
    else {
        panic!("expected original rejection content")
    };
    assert_eq!(notes, &["one actual note"]);
    assert!(notes.capacity() < capacity);
    assert!(output.uses_budget(&memory));
    // The separate source allocation remains accounted until normalization
    // has fully returned, even though the input DTO was consumed by the API.
    drop(input_fee);
    assert_eq!(notes.len(), 1);
}
