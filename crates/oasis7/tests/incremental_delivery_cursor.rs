#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    ProductValidationDeliveryCursor, incremental_delivery_cursor::*, incremental_field_member::*,
    incremental_field_read::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
#[test]
fn era_and_event_cursor_survive_real_cold_root_reads() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let cursor = ProductValidationDeliveryCursor {
            event_id_era: 7,
            routed_through_event_id: u64::MAX,
        };
        let root_bytes = {
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            let mut values = store.value_session(physical());
            let _batch = producer.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(2);
            let mut remaining = 2;
            prepare_delivery_cursor_genesis_v1(
                &cursor,
                &mut MemberRecordWriteContext {
                    sink: &mut values,
                    memory: &producer,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 16,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
                &mut remaining,
                &mut |write| {
                    assert!(rows.len() < 2);
                    assert_eq!(write.namespace, StateNamespaceV1::Scheduler);
                    rows.push(write);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(remaining, 0);
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let workspace = producer.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
            let mut changes: Vec<_> = rows
                .iter()
                .map(|row| StateWriteV1 {
                    key: row.mutation.key.clone(),
                    before: None,
                    after: row.mutation.after,
                })
                .collect();
            changes.sort_by(|a, b| a.key.cmp(&b.key));
            let delta = workspace.retain(
                StateDeltaV1::build(
                    StateRootV1::empty(StateNamespaceV1::Scheduler).unwrap(),
                    changes,
                    &state,
                    2 << 20,
                )
                .unwrap(),
            );
            assert_eq!(delta.after.leaf_count, 2);
            values.put_state_pages(&delta, &producer).unwrap();
            delta.after.canonical_bytes().unwrap()
        };
        assert_eq!(producer.used(), 0);
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(
                4 << 20,
                if class == MemoryClass::Recovery {
                    3 << 20
                } else {
                    1 << 20
                },
            )
            .unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let _filler = if class == MemoryClass::Recovery {
                Some(
                    memory
                        .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
                        .unwrap(),
                )
            } else {
                None
            };
            let root = StateRootV1::from_canonical_bytes(&root_bytes).unwrap();
            let state = store.state_session(physical(), class).unwrap();
            let values = store.value_session(physical());
            let mut context = WorldFieldReadContextV1 {
                root: &root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 4096,
                },
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || {
                    state.take_read_error().map(|_| {
                        oasis7_formal_protocol::incremental_value::ValueTreeError::Invalid(
                            "state page failure",
                        )
                    })
                },
                cancelled: || false,
            };
            assert_eq!(read_delivery_cursor_v1(&mut context).unwrap(), cursor);
            let wrong = StateRootV1::empty(StateNamespaceV1::Allocator).unwrap();
            context.root = &wrong;
            assert!(read_delivery_cursor_v1(&mut context).is_err());
            let missing = StateRootV1::empty(StateNamespaceV1::Scheduler).unwrap();
            context.root = &missing;
            assert!(read_delivery_cursor_v1(&mut context).is_err());
        }
        let next_bytes = {
            use oasis7_formal_protocol::incremental_execution::{
                AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1,
                TransactionLimits, TransitionVerifier,
            };
            use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
            use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let mut roots = RootSetV1::empty("delivery-cursor".into(), [3; 32]).unwrap();
            *roots
                .state
                .iter_mut()
                .find(|root| root.namespace == StateNamespaceV1::Scheduler)
                .unwrap() = StateRootV1::from_canonical_bytes(&root_bytes).unwrap();
            let parent = ExecutionParent {
                commit: [4; 32],
                roots,
                authority: AuthorityBinding {
                    token: [5; 32],
                    epoch: 7,
                },
            };
            let live = parent.fence().unwrap();
            let limits = TransactionLimits {
                max_reads: 8,
                max_writes: 4,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            };
            let first = ProductValidationDeliveryCursor {
                event_id_era: 8,
                routed_through_event_id: 0,
            };
            let final_cursor = ProductValidationDeliveryCursor {
                event_id_era: 8,
                routed_through_event_id: 3,
            };
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut transaction =
                ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits)
                    .unwrap();
            let values = store.value_session(physical());
            let mut writer = store.value_session(physical());
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || {
                        state.take_read_error().map(|_| {
                            oasis7_formal_protocol::incremental_value::ValueTreeError::Invalid(
                                "state page failure",
                            )
                        })
                    },
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 16,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                };
                for (before, after, changed) in [
                    (&cursor, &first, true),
                    (&first, &final_cursor, true),
                    (&final_cursor, &final_cursor, false),
                ] {
                    let guard = read.transaction.guard();
                    assert_eq!(
                        change_delivery_cursor_overlay_v1(
                            before, after, &mut read, &mut write, &guard, &live
                        )
                        .unwrap(),
                        changed
                    );
                    assert_eq!(read_delivery_cursor_overlay_v1(&mut read).unwrap(), *after);
                }
            }
            let guard = transaction.guard();
            let sealed = transaction.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.reads.len(), 2);
            assert_eq!(sealed.record.writes.len(), 1);
            let verifier = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let verified = TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &verifier,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            }
            .verify(&sealed.record)
            .unwrap();
            assert_eq!(verified.record.after, sealed.record.after);
            let mut pages = store.value_session(physical());
            for node in &verified.new_nodes {
                let charge = memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap();
                pages
                    .put_node(
                        *blake3::hash(&node.bytes).as_bytes(),
                        charge.retain(node.bytes.clone()),
                    )
                    .unwrap();
            }
            for wrong in [
                ProductValidationDeliveryCursor {
                    event_id_era: 7,
                    routed_through_event_id: 0,
                },
                ProductValidationDeliveryCursor {
                    event_id_era: 8,
                    routed_through_event_id: 1,
                },
            ] {
                let state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let mut transaction =
                    ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits)
                        .unwrap();
                {
                    let mut read = WorldFieldOverlayContextV1 {
                        transaction: &mut transaction,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class: MemoryClass::Ordinary,
                            io: &mut logical(),
                        },
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 4096,
                        },
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                        take_state_error: || {
                            state.take_read_error().map(|_| {
                                oasis7_formal_protocol::incremental_value::ValueTreeError::Invalid(
                                    "state page failure",
                                )
                            })
                        },
                        cancelled: || false,
                    };
                    let mut write = MemberRecordWriteContext {
                        sink: &mut writer,
                        memory: &memory,
                        io: &mut logical(),
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 4096,
                        },
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 16,
                            record_bytes: 4096,
                        },
                        cancelled: || false,
                    };
                    let guard = read.transaction.guard();
                    change_delivery_cursor_overlay_v1(
                        &cursor, &first, &mut read, &mut write, &guard, &live,
                    )
                    .unwrap();
                    let guard = read.transaction.guard();
                    let result = change_delivery_cursor_overlay_v1(
                        &wrong,
                        &final_cursor,
                        &mut read,
                        &mut write,
                        &guard,
                        &live,
                    );
                    assert!(matches!(result, Err(oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
                        oasis7_formal_protocol::incremental_value::ValueTreeError::Invalid("delivery cursor expected value")))));
                }
                let guard = transaction.guard();
                assert!(transaction.seal(&guard, &live, 1).is_err());
            }
            verified
                .record
                .after
                .state
                .iter()
                .find(|root| root.namespace == StateNamespaceV1::Scheduler)
                .unwrap()
                .canonical_bytes()
                .unwrap()
        };
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(
                4 << 20,
                if class == MemoryClass::Recovery {
                    3 << 20
                } else {
                    1 << 20
                },
            )
            .unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            for (bytes, expected) in [(&root_bytes, (7, u64::MAX)), (&next_bytes, (8, 3))] {
                let root = StateRootV1::from_canonical_bytes(bytes).unwrap();
                let state = store.state_session(physical(), class).unwrap();
                let values = store.value_session(physical());
                let mut read = WorldFieldReadContextV1 {
                    root: &root,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || {
                        state.take_read_error().map(|_| {
                            oasis7_formal_protocol::incremental_value::ValueTreeError::Invalid(
                                "state page failure",
                            )
                        })
                    },
                    cancelled: || false,
                };
                let actual = read_delivery_cursor_v1(&mut read).unwrap();
                assert_eq!(
                    (actual.event_id_era, actual.routed_through_event_id),
                    expected
                );
            }
        }
    }
}
