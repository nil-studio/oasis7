use super::*;
use crate::world::incremental_allocators::{RuntimeAllocatorV1 as Alloc, RuntimeSequenceV1 as Seq};
use crate::world::incremental_step::IwsStepContext;
use oasis7_agent_api::world_action::GeoPos;
use oasis7_world_state::incremental_agent_fields::{
    AgentFieldKeyV1, AgentFieldV1, AgentScalarRefV1,
};

fn quota() -> oasis7_formal_protocol::incremental_value::ValueIoBudget {
    oasis7_formal_protocol::incremental_value::ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 128 << 20,
        write_bytes_remaining: 128 << 20,
    }
}
fn txn_limits() -> oasis7_formal_protocol::incremental_execution::TransactionLimits {
    let mut result = limits();
    result.max_reads = 128;
    result.max_writes = 128;
    result
}
fn add_leaf(
    parent: &mut ExecutionParent,
    state: &mut Pages,
    namespace: Ns,
    key: Vec<u8>,
    id: [u8; 32],
) {
    let slot = namespace as usize - 1;
    let delta = StateDeltaV1::build(
        parent.roots.state[slot].clone(),
        vec![StateWriteV1 {
            key,
            before: None,
            after: Some(id),
        }],
        state,
        4 << 20,
    )
    .unwrap();
    parent.roots.state[slot] = delta.after;
    for page in delta.new_nodes {
        state.0.insert(page.digest, page.bytes);
    }
}
fn setup(memory: &MemoryBudget) -> (World, ExecutionParent, Pages, Values) {
    let (mut world, mut parent, mut pages) = fixture(memory);
    let mut values = Values::default();
    world.module_tick_schedule.insert("module".into(), 12);
    seed(
        &world,
        &mut parent,
        &mut pages,
        &mut values,
        memory,
        EventSequenceV1 { next: 1, era: 0 },
        MailboxFrontierV1 { head: 0, tail: 0 },
    );
    scheduler::seed_schedules(&world, &mut parent, &mut pages, &mut values, memory);
    for field in [
        AgentFieldV1::PositionX,
        AgentFieldV1::PositionY,
        AgentFieldV1::PositionZ,
    ] {
        let id = crate::world::incremental_agent_genesis::write_agent_scalar_value(
            field,
            AgentScalarRefV1::I64(0),
            &mut BodyMutationContext {
                sink: &mut values,
                memory,
                io: &mut quota(),
                cancelled: || false,
            },
            oasis7_world_state::incremental_agent_fields::AGENT_I64_VALUE_BYTES as u64,
        )
        .unwrap();
        add_leaf(
            &mut parent,
            &mut pages,
            Ns::Business,
            AgentFieldKeyV1::new("alice", field, 0)
                .unwrap()
                .as_bytes()
                .to_vec(),
            id,
        );
    }
    (world, parent, pages, values)
}

#[test]
fn root_step_dispatches_successive_events_and_due_work_in_one_overlay() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let mut encoding_work = original_work();
    let (world, parent, pages, mut values) = setup(&memory);
    let mut material_owner = super::material_fixture(&world, &parent, &pages, &values, &memory);
    let mut materials =
        material_owner
            .fixture
            .context(&parent.roots, &material_owner.pages, || None);
    let mut body_event = event(&world, 2);
    body_event.time = 12;
    // No World is passed to the context or reducers after input construction.
    drop(world);
    let source = values.clone();
    let mut work = quota();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(
        parent.clone(),
        [7; 32],
        &material_owner.pages,
        &memory,
        txn_limits(),
    )
    .unwrap();
    let guard = txn.guard();
    let mut mutation = BodyMutationContext {
        sink: &mut values,
        memory: &memory,
        io: &mut work,
        cancelled: || false,
    };
    let mut step = IwsStepContext::begin(
        &mut txn,
        11,
        &guard,
        &live,
        &source,
        &mut mutation,
        &mut encoding_work,
    )
    .unwrap();
    assert_eq!(step.time(), 12);
    let (id, next) = step
        .allocate(Alloc::Event, Seq { next: 1, era: 0 })
        .unwrap();
    assert_eq!(id, 1);
    assert_eq!(next, Seq { next: 2, era: 0 });
    step.dispatch_agent_event(&body_event, &mut materials, &live, 4096, |original| {
        collect_original(original, &memory).map(|_| ())
    })
    .unwrap();
    let moved = WorldEvent {
        id: 3,
        time: 12,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::AgentMoved {
            agent_id: "alice".into(),
            from: GeoPos::new(99, 99, 99),
            to: GeoPos::new(4, 5, 6),
        }),
    };
    step.dispatch_agent_event(&moved, &mut materials, &live, 4096, |original| {
        collect_original(original, &memory).map(|_| ())
    })
    .unwrap();
    assert_eq!(
        step.read_allocator(Alloc::Event).unwrap(),
        Seq { next: 4, era: 0 }
    );
    step.module_wake("module", Some(12)).unwrap();
    assert!(
        step.due_module_tick("module", |instance, now, txn, context| {
            assert_eq!((instance, now), ("module", 12));
            assert_eq!(
                World::read_iws_allocator(Alloc::Event, txn, &source, context)?,
                Seq { next: 4, era: 0 }
            );
            Ok(None)
        })
        .unwrap()
    );
    assert!(
        !step
            .due_module_tick("module", |_, _, _, _| panic!("cleared wake replayed"))
            .unwrap()
    );
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.facts.len(), 2);
    TransitionVerifier {
        parent: &parent,
        operation: [7; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&prepared.record)
    .unwrap();
    let key = AgentFieldKeyV1::new("alice", AgentFieldV1::PositionY, 0).unwrap();
    assert!(
        prepared
            .record
            .writes
            .iter()
            .any(|write| write.mutation.key == key.as_bytes())
    );
}

#[test]
fn root_step_original_callback_final_cancel_must_abort_prefix() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let mut encoding_work = original_work();
    let (world, parent, pages, mut values) = setup(&memory);
    let mut material_owner = super::material_fixture(&world, &parent, &pages, &values, &memory);
    let mut materials =
        material_owner
            .fixture
            .context(&parent.roots, &material_owner.pages, || None);
    let mut original = event(&world, 1);
    original.time = 12;
    let source = values.clone();
    let mut txn = ExecutionTransaction::begin(
        parent.clone(),
        [8; 32],
        &material_owner.pages,
        &memory,
        txn_limits(),
    )
    .unwrap();
    let guard = txn.guard();
    let cancelled = Cell::new(false);
    let persisted = Cell::new(false);
    let mut work = quota();
    let mut mutation = BodyMutationContext {
        sink: &mut values,
        memory: &memory,
        io: &mut work,
        cancelled: || cancelled.get(),
    };
    let mut step = IwsStepContext::begin(
        &mut txn,
        11,
        &guard,
        &parent.fence().unwrap(),
        &source,
        &mut mutation,
        &mut encoding_work,
    )
    .unwrap();
    let result = step.dispatch_agent_event(
        &original,
        &mut materials,
        &parent.fence().unwrap(),
        4096,
        |original| {
            collect_original(original, &memory)?;
            persisted.set(true);
            cancelled.set(true);
            Ok(())
        },
    );
    assert!(persisted.get());
    assert!(matches!(
        result,
        Err(BodyMutationError::Value(ValueTreeError::Cancelled))
    ));
    let guard = txn.guard();
    assert!(matches!(
        txn.seal(&guard, &parent.fence().unwrap(), 0),
        Err(ExecutionError::Aborted)
    ));
}

#[test]
fn root_step_stream_requires_consumption_and_finish_and_preserves_spent_work_on_error() {
    for mode in 0..3 {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let (world, parent, pages, mut values) = setup(&memory);
        let mut material_owner = super::material_fixture(&world, &parent, &pages, &values, &memory);
        let mut materials =
            material_owner
                .fixture
                .context(&parent.roots, &material_owner.pages, || None);
        let mut original = event(&world, 2);
        original.time = 12;
        drop(world);
        let source = values.clone();
        let puts = Rc::new(Cell::new(0));
        let mut counted = CountWrites {
            values: &mut values,
            puts: puts.clone(),
        };
        let mut work = quota();
        let mut encoding_work = original_work();
        let initial = (
            encoding_work.visits_remaining,
            encoding_work.bytes_remaining,
        );
        let mut mutation = BodyMutationContext {
            sink: &mut counted,
            memory: &memory,
            io: &mut work,
            cancelled: || false,
        };
        let mut txn = ExecutionTransaction::begin(
            parent.clone(),
            [8; 32],
            &material_owner.pages,
            &memory,
            txn_limits(),
        )
        .unwrap();
        let guard = txn.guard();
        let consumed = Cell::new(false);
        let result = {
            let mut step = IwsStepContext::begin(
                &mut txn,
                11,
                &guard,
                &parent.fence().unwrap(),
                &source,
                &mut mutation,
                &mut encoding_work,
            )
            .unwrap();
            step.allocate(Alloc::Event, Seq { next: 1, era: 0 })
                .unwrap();
            assert!(puts.get() > 0);
            step.dispatch_agent_event(
                &original,
                &mut materials,
                &parent.fence().unwrap(),
                4096,
                |original| {
                    if mode == 0 {
                        return Ok(());
                    }
                    let result = original.persist_to(
                        &mut |stream: &mut dyn AgentEventOriginalStreamV1,
                              _cancelled: &mut dyn FnMut() -> bool| {
                            stream.write_to(&mut std::io::sink())?;
                            consumed.set(true);
                            Err(BodyMutationError::Original(Box::new(
                                std::io::Error::other("step finish failure"),
                            )))
                        },
                    );
                    if mode == 1 {
                        result
                    } else {
                        assert!(result.is_err());
                        Ok(()) // Swallowing finish failure cannot manufacture completion.
                    }
                },
            )
        };
        assert_eq!(consumed.get(), mode != 0);
        if mode == 1 {
            assert!(matches!(result, Err(BodyMutationError::Original(_))));
        } else {
            assert!(matches!(
                result,
                Err(BodyMutationError::Invalid(
                    "agent original persistence incomplete"
                ))
            ));
        }
        assert!(encoding_work.visits_remaining < initial.0);
        assert!(encoding_work.bytes_remaining < initial.1);
        let guard = txn.guard();
        assert!(matches!(
            txn.seal(&guard, &parent.fence().unwrap(), 0),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[test]
fn root_step_rejects_stale_clock_fence_and_later_failure_aborts_prefix() {
    for mode in 0..4 {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let mut encoding_work = original_work();
        let (world, parent, pages, mut values) = setup(&memory);
        let mut material_owner = super::material_fixture(&world, &parent, &pages, &values, &memory);
        let mut materials =
            material_owner
                .fixture
                .context(&parent.roots, &material_owner.pages, || None);
        drop(world);
        let source = values.clone();
        let mut live = parent.fence().unwrap();
        if mode == 1 {
            live.parent_commit = [9; 32];
        }
        let mut txn = ExecutionTransaction::begin(
            parent.clone(),
            [8; 32],
            &material_owner.pages,
            &memory,
            txn_limits(),
        )
        .unwrap();
        let guard = txn.guard();
        let mut work = quota();
        let mut context = BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut work,
            cancelled: || false,
        };
        let result = IwsStepContext::begin(
            &mut txn,
            if mode == 0 { 10 } else { 11 },
            &guard,
            &live,
            &source,
            &mut context,
            &mut encoding_work,
        );
        if mode < 2 {
            assert!(result.is_err());
        } else {
            let mut step = result.unwrap();
            step.allocate(Alloc::Event, Seq { next: 1, era: 0 })
                .unwrap();
            if mode == 2 {
                assert!(
                    step.allocate(Alloc::Event, Seq { next: 1, era: 0 })
                        .is_err()
                );
            } else {
                let unsupported = WorldEvent {
                    id: 2,
                    time: 12,
                    caused_by: None,
                    body: WorldEventBody::Domain(DomainEvent::AgentRegistered {
                        registration_contract_version: 0,
                        agent_id: "bob".into(),
                        pos: GeoPos::new(0, 0, 0),
                    }),
                };
                assert!(matches!(
                    step.dispatch_agent_event(
                        &unsupported,
                        &mut materials,
                        &live,
                        4096,
                        |_| panic!("unsupported event persisted"),
                    ),
                    Err(BodyMutationError::Invalid(
                        "unsupported incremental agent event"
                    ))
                ));
            }
        }
        let guard = txn.guard();
        assert!(matches!(
            txn.seal(&guard, &parent.fence().unwrap(), 0),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[test]
fn root_step_noop_wake_last_poll_cancels_after_successful_clock_prefix() {
    // Observed on 4d11ea428: the unchanged-wake operation makes 59 polls.
    // Freeze this boundary instead of recalibrating it from a shortened path.
    for channel in 0..3 {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let mut encoding_work = original_work();
        let (_, parent, pages, mut values) = setup(&memory);
        let source = values.clone();
        let puts = Rc::new(Cell::new(0));
        let mut counted = CountWrites {
            values: &mut values,
            puts: puts.clone(),
        };
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [8; 32], &pages, &memory, txn_limits())
                .unwrap();
        let guard = txn.guard();
        let mut work = quota();
        let calls = Cell::new(0u64);
        let armed = Cell::new(false);
        let read_cancel = Cell::new(false);
        let write_cancel = Cell::new(false);
        let mut context = BodyMutationContext {
            sink: &mut counted,
            memory: &memory,
            io: &mut work,
            cancelled: || {
                calls.set(calls.get() + 1);
                if armed.get() && calls.get() == 59 {
                    read_cancel.set(channel == 1);
                    write_cancel.set(channel == 2);
                }
                read_cancel.get() || write_cancel.get()
            },
        };
        let mut step = IwsStepContext::begin(
            &mut txn,
            11,
            &guard,
            &parent.fence().unwrap(),
            &source,
            &mut context,
            &mut encoding_work,
        )
        .unwrap();
        calls.set(0);
        armed.set(true);
        // Clock comparison charges canonical hashing work even with Discard;
        // use actual sink calls, not zero logical write IO, as the NoOp oracle.
        assert!(puts.get() > 0);
        puts.set(0);
        let result = step.module_wake("module", Some(12));
        assert_eq!(calls.get(), 59);
        assert_eq!(puts.get(), 0);
        if channel == 0 {
            result.unwrap();
            let guard = txn.guard();
            let prepared = txn.seal(&guard, &parent.fence().unwrap(), 0).unwrap();
            assert_eq!(prepared.record.writes.len(), 1); // Time entry only.
        } else {
            assert!(matches!(
                result,
                Err(BodyMutationError::Value(ValueTreeError::Cancelled))
            ));
            let guard = txn.guard();
            assert!(matches!(
                txn.seal(&guard, &parent.fence().unwrap(), 0),
                Err(ExecutionError::Aborted)
            ));
        }
    }
}

struct CountWrites<'a> {
    values: &'a mut Values,
    puts: Rc<Cell<u64>>,
}

#[test]
fn root_step_executor_cannot_change_the_step_clock() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let mut encoding_work = original_work();
    let (_, parent, pages, mut values) = setup(&memory);
    let source = values.clone();
    let mut work = quota();
    let mut mutation = BodyMutationContext {
        sink: &mut values,
        memory: &memory,
        io: &mut work,
        cancelled: || false,
    };
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [8; 32], &pages, &memory, txn_limits())
            .unwrap();
    let guard = txn.guard();
    let mut step = IwsStepContext::begin(
        &mut txn,
        11,
        &guard,
        &parent.fence().unwrap(),
        &source,
        &mut mutation,
        &mut encoding_work,
    )
    .unwrap();
    let result = step.due_module_tick("module", |_, now, transaction, context| {
        assert_eq!(
            crate::world::incremental_clock::advance_clock(now, transaction, context)?,
            13
        );
        Ok(None)
    });
    assert!(matches!(
        result,
        Err(BodyMutationError::Invalid("world clock precondition"))
    ));
    let guard = txn.guard();
    assert!(matches!(
        txn.seal(&guard, &parent.fence().unwrap(), 0),
        Err(ExecutionError::Aborted)
    ));
}

#[test]
fn root_step_retains_one_value_budget_and_source_errors_abort_real_prefix() {
    for missing_source in [false, true] {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let mut encoding_work = original_work();
        let (world, parent, pages, mut values) = setup(&memory);
        let mut original = event(&world, 1);
        original.time = 12;
        let source = if missing_source {
            Values::default()
        } else {
            values.clone()
        };
        let mut material_owner = super::material_fixture(&world, &parent, &pages, &values, &memory);
        let mut materials =
            material_owner
                .fixture
                .context(&parent.roots, &material_owner.pages, || None);
        let puts = Rc::new(Cell::new(0));
        let mut counted = CountWrites {
            values: &mut values,
            puts: puts.clone(),
        };
        let mut work = quota();
        if !missing_source {
            // 1143's bounded first operation spends 28 logical nodes. The
            // next getter must share the exhausted remainder, not reset it.
            work.nodes_remaining = 28;
        }
        let mut mutation = BodyMutationContext {
            sink: &mut counted,
            memory: &memory,
            io: &mut work,
            cancelled: || false,
        };
        let mut txn = ExecutionTransaction::begin(
            parent.clone(),
            [8; 32],
            &material_owner.pages,
            &memory,
            txn_limits(),
        )
        .unwrap();
        let guard = txn.guard();
        let mut step = IwsStepContext::begin(
            &mut txn,
            11,
            &guard,
            &parent.fence().unwrap(),
            &source,
            &mut mutation,
            &mut encoding_work,
        )
        .unwrap();
        assert!(puts.get() > 0);
        let original_staged = Cell::new(false);
        let result = step.dispatch_agent_event(
            &original,
            &mut materials,
            &parent.fence().unwrap(),
            4096,
            |original| {
                collect_original(original, &memory)?;
                original_staged.set(true);
                Ok(())
            },
        );
        if missing_source {
            assert!(!original_staged.get());
            assert!(matches!(
                result,
                Err(BodyMutationError::Value(ValueTreeError::Invalid(
                    "test value missing"
                )))
            ));
        } else {
            result.unwrap();
            assert!(original_staged.get());
            assert!(matches!(
                step.read_allocator(Alloc::Event),
                Err(BodyMutationError::Value(ValueTreeError::ResourceLimited(_)))
            ));
        }
        let guard = txn.guard();
        assert!(matches!(
            txn.seal(&guard, &parent.fence().unwrap(), 0),
            Err(ExecutionError::Aborted)
        ));
    }
}
impl ValueNodeWriter for CountWrites<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.puts.set(self.puts.get() + 1);
        self.values.put_node(id, bytes)
    }
}
impl ValueNodeReader for CountWrites<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.values.read_node(id, max, memory, class)
    }
}
impl oasis7_formal_protocol::incremental_schedule::SchedulePageWriter for CountWrites<'_> {}

struct ForbidValueRoot {
    values: Values,
    forbidden: [u8; 32],
}
impl ValueNodeReader for ForbidValueRoot {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        assert_ne!(
            *id, self.forbidden,
            "step scanned untouched large agent field"
        );
        self.values.read_node(id, max, memory, class)
    }
}

#[test]
fn root_step_does_not_read_unmodified_large_agent_summary() {
    let mut measured = None;
    for size in [64, 32 << 20] {
        // Setup is excluded from the bounded operation, and its retained pages
        // remain charged. This in-memory fixture does not claim total C<S.
        let memory = MemoryBudget::new(256 << 20, 4 << 20).unwrap();
        let mut encoding_work = original_work();
        let (world, mut parent, mut pages, mut values) = setup(&memory);
        let input_charge = memory.reserve(size, MemoryClass::Ordinary).unwrap();
        let input = input_charge.retain("x".repeat(size));
        let forbidden = crate::world::incremental_agent_genesis::write_agent_scalar_value(
            AgentFieldV1::IntentSummary,
            AgentScalarRefV1::Text(&input),
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut quota(),
                cancelled: || false,
            },
            size as u64 + 256,
        )
        .unwrap();
        add_leaf(
            &mut parent,
            &mut pages,
            Ns::Business,
            AgentFieldKeyV1::new("alice", AgentFieldV1::IntentSummary, 0)
                .unwrap()
                .as_bytes()
                .to_vec(),
            forbidden,
        );
        let source = ForbidValueRoot {
            values: values.clone(),
            forbidden,
        };
        let mut material_owner = super::material_fixture(&world, &parent, &pages, &source, &memory);
        let mut materials =
            material_owner
                .fixture
                .context(&parent.roots, &material_owner.pages, || None);
        let mut original = event(&world, 1);
        original.time = 12;
        drop(input);
        drop(world);
        source.values.reads.set(0);
        let mut txn = ExecutionTransaction::begin(
            parent.clone(),
            [7; 32],
            &material_owner.pages,
            &memory,
            txn_limits(),
        )
        .unwrap();
        let guard = txn.guard();
        let mut work = oasis7_formal_protocol::incremental_value::ValueIoBudget {
            nodes_remaining: 256,
            read_bytes_remaining: 1 << 20,
            write_bytes_remaining: 1 << 20,
        };
        let mut mutation = BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut work,
            cancelled: || false,
        };
        let mut step = IwsStepContext::begin(
            &mut txn,
            11,
            &guard,
            &parent.fence().unwrap(),
            &source,
            &mut mutation,
            &mut encoding_work,
        )
        .unwrap();
        step.dispatch_agent_event(
            &original,
            &mut materials,
            &parent.fence().unwrap(),
            4096,
            |original| collect_original(original, &memory).map(|_| ()),
        )
        .unwrap();
        let guard = txn.guard();
        let prepared = txn.seal(&guard, &parent.fence().unwrap(), 0).unwrap();
        TransitionVerifier {
            parent: &parent,
            operation: [7; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        }
        .verify(&prepared.record)
        .unwrap();
        let current = (
            source.values.reads.get(),
            work.nodes_remaining,
            work.read_bytes_remaining,
            work.write_bytes_remaining,
        );
        eprintln!("step-unmodified-body bytes={size} value_measurement={current:?}");
        if let Some(first) = measured {
            assert_eq!(first, current);
        } else {
            measured = Some(current);
        }
    }
}

#[path = "incremental_module_state_step_tests.rs"]
mod module_state;
