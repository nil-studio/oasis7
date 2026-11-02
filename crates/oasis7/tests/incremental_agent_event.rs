#![cfg(all(unix, not(target_arch = "wasm32")))]

#[path = "support/incremental_agent_event.rs"]
mod support;
use oasis7_agent_api::world_action::GeoPos;
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionParent, TransitionVerifier},
    incremental_state::StateNamespaceV1 as Ns,
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    WorldEvent,
    incremental_agent_fields::{AgentFieldKeyV1, AgentFieldV1, AgentScalarRefV1},
};
use std::sync::atomic::AtomicBool;
use support::*;

#[test]
fn actual_agent_events_reopen_originals_and_continue_from_authenticated_pages() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let root = directory.0.join("pages");
        let seed_memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let seed_store =
            IncrementalPageStore::create(&root, seed_memory.clone(), compression).unwrap();
        let (world, parent) = fixture(&seed_store, &seed_memory);
        drop(seed_store);
        drop(seed_memory);
        // Both independent branches start at the same immutable genesis root.
        // Close/reopen each branch with fresh sessions and memory; unrelated
        // provisional pages in this CAS cannot replace its authenticated root.
        for movement in [false, true] {
            let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &root,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let original = event(&world, 1, movement);
            let first = contribute(&world, &original, parent.clone(), &store, &memory);
            let mut source = store
                .state_session(state_physical(), MemoryClass::Ordinary)
                .unwrap();
            source.persist_transition_pages(&first).unwrap();
            let next_parent = ExecutionParent {
                commit: [8; 32],
                roots: first.record.after.clone(),
                authority: parent.authority,
            };
            let next_event = event(&world, 2, movement);
            let expected = contribute(&world, &next_event, next_parent.clone(), &store, &memory);
            if movement {
                // Original cache x=0 differs from authenticated first-move x=3.
                // The next target is x=0: skipping a cache-equal write is wrong.
                let key = AgentFieldKeyV1::new("alice", AgentFieldV1::PositionX, 0).unwrap();
                let write = expected
                    .record
                    .writes
                    .iter()
                    .find(|w| w.namespace == Ns::Business && w.mutation.key == key.as_bytes())
                    .expect("authenticated x=3 must be changed to target x=0");
                assert_ne!(write.mutation.before, write.mutation.after);
                let values = store.value_session(physical());
                let mut quota = logical();
                let mut reader = ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut quota,
                };
                for (id, amount) in [
                    (write.mutation.before.unwrap(), 3),
                    (write.mutation.after.unwrap(), 0),
                ] {
                    let value = reader.load_root(&id).unwrap();
                    let mut bytes = Vec::new();
                    reader
                        .read_range(&value, 0, value.0.logical_len, &mut bytes, || false)
                        .unwrap();
                    assert_eq!(
                        AgentScalarRefV1::from_canonical_bytes(&bytes, AgentFieldV1::PositionX)
                            .unwrap(),
                        AgentScalarRefV1::I64(amount)
                    );
                }
            }
            drop(source);
            drop(store);
            drop(memory);
            let recovery = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let reopened = IncrementalPageStore::open(
                &root,
                recovery.clone(),
                compression,
                MemoryClass::Recovery,
            )
            .unwrap();
            let source = reopened
                .state_session(state_physical(), MemoryClass::Recovery)
                .unwrap();
            TransitionVerifier {
                parent: &parent,
                operation: [4; 32],
                source: &source,
                memory: &recovery,
                class: MemoryClass::Recovery,
                max_pending_node_bytes: 4 << 20,
            }
            .verify(&first.record)
            .unwrap();
            let cancelled = AtomicBool::new(false);
            let objects = IncrementalObjectStore::open(
                &reopened,
                MemoryClass::Recovery,
                &mut physical(),
                &cancelled,
            )
            .unwrap();
            let bytes = objects
                .read_object_owned(
                    &first.record.facts[0].id,
                    1 << 20,
                    &mut ObjectIo {
                        physical: &mut physical(),
                        logical: &mut logical(),
                        class: MemoryClass::Recovery,
                        cancelled: &cancelled,
                    },
                )
                .unwrap();
            assert_eq!(
                bytes.as_slice(),
                oasis7_wasm_abi::encode_canonical_cbor(&original).unwrap()
            );
            assert_eq!(
                serde_cbor::from_slice::<WorldEvent>(&bytes).unwrap(),
                original
            );
            let resumed = contribute(&world, &next_event, next_parent, &reopened, &recovery);
            assert_eq!(resumed.record, expected.record);
            assert_eq!(resumed.new_nodes, expected.new_nodes);
            assert_eq!(
                world.state().agents["alice"].state.pos,
                GeoPos::new(0, 0, 0)
            );
            assert_eq!(world.state().agents["alice"].last_active, 3);
        }
    }
}

#[path = "support/incremental_agent_event_original.rs"]
mod originals;
