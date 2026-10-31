use super::*;
use oasis7_distfs::incremental_page_store::PageIoBudget;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionError, ExecutionParent, ExecutionTransaction, RootSetV1,
    TransactionLimits,
};
use oasis7_runtime::{
    ProductValidationDeliveryCursor, incremental_delivery_cursor::*, incremental_field_overlay::*,
    incremental_world_id_sets::*,
};
use std::cell::Cell;

#[derive(Clone, Copy, Debug)]
enum Kind {
    Unsigned,
    Cursor,
    IdSet,
}

fn io() -> PageIoBudget {
    // A combined three-family fixture, not a production/cold-read allowance.
    let mut quota = physical();
    quota.reads_remaining = 1_000_000;
    quota
}

fn seed(store: &IncrementalPageStore, memory: &MemoryBudget) -> RootSetV1 {
    let source = WorldState {
        next_module_instance_id: 7,
        completed_logistics_route_ids: ["done".into()].into(),
        ..WorldState::default()
    };
    let mut values = store.value_session(io());
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(32);
    let mut remaining = 32;
    let mut output = |row| {
        assert!(rows.len() < 32);
        rows.push(row);
        Ok(())
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut values,
        memory,
        io: &mut logical(),
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 16_384,
        },
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1024,
            payload_bytes: 16,
            record_bytes: 4096,
        },
        cancelled: || false,
    };
    prepare_world_u64_genesis_v1(&source, &mut write, &mut remaining, &mut output).unwrap();
    prepare_delivery_cursor_genesis_v1(
        &source.product_validation_delivery_cursor,
        &mut write,
        &mut remaining,
        &mut output,
    )
    .unwrap();
    prepare_world_id_sets_genesis_v1(&source, &mut write, &mut remaining, &mut output).unwrap();
    let mut roots = RootSetV1::empty("control-noop-cancel".into(), [3; 32]).unwrap();
    for (index, namespace) in [
        (1, StateNamespaceV1::Allocator),
        (2, StateNamespaceV1::Scheduler),
        (3, StateNamespaceV1::Authorization),
    ] {
        let work = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
        let mut changes: Vec<_> = rows
            .iter()
            .filter(|row| row.namespace == namespace)
            .map(|row| StateWriteV1 {
                key: row.mutation.key.clone(),
                before: None,
                after: row.mutation.after,
            })
            .collect();
        changes.sort_by(|a, b| a.key.cmp(&b.key));
        let state = store.state_session(io(), MemoryClass::Ordinary).unwrap();
        let delta = work.retain(
            StateDeltaV1::build(
                StateRootV1::empty(namespace).unwrap(),
                changes,
                &state,
                4 << 20,
            )
            .unwrap(),
        );
        store
            .value_session(io())
            .put_state_pages(&delta, memory)
            .unwrap();
        roots.state[index] = delta.after.clone();
    }
    roots
}

#[derive(Debug)]
struct Outcome {
    kind: Kind,
    write_cancel: bool,
    resolver_polls: usize,
    polls: usize,
    cancelled: bool,
    aborted: bool,
    sealed_with_prefix: bool,
}

#[test]
fn noop_setters_check_both_cancellation_channels_after_public_resolvers() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _fixture = memory.reserve(256 << 10, MemoryClass::Ordinary).unwrap();
    let directory = Directory::new();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let parent = ExecutionParent {
        commit: [4; 32],
        roots: seed(&store, &memory),
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let mut outcomes = Vec::with_capacity(6);
    for kind in [Kind::Unsigned, Kind::Cursor, Kind::IdSet] {
        let mut resolver_polls = 0;
        // First calibrate the public read with a genuine typed prefix. Each
        // setter then gets a fresh identical transaction and N+1 cancellation.
        for phase in 0..3 {
            let state = store.state_session(io(), MemoryClass::Ordinary).unwrap();
            let mut transaction = ExecutionTransaction::begin(
                parent.clone(),
                [6; 32],
                &state,
                &memory,
                TransactionLimits {
                    max_reads: 16,
                    max_writes: 8,
                    max_facts: 8,
                    max_pending_node_bytes: 4 << 20,
                },
            )
            .unwrap();
            let values = store.value_session(io());
            let mut output = store.value_session(io());
            let mut output_io = logical();
            {
                let guard = transaction.guard();
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16_384,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut output,
                    memory: &memory,
                    io: &mut output_io,
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16_384,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 16,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                };
                let before = write.io.nodes_remaining;
                assert!(
                    change_world_u64_overlay_v1(
                        WorldU64FieldV1::Time,
                        0,
                        1,
                        &mut read,
                        &mut write,
                        &guard,
                        &live,
                    )
                    .unwrap()
                );
                assert!(write.io.nodes_remaining < before);
            }
            let guard = transaction.guard();
            let polls = Cell::new(0);
            let poll = || {
                polls.set(polls.get() + 1);
                phase != 0 && polls.get() == resolver_polls + 1
            };
            let before = output_io.nodes_remaining;
            let result = {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16_384,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || (phase == 0 || phase == 1) && poll(),
                };
                if phase == 0 {
                    match kind {
                        Kind::Unsigned => assert_eq!(
                            read_world_u64_overlay_v1(
                                WorldU64FieldV1::NextModuleInstanceId,
                                &mut read,
                            )
                            .unwrap(),
                            7
                        ),
                        Kind::Cursor => assert_eq!(
                            read_delivery_cursor_overlay_v1(&mut read,).unwrap(),
                            ProductValidationDeliveryCursor::default()
                        ),
                        Kind::IdSet => assert!(
                            world_id_set_contains_overlay_v1(
                                WorldIdSetFieldV1::CompletedLogisticsRoutes,
                                WorldIdSetKeyRefV1::Text("done"),
                                &mut read,
                            )
                            .unwrap()
                        ),
                    }
                    Ok(false)
                } else {
                    let mut write = MemberRecordWriteContext {
                        sink: &mut output,
                        memory: &memory,
                        io: &mut output_io,
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 16_384,
                        },
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 16,
                            record_bytes: 4096,
                        },
                        cancelled: || phase == 2 && poll(),
                    };
                    match kind {
                        Kind::Unsigned => change_world_u64_overlay_v1(
                            WorldU64FieldV1::NextModuleInstanceId,
                            7,
                            7,
                            &mut read,
                            &mut write,
                            &guard,
                            &live,
                        ),
                        Kind::Cursor => change_delivery_cursor_overlay_v1(
                            &ProductValidationDeliveryCursor::default(),
                            &ProductValidationDeliveryCursor::default(),
                            &mut read,
                            &mut write,
                            &guard,
                            &live,
                        ),
                        Kind::IdSet => change_world_id_set_overlay_v1(
                            WorldIdSetChangeV1 {
                                field: WorldIdSetFieldV1::CompletedLogisticsRoutes,
                                key: WorldIdSetKeyRefV1::Text("done"),
                                expected: true,
                                after: true,
                            },
                            &mut read,
                            &mut write,
                            &guard,
                            &live,
                        ),
                    }
                }
            };
            assert_eq!(
                output_io.nodes_remaining, before,
                "no-op must not encode values"
            );
            let seal_guard = transaction.guard();
            let sealed = transaction.seal(&seal_guard, &live, 1);
            if phase == 0 {
                resolver_polls = polls.get();
                assert!(resolver_polls > 0);
                assert_eq!(sealed.unwrap().record.writes.len(), 1);
            } else {
                outcomes.push(Outcome {
                    kind,
                    write_cancel: phase == 2,
                    resolver_polls,
                    polls: polls.get(),
                    cancelled: matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                    ),
                    aborted: matches!(sealed, Err(ExecutionError::Aborted)),
                    sealed_with_prefix: sealed
                        .as_ref()
                        .is_ok_and(|value| value.record.writes.len() == 1),
                });
            }
        }
    }
    for outcome in &outcomes {
        assert!(
            outcome.cancelled
                && outcome.aborted
                && !outcome.sealed_with_prefix
                && outcome.polls == outcome.resolver_polls + 1,
            "N+1 cancellation must stop {:?} write_channel={} after {} resolver polls: {outcome:?}; all={outcomes:?}",
            outcome.kind,
            outcome.write_cancel,
            outcome.resolver_polls,
        );
    }
}
