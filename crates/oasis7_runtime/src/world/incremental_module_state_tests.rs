use super::*;
use crate::ModuleStateEventContext;
use oasis7_wasm_abi::ModuleStateUpdate;
use oasis7_world_state::incremental_module_fields::ModuleStateKeyV1;
use std::io::Read;

#[path = "incremental_module_state_physical_tests.rs"]
mod physical;

fn module_event(id: u64, state: Vec<u8>) -> WorldEvent {
    WorldEvent {
        id,
        time: 11,
        caused_by: Some(CausedBy::Effect {
            intent_id: "actual-cause\0雪".into(),
        }),
        body: WorldEventBody::ModuleStateUpdated(ModuleStateUpdate {
            module_id: "module#stable-2".into(),
            trace_id: format!("tick-11-module#stable-2-{id}"),
            state,
        }),
    }
}
fn setup(memory: &MemoryBudget) -> (World, ExecutionParent, Pages, Values) {
    let (mut world, mut parent, mut pages) = fixture(memory);
    let mut values = Values::default();
    seed(
        &world,
        &mut parent,
        &mut pages,
        &mut values,
        memory,
        EventSequenceV1 { next: 1, era: 0 },
        MailboxFrontierV1 { head: 2, tail: 3 },
    );
    world
        .state
        .module_states
        .insert("module#stable-2".into(), vec![99; 100]);
    world.state.module_states.insert("other".into(), Vec::new());
    let mut writes = Vec::new();
    assert_eq!(
        world
            .write_iws_module_state_genesis(
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory,
                    io: &mut io(),
                    cancelled: || false
                },
                &mut 2,
                |key, id| {
                    writes.push(StateWriteV1 {
                        key: key.to_vec(),
                        before: None,
                        after: Some(id),
                    });
                    Ok(())
                }
            )
            .unwrap(),
        2
    );
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta =
        StateDeltaV1::build(parent.roots.state[0].clone(), writes, &pages, 4 << 20).unwrap();
    parent.roots.state[0] = delta.after;
    for node in delta.new_nodes {
        pages.0.insert(node.digest, node.bytes);
    }
    (world, parent, pages, values)
}
fn state_value(id: [u8; 32], values: &Values, memory: &MemoryBudget) -> Vec<u8> {
    let mut quota = io();
    let mut reader = ValueReadContext {
        source: values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    let root = reader.load_root(&id).unwrap();
    reader.verify_closure(&id, 100_000, || false).unwrap();
    let mut output = Vec::new();
    reader
        .read_range(&root, 0, root.0.logical_len, &mut output, || false)
        .unwrap();
    output
}
#[test]
fn module_state_events_share_allocator_preserve_exact_originals_and_do_not_touch_mailboxes() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (mut world, parent, pages, mut values) = setup(&memory);
    world
        .state
        .module_states
        .get_mut("module#stable-2")
        .unwrap()
        .fill(7);
    let old_cache = world.state.module_states.clone();
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    let mut originals = Vec::new();
    let mut quota = io();
    for event in [
        module_event(1, (0..=255).collect()),
        module_event(2, Vec::new()),
    ] {
        world
            .contribute_iws_module_state_event(
                &event,
                &mut txn,
                &mut ModuleStateEventContext {
                    values: &source,
                    mutation: BodyMutationContext {
                        sink: &mut values,
                        memory: &memory,
                        io: &mut quota,
                        cancelled: || false,
                    },
                    max_event_bytes: 1 << 20,
                    persist_original: |reader: &mut dyn Read, length: u64, id| {
                        let charge = memory
                            .reserve(length as usize, MemoryClass::Ordinary)
                            .map_err(ExecutionError::Memory)?;
                        let mut bytes = Vec::with_capacity(length as usize);
                        reader.read_to_end(&mut bytes).map_err(ValueTreeError::Io)?;
                        assert_eq!(
                            bytes,
                            oasis7_wasm_abi::encode_canonical_cbor(&event).unwrap()
                        );
                        assert_eq!(*blake3::hash(&bytes).as_bytes(), id);
                        originals.push((id, charge.retain(bytes)));
                        Ok(())
                    },
                },
            )
            .unwrap();
        let key = ModuleStateKeyV1::new("module#stable-2").unwrap();
        let id = txn.read(Ns::Business, key.as_bytes()).unwrap().unwrap();
        let WorldEventBody::ModuleStateUpdated(update) = &event.body else {
            panic!()
        };
        assert_eq!(state_value(id, &source, &memory), update.state);
    }
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.facts.len(), 2);
    assert_eq!(
        prepared
            .record
            .facts
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        originals.iter().map(|(id, _)| *id).collect::<Vec<_>>()
    );
    assert_eq!(prepared.record.writes.len(), 2);
    assert!(
        prepared
            .record
            .writes
            .iter()
            .all(|w| w.namespace != Ns::Scheduler)
    );
    let sequence = prepared
        .record
        .writes
        .iter()
        .find(|w| w.mutation.key == EVENT_SEQUENCE_KEY_V1)
        .unwrap();
    assert_eq!(
        EventSequenceV1::from_canonical_bytes(&state_value(
            sequence.mutation.after.unwrap(),
            &source,
            &memory
        ))
        .unwrap(),
        EventSequenceV1 { next: 3, era: 0 }
    );
    assert_eq!(world.state.module_states, old_cache);
    TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&prepared.record)
    .unwrap();
    let mut reference = World::new();
    reference.state.time = 11;
    for event in [
        module_event(1, (0..=255).collect()),
        module_event(2, Vec::new()),
    ] {
        reference.append_event(event.body, event.caused_by).unwrap();
    }
    assert_eq!(
        reference.state.module_states["module#stable-2"],
        Vec::<u8>::new()
    );
}

#[test]
fn module_event_allocator_rollover_and_absent_to_empty_state_are_literal() {
    for (next, era, id, after) in [
        (0, 0, 1, EventSequenceV1 { next: 2, era: 0 }),
        (u64::MAX, 4, u64::MAX, EventSequenceV1 { next: 1, era: 5 }),
        (
            u64::MAX,
            u64::MAX,
            u64::MAX,
            EventSequenceV1 {
                next: 1,
                era: u64::MAX,
            },
        ),
    ] {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (world, mut parent, mut pages) = fixture(&memory);
        let mut values = Values::default();
        seed(
            &world,
            &mut parent,
            &mut pages,
            &mut values,
            &memory,
            EventSequenceV1 { next, era },
            MailboxFrontierV1 { head: 0, tail: 0 },
        );
        let source = values.clone();
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
        world
            .contribute_iws_module_state_event(
                &module_event(id, Vec::new()),
                &mut txn,
                &mut ModuleStateEventContext {
                    values: &source,
                    mutation: BodyMutationContext {
                        sink: &mut values,
                        memory: &memory,
                        io: &mut io(),
                        cancelled: || false,
                    },
                    max_event_bytes: 1 << 20,
                    persist_original: |reader: &mut dyn Read, _, _| {
                        std::io::copy(reader, &mut std::io::sink()).map_err(ValueTreeError::Io)?;
                        Ok(())
                    },
                },
            )
            .unwrap();
        let guard = txn.guard();
        let prepared = txn.seal(&guard, &live, 0).unwrap();
        let sequence = prepared
            .record
            .writes
            .iter()
            .find(|w| w.mutation.key == EVENT_SEQUENCE_KEY_V1)
            .unwrap();
        assert_eq!(
            EventSequenceV1::from_canonical_bytes(&state_value(
                sequence.mutation.after.unwrap(),
                &source,
                &memory
            ))
            .unwrap(),
            after
        );
        let state = prepared
            .record
            .writes
            .iter()
            .find(|w| w.namespace == Ns::Business)
            .unwrap();
        assert_eq!(state.mutation.before, None);
        assert!(state.mutation.after.is_some());
    }
}

#[test]
fn unchanged_reducer_state_keeps_business_root_but_retains_event_and_allocator_progress() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (mut world, parent, pages, mut values) = setup(&memory);
    world
        .state
        .module_states
        .get_mut("module#stable-2")
        .unwrap()
        .fill(7);
    let source = values.clone();
    let live = parent.fence().unwrap();
    let before = parent.roots.state[0].clone();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    world
        .contribute_iws_module_state_event(
            &module_event(1, vec![99; 100]),
            &mut txn,
            &mut ModuleStateEventContext {
                values: &source,
                mutation: BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                max_event_bytes: 1 << 20,
                persist_original: |reader: &mut dyn Read, _, _| {
                    std::io::copy(reader, &mut std::io::sink()).map_err(ValueTreeError::Io)?;
                    Ok(())
                },
            },
        )
        .unwrap();
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.after.state[0], before);
    assert_eq!(prepared.record.facts.len(), 1);
    assert_eq!(prepared.record.writes.len(), 1);
    assert_eq!(prepared.record.writes[0].namespace, Ns::Allocator);
    assert_eq!(world.state.module_states["module#stable-2"], vec![7; 100]);
}

#[test]
fn failures_short_original_cancel_and_unwind_abort_all_prior_work() {
    for failure in 0..13 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (world, parent, pages, mut values) = setup(&memory);
        let before = world.state.module_states.clone();
        let source = values.clone();
        let mut event = module_event(1, vec![1, 24, 255]);
        if failure == 0 {
            event.time += 1;
        }
        if failure == 1 {
            event.id += 1;
        }
        if failure == 9 {
            let WorldEventBody::ModuleStateUpdated(update) = &mut event.body else {
                panic!()
            };
            update.module_id = "x".repeat(1024);
        }
        let mut limits = limits();
        if failure == 11 {
            limits.max_facts = 0;
        }
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits).unwrap();
        txn.write(Ns::Effects, b"prior", Some([8; 32])).unwrap();
        let mut quota = io();
        if failure == 3 {
            quota.nodes_remaining = 0;
        }
        if failure == 4 {
            quota.write_bytes_remaining = 0;
        }
        let alien = MemoryBudget::new(32 << 20, 0).unwrap();
        let cancelled = Cell::new(false);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.contribute_iws_module_state_event(
                &event,
                &mut txn,
                &mut ModuleStateEventContext {
                    values: &source,
                    mutation: BodyMutationContext {
                        sink: &mut values,
                        memory: if failure == 10 { &alien } else { &memory },
                        io: &mut quota,
                        cancelled: || cancelled.get(),
                    },
                    max_event_bytes: if failure == 2 { 1 } else { 1 << 20 },
                    persist_original: |reader: &mut dyn Read, _, _| {
                        if failure == 5 {
                            return Ok(());
                        }
                        if failure == 7 {
                            cancelled.set(true);
                        }
                        std::io::copy(reader, &mut std::io::sink()).map_err(ValueTreeError::Io)?;
                        assert_ne!(failure, 8, "injected original persistence unwind");
                        if failure == 12 {
                            cancelled.set(true);
                        }
                        if failure == 6 {
                            return Err(BodyMutationError::Invalid("original failed"));
                        }
                        Ok(())
                    },
                },
            )
        }));
        if failure == 8 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err(), "failure {failure}");
        }
        let guard = txn.guard();
        assert!(txn.seal(&guard, &live, 0).is_err());
        assert_eq!(world.state.module_states, before);
    }
}

#[test]
fn module_state_genesis_is_borrowed_entry_limited_and_preserves_empty_keys_and_payloads() {
    let memory = MemoryBudget::new(8 << 20, 2 << 20).unwrap();
    let mut world = World::new();
    world.state.module_states.insert("".into(), Vec::new());
    world
        .state
        .module_states
        .insert("other#1".into(), vec![1, 2, 3]);
    let mut values = Values::default();
    let mut entries = Vec::new();
    assert_eq!(
        world
            .write_iws_module_state_genesis(
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false
                },
                &mut 2,
                |key, id| {
                    entries.push((key.to_vec(), id));
                    Ok(())
                }
            )
            .unwrap(),
        2
    );
    for (key, id) in entries {
        let key = ModuleStateKeyV1::state_key_from_bytes(&key).unwrap();
        assert_eq!(
            state_value(id, &values, &memory),
            world.state.module_states[key]
        );
    }
    let mut emitted = 0;
    assert!(
        world
            .write_iws_module_state_genesis(
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false
                },
                &mut 1,
                |_, _| {
                    emitted += 1;
                    Ok(())
                }
            )
            .is_err()
    );
    assert_eq!(emitted, 1);
}
