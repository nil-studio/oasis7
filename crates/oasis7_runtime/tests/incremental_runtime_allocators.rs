#![cfg(all(unix, not(target_arch = "wasm32")))]
//! Physical allocator roots and positive continuation without a World snapshot.
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
        TransitionVerifier,
    },
    incremental_state::{StateDeltaV1, StateWriteV1},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{BodyMutationContext, RuntimeAllocatorV1, RuntimeSequenceV1, World};
use std::sync::atomic::AtomicBool;
#[allow(dead_code)]
#[path = "support/incremental_agent_event.rs"]
mod support;
use support::{Directory, logical, physical};
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 16,
        max_writes: 16,
        max_facts: 16,
        max_pending_node_bytes: 4 << 20,
    }
}
#[test]
fn allocator_raw_zstd_cold_roots_continue_without_materialized_world() {
    let directory = Directory::new();
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let path = directory.0.join(format!("{compression:?}"));
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
        let mut parent = ExecutionParent {
            commit: [81; 32],
            roots: RootSetV1::empty("allocator-physical".into(), [82; 32]).unwrap(),
            authority: AuthorityBinding {
                token: [83; 32],
                epoch: 4,
            },
        };
        {
            let world = World::new();
            let mut writes = Vec::new();
            let mut sink = store.value_session(physical());
            world
                .write_iws_allocator_genesis(
                    &mut BodyMutationContext {
                        sink: &mut sink,
                        memory: &memory,
                        io: &mut logical(),
                        cancelled: || false,
                    },
                    |key, id| {
                        writes.push(StateWriteV1 {
                            key: key.to_vec(),
                            before: None,
                            after: Some(id),
                        });
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!(writes.len(), 4);
            writes.sort_unstable_by(|a, b| a.key.cmp(&b.key));
            let source = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let delta =
                StateDeltaV1::build(parent.roots.state[1].clone(), writes, &source, 4 << 20)
                    .unwrap();
            parent.roots.state[1] = delta.after;
            for node in delta.new_nodes {
                let id = hex::decode(node.digest).unwrap().try_into().unwrap();
                let charge = memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap();
                store
                    .put_page(
                        id,
                        &charge.retain(node.bytes),
                        &mut physical(),
                        &AtomicBool::new(false),
                    )
                    .unwrap();
            }
        } // Drop the only World before any allocation or recovery.
        let next = {
            let source = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut sink = store.value_session(physical());
            let mut txn =
                ExecutionTransaction::begin(parent.clone(), [84; 32], &source, &memory, limits())
                    .unwrap();
            for allocator in RuntimeAllocatorV1::ALL {
                let result = World::contribute_iws_allocate(
                    allocator,
                    RuntimeSequenceV1 { next: 1, era: 0 },
                    &mut txn,
                    &values,
                    &mut BodyMutationContext {
                        sink: &mut sink,
                        memory: &memory,
                        io: &mut logical(),
                        cancelled: || false,
                    },
                )
                .unwrap();
                assert_eq!(result, (1, RuntimeSequenceV1 { next: 2, era: 0 }));
            }
            let guard = txn.guard();
            let prepared = txn.seal(&guard, &parent.fence().unwrap(), 0).unwrap();
            let verification_source = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            TransitionVerifier {
                parent: &parent,
                operation: [84; 32],
                source: &verification_source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            }
            .verify(&prepared.record)
            .unwrap();
            let mut destination = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            destination.persist_transition_pages(&prepared).unwrap();
            ExecutionParent {
                commit: [85; 32],
                roots: prepared.record.after.clone(),
                authority: parent.authority,
            }
        };
        // Only the fixed authenticated root descriptor crosses the reopen boundary.
        let next_bytes = next.roots.canonical_bytes().unwrap();
        drop(store);
        drop(memory);
        drop(parent);
        {
            // A narrow cold read has a separate fixed allowance from execution.
            let cold_memory = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
            let cold = IncrementalPageStore::open(
                &path,
                cold_memory.clone(),
                compression,
                MemoryClass::Recovery,
            )
            .unwrap();
            let source = cold
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = cold.value_session(physical());
            let mut sink = cold.value_session(physical());
            let mut txn = ExecutionTransaction::begin(
                next.clone(),
                [87; 32],
                &source,
                &cold_memory,
                limits(),
            )
            .unwrap();
            for allocator in RuntimeAllocatorV1::ALL {
                assert_eq!(
                    World::read_iws_allocator(
                        allocator,
                        &mut txn,
                        &values,
                        &mut BodyMutationContext {
                            sink: &mut sink,
                            memory: &cold_memory,
                            io: &mut logical(),
                            cancelled: || false
                        }
                    )
                    .unwrap(),
                    RuntimeSequenceV1 { next: 2, era: 0 }
                );
            }
        }
        // The Zstd page encoder reserves 16MiB in addition to value and
        // transaction workspaces. Use the existing physical-fixture allowance;
        // the 4MiB allowance above covers narrow reads only.
        let recovery = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, recovery.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let next = ExecutionParent {
            roots: RootSetV1::from_canonical_bytes(&next_bytes).unwrap(),
            ..next
        };
        let source = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut sink = store.value_session(physical());
        let mut txn =
            ExecutionTransaction::begin(next.clone(), [86; 32], &source, &recovery, limits())
                .unwrap();
        for allocator in RuntimeAllocatorV1::ALL {
            assert_eq!(
                World::read_iws_allocator(
                    allocator,
                    &mut txn,
                    &values,
                    &mut BodyMutationContext {
                        sink: &mut sink,
                        memory: &recovery,
                        io: &mut logical(),
                        cancelled: || false
                    }
                )
                .unwrap(),
                RuntimeSequenceV1 { next: 2, era: 0 }
            );
            assert_eq!(
                World::contribute_iws_allocate(
                    allocator,
                    RuntimeSequenceV1 { next: 2, era: 0 },
                    &mut txn,
                    &values,
                    &mut BodyMutationContext {
                        sink: &mut sink,
                        memory: &recovery,
                        io: &mut logical(),
                        cancelled: || false
                    }
                )
                .unwrap(),
                (2, RuntimeSequenceV1 { next: 3, era: 0 })
            );
        }
        let guard = txn.guard();
        let prepared = txn.seal(&guard, &next.fence().unwrap(), 0).unwrap();
        assert_eq!(prepared.record.writes.len(), 4);
        // Independent verification has its own original finite I/O allowance;
        // the execution session is never reset or replenished.
        let verification_source = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        TransitionVerifier {
            parent: &next,
            operation: [86; 32],
            source: &verification_source,
            memory: &recovery,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        }
        .verify(&prepared.record)
        .unwrap();
    }
}
