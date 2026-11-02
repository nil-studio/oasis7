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
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut writer = store.value_session(physical());
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [0x79; 32],
        &state,
        memory,
        TransactionLimits {
            max_reads: 8,
            max_writes: 4,
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
        records_remaining: 32,
        json: JsonWriteWorkV1 {
            cells_remaining: 128,
            json_nodes_remaining: 16,
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
        for group in [
            CognitionOptionalGroupV1::ReorgEpoch,
            CognitionOptionalGroupV1::ContinuationEvaluations,
        ] {
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
        }
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(sealed.record.writes.len(), 2); // Epoch recreated unchanged; evaluations is new.
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
                cells_remaining: 128,
                json_nodes_remaining: 16,
                max_depth: 8,
                max_payload_bytes: 4096,
            },
            paths_remaining: 16,
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
        let heap = memory.reserve(640 << 10, MemoryClass::Ordinary).unwrap();
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
    assert!(
        raw.json_entry(
            CognitionJsonFieldV1::ContinuationEvaluations,
            &sealed.record.after.digest().unwrap()
        )
        .unwrap()
        .is_some()
    );
}
