use super::*;
use oasis7_formal_protocol::incremental_execution::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{
    incremental_cognition_control_overlay::*, incremental_cognition_family::*,
    incremental_field_overlay::WorldFieldOverlayContextV1,
};
pub fn exercise(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ExecutionParent,
    family: &ReservedValue<CognitionCurrentStateQualificationV1>,
) {
    let field = CognitionControlFieldV1::JournalHeadSeq;
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut writer = store.value_session(physical());
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [0x73; 32],
        &state,
        memory,
        TransactionLimits {
            max_reads: 8,
            max_writes: 2,
            max_facts: 0,
            max_pending_node_bytes: 2 << 20,
        },
    )
    .unwrap();
    let live = parent.fence().unwrap();
    let mut io = fixture::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write_io = fixture::logical();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    {
        let mut context = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        };
        for (before, after) in [(u64::MAX, 0), (0, 1), (1, 1)] {
            check_cognition_control_overlay_v1(
                field,
                CognitionControlValueV1::Unsigned(before),
                &mut context,
            )
            .unwrap();
            let guard = context.transaction.guard();
            assert_eq!(
                change_cognition_control_v1(
                    field,
                    (
                        CognitionControlValueV1::Unsigned(before),
                        CognitionControlValueV1::Unsigned(after)
                    ),
                    &mut context,
                    &mut write,
                    &guard,
                    &live
                )
                .unwrap(),
                before != after
            );
        }
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(sealed.record.writes.len(), 1);
    let verifier = TransitionVerifier {
        parent,
        operation: [0x73; 32],
        source: &state,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 2 << 20,
    };
    let root = parent
        .roots
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Allocator)
        .unwrap();
    let mut context = WorldFieldReadContextV1 {
        root,
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let qualified = accept_cognition_current_scalar_transition_v1(
        family,
        &verifier,
        &sealed.record,
        &mut context,
        &mut oasis7_runtime::incremental_json_records::JsonScalarChangeWorkV1 {
            json: oasis7_runtime::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 64,
                json_nodes_remaining: 8,
                max_depth: 4,
                max_payload_bytes: 4096,
            },
            paths_remaining: 4096,
            max_pending_pairs: 8,
        },
    )
    .unwrap();
    qualified
        .validate_binding(&sealed.record.after.digest().unwrap())
        .unwrap();
    let generic = verifier.verify(&sealed.record).unwrap();
    let after = sealed
        .record
        .after
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Allocator)
        .unwrap();
    let heap = memory.reserve(640 << 10, MemoryClass::Ordinary).unwrap();
    let delta = heap.retain(StateDeltaV1 {
        before: root.clone(),
        after: after.clone(),
        writes: vec![sealed.record.writes[0].mutation.clone()],
        new_nodes: generic.new_nodes.clone(),
    });
    writer
        .put_state_pages_cancelled(&delta, memory, &mut || false)
        .unwrap();
    let mut context = WorldFieldReadContextV1 {
        root: after,
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    assert_eq!(
        read_cognition_control_v1(field, &mut context, &mut std::io::sink()).unwrap(),
        CognitionControlReadV1::Unsigned(1)
    );
    no_op_cancellation(store, memory, parent);
}

fn no_op_cancellation(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ExecutionParent,
) {
    use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
    use std::cell::Cell;
    let _text_charge = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
    let left = "取".repeat(1024);
    let right = "取".repeat(1024);
    assert_ne!(left.as_ptr(), right.as_ptr());
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut writer = store.value_session(physical());
    for (field, before, expected, after) in [
        (
            CognitionControlFieldV1::JournalHeadSeq,
            CognitionControlValueV1::Unsigned(u64::MAX),
            CognitionControlValueV1::Unsigned(1),
            CognitionControlValueV1::Unsigned(1),
        ),
        (
            CognitionControlFieldV1::JournalHeadDigest,
            CognitionControlValueV1::Text("原digest"),
            CognitionControlValueV1::Text(&left),
            CognitionControlValueV1::Text(&right),
        ),
    ] {
        for channel in 0..2 {
            let read_polls = Cell::new(0usize);
            let write_polls = Cell::new(0usize);
            let target = Cell::new(usize::MAX);
            let mut tx = ExecutionTransaction::begin(
                parent.clone(),
                [0x78 + channel; 32],
                &state,
                memory,
                TransactionLimits {
                    max_reads: 8,
                    max_writes: 2,
                    max_facts: 0,
                    max_pending_node_bytes: 2 << 20,
                },
            )
            .unwrap();
            let live = parent.fence().unwrap();
            let mut io = fixture::logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            };
            let mut write_io = fixture::logical();
            let mut write_keys = SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            };
            {
                let mut context = WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: &values,
                        memory,
                        class: MemoryClass::Ordinary,
                        io: &mut io,
                    },
                    key_work: &mut keys,
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || state.take_read_error().map(state_error),
                    cancelled: || {
                        let n = read_polls.get() + 1;
                        read_polls.set(n);
                        channel == 0 && n == target.get()
                    },
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory,
                    io: &mut write_io,
                    key_work: &mut write_keys,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 4096,
                        record_bytes: 8192,
                    },
                    cancelled: || {
                        let n = write_polls.get() + 1;
                        write_polls.set(n);
                        channel == 1 && n == target.get()
                    },
                };
                let guard = context.transaction.guard();
                assert!(
                    change_cognition_control_v1(
                        field,
                        (before, expected),
                        &mut context,
                        &mut write,
                        &guard,
                        &live
                    )
                    .unwrap()
                );
                read_polls.set(0);
                write_polls.set(0);
                check_cognition_control_overlay_v1(field, expected, &mut context).unwrap();
                let n = read_polls.get();
                assert!(n > 0);
                assert_eq!(write_polls.get(), 0);
                read_polls.set(0);
                write_polls.set(0);
                target.set(n + 1);
                let guard = context.transaction.guard();
                assert!(matches!(
                    change_cognition_control_v1(
                        field,
                        (expected, after),
                        &mut context,
                        &mut write,
                        &guard,
                        &live
                    ),
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                ));
                assert_eq!(
                    if channel == 0 {
                        read_polls.get()
                    } else {
                        write_polls.get()
                    },
                    n + 1
                );
                eprintln!(
                    "Cognition control {field:?} independent getter N={n} NoOp N+1={} channel={channel}",
                    n + 1
                );
            }
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 2),
                Err(ExecutionError::Aborted)
            ));
        }
    }
}
