use super::execution_test_support::runtime_genesis_work as work;
use super::*;
#[cfg(test)]
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueNodeWriter};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_world_state::{
    incremental_field_member::MemberRecordLimitsV1, incremental_semantic_key::SemanticKeyBudget,
};
#[cfg(test)]
use oasis7_world_state::{
    incremental_module_state_recovery::ModuleStateContentBudgetV1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
#[cfg(test)]
use std::collections::BTreeSet;
use std::{collections::BTreeMap, sync::Arc};
#[path = "incremental_genesis_projection_commitment_tests.rs"]
pub(in crate::world) mod projection_commitment;
#[derive(Default)]
struct Pages(BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>);
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.0.len() == 262144 && !self.0.contains_key(&id) {
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
#[cfg(test)]
#[path = "incremental_current_fixture_work.rs"]
mod current_fixture_work;
#[cfg(test)]
use current_fixture_work::set_current_fixture_outer_scan_work;
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
    let mut complete_rows = Vec::new();
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
                complete_rows.push((*row).clone());
                assert!(writes.insert((row.namespace as u8, row.mutation.key.clone())));
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(result.typed.world_state.native_fields, 98);
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
    complete::exercise(&memory, &mut pages, &complete_rows, &originals);
}
#[cfg(test)]
#[path = "incremental_runtime_complete_tests.rs"]
mod complete;

#[cfg(test)]
#[path = "incremental_world_raw_complete_tests.rs"]
mod raw_complete;

#[cfg(test)]
#[path = "incremental_module_execution_tests.rs"]
pub(in crate::world) mod module_execution;

#[test]
fn projected_genesis_event_commitment_retains_the_complete_journal_and_original_context() {
    projected_genesis_complete_journal_case();
}

pub(super) fn projected_genesis_complete_journal_case() {
    use oasis7_formal_protocol::controlled_authority::formal::{
        FormalError,
        bounded_context::{FormalBoundedContextV1, FormalBoundedWorkV1},
    };

    fn context_refusal(error: BodyMutationError, note: &str) {
        let BodyMutationError::Original(error) = error else {
            panic!("expected original Common refusal: {error:?}");
        };
        assert!(matches!(
            error.downcast_ref::<FormalError>(),
            Some(FormalError::Invalid(actual)) if actual == note
        ));
    }

    let memory = MemoryBudget::new(512 << 20, 16 << 20).unwrap();
    let _input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let baseline = memory.used();
    let mut world = World::new_with_state(crate::WorldState::default());
    // A genuine publication across ID eras, with neither event at capture time.
    // GEN commits the entire journal, not a numeric-ID or current-time suffix.
    world.next_event_id = u64::MAX;
    world.next_event_id_era = 7;
    for (time, amount) in [(11, -1), (12, -2)] {
        world.state.time = time;
        world
            .append_event(
                crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                    action_id: 7,
                    reason: crate::RejectReason::InvalidAmount { amount },
                }),
                None,
            )
            .unwrap();
    }
    world.state.time = 99;
    world
        .bind_cognition_runtime("genesis-events", "main", 0, None, "pending", 0)
        .unwrap();
    assert_eq!(world.journal().events.len(), 2);
    assert_eq!(world.journal().events[0].id, u64::MAX);
    assert_eq!(world.journal().events[1].id, 1);
    let wire = oasis7_world_state::codec::journal_events_cbor(&world.journal().events).unwrap();
    let expected = (
        *blake3::hash(&wire).as_bytes(),
        world.journal().events.len(),
    );
    let meter = FormalBoundedWorkV1::new(256_000_000, 4 << 30).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let mut pages = Pages::default();
    let mut emitted = Vec::new();
    let mut originals = BTreeMap::new();
    let mut io = ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let projected = world
        .prepare_iws_genesis_with_projection_v1(
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
            &mut work(),
            &mut |reference, bytes| {
                assert_eq!(reference.id, *blake3::hash(bytes).as_bytes());
                assert_eq!(reference.byte_len, bytes.len() as u64);
                let fee = memory.reserve(bytes.len(), MemoryClass::Ordinary)?;
                originals.insert(reference.id, fee.retain(bytes.to_vec()));
                Ok(())
            },
            &mut |_, _| Err(ValueTreeError::Invalid("unexpected pending action")),
            &mut |_, _, _| Err(BodyMutationError::Invalid("unexpected mailbox")),
            &mut |row| {
                assert!(row.uses_budget(&memory));
                emitted.push(row);
                Ok(())
            },
            &mut common,
        )
        .unwrap();
    assert!(!emitted.is_empty());
    assert!(!pages.0.is_empty());
    drop(world);
    assert_eq!(
        projected
            .formal_event_commitment_bounded_v1(&mut common)
            .unwrap(),
        expected
    );

    // Numerically equal fresh work cannot replace the capture's actual Core.
    let other_work =
        FormalBoundedWorkV1::new(meter.remaining_steps(), meter.remaining_read_bytes()).unwrap();
    let mut other_stop = || false;
    let mut other = FormalBoundedContextV1::new(&memory, &other_work, &mut other_stop);
    assert!(matches!(
        projected.formal_event_commitment_bounded_v1(&mut other),
        Err(BodyMutationError::Invalid("projection column seed context"))
    ));
    let other_memory = MemoryBudget::new(512 << 20, 16 << 20).unwrap();
    let mut other_stop = || false;
    let mut other = FormalBoundedContextV1::new(&other_memory, &meter, &mut other_stop);
    assert!(matches!(
        projected.formal_event_commitment_bounded_v1(&mut other),
        Err(BodyMutationError::Invalid("GEN projected owner pool"))
    ));
    let before_cancel = (meter.remaining_steps(), meter.remaining_read_bytes());
    let mut cancelled = || true;
    let mut cancelled_common = FormalBoundedContextV1::new(&memory, &meter, &mut cancelled);
    context_refusal(
        projected
            .formal_event_commitment_bounded_v1(&mut cancelled_common)
            .unwrap_err(),
        "bounded archive read cancelled",
    );
    assert_eq!(
        (meter.remaining_steps(), meter.remaining_read_bytes()),
        before_cancel
    );
    assert_eq!(
        projected
            .formal_event_commitment_bounded_v1(&mut common)
            .unwrap(),
        expected
    );
    common
        .consume_read_bytes(meter.remaining_read_bytes() - 39)
        .unwrap();
    context_refusal(
        projected
            .formal_event_commitment_bounded_v1(&mut common)
            .unwrap_err(),
        "bounded archive read bytes",
    );
    assert_eq!(meter.remaining_read_bytes(), 0);
    common.consume_steps(meter.remaining_steps()).unwrap();
    context_refusal(
        projected
            .formal_event_commitment_bounded_v1(&mut common)
            .unwrap_err(),
        "bounded archive read steps",
    );
    assert_eq!(meter.remaining_steps(), 0);
    drop(projected);
    drop(emitted);
    drop(pages);
    drop(originals);
    assert_eq!(memory.used(), baseline);
}

#[test]
fn projected_genesis_event_capture_refuses_hash_failure_before_physical_emissions() {
    projected_genesis_hash_failure_case();
}

pub(super) fn projected_genesis_hash_failure_case() {
    use crate::world::incremental_genesis_projection::{
        RuntimeProjectedGenesisErrorV1, RuntimeProjectedGenesisV1,
    };
    use oasis7_formal_protocol::controlled_authority::formal::{
        FormalError,
        bounded_context::{FormalBoundedContextV1, FormalBoundedWorkV1},
    };

    let memory = MemoryBudget::new(64 << 20, 0).unwrap();
    let _input = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    world
        .append_event(
            crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                action_id: 7,
                reason: crate::RejectReason::AgentNotFound {
                    agent_id: "原".repeat(8192),
                },
            }),
            None,
        )
        .unwrap();
    world
        .bind_cognition_runtime("genesis-events", "main", 0, None, "pending", 0)
        .unwrap();
    let wire = oasis7_world_state::codec::journal_events_cbor(&world.journal().events).unwrap();
    let owner_bytes = std::mem::size_of::<RuntimeProjectedGenesisV1>() as u64;
    let exact_bytes = owner_bytes + (2 * wire.len() + 32) as u64;
    let mut pages = Pages::default();
    let mut producer = work();
    let mut io = ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let baseline = memory.used();
    for cancel_during_hash in [true, false] {
        let allowance = exact_bytes - u64::from(!cancel_during_hash);
        let meter = FormalBoundedWorkV1::new(100_000, allowance).unwrap();
        let mut stop = || {
            cancel_during_hash && allowance - meter.remaining_read_bytes() >= owner_bytes + 2 * 8192
        };
        let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        let result = world.prepare_iws_genesis_with_projection_v1(
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
            &mut producer,
            &mut |_, _| panic!("failed journal hash reached artifact emission"),
            &mut |_, _| panic!("failed journal hash reached action emission"),
            &mut |_, _, _| panic!("failed journal hash reached mailbox emission"),
            &mut |_| panic!("failed journal hash reached physical row emission"),
            &mut common,
        );
        let Err(RuntimeProjectedGenesisErrorV1::Source(BodyMutationError::Original(error))) =
            result
        else {
            panic!("expected original Common hash refusal and no projected owner");
        };
        let note = if cancel_during_hash {
            assert!(meter.remaining_read_bytes() > 32);
            "bounded archive read cancelled"
        } else {
            assert_eq!(meter.remaining_read_bytes(), 0);
            "bounded archive read bytes"
        };
        assert!(matches!(
            error.downcast_ref::<FormalError>(),
            Some(FormalError::Invalid(actual)) if actual == note
        ));
        assert!(meter.remaining_steps() < 100_000);
        assert!(meter.remaining_read_bytes() < allowance);
        assert_eq!(memory.used(), baseline);
        assert!(pages.0.is_empty());
        assert_eq!(io.nodes_remaining, 1_000_000);
        assert_eq!(io.read_bytes_remaining, 256 << 20);
        assert_eq!(io.write_bytes_remaining, 256 << 20);
        assert_eq!(keys.bytes_remaining, 64 << 20);
    }
}
