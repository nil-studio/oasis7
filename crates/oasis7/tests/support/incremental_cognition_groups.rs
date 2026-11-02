use super::*;
use oasis7_formal_protocol::incremental_execution::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{
    incremental_cognition_family::*, incremental_cognition_groups::*,
    incremental_field_overlay::WorldFieldOverlayContextV1, incremental_json_records::*,
};
pub fn exercise(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ExecutionParent,
    family: &ReservedValue<CognitionCurrentStateQualificationV1>,
) {
    for group in [
        CognitionOptionalGroupV1::Economy,
        CognitionOptionalGroupV1::Delegation,
        CognitionOptionalGroupV1::ContinuationContexts,
        CognitionOptionalGroupV1::ContinuationEvaluations,
        CognitionOptionalGroupV1::ReorgEpoch,
    ] {
        exercise_group(store, memory, parent, family, group);
    }
}
fn exercise_group(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ExecutionParent,
    family: &ReservedValue<CognitionCurrentStateQualificationV1>,
    group: CognitionOptionalGroupV1,
) {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let whole = PageIoBudget {
        reads_remaining: 8_000_000,
        read_bytes_remaining: 2 << 30,
        ..physical()
    };
    let values = store.value_session(whole.clone());
    let mut writer = store.value_session(whole);
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [0x79; 32],
        &state,
        memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 28,
            max_facts: 0,
            max_pending_node_bytes: 2 << 20,
        },
    )
    .unwrap();
    let live = parent.fence().unwrap();
    let mut io = fixture::logical();
    let mut write_io = fixture::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let source = super::source(true);
    let source = CognitionEnvelopeRefV1::new(&source).unwrap();
    let mut work = CognitionGroupWriteWorkV1 {
        records_remaining: 128,
        json: JsonWriteWorkV1 {
            cells_remaining: 4096,
            json_nodes_remaining: 512,
            max_json_depth: 8,
            max_pending_node_bytes: 2 << 20,
        },
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
        let before = read_cognition_group_overlay_v1(group, family, &mut context).unwrap();
        let guard = context.transaction.guard();
        assert_eq!(
            change_cognition_group_v1(
                &before,
                None,
                &mut context,
                &mut write,
                &mut work,
                &guard,
                &live
            )
            .unwrap(),
            before.present()
        );
        let absent = read_cognition_group_overlay_v1(group, family, &mut context).unwrap();
        assert!(!absent.present());
        let guard = context.transaction.guard();
        assert!(
            !change_cognition_group_v1(
                &absent,
                None,
                &mut context,
                &mut write,
                &mut work,
                &guard,
                &live
            )
            .unwrap()
        );
        let guard = context.transaction.guard();
        assert!(
            change_cognition_group_v1(
                &absent,
                Some(source),
                &mut context,
                &mut write,
                &mut work,
                &guard,
                &live
            )
            .unwrap()
        );
        let present = read_cognition_group_overlay_v1(group, family, &mut context).unwrap();
        assert!(present.present());
        if group == CognitionOptionalGroupV1::ContinuationEvaluations {
            use oasis7_runtime::incremental_cognition_json_overlay::*;
            let proof = qualify_cognition_group_json_overlay_v1(
                &present,
                CognitionJsonFieldV1::ContinuationEvaluations,
                &mut context,
                &mut JsonReadWorkV1 {
                    cells_remaining: 128,
                    json_nodes_remaining: 16,
                    max_depth: 8,
                    max_payload_bytes: 4096,
                },
            )
            .unwrap();
            let guard = context.transaction.guard();
            let updated = change_cognition_json_scalar_v1(
                CognitionJsonScalarChangeV1 {
                    field: CognitionJsonFieldV1::ContinuationEvaluations,
                    qualification: &proof,
                    path: &[
                        JsonPathSegmentV1::Name("n"),
                        JsonPathSegmentV1::Name("tick"),
                    ],
                    change: (
                        JsonScalarRefV1::Unsigned(u64::MAX),
                        JsonScalarRefV1::Unsigned(0),
                    ),
                },
                &mut context,
                &mut write,
                &mut oasis7_runtime::incremental_cognition_json_overlay::CognitionJsonPointWorkV1 {
                    point: JsonWriteWorkV1 {
                        cells_remaining: 128,
                        json_nodes_remaining: 16,
                        max_json_depth: 8,
                        max_pending_node_bytes: 2 << 20,
                    },
                    changed: JsonChangeWorkV1 {
                        json: JsonReadWorkV1 {
                            cells_remaining: 128,
                            json_nodes_remaining: 16,
                            max_depth: 8,
                            max_payload_bytes: 4096,
                        },
                        paths_remaining: 4096,
                        max_pending_pairs: 8,
                    },
                },
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            assert_ne!(proof.body_root(), updated.body_root());
        }
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(
        sealed.record.writes.len(),
        match group {
            CognitionOptionalGroupV1::Economy => 28,
            CognitionOptionalGroupV1::Delegation => 8,
            CognitionOptionalGroupV1::ContinuationEvaluations => 2,
            _ => 0,
        }
    );
    let verifier = TransitionVerifier {
        parent,
        operation: [0x79; 32],
        source: &state,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 2 << 20,
    };
    let mut context = WorldFieldReadContextV1 {
        root: &parent.roots.state[0],
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
    let accepted = accept_cognition_current_lifecycle_transition_v1(
        family,
        &verifier,
        &sealed.record,
        &mut context,
        &mut JsonChangeWorkV1 {
            json: JsonReadWorkV1 {
                cells_remaining: 4096,
                json_nodes_remaining: 512,
                max_depth: 8,
                max_payload_bytes: 4096,
            },
            paths_remaining: 4096,
            max_pending_pairs: 8,
        },
    )
    .unwrap();
    accepted
        .validate_binding(&sealed.record.after.digest().unwrap())
        .unwrap();
    let generic = verifier.verify(&sealed.record).unwrap();
    for root in &sealed.record.after.state {
        let before = parent
            .roots
            .state
            .iter()
            .find(|old| old.namespace == root.namespace)
            .unwrap();
        // Charge each actual cloned allocation, including digest Strings and
        // Vec metadata/capacity slack, before constructing the test-only delta.
        let copied_nodes = generic
            .new_nodes
            .iter()
            .map(|node| 512 + 2 * node.bytes.len() + 2 * node.digest.len())
            .sum::<usize>();
        let copied_writes = sealed
            .record
            .writes
            .iter()
            .filter(|write| write.namespace == root.namespace)
            .map(|write| 512 + 2 * write.mutation.key.len())
            .sum::<usize>();
        let heap = memory
            .reserve(
                (128 << 10) + copied_nodes + copied_writes,
                MemoryClass::Ordinary,
            )
            .unwrap();
        let delta = heap.retain(StateDeltaV1 {
            before: before.clone(),
            after: root.clone(),
            writes: sealed
                .record
                .writes
                .iter()
                .filter(|write| write.namespace == root.namespace)
                .map(|write| write.mutation.clone())
                .collect(),
            new_nodes: generic.new_nodes.clone(),
        });
        writer
            .put_state_pages_cancelled(&delta, memory, &mut || false)
            .unwrap();
    }
    let raw = super::family_faults::qualify_ordinary(store, memory, &sealed.record.after);
    raw.validate_binding(&sealed.record.after.digest().unwrap())
        .unwrap();
    let _parent_copy = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let after_parent = ExecutionParent {
        roots: sealed.record.after.clone(),
        commit: [0x7a; 32],
        authority: parent.authority.clone(),
    };
    delete_and_reject_partial(store, memory, &after_parent, &accepted, group);
    eprintln!(
        "Cognition optional group {group:?} accepted writes={} remaining records={} state={:?} values={:?} writer={:?}",
        sealed.record.writes.len(),
        work.records_remaining,
        state.remaining_io(),
        values.remaining_io(),
        writer.remaining_io()
    );
}

fn delete_and_reject_partial(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ExecutionParent,
    family: &ReservedValue<CognitionCurrentStateQualificationV1>,
    group: CognitionOptionalGroupV1,
) {
    use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
    for partial in [true, false] {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut writer = store.value_session(physical());
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [0x7b; 32],
            &state,
            memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 28,
                max_facts: 0,
                max_pending_node_bytes: 2 << 20,
            },
        )
        .unwrap();
        let live = parent.fence().unwrap();
        let mut io = fixture::logical();
        let mut write_io = fixture::logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
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
            let expected = read_cognition_group_overlay_v1(group, family, &mut context).unwrap();
            assert!(expected.present());
            if partial {
                let descriptor = match group {
                    CognitionOptionalGroupV1::Economy => {
                        CognitionControlFieldV1::EconomySchema.descriptor()
                    }
                    CognitionOptionalGroupV1::Delegation => {
                        CognitionJsonFieldV1::DelegationGrants.descriptor()
                    }
                    CognitionOptionalGroupV1::ContinuationContexts => {
                        CognitionJsonFieldV1::ContinuationContexts.descriptor()
                    }
                    CognitionOptionalGroupV1::ContinuationEvaluations => {
                        CognitionJsonFieldV1::ContinuationEvaluations.descriptor()
                    }
                    CognitionOptionalGroupV1::ReorgEpoch => {
                        CognitionControlFieldV1::ReorgInvalidationEpoch.descriptor()
                    }
                };
                let key = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
                    .unwrap();
                let namespace = WorldFieldKeyV1::from_bytes(key.as_bytes())
                    .unwrap()
                    .namespace;
                context
                    .transaction
                    .write(namespace, key.as_bytes(), None)
                    .unwrap();
            } else {
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
                let guard = context.transaction.guard();
                let mut work = CognitionGroupWriteWorkV1 {
                    records_remaining: 0,
                    json: JsonWriteWorkV1 {
                        cells_remaining: 0,
                        json_nodes_remaining: 0,
                        max_json_depth: 0,
                        max_pending_node_bytes: 0,
                    },
                };
                assert!(
                    change_cognition_group_v1(
                        &expected,
                        None,
                        &mut context,
                        &mut write,
                        &mut work,
                        &guard,
                        &live
                    )
                    .unwrap()
                );
            }
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent,
            operation: [0x7b; 32],
            source: &state,
            memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 2 << 20,
        };
        let generic = verifier.verify(&sealed.record).unwrap();
        let mut context = WorldFieldReadContextV1 {
            root: &parent.roots.state[0],
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
        let result = accept_cognition_current_lifecycle_transition_v1(
            family,
            &verifier,
            &sealed.record,
            &mut context,
            &mut JsonChangeWorkV1 {
                json: JsonReadWorkV1 {
                    cells_remaining: 0,
                    json_nodes_remaining: 0,
                    max_depth: 0,
                    max_payload_bytes: 0,
                },
                paths_remaining: 128,
                max_pending_pairs: 0,
            },
        );
        if partial {
            assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ));
        } else {
            result
                .unwrap()
                .validate_binding(&sealed.record.after.digest().unwrap())
                .unwrap();
            for root in &sealed.record.after.state {
                let before = parent
                    .roots
                    .state
                    .iter()
                    .find(|old| old.namespace == root.namespace)
                    .unwrap();
                let copied_nodes = generic
                    .new_nodes
                    .iter()
                    .map(|node| 512 + 2 * node.bytes.len() + 2 * node.digest.len())
                    .sum::<usize>();
                let copied_writes = sealed
                    .record
                    .writes
                    .iter()
                    .filter(|write| write.namespace == root.namespace)
                    .map(|write| 512 + 2 * write.mutation.key.len())
                    .sum::<usize>();
                let heap = memory
                    .reserve(
                        (128 << 10) + copied_nodes + copied_writes,
                        MemoryClass::Ordinary,
                    )
                    .unwrap();
                let delta = heap.retain(StateDeltaV1 {
                    before: before.clone(),
                    after: root.clone(),
                    writes: sealed
                        .record
                        .writes
                        .iter()
                        .filter(|write| write.namespace == root.namespace)
                        .map(|write| write.mutation.clone())
                        .collect(),
                    new_nodes: generic.new_nodes.clone(),
                });
                writer
                    .put_state_pages_cancelled(&delta, memory, &mut || false)
                    .unwrap();
            }
            let raw = super::family_faults::qualify_ordinary(store, memory, &sealed.record.after);
            raw.validate_binding(&sealed.record.after.digest().unwrap())
                .unwrap();
            eprintln!(
                "Cognition optional group {group:?} genuine final deletion accepted writes={} and generic-valid orphan rejected",
                sealed.record.writes.len()
            );
        }
    }
}
