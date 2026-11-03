use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
#[test]
fn generic_valid_scheduler_mutations_still_require_typed_closure() {
    for mode in 0..4 {
        let compression = PageCompression::Raw;
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&directory.0.join("ops"), memory.clone(), compression)
                .unwrap();
        let roots = seed(&store, &memory);
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Scheduler)
                .unwrap(),
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family = qualify_runtime_scheduler_controls_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            32,
        )
        .unwrap();

        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 16,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut writer = store.value_session(physical());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: limits(),
            cancelled: || false,
        };
        // Independent literals; no owner enum/profile helper supplies the malformed row.
        let descriptor = FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: "scheduler_cursor",
            shape: FieldShape::Value,
            destination: FieldDestination::State(StateNamespaceV1::Scheduler),
        };
        let digest = *blake3::hash(b"oasis7.iws.semantic-key.v1\0\0").as_bytes();
        let original =
            WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Entry, Some(digest)).unwrap();
        let (key, value) = match mode {
            0 => (original, None), // required global cannot disappear
            1 => {
                let row = write_member_record_v1(
                    descriptor,
                    WorldFieldRecordKindV1::Entry,
                    &(),
                    MemberPayloadRefV1::Bool(false),
                    &mut write,
                )
                .unwrap();
                (row.field_key, Some(row.value_root))
            }
            2 => {
                let row = write_member_record_v1(
                    descriptor,
                    WorldFieldRecordKindV1::Entry,
                    &"extra",
                    MemberPayloadRefV1::Text(""),
                    &mut write,
                )
                .unwrap();
                (row.field_key, Some(row.value_root))
            }
            3 => {
                let key = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
                    .unwrap();
                let row = write_member_record_v1(
                    descriptor,
                    WorldFieldRecordKindV1::Entry,
                    &(),
                    MemberPayloadRefV1::Bool(false),
                    &mut write,
                )
                .unwrap();
                (key, Some(row.value_root))
            }
            _ => unreachable!(),
        };
        tx.write(StateNamespaceV1::Scheduler, key.as_bytes(), value)
            .unwrap();
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let generic = verifier.verify(&sealed.record).unwrap();
        let state2 = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values2 = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut receiver = read_ctx!(
            sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Scheduler)
                .unwrap(),
            &state2,
            &values2,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let result = accept_runtime_scheduler_controls_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut receiver,
            &mut 8192,
            32,
        );
        assert!(
            matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ),
            "typed mode={mode}: {result:?}"
        );
        for node in &generic.new_nodes {
            writer
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        let state3 = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values3 = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut raw = read_ctx!(
            sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Scheduler)
                .unwrap(),
            &state3,
            &values3,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let result = qualify_runtime_scheduler_controls_v1(
            &sealed.record.after,
            &mut raw,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            32,
        );
        assert!(
            matches!(result, Err(ValueTreeError::Invalid(_))),
            "raw mode={mode}: {result:?}"
        );
    }
}
