use super::*;
use crate::incremental_pending_action_original_binding::tests::{Objects, work};
#[test]
fn mailbox_domain_original_requires_actual_typed_bytes_class_hash_and_finite_work() {
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let event = DomainEvent::ActionAccepted {
        action_id: 7,
        action_kind: "mailbox-original".into(),
        actor_id: "agent-a".into(),
        eta_ticks: 3,
        notes: vec!["borrowed雪".into()],
    };
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&event).unwrap();
    let id = *blake3::hash(&bytes).as_bytes();
    let mut objects = Objects {
        memory: memory.clone(),
        class: MemoryClass::Ordinary,
        bytes: bytes.clone(),
        swallow: false,
    };
    let mut budget = work();
    let before = budget.original_bytes_remaining;
    let checked = verify_domain_event_original_v1(
        &event,
        id,
        &mut objects,
        &memory,
        MemoryClass::Ordinary,
        &mut budget,
        || false,
    )
    .unwrap();
    assert_eq!(checked.original().id, id);
    assert_eq!(checked.original().byte_len, bytes.len() as u64);
    assert_eq!(budget.original_bytes_remaining, before - bytes.len() as u64);
    assert_eq!(budget.objects_remaining, 9);
    CheckedDomainEventOriginalV1::validate(&checked, &id, &memory, MemoryClass::Ordinary).unwrap();
    assert!(
        CheckedDomainEventOriginalV1::validate(&checked, &[99; 32], &memory, MemoryClass::Ordinary)
            .is_err()
    );
    assert!(
        CheckedDomainEventOriginalV1::validate(&checked, &id, &memory, MemoryClass::Recovery)
            .is_err()
    );
    drop(checked);
    let used = memory.used();
    assert!(
        verify_domain_event_original_v1(
            &event,
            [99; 32],
            &mut objects,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        )
        .is_err()
    );
    assert_eq!(memory.used(), used);
    objects.bytes[0] ^= 1;
    assert!(
        verify_domain_event_original_v1(
            &event,
            id,
            &mut objects,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        )
        .is_err()
    );
    assert_eq!(memory.used(), used);
    objects.bytes = bytes.clone();
    objects.class = MemoryClass::Recovery;
    assert!(
        verify_domain_event_original_v1(
            &event,
            id,
            &mut objects,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        )
        .is_err()
    );
    objects.class = MemoryClass::Ordinary;
    let mut limited = work();
    limited.original_bytes_remaining = 1;
    assert!(
        verify_domain_event_original_v1(
            &event,
            id,
            &mut objects,
            &memory,
            MemoryClass::Ordinary,
            &mut limited,
            || false
        )
        .is_err()
    );
    assert_eq!(memory.used(), used);
    objects.bytes.extend_from_slice(b"overflow");
    objects.swallow = true;
    assert!(
        verify_domain_event_original_v1(
            &event,
            id,
            &mut objects,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        )
        .is_err()
    );
    assert_eq!(memory.used(), used);
    assert!(
        verify_domain_event_original_v1(
            &event,
            id,
            &mut objects,
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || true
        )
        .is_err()
    );
    assert_eq!(memory.used(), used);
}
