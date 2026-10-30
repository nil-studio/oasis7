#![cfg(all(unix, not(target_arch = "wasm32")))]
//! Real page-store query proof, not a complete World/authority activation.
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
        TransitionVerifier,
    },
    incremental_schedule::ScheduleScanBudget,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueIoBudget, ValueNodeWriter, ValueStreamWriter},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    BodyMutationContext, DueModuleBatchLimitsV1, World, WorldState,
    incremental_agent_body::{WORLD_TIME_KEY_V1, world_time_bytes_v1},
};
use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "oasis7-due-batch-{}-{now}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 32,
        max_writes: 16,
        max_facts: 1,
        max_pending_node_bytes: 4 << 20,
    }
}
#[test]
fn real_schedule_pages_reopen_and_select_in_original_instance_order() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let encoded = {
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            let world = World::new_with_state(WorldState {
                time: 11,
                ..WorldState::default()
            });
            let mut sink = store.value_session(physical());
            let mut rows = Vec::with_capacity(1);
            let _rows_charge = producer.reserve(4096, MemoryClass::Ordinary).unwrap();
            assert_eq!(
                world
                    .write_iws_module_schedule_genesis(
                        &mut BodyMutationContext {
                            sink: &mut sink,
                            memory: &producer,
                            io: &mut logical(),
                            cancelled: || false
                        },
                        &mut 0,
                        |key, id| {
                            assert!(rows.is_empty());
                            rows.push(StateWriteV1 {
                                key: key.to_vec(),
                                before: None,
                                after: Some(id),
                            });
                            Ok(())
                        }
                    )
                    .unwrap(),
                0
            );
            let clock = world_time_bytes_v1(11);
            let mut quota = logical();
            let mut cancelled = || false;
            let mut writer = ValueStreamWriter::new(
                &mut sink,
                &producer,
                &mut quota,
                clock.len() as u64,
                &mut cancelled,
            )
            .unwrap();
            writer.write_all(&clock).unwrap();
            let clock_root = writer.finish().unwrap().digest().unwrap();
            let mut roots = RootSetV1::empty("due-batch".into(), [3; 32]).unwrap();
            for (namespace, writes) in [
                (
                    Ns::Allocator,
                    vec![StateWriteV1 {
                        key: WORLD_TIME_KEY_V1.to_vec(),
                        before: None,
                        after: Some(clock_root),
                    }],
                ),
                (Ns::Scheduler, rows),
            ] {
                let source = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let workspace = producer.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
                let delta = workspace.retain(
                    StateDeltaV1::build(
                        StateRootV1::empty(namespace).unwrap(),
                        writes,
                        &source,
                        2 << 20,
                    )
                    .unwrap(),
                );
                sink.put_state_pages(&delta, &producer).unwrap();
                *roots
                    .state
                    .iter_mut()
                    .find(|root| root.namespace == namespace)
                    .unwrap() = delta.after.clone();
            }
            let parent = ExecutionParent {
                commit: [4; 32],
                roots,
                authority: AuthorityBinding {
                    token: [5; 32],
                    epoch: 7,
                },
            };
            let source = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut txn =
                ExecutionTransaction::begin(parent.clone(), [6; 32], &source, &producer, limits())
                    .unwrap();
            for (id, wake) in [
                ("z#", 0),
                ("a#", 10),
                ("m#\0", 11),
                ("é", 3),
                ("future", 12),
            ] {
                world
                    .contribute_iws_module_wake(
                        id,
                        Some(wake),
                        &mut txn,
                        &values,
                        &mut BodyMutationContext {
                            sink: &mut sink,
                            memory: &producer,
                            io: &mut logical(),
                            cancelled: || false,
                        },
                    )
                    .unwrap();
            }
            let live = parent.fence().unwrap();
            let guard = txn.guard();
            let sealed = txn.seal(&guard, &live, 1).unwrap();
            let verifier = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let verified = TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &verifier,
                memory: &producer,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            }
            .verify(&sealed.record)
            .unwrap();
            assert_eq!(verified.record.after, sealed.record.after);
            for node in &verified.new_nodes {
                let charge = producer
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap();
                sink.put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    charge.retain(node.bytes.clone()),
                )
                .unwrap();
            }
            verified.record.after.canonical_bytes().unwrap()
        };
        assert_eq!(producer.used(), 0);
        // Drop the preparation World, transaction, sessions and store before a new pool.
        let memory = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let world = World::new_with_state(WorldState {
            time: 11,
            ..WorldState::default()
        });
        let parent = ExecutionParent {
            commit: [9; 32],
            roots: RootSetV1::from_canonical_bytes(&encoded).unwrap(),
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut sink = store.value_session(physical());
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [10; 32], &state, &memory, limits())
                .unwrap();
        let mut scan = ScheduleScanBudget {
            nodes_remaining: 50_000,
            read_bytes_remaining: 16 << 20,
            buckets_remaining: 10,
            instances_remaining: 10,
        };
        let batch = world
            .read_iws_due_module_batch(
                &mut txn,
                &values,
                &mut BodyMutationContext {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut logical(),
                    cancelled: || false,
                },
                &mut scan,
                DueModuleBatchLimitsV1 {
                    entries: 4,
                    instance_bytes: 64,
                },
            )
            .unwrap();
        assert_eq!(
            batch.iter().collect::<Vec<_>>(),
            [("a#", 10), ("m#\0", 11), ("z#", 0), ("é", 3)]
        );
        let live = parent.fence().unwrap();
        txn.validate_guard(batch.guard(), &live).unwrap();
        let sealed = txn.seal(batch.guard(), &live, 2).unwrap();
        assert!(sealed.record.writes.is_empty());
        assert_eq!(sealed.record.reads.len(), 6);
        let verifier = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        TransitionVerifier {
            parent: &parent,
            operation: [10; 32],
            source: &verifier,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        }
        .verify(&sealed.record)
        .unwrap();
    }
}
