use super::*;
use crate::DueModuleBatchLimitsV1;
use oasis7_formal_protocol::incremental_schedule::ScheduleScanBudget;
fn scan_budget() -> ScheduleScanBudget {
    ScheduleScanBudget {
        nodes_remaining: 50_000,
        read_bytes_remaining: 16 << 20,
        buckets_remaining: 100,
        instances_remaining: 100,
    }
}
fn batch_limits() -> DueModuleBatchLimitsV1 {
    DueModuleBatchLimitsV1 {
        entries: 4,
        instance_bytes: 64,
    }
}
fn due_fixture(memory: &MemoryBudget) -> (World, ExecutionParent, Pages, Values) {
    let (mut world, mut parent, mut state) = fixture(memory);
    world.module_tick_schedule = [
        ("z#".into(), 0),
        ("a#".into(), 10),
        ("m#\0".into(), 11),
        ("é".into(), 3),
        ("future".into(), 12),
    ]
    .into();
    let mut values = Values::default();
    seed_schedules(&world, &mut parent, &mut state, &mut values, memory);
    write_field_bytes(
        &oasis7_world_state::incremental_agent_body::world_time_bytes_v1(11),
        &mut BodyMutationContext {
            sink: &mut values,
            memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    (world, parent, state, values)
}
#[test]
fn due_batch_restores_original_id_order_from_index_not_legacy_map_and_retains_charge() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let (mut world, parent, pages, mut values) = due_fixture(&memory);
    world.module_tick_schedule = [("bogus".into(), 0)].into();
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    let held = memory.used();
    let batch = world
        .read_iws_due_module_batch(
            &mut txn,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
            &mut scan_budget(),
            batch_limits(),
        )
        .unwrap();
    // Literal original BTreeMap<String, _> order; wakes and hash traversal differ.
    assert_eq!(
        batch.iter().collect::<Vec<_>>(),
        [("a#", 10), ("m#\0", 11), ("z#", 0), ("é", 3)]
    );
    assert!(memory.used() > held);
    txn.validate_guard(batch.guard(), &live).unwrap();
    let alien =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    assert!(alien.validate_guard(batch.guard(), &live).is_err());
    drop(alien);
    let sealed = txn.seal(batch.guard(), &live, 0).unwrap();
    assert!(sealed.record.writes.is_empty());
    assert_eq!(sealed.record.reads.len(), 6); // clock, index and four scalar wakes
    TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&sealed.record)
    .unwrap();
    let before_drop = memory.used();
    drop(batch);
    assert!(memory.used() < before_drop);
    assert_eq!(world.module_tick_schedule, [("bogus".into(), 0)].into());
}
#[test]
fn due_batch_failure_never_returns_a_prefix_or_seals_earlier_outer_work() {
    for failure in 0..6 {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let (mut world, parent, pages, mut values) = due_fixture(&memory);
        let source = values.clone();
        let live = parent.fence().unwrap();
        let before_transaction = memory.used();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
        txn.write(Ns::Effects, b"outer", Some([8; 32])).unwrap();
        let mut cap = batch_limits();
        let mut scan = scan_budget();
        let alien = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        match failure {
            0 => cap.entries = 3,
            1 => cap.instance_bytes = 1,
            2 => scan.nodes_remaining = 0,
            3 => world.state.time = 12,
            _ => {}
        }
        let result = world.read_iws_due_module_batch(
            &mut txn,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: if failure == 5 { &alien } else { &memory },
                io: &mut io(),
                cancelled: || failure == 4,
            },
            &mut scan,
            cap,
        );
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("unexpected prefix: {failure}"),
        };
        match (failure, error) {
            (
                0,
                BodyMutationError::Value(ValueTreeError::ResourceLimited("module due entries")),
            )
            | (
                1,
                BodyMutationError::Value(ValueTreeError::ResourceLimited(
                    "module due instance bytes",
                )),
            )
            | (2, BodyMutationError::Value(ValueTreeError::ResourceLimited("module due scan")))
            | (3, BodyMutationError::Invalid("scheduler clock precondition"))
            | (4, BodyMutationError::Value(ValueTreeError::Cancelled))
            | (5, BodyMutationError::Invalid("scheduler transaction pool")) => {}
            (case, error) => panic!("unexpected error {case}: {error:?}"),
        }
        let guard = txn.guard();
        assert!(txn.seal(&guard, &live, 0).is_err());
        drop(guard);
        assert_eq!(memory.used(), before_transaction);
    }
}
#[test]
fn due_batch_rejects_index_scalar_mismatch_and_stale_same_owner_guard() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let (world, parent, pages, mut values) = due_fixture(&memory);
    let source = values.clone();
    let live = parent.fence().unwrap();
    for mismatch in [false, true] {
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits())
                .unwrap();
        if mismatch {
            let root = write_field_bytes(
                &ModuleWakeV1(99).canonical_bytes(),
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
            )
            .unwrap();
            txn.write(
                Ns::Scheduler,
                ModuleWakeKeyV1::new("a#").unwrap().as_bytes(),
                Some(root),
            )
            .unwrap();
        }
        let result = world.read_iws_due_module_batch(
            &mut txn,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
            &mut scan_budget(),
            batch_limits(),
        );
        if mismatch {
            assert!(matches!(
                result,
                Err(BodyMutationError::Invalid(
                    "module due index/scalar mismatch"
                ))
            ));
            let guard = txn.guard();
            assert!(txn.seal(&guard, &live, 0).is_err());
        } else {
            let batch = result.unwrap();
            txn.write(Ns::Effects, b"later", Some([8; 32])).unwrap();
            assert!(txn.validate_guard(batch.guard(), &live).is_err());
        }
    }
}
#[test]
fn due_batch_rejects_same_instance_at_two_wakes_before_scalar_acceptance() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let (world, parent, pages, mut values) = due_fixture(&memory);
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    let root = decoded_index(
        txn.read(Ns::Scheduler, MODULE_WAKE_INDEX_KEY_V1)
            .unwrap()
            .unwrap(),
        &source,
        &memory,
    );
    let mut logical = io();
    let reader = crate::world::incremental_scheduler_index::IndexReader::new(
        &source,
        &memory,
        MemoryClass::Ordinary,
        &mut logical,
    );
    let delta = root
        .update("z#", None, Some(2), &reader, &memory, 256 << 10)
        .unwrap();
    crate::world::incremental_scheduler_index::persist(
        &delta,
        &mut BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    let root_value = write_field_bytes(
        &delta.after.canonical_bytes().unwrap(),
        &mut BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    txn.write(Ns::Scheduler, MODULE_WAKE_INDEX_KEY_V1, Some(root_value))
        .unwrap();
    let result = world.read_iws_due_module_batch(
        &mut txn,
        &source,
        &mut BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut io(),
            cancelled: || false,
        },
        &mut scan_budget(),
        DueModuleBatchLimitsV1 {
            entries: 5,
            instance_bytes: 64,
        },
    );
    assert!(matches!(
        result,
        Err(BodyMutationError::Invalid("module due duplicate instance"))
    ));
    let guard = txn.guard();
    assert!(txn.seal(&guard, &live, 0).is_err());
}

#[test]
fn due_batch_future_growth_does_not_increase_selected_index_reads() {
    let mut measured = Vec::new();
    for future in [0, 40] {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let (mut world, mut parent, mut pages) = fixture(&memory);
        world.module_tick_schedule = [
            ("z#".into(), 0),
            ("a#".into(), 10),
            ("m#\0".into(), 11),
            ("é".into(), 3),
        ]
        .into();
        for i in 0..future {
            world
                .module_tick_schedule
                .insert(format!("future-{i}"), 100_000 + i);
        }
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
        source.reads.set(0);
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
        let mut quota = scan_budget();
        let batch = world
            .read_iws_due_module_batch(
                &mut txn,
                &source,
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                &mut quota,
                batch_limits(),
            )
            .unwrap();
        assert_eq!(
            batch.iter().collect::<Vec<_>>(),
            [("a#", 10), ("m#\0", 11), ("z#", 0), ("é", 3)]
        );
        assert_eq!(quota.instances_remaining, 96);
        measured.push((
            source.reads.get(),
            quota.nodes_remaining,
            quota.read_bytes_remaining,
        ));
    }
    assert_eq!(measured[0], measured[1]);
}
#[test]
fn due_batch_empty_selection_and_mid_read_cancellation_are_explicit() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let (mut world, mut parent, mut pages) = fixture(&memory);
    world.module_tick_schedule.insert("future".into(), 12);
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
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    let mut quota = scan_budget();
    quota.buckets_remaining = 0;
    quota.instances_remaining = 0;
    let batch = world
        .read_iws_due_module_batch(
            &mut txn,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
            &mut quota,
            DueModuleBatchLimitsV1 {
                entries: 0,
                instance_bytes: 0,
            },
        )
        .unwrap();
    assert_eq!(batch.iter().len(), 0);
    drop(batch);

    let (world, parent, pages, mut values) = due_fixture(&memory);
    let source = values.clone();
    source.reads.set(0);
    let live = parent.fence().unwrap();
    let before_transaction = memory.used();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    txn.write(Ns::Effects, b"earlier", Some([8; 32])).unwrap();
    let result = world.read_iws_due_module_batch(
        &mut txn,
        &source,
        &mut BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut io(),
            cancelled: || source.reads.get() > 20,
        },
        &mut scan_budget(),
        batch_limits(),
    );
    assert!(source.reads.get() > 20);
    assert!(matches!(
        result,
        Err(BodyMutationError::Value(ValueTreeError::Cancelled))
    ));
    let guard = txn.guard();
    assert!(txn.seal(&guard, &live, 0).is_err());
    drop(guard);
    assert_eq!(memory.used(), before_transaction);
}
