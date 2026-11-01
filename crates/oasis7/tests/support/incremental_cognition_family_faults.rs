//! Generic-valid state trees are insufficient to mint typed Cognition closure.
use super::*;
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_runtime::incremental_cognition_family::qualify_cognition_current_state_v1;

pub fn reject_malformed_outer(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &RootSetV1,
) {
    let field = CognitionJsonFieldV1::FeedbackOutbox.descriptor();
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None).unwrap();
    let root = parent
        .state
        .iter()
        .find(|root| field.destination == FieldDestination::State(root.namespace))
        .unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let old = root.get(metadata.as_bytes(), &state).unwrap().unwrap();
    // One continuous finite writer scope covers every independent mutation.
    let mut writer = store.value_session(physical());
    for case in 0..3 {
        let (key, before, after) = match case {
            0 => {
                let unknown = FieldDescriptor {
                    name: "unregistered_cognition_field",
                    ..field
                };
                (
                    WorldFieldKeyV1::new(unknown, WorldFieldRecordKindV1::Metadata, None).unwrap(),
                    None,
                    Some(old),
                )
            }
            1 => (metadata.clone(), Some(old), None),
            _ => (
                WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Entry, Some([0x7f; 32]))
                    .unwrap(),
                None,
                Some(old),
            ),
        };
        let admission = memory
            .reserve((512 << 10) + (128 << 10), MemoryClass::Ordinary)
            .unwrap();
        let delta = admission.retain(
            StateDeltaV1::build(
                root.clone(),
                vec![StateWriteV1 {
                    key: key.as_bytes().to_vec(),
                    before,
                    after,
                }],
                &state,
                2 << 20,
            )
            .unwrap(),
        );
        writer
            .put_state_pages_cancelled(&delta, memory, &mut || false)
            .unwrap();
        // This is a fully authenticated/persisted Merkle tree, not a missing-page
        // or I/O-budget error masquerading as a schema rejection.
        assert_eq!(delta.after.get(key.as_bytes(), &state).unwrap(), after);
        let _root_charge = memory.reserve(32 << 10, MemoryClass::Ordinary).unwrap();
        let mut candidate = parent.clone();
        *candidate
            .state
            .iter_mut()
            .find(|entry| entry.namespace == root.namespace)
            .unwrap() = delta.after.clone();
        let values = store.value_session(physical());
        let mut io = fixture::logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: &candidate.state[0],
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
        let result = qualify_cognition_current_state_v1(
            &candidate,
            &mut context,
            &mut StateScanBudget {
                nodes_remaining: 100_000,
                read_bytes_remaining: 64 << 20,
                leaves_remaining: 1000,
            },
            &mut oasis7_runtime::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 4096,
                json_nodes_remaining: 512,
                max_depth: 4,
                max_payload_bytes: 4096,
            },
            &mut 1000,
        );
        assert!(
            matches!(
                result,
                Err(ValueTreeError::Invalid(
                    "cognition current-state family closure"
                ))
            ),
            "case={case}: {result:?}"
        );
    }
}
