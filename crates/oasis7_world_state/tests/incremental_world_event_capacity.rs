//! Capacity regression for the real Vec<u8>-sequence WorldEvent wire.
use oasis7_proto::{
    cbor_decode_preflight::CborPreflightWorkV1,
    memory_budget::{MemoryBudget, MemoryClass},
};
use oasis7_wasm_abi::{CanonicalCborWorkV1, ModuleStateUpdate, encode_canonical_cbor};
use oasis7_world_state::{CausedBy, WorldEvent, WorldEventBody, incremental_world_event_decode::*};
fn work() -> WorldEventDecodeWorkV1 {
    WorldEventDecodeWorkV1 {
        structure: CborPreflightWorkV1 {
            bytes_remaining: 32 << 20,
            values_remaining: 16 << 20,
            max_depth: 32,
        },
        canonical: CanonicalCborWorkV1 {
            visits_remaining: 16 << 20,
            bytes_remaining: 32 << 20,
            max_depth: 32,
            max_map_keys: 32,
            max_key_bytes: 128,
        },
        max_original_bytes: 4 << 20,
        max_heap_bytes: 256 << 20,
    }
}
fn roundtrip(bytes: usize) {
    let memory = MemoryBudget::new(512 << 20, 64 << 20).unwrap();
    // The fixture's generic canonical encoder uses an owned CBOR value tree.
    // Admit that test-only input construction separately; decoder must fit
    // the remaining SAME 512MiB process pool with max_heap <=256MiB.
    let fixture_fee = bytes
        .checked_mul(128)
        .unwrap()
        .checked_add(16 << 20)
        .unwrap();
    let _fixture = memory.reserve(fixture_fee, MemoryClass::Ordinary).unwrap();
    let event = WorldEvent {
        id: 17,
        time: 29,
        caused_by: Some(CausedBy::Action(11)),
        body: WorldEventBody::ModuleStateUpdated(ModuleStateUpdate {
            module_id: "actual-module".into(),
            trace_id: "actual-tick-29".into(),
            state: (0..bytes).map(|n| (n % 256) as u8).collect(),
        }),
    };
    let raw = encode_canonical_cbor(&event).unwrap();
    let before = memory.used();
    let mut work = work();
    let decoded=decode_world_event_original_v1(&raw,&memory,MemoryClass::Ordinary,&mut work,||false)
        .unwrap_or_else(|e|panic!("actual ModuleStateUpdated bytes={bytes}, encoded={}, bounded512MiB decode failed: {e:?}",raw.len()));
    assert_eq!(decoded.event(), &event);
    assert_eq!(decoded.allocation_class(), MemoryClass::Ordinary);
    assert!(decoded.reserved_bytes() <= 256 << 20);
    assert!(work.structure.values_remaining < 16 << 20);
    drop(decoded);
    assert_eq!(memory.used(), before);
}
#[test]
fn actual_module_state_world_event_64kib_roundtrips_within_512mib_pool() {
    roundtrip(64 << 10);
}
#[test]
fn actual_module_state_world_event_1mib_roundtrips_within_512mib_pool() {
    roundtrip(1 << 20);
}
