use super::*;

#[test]
fn legacy_memory_api_is_the_single_native_l2_store() {
    use std::any::TypeId;
    assert_eq!(
        TypeId::of::<AgentMemory>(),
        TypeId::of::<oasis7_simulator::AgentMemory>()
    );
    assert_eq!(
        TypeId::of::<LongTermMemory>(),
        TypeId::of::<oasis7_simulator::LongTermMemory>()
    );
    assert_eq!(
        TypeId::of::<ShortTermMemory>(),
        TypeId::of::<oasis7_simulator::ShortTermMemory>()
    );
    assert_eq!(
        TypeId::of::<MemoryEntry>(),
        TypeId::of::<oasis7_simulator::MemoryEntry>()
    );
    assert_eq!(
        TypeId::of::<MemoryEntryKind>(),
        TypeId::of::<oasis7_simulator::MemoryEntryKind>()
    );
}

#[test]
fn native_and_historical_memory_cbor_and_json_are_identical() {
    let mut legacy: AgentMemory = AgentMemory::with_capacities(4, 4);
    legacy.record_observation(7, "first real observation");
    legacy.record_note(8, "same persisted context");
    legacy.consolidate(9, 0.4);
    let wire = serde_json::to_vec(&legacy).expect("encode historical memory");
    let native: oasis7_simulator::AgentMemory =
        serde_json::from_slice(&wire).expect("decode by canonical native owner");
    assert_eq!(native, legacy);
    assert_eq!(serde_json::to_vec(&native).unwrap(), wire);
    let cbor = serde_cbor::to_vec(&legacy).expect("legacy CBOR");
    let restored: AgentMemory = serde_cbor::from_slice(&cbor).expect("native/legacy CBOR");
    assert_eq!(restored, native);
    assert_eq!(restored.context_summary(3), legacy.context_summary(3));
}
