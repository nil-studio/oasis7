use super::*;
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueNodeWriter};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_world_state::{
    incremental_field_member::MemberRecordLimitsV1,
    incremental_module_state_recovery::ModuleStateContentBudgetV1,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Default)]
struct Pages(BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>);
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.0.len() == 65536 && !self.0.contains_key(&id) {
            return Err(ValueTreeError::ResourceLimited("bounded GEN fixture pages"));
        }
        self.0.insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let value = self
            .0
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("GEN missing page"))?;
        if class != MemoryClass::Ordinary || value.len() > max || !value.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("GEN page admission"));
        }
        Ok(value)
    }
}
impl StatePageWriter for Pages {}
impl SchedulePageWriter for Pages {}
fn work() -> RuntimeGenesisWorkV1 {
    RuntimeGenesisWorkV1 {
        typed: super::super::incremental_genesis_typed::test_work(),
        raw: RuntimeRawGenesisWorkV1 {
            writes_remaining: 1000,
            agents_remaining: 100,
            instances_remaining: 100,
            instance_count_rows_remaining: 100,
            schedule_entries_remaining: 100,
            module_states: ModuleStateContentBudgetV1 {
                states_remaining: 100,
                logical_bytes_remaining: 1 << 20,
                max_state_bytes: 1 << 20,
            },
            max_scalar_bytes: 1 << 20,
        },
    }
}
#[test]
fn runtime_whole_genesis_streams_all_native_and_runtime_fields_with_original_actions() {
    let memory = MemoryBudget::new(512 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    world.submit_action(crate::Action::QueryObservation {
        agent_id: "original-a".into(),
    });
    world.submit_action(crate::Action::QueryObservation {
        agent_id: "original-b".into(),
    });
    let mut pages = Pages::default();
    let mut limits = work();
    let mut io = ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut writes = BTreeSet::new();
    let mut metadata = 0;
    let mut cognition_metadata = BTreeSet::new();
    let mut originals = BTreeMap::new();
    let mut original_work = oasis7_wasm_abi::CanonicalCborWorkV1 {
        visits_remaining: 100_000,
        bytes_remaining: 1 << 20,
        max_depth: 32,
        max_map_keys: 64,
        max_key_bytes: 4096,
    };
    let result = world
        .prepare_iws_genesis_v1(
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut io,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 8 << 20,
                    record_bytes: 9 << 20,
                },
                cancelled: || false,
            },
            &mut limits,
            &mut |_, _| Ok(()),
            &mut |ordinal, envelope| {
                assert!(std::ptr::eq(
                    envelope,
                    &world.pending_actions[ordinal as usize]
                ));
                let receipt=super::super::incremental_action_original::persist_action_original_v1(
                    envelope,&memory,&mut original_work,&mut ||false,1<<20,
                    &mut |stream:&mut dyn super::super::incremental_action_original::ActionOriginalStreamV1,
                          cancelled:&mut dyn FnMut()->bool| {
                        let length=usize::try_from(stream.original_bytes()).unwrap();
                        assert!(length<=1<<20);
                        let charge=memory.reserve(length,MemoryClass::Ordinary)
                            .map_err(oasis7_formal_protocol::incremental_execution::ExecutionError::Memory)?;
                        let mut bytes=Vec::with_capacity(length);
                        stream.write_to(&mut bytes)?;
                        assert_eq!(bytes.len(),length);
                        assert_eq!(blake3::hash(&bytes).as_bytes(),&stream.original_id());
                        if cancelled(){return Err(ValueTreeError::Cancelled.into());}
                        assert!(originals.insert(stream.original_id(),charge.retain(bytes)).is_none());
                        Ok(())
                    }).unwrap();
                Ok(oasis7_formal_protocol::incremental_artifacts::OriginalArtifactRefV1 { id: receipt.original_id(), byte_len: receipt.original_bytes() })
            },
            &mut |_, _, _| Err(BodyMutationError::Invalid("unexpected mailbox")),
            &mut |row| {
                assert!(row.uses_budget(&memory));
                assert_eq!(row.mutation.before, None);
                if row
                    .mutation
                    .key
                    .starts_with(b"oasis7.iws.world-field-key.v1\0")
                {
                    let parsed = WorldFieldKeyV1::from_bytes(&row.mutation.key).unwrap();
                    if parsed.kind == WorldFieldRecordKindV1::Metadata {
                        metadata += 1;
                        if parsed.owner
                            == oasis7_world_state::incremental_projection::FieldOwner::Cognition
                        {
                            assert!(cognition_metadata.insert(parsed.name.to_owned()));
                        }
                    }
                }
                assert!(writes.insert((row.namespace as u8, row.mutation.key.clone())));
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(result.typed.world_state.native_fields, 97);
    assert_eq!(result.typed.runtime_fields, 46);
    assert_eq!(result.typed.cognition_fields, 9);
    assert_eq!(
        cognition_metadata,
        BTreeSet::from(
            [
                "schema_version",
                "journal.schema_version",
                "journal.head_seq",
                "journal.head_digest",
                "feedback_outbox",
                "idempotency_index",
                "staged_actions",
                "scheduler_state",
                "continuations",
            ]
            .map(str::to_owned)
        )
    );
    assert_eq!(metadata, 152);
    assert_eq!(result.raw_rows, 6);
    assert_eq!(originals.len(), 2);
    for original in originals.values() {
        let envelope: ActionEnvelope = serde_cbor::from_slice(original).unwrap();
        assert!(
            world
                .pending_actions
                .iter()
                .any(|expected| expected == &envelope)
        );
    }
    assert!(io.write_bytes_remaining < 256 << 20);
    assert!(keys.bytes_remaining < 64 << 20);
}
