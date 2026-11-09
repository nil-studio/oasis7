use super::*;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateScanBudget,
        StateWriteV1,
    },
    incremental_value::write_value,
};
use oasis7_world_state::{
    incremental_cognition_absence::qualify_cognition_current_absence_v1,
    incremental_cognition_fields::CognitionControlFieldV1,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_semantic_key::hash_semantic_key_v1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};

#[test]
fn absence_audit_rejects_unknown_orphan_and_wrong_namespace_cognition_cells() {
    for mode in 0..3 {
        let directory = Directory::new();
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("absence"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut writer = store.value_session(physical());
        let mut io = logical();
        let value = write_value(&mut &[0x35u8][..], 1, &mut writer, &memory, &mut io, || {
            false
        })
        .unwrap()
        .digest()
        .unwrap();
        let mut roots = RootSetV1::empty("absence-fixture".into(), [0x55; 32]).unwrap();
        let _heap = memory.reserve(640 << 10, MemoryClass::Ordinary).unwrap();
        let (namespace, key) = match mode {
            0 => (
                Ns::Artifacts,
                WorldFieldKeyV1::new(
                    FieldDescriptor {
                        owner: FieldOwner::Cognition,
                        name: "unregistered_cognition",
                        destination: FieldDestination::State(Ns::Artifacts),
                        shape: FieldShape::Value,
                    },
                    WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .unwrap(),
            ),
            1 => {
                let semantic = hash_semantic_key_v1(
                    &(),
                    &memory,
                    MemoryClass::Ordinary,
                    &mut SemanticKeyBudget {
                        bytes_remaining: 1024,
                    },
                    &mut || false,
                )
                .unwrap();
                (
                    Ns::Allocator,
                    WorldFieldKeyV1::new(
                        CognitionControlFieldV1::JournalHeadSeq.descriptor(),
                        WorldFieldRecordKindV1::Entry,
                        Some(semantic.digest),
                    )
                    .unwrap(),
                )
            }
            _ => (
                Ns::Business,
                WorldFieldKeyV1::new(
                    CognitionControlFieldV1::Schema.descriptor(),
                    WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .unwrap(),
            ),
        };
        // Actual generic-valid tree and Value closure; semantic ownership is
        // rejected even though no complete enabled schema can be found.
        let delta = _heap.retain(
            StateDeltaV1::build(
                StateRootV1::empty(namespace).unwrap(),
                vec![StateWriteV1 {
                    key: key.as_bytes().to_vec(),
                    before: None,
                    after: Some(value),
                }],
                &state,
                1 << 20,
            )
            .unwrap(),
        );
        writer
            .put_state_pages_cancelled(&delta, &memory, &mut || false)
            .unwrap();
        *roots
            .state
            .iter_mut()
            .find(|root| root.namespace == namespace)
            .unwrap() = delta.after.clone();
        let values = store.value_session(physical());
        let mut read_io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: roots.state_root(Ns::Artifacts),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut keys,
            semantic_bytes: 256,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        context.values.verify_closure(&value, 1, || false).unwrap();
        let result = qualify_cognition_current_absence_v1(
            &roots,
            &mut context,
            &mut StateScanBudget {
                nodes_remaining: 4096,
                read_bytes_remaining: 4 << 20,
                leaves_remaining: 128,
            },
            &mut 128,
        );
        assert!(
            matches!(
                result,
                Err(ValueTreeError::Invalid(
                    "Cognition cell present during absence audit"
                ))
            ),
            "mode={mode}"
        );
    }
}
