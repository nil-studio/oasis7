use super::*;
use oasis7_wasm_abi::ModuleTickLifecycleDirective as D;
use oasis7_world_state::incremental_scheduler_fields::{
    MODULE_WAKE_INDEX_KEY_V1, ModuleWakeKeyV1, ModuleWakeV1,
};

#[path = "incremental_scheduler_consistency_tests.rs"]
mod consistency;

fn io() -> oasis7_formal_protocol::incremental_value::ValueIoBudget {
    oasis7_formal_protocol::incremental_value::ValueIoBudget {
        nodes_remaining: 50_000,
        read_bytes_remaining: 16 << 20,
        write_bytes_remaining: 16 << 20,
    }
}

pub(super) fn seed_schedules(
    world: &World,
    parent: &mut ExecutionParent,
    state: &mut Pages,
    values: &mut Values,
    memory: &MemoryBudget,
) {
    let mut writes = Vec::new();
    let count = world
        .write_iws_module_schedule_genesis(
            &mut BodyMutationContext {
                sink: values,
                memory,
                io: &mut io(),
                cancelled: || false,
            },
            &mut 100,
            |key, id| {
                writes.push(StateWriteV1 {
                    key: key.to_vec(),
                    before: None,
                    after: Some(id),
                });
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(count as usize, world.module_tick_schedule.len());
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = StateDeltaV1::build(parent.roots.state[2].clone(), writes, state, 4 << 20).unwrap();
    parent.roots.state[2] = delta.after;
    for node in delta.new_nodes {
        state.0.insert(node.digest, node.bytes);
    }
}
fn decoded(id: [u8; 32], values: &Values, memory: &MemoryBudget) -> u64 {
    let mut quota = io();
    let mut reader = ValueReadContext {
        source: values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    let root = reader.load_root(&id).unwrap();
    let mut bytes = Vec::new();
    reader
        .read_range(&root, 0, root.0.logical_len, &mut bytes, || false)
        .unwrap();
    ModuleWakeV1::from_canonical_bytes(&bytes).unwrap().0
}
fn decoded_index(
    id: [u8; 32],
    values: &Values,
    memory: &MemoryBudget,
) -> oasis7_formal_protocol::incremental_schedule::ScheduleIndexV1 {
    let mut quota = io();
    let mut reader = ValueReadContext {
        source: values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    let root = reader.load_root(&id).unwrap();
    assert!(root.0.logical_len <= 128);
    let _charge = memory.reserve(128, MemoryClass::Ordinary).unwrap();
    let mut bytes = Vec::with_capacity(128);
    reader
        .read_range(&root, 0, root.0.logical_len, &mut bytes, || false)
        .unwrap();
    oasis7_formal_protocol::incremental_schedule::ScheduleIndexV1::from_canonical_bytes(&bytes)
        .unwrap()
}
#[test]
fn schedule_genesis_has_fixed_values_exact_instances_and_provisional_entry_limits() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let mut world = World::new();
    world.module_tick_schedule.insert("m#1".into(), 0);
    world.module_tick_schedule.insert("m#2".into(), u64::MAX);
    let before = world.module_tick_schedule.clone();
    let mut values = Values::default();
    let mut fields = Vec::new();
    let mut remaining = 2;
    assert_eq!(
        world
            .write_iws_module_schedule_genesis(
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false
                },
                &mut remaining,
                |key, id| {
                    fields.push((key.to_vec(), id));
                    Ok(())
                }
            )
            .unwrap(),
        2
    );
    assert_eq!(remaining, 0);
    for (key, id) in fields {
        if key == MODULE_WAKE_INDEX_KEY_V1 {
            continue;
        }
        let instance = ModuleWakeKeyV1::instance_from_bytes(&key).unwrap();
        assert_eq!(decoded(id, &values, &memory), before[instance]);
    }
    let mut emitted = 0;
    assert!(
        world
            .write_iws_module_schedule_genesis(
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
    assert_eq!(world.module_tick_schedule, before);
}

#[test]
fn due_execution_reads_overlay_clears_first_and_schedules_absolute_stable_instance() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let (mut world, mut parent, mut pages) = fixture(&memory);
    world.module_tick_schedule.insert("m#1".into(), 5);
    world.module_tick_schedule.insert("m#2".into(), 20);
    let mut values = Values::default();
    seed_schedules(&world, &mut parent, &mut pages, &mut values, &memory);
    // fixture's clock value was originally computed through a discard sink.
    write_field_bytes(
        &oasis7_world_state::incremental_agent_body::world_time_bytes_v1(11),
        &mut BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    world.module_tick_schedule.insert("m#1".into(), 999);
    let before = world.module_tick_schedule.clone();
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    let mut quota = io();
    let mut ctx = BodyMutationContext {
        sink: &mut values,
        memory: &memory,
        io: &mut quota,
        cancelled: || false,
    };
    world
        .contribute_iws_module_wake("m#1", Some(7), &mut txn, &source, &mut ctx)
        .unwrap();
    let key = ModuleWakeKeyV1::new("m#1").unwrap();
    assert!(
        world
            .contribute_iws_due_module_tick(
                "m#1",
                &mut txn,
                &source,
                &mut ctx,
                |instance, now, txn, ctx| {
                    assert_eq!((instance, now), ("m#1", 11));
                    assert_eq!(txn.read(Ns::Scheduler, key.as_bytes())?, None);
                    let index = decoded_index(
                        txn.read(Ns::Scheduler, MODULE_WAKE_INDEX_KEY_V1)?.unwrap(),
                        &source,
                        &memory,
                    );
                    assert_eq!(index.entries, 1);
                    let mut quota = io();
                    let reader = crate::world::incremental_scheduler_index::IndexReader::new(
                        &source,
                        &memory,
                        MemoryClass::Ordinary,
                        &mut quota,
                    );
                    assert!(!index.contains(7, "m#1", &reader)?);
                    assert!(index.contains(20, "m#2", &reader)?);
                    let id = write_field_bytes(b"business result", ctx)?;
                    txn.write(Ns::Business, b"module-result", Some(id))?;
                    Ok(Some(D::WakeAfterTicks { ticks: 0 }))
                }
            )
            .unwrap()
    );
    for instance in ["m#2", "absent"] {
        assert!(
            !world
                .contribute_iws_due_module_tick(
                    instance,
                    &mut txn,
                    &source,
                    &mut ctx,
                    |_, _, _, _| panic!("not due")
                )
                .unwrap()
        );
    }
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    let wake = prepared
        .record
        .writes
        .iter()
        .find(|w| w.mutation.key == key.as_bytes())
        .unwrap();
    assert_eq!(decoded(wake.mutation.before.unwrap(), &source, &memory), 5);
    assert_eq!(decoded(wake.mutation.after.unwrap(), &source, &memory), 12);
    assert_eq!(prepared.record.writes.len(), 3);
    let index_write = prepared
        .record
        .writes
        .iter()
        .find(|w| w.mutation.key == MODULE_WAKE_INDEX_KEY_V1)
        .unwrap();
    let index = decoded_index(index_write.mutation.after.unwrap(), &source, &memory);
    assert_eq!(index.entries, 2);
    let mut index_quota = io();
    let index_reader = crate::world::incremental_scheduler_index::IndexReader::new(
        &source,
        &memory,
        MemoryClass::Ordinary,
        &mut index_quota,
    );
    assert!(index.contains(12, "m#1", &index_reader).unwrap());
    assert!(!index.contains(5, "m#1", &index_reader).unwrap());
    assert!(!index.contains(7, "m#1", &index_reader).unwrap());
    assert!(index.contains(20, "m#2", &index_reader).unwrap());
    assert_eq!(world.module_tick_schedule, before);
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
}

#[test]
fn suspension_absence_and_saturated_directives_preserve_existing_tick_rules() {
    use crate::world::module_tick_runtime::wake_after_directive;
    for (now, ticks, expected) in [
        (0, 0, 1),
        (11, 2, 13),
        (u64::MAX - 1, 99, u64::MAX),
        (u64::MAX, 0, u64::MAX),
    ] {
        assert_eq!(
            wake_after_directive(now, Some(&D::WakeAfterTicks { ticks })),
            Some(expected)
        );
    }
    assert_eq!(wake_after_directive(11, None), None);
    assert_eq!(wake_after_directive(11, Some(&D::Suspend)), None);
    for directive in [None, Some(D::Suspend)] {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let (mut world, mut parent, mut pages) = fixture(&memory);
        world.module_tick_schedule.insert("m#1".into(), 11);
        let mut values = Values::default();
        seed_schedules(&world, &mut parent, &mut pages, &mut values, &memory);
        write_field_bytes(
            &oasis7_world_state::incremental_agent_body::world_time_bytes_v1(11),
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
        )
        .unwrap();
        let source = values.clone();
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
        let mut quota = io();
        let mut ctx = BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut quota,
            cancelled: || false,
        };
        assert!(
            world
                .contribute_iws_due_module_tick(
                    "m#1",
                    &mut txn,
                    &source,
                    &mut ctx,
                    |_, _, _, _| Ok(directive)
                )
                .unwrap()
        );
        assert!(
            !world
                .contribute_iws_due_module_tick(
                    "m#1",
                    &mut txn,
                    &source,
                    &mut ctx,
                    |_, _, _, _| panic!("consumed wake")
                )
                .unwrap()
        );
        let guard = txn.guard();
        let prepared = txn.seal(&guard, &live, 0).unwrap();
        assert_eq!(prepared.record.writes.len(), 2);
        let wake = prepared
            .record
            .writes
            .iter()
            .find(|w| ModuleWakeKeyV1::is_wake_key(&w.mutation.key))
            .unwrap();
        assert_eq!(wake.mutation.after, None);
    }
}

#[test]
fn scheduler_failures_poison_preceding_work_and_leave_cached_schedules_unchanged() {
    for failure in 0..9 {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let (mut world, mut parent, mut pages) = fixture(&memory);
        world.module_tick_schedule.insert("m#1".into(), 5);
        let before = world.module_tick_schedule.clone();
        let mut values = Values::default();
        seed_schedules(&world, &mut parent, &mut pages, &mut values, &memory);
        if failure == 7 {
            let key = ModuleWakeKeyV1::new("m#1").unwrap();
            let id = write_field_bytes(
                &[0; oasis7_world_state::incremental_scheduler_fields::MODULE_WAKE_BYTES],
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
            )
            .unwrap();
            let delta = StateDeltaV1::build(
                parent.roots.state[2].clone(),
                vec![StateWriteV1 {
                    key: key.as_bytes().to_vec(),
                    before: parent.roots.state[2].get(key.as_bytes(), &pages).unwrap(),
                    after: Some(id),
                }],
                &pages,
                4 << 20,
            )
            .unwrap();
            parent.roots.state[2] = delta.after;
            for node in delta.new_nodes {
                pages.0.insert(node.digest, node.bytes);
            }
        }
        write_field_bytes(
            &oasis7_world_state::incremental_agent_body::world_time_bytes_v1(11),
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
        )
        .unwrap();
        if failure == 0 {
            world.state.time += 1;
        }
        let source = values.clone();
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
        txn.write(Ns::Effects, b"outer-work", Some([7; 32]))
            .unwrap();
        let alien = MemoryBudget::new(32 << 20, 0).unwrap();
        let late_cancel = Cell::new(failure == 1);
        let mut quota = io();
        if failure == 2 {
            quota.nodes_remaining = 0;
        }
        if failure == 6 {
            quota.write_bytes_remaining = 0;
        }
        let mut ctx = BodyMutationContext {
            sink: &mut values,
            memory: if failure == 3 { &alien } else { &memory },
            io: &mut quota,
            cancelled: || late_cancel.get(),
        };
        let bad_key = "x".repeat(1024);
        let result = world.contribute_iws_due_module_tick(
            if failure == 8 { &bad_key } else { "m#1" },
            &mut txn,
            &source,
            &mut ctx,
            |_, _, txn, _| {
                txn.write(Ns::Business, b"callback-work", Some([8; 32]))?;
                if failure == 4 {
                    return Err(BodyMutationError::Invalid("injected module failure"));
                }
                if failure == 5 {
                    late_cancel.set(true);
                }
                Ok((failure == 6).then_some(D::WakeAfterTicks { ticks: 1 }))
            },
        );
        assert!(result.is_err(), "failure {failure}");
        let guard = txn.guard();
        assert!(txn.seal(&guard, &live, 0).is_err());
        assert_eq!(world.module_tick_schedule, before);
    }
}
