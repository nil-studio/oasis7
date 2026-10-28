use super::*;
use crate::world::incremental_body::tests::{Pages, fixture, io, limits};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionParent, TransitionVerifier},
    incremental_state::{StateDeltaV1, StateWriteV1},
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::MemoryBudget;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};

type ValuePages = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;

#[derive(Clone)]
struct Values {
    pages: Rc<RefCell<ValuePages>>,
    reads: Rc<Cell<usize>>,
}
impl Default for Values {
    fn default() -> Self {
        Self {
            pages: Rc::new(RefCell::new(BTreeMap::new())),
            reads: Rc::new(Cell::new(0)),
        }
    }
}
impl oasis7_formal_protocol::incremental_schedule::SchedulePageWriter for Values {}
impl ValueNodeWriter for Values {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.pages.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for Values {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        _: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.reads.set(self.reads.get() + 1);
        let page = self
            .pages
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("test value missing"))?;
        if page.len() > max || !page.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("test value bound/pool"));
        }
        Ok(page)
    }
}
fn seed(
    world: &World,
    parent: &mut ExecutionParent,
    state: &mut Pages,
    values: &mut Values,
    memory: &MemoryBudget,
    sequence: EventSequenceV1,
    frontier: MailboxFrontierV1,
) {
    for (namespace, key, bytes) in [
        (
            Ns::Allocator,
            EVENT_SEQUENCE_KEY_V1.to_vec(),
            sequence.canonical_bytes().to_vec(),
        ),
        (
            Ns::Scheduler,
            MailboxKeyV1::new("alice", None)
                .unwrap()
                .as_bytes()
                .to_vec(),
            frontier.canonical_bytes().unwrap().to_vec(),
        ),
    ] {
        let id = write_field_bytes(
            &bytes,
            &mut BodyMutationContext {
                sink: values,
                memory,
                io: &mut io(),
                cancelled: || false,
            },
        )
        .unwrap();
        let index = namespace as usize - 1;
        let delta = StateDeltaV1::build(
            parent.roots.state[index].clone(),
            vec![StateWriteV1 {
                key,
                before: None,
                after: Some(id),
            }],
            state,
            4 * 1024 * 1024,
        )
        .unwrap();
        parent.roots.state[index] = delta.after;
        for page in delta.new_nodes {
            state.0.insert(page.digest, page.bytes);
        }
    }
    let body = &world.state.agents["alice"];
    super::super::incremental_body::write_body_value(
        &oasis7_world_state::incremental_agent_body::AgentBodyValueV1 {
            body_view: body.state.body_view.clone(),
            last_active: body.last_active,
        },
        &mut BodyMutationContext {
            sink: values,
            memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    write_field_bytes(
        &oasis7_world_state::incremental_agent_body::world_time_bytes_v1(world.state.time),
        &mut BodyMutationContext {
            sink: values,
            memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    assert_eq!(world.state.agents.len(), 1);
}
fn event(world: &World, id: u64) -> WorldEvent {
    let mut view = world.state.agents["alice"].state.body_view.clone();
    view.mass_kg += 1;
    WorldEvent {
        id,
        time: world.state.time,
        caused_by: Some(CausedBy::Effect {
            intent_id: "effect-original".into(),
        }),
        body: WorldEventBody::Domain(DomainEvent::BodyAttributesUpdated {
            agent_id: "alice".into(),
            view,
            reason: "actual body event".into(),
        }),
    }
}

#[test]
fn body_event_uses_original_fact_and_four_leaves_with_fixed_control_reads() {
    let memory = MemoryBudget::new(32 * 1024 * 1024, 1024 * 1024).unwrap();
    let (world, mut parent, mut state) = fixture(&memory);
    let mut values = Values::default();
    seed(
        &world,
        &mut parent,
        &mut state,
        &mut values,
        &memory,
        EventSequenceV1 { next: 1, era: 0 },
        MailboxFrontierV1 { head: 0, tail: 0 },
    );
    let source = values.clone();
    let original = event(&world, 1);
    let expected = oasis7_wasm_abi::encode_canonical_cbor(&original).unwrap();
    let before = world.state.clone();
    let mut originals = BTreeMap::new();
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &state, &memory, limits()).unwrap();
    world
        .contribute_iws_agent_event(
            &original,
            &mut txn,
            &mut AgentEventContext {
                values: &source,
                mutation: BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                persist_original: |bytes: &ReservedValue<Vec<u8>>, id| {
                    originals.insert(id, bytes.as_slice().to_vec());
                    Ok(())
                },
                max_event_bytes: 1024 * 1024,
            },
        )
        .unwrap();
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.writes.len(), 4);
    assert_eq!(prepared.record.reads.len(), 5); // clock, body, sequence, frontier, absent ordinal
    assert_eq!(prepared.record.facts.len(), 1);
    let fact = &prepared.record.facts[0];
    assert_eq!(fact.kind, ExecutionFactKindV1::Event);
    assert_eq!(originals[&fact.id], expected);
    let mut legacy = world.clone();
    let WorldEventBody::Domain(DomainEvent::BodyAttributesUpdated {
        agent_id,
        view,
        reason,
    }) = &original.body
    else {
        panic!("test body shape");
    };
    assert_eq!(
        legacy
            .record_body_attributes_update(
                agent_id,
                view.clone(),
                reason,
                original.caused_by.clone()
            )
            .unwrap(),
        original.id
    );
    assert_eq!(legacy.journal.events.last().unwrap(), &original);
    assert_eq!(
        legacy.state.agents["alice"].mailbox.back(),
        Some(match &original.body {
            WorldEventBody::Domain(event) => event,
            _ => unreachable!(),
        })
    );
    assert_eq!(source.reads.get(), 6); // two controls and the current body value
    assert_eq!(world.state, before);
    TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 * 1024 * 1024,
    }
    .verify(&prepared.record)
    .unwrap();
    let frontier_key = MailboxKeyV1::new("alice", None).unwrap();
    let frontier_write = prepared
        .record
        .writes
        .iter()
        .find(|write| write.mutation.key == frontier_key.as_bytes())
        .unwrap();
    let mut quota = io();
    let mut reader = ValueReadContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    let root = reader
        .load_root(&frontier_write.mutation.after.unwrap())
        .unwrap();
    let mut bytes = Vec::new();
    reader
        .read_range(&root, 0, root.0.logical_len, &mut bytes, || false)
        .unwrap();
    assert_eq!(
        MailboxFrontierV1::from_canonical_bytes(&bytes).unwrap(),
        MailboxFrontierV1 { head: 0, tail: 1 }
    );
}

#[test]
fn late_original_or_fact_failure_prevents_sealing_all_staged_state() {
    for mode in 0..5 {
        let memory = MemoryBudget::new(32 * 1024 * 1024, 1024 * 1024).unwrap();
        let (world, mut parent, mut state) = fixture(&memory);
        let mut values = Values::default();
        seed(
            &world,
            &mut parent,
            &mut state,
            &mut values,
            &memory,
            EventSequenceV1 { next: 1, era: 0 },
            MailboxFrontierV1 { head: 0, tail: 0 },
        );
        let source = values.clone();
        let mut original = event(&world, 1);
        if mode == 2 {
            original.id = 2;
        }
        if mode == 3 {
            original.time += 1;
        }
        let live = parent.fence().unwrap();
        let mut profile = limits();
        if mode == 1 {
            profile.max_facts = 0;
        }
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &state, &memory, profile).unwrap();
        txn.write(Ns::Business, b"outer-debit", Some([9; 32]))
            .unwrap();
        let called = Cell::new(false);
        let result = world.contribute_iws_agent_event(
            &original,
            &mut txn,
            &mut AgentEventContext {
                values: &source,
                mutation: BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                persist_original: |_: &ReservedValue<Vec<u8>>, _| {
                    called.set(true);
                    if mode == 0 {
                        Err(BodyMutationError::Original(Box::new(std::io::Error::from(
                            std::io::ErrorKind::WriteZero,
                        ))))
                    } else {
                        Ok(())
                    }
                },
                max_event_bytes: if mode == 4 { 1 } else { 1024 * 1024 },
            },
        );
        assert!(result.is_err());
        if mode == 0 {
            let error = result.as_ref().err().unwrap();
            let source = std::error::Error::source(error).unwrap();
            assert_eq!(
                source.downcast_ref::<std::io::Error>().unwrap().kind(),
                std::io::ErrorKind::WriteZero
            );
        }
        assert_eq!(called.get(), mode <= 1);
        let guard = txn.guard();
        assert!(matches!(
            txn.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[test]
fn mailbox_offsets_and_event_era_preserve_original_allocator_semantics() {
    let memory = MemoryBudget::new(32 * 1024 * 1024, 1024 * 1024).unwrap();
    let (world, mut parent, mut state) = fixture(&memory);
    let mut values = Values::default();
    seed(
        &world,
        &mut parent,
        &mut state,
        &mut values,
        &memory,
        EventSequenceV1 {
            next: u64::MAX,
            era: 17,
        },
        MailboxFrontierV1 {
            head: 50_000,
            tail: 500_000,
        },
    );
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &state, &memory, limits()).unwrap();
    world
        .contribute_iws_agent_event(
            &event(&world, u64::MAX),
            &mut txn,
            &mut AgentEventContext {
                values: &source,
                mutation: BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                persist_original: |_: &ReservedValue<Vec<u8>>, _| Ok(()),
                max_event_bytes: 1024 * 1024,
            },
        )
        .unwrap();
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert!(prepared.record.writes.iter().any(|write| {
        write.mutation.key
            == MailboxKeyV1::new("alice", Some(500_000))
                .unwrap()
                .as_bytes()
    }));
    assert_eq!(source.reads.get(), 6);
    let sequence = prepared
        .record
        .writes
        .iter()
        .find(|write| write.mutation.key == EVENT_SEQUENCE_KEY_V1)
        .unwrap();
    let mut quota = io();
    let mut reader = ValueReadContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    let root = reader.load_root(&sequence.mutation.after.unwrap()).unwrap();
    let mut bytes = Vec::new();
    reader
        .read_range(&root, 0, root.0.logical_len, &mut bytes, || false)
        .unwrap();
    assert_eq!(
        EventSequenceV1::from_canonical_bytes(&bytes).unwrap(),
        EventSequenceV1 { next: 1, era: 18 }
    );
}

#[test]
fn event_control_codecs_reject_wrong_domains_lengths_and_frontiers() {
    let reference = mailbox_event_ref_v1([7; 32]);
    assert_eq!(mailbox_event_id_v1(&reference).unwrap(), [7; 32]);
    assert!(mailbox_event_id_v1(&reference[1..]).is_err());
    let mut wrong_reference = reference;
    wrong_reference[0] ^= 1;
    assert!(mailbox_event_id_v1(&wrong_reference).is_err());
    let sequence = EventSequenceV1 {
        next: u64::MAX,
        era: u64::MAX,
    };
    let bytes = sequence.canonical_bytes();
    assert_eq!(
        EventSequenceV1::from_canonical_bytes(&bytes).unwrap(),
        sequence
    );
    assert!(EventSequenceV1::from_canonical_bytes(&bytes[1..]).is_err());
    let mut wrong = bytes;
    wrong[0] ^= 1;
    assert!(EventSequenceV1::from_canonical_bytes(&wrong).is_err());
    assert!(
        MailboxFrontierV1 { head: 2, tail: 1 }
            .canonical_bytes()
            .is_err()
    );
    let frontier = MailboxFrontierV1 { head: 4, tail: 7 };
    let mut bytes = frontier.canonical_bytes().unwrap();
    assert_eq!(
        MailboxFrontierV1::from_canonical_bytes(&bytes).unwrap(),
        frontier
    );
    let len = bytes.len();
    bytes[len - 8..].copy_from_slice(&3u64.to_be_bytes());
    assert!(MailboxFrontierV1::from_canonical_bytes(&bytes).is_err());
    assert_ne!(
        MailboxKeyV1::new("alice", None).unwrap().as_bytes(),
        MailboxKeyV1::new("alice", Some(0)).unwrap().as_bytes()
    );
    assert_ne!(
        MailboxKeyV1::new("alice", Some(1)).unwrap().as_bytes(),
        MailboxKeyV1::new("alice", Some(2)).unwrap().as_bytes()
    );
}

#[test]
fn successive_body_events_read_previous_overlay_without_installing_world_cache() {
    let memory = MemoryBudget::new(32 * 1024 * 1024, 1024 * 1024).unwrap();
    let (world, mut parent, mut state) = fixture(&memory);
    let original_state = world.state.clone();
    let mut values = Values::default();
    seed(
        &world,
        &mut parent,
        &mut state,
        &mut values,
        &memory,
        EventSequenceV1 { next: 1, era: 0 },
        MailboxFrontierV1 { head: 0, tail: 0 },
    );
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &state, &memory, limits()).unwrap();
    let mut ids = Vec::new();
    let mut quota = io();
    let mut context = AgentEventContext {
        values: &source,
        mutation: BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut quota,
            cancelled: || false,
        },
        persist_original: |_: &ReservedValue<Vec<u8>>, id| {
            ids.push(id);
            Ok(())
        },
        max_event_bytes: 1024 * 1024,
    };
    for id in 1..=2 {
        let mut next = event(&world, id);
        if let WorldEventBody::Domain(DomainEvent::BodyAttributesUpdated { view, .. }) =
            &mut next.body
        {
            view.mass_kg += id - 1;
        }
        world
            .contribute_iws_agent_event(&next, &mut txn, &mut context)
            .unwrap();
    }
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(
        prepared
            .record
            .facts
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(prepared.record.writes.len(), 5);
    assert_eq!(world.state, original_state);
    assert_eq!(source.reads.get(), 12);
}

#[path = "incremental_body_event_physical_tests.rs"]
mod physical;

#[path = "incremental_commit_tests.rs"]
mod commit;

#[path = "incremental_agent_move_tests.rs"]
mod moves;

#[path = "incremental_agent_registration_tests.rs"]
mod registration;

#[path = "incremental_scheduler_tests.rs"]
mod scheduler;
