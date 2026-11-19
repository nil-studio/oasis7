//! Business-owner failure regressions; the Root queue oracle remains in the parent.
use super::*;

#[derive(Debug, Clone, Copy)]
enum TransferResourceFailureCase {
    ForeignRuleOutcome,
    ReceiverOverflow,
    OriginalSink,
    ContinuousWork,
    CancellationDuringOriginal,
}

fn run_transfer_resource_failure(case: TransferResourceFailureCase) {
    use oasis7_agent_api::world_action::ResourceKind;

    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = transfer_resource_world();
    if matches!(case, TransferResourceFailureCase::ReceiverOverflow) {
        world
            .state
            .agents
            .get_mut("receiver")
            .unwrap()
            .state
            .resources
            .set(ResourceKind::Electricity, i64::MAX)
            .unwrap();
    }
    world.submit_action(crate::Action::EmitResourceTransfer {
        from_agent_id: "sender".into(),
        to_agent_id: "receiver".into(),
        kind: ResourceKind::Electricity,
        amount: 3,
    });
    if matches!(case, TransferResourceFailureCase::ForeignRuleOutcome) {
        world.submit_action(crate::Action::EmitResourceTransfer {
            from_agent_id: "sender".into(),
            to_agent_id: "receiver".into(),
            kind: ResourceKind::Electricity,
            amount: 1,
        });
    }
    let expected_error = if matches!(case, TransferResourceFailureCase::ReceiverOverflow) {
        let body = world
            .action_to_event(world.pending_actions.front().unwrap())
            .unwrap();
        Some(world.preflight_domain_event(&body).unwrap_err())
    } else {
        None
    };
    let material_limits = MaterialFixtureLimitsV1::for_state(&world.state);
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut material_backing = MaterialFixtureV1::build(
        &fixture.roots,
        &reader,
        &reader,
        &memory,
        material_limits,
        || reader.cas.error(),
        || false,
    );
    let mut materials = material_backing.context(&fixture.roots, &reader, || reader.cas.error());
    let mut originals = Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [151; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: oasis7_formal_protocol::incremental_execution::EXECUTION_MAX_READS,
            max_writes: 1024,
            max_facts: 64,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &tx, &memory).unwrap();
    let directory = {
        let mut io = genesis::io();
        let mut keys = genesis::keys();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        build_accepted_invocation_directory_v1(
            &fixture.runtime,
            &fixture.head,
            fixture.current.raw().instances(),
            fixture.current.runtime().config(),
            &reader,
            &mut overlay,
            &mut rules::directory_work(),
            &live,
        )
        .unwrap()
    };
    let mut pending_work = genesis::current_work(&fixture.roots).runtime.pending;
    let guard = tx.guard();
    let mut pending =
        oasis7_world_state::incremental_pending_action_overlay::begin_pending_action_current_v1(
            fixture.current.runtime().pending(),
            &fixture.roots,
            &mut tx,
            &memory,
            &guard,
            &live,
            &mut pending_work.index,
            || false,
        )
        .unwrap();
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut read_io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: stopped(&stop),
    };
    let mut parent_io = genesis::io();
    let mut parent_keys = genesis::keys();
    let mut parent_read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut parent_io,
        },
        key_work: &mut parent_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: stopped(&stop),
    };
    let mut write_io = genesis::io();
    let mut sink = fixture.cas.clone();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut write_io,
        cancelled: stopped(&stop),
    };
    let mut canonical = genesis::canonical();
    let guard = tx.guard();
    let failed = {
        let mut step = IwsStepContext::begin_counted(
            &mut tx,
            0,
            &guard,
            &live,
            &reader,
            &mut mutation,
            &mut canonical,
            &counter,
        )
        .unwrap();
        let mut work = transfer_resource_work();
        if matches!(case, TransferResourceFailureCase::ContinuousWork) {
            // One continuous counter covers identity, presence, positions,
            // balances and writes; exhaustion must abort before publication.
            work.steps_remaining = 5;
            work.bytes_remaining = 4096;
        }
        let mut audits = transfer_resource_audit_work();
        let selected = take_pending_action(
            &mut pending,
            step.transaction,
            &mut read,
            &mut pending_work,
            &mut originals,
            &live,
        )
        .unwrap()
        .unwrap();
        let outcome = if matches!(case, TransferResourceFailureCase::ForeignRuleOutcome) {
            let foreign = take_pending_action(
                &mut pending,
                step.transaction,
                &mut read,
                &mut pending_work,
                &mut originals,
                &live,
            )
            .unwrap()
            .unwrap();
            rules::outcome(
                &fixture,
                &reader,
                step.transaction,
                &foreign,
                &directory,
                &counter,
                &mut originals,
                &memory,
                step.time(),
                &live,
            )
        } else {
            rules::outcome(
                &fixture,
                &reader,
                step.transaction,
                &selected,
                &directory,
                &counter,
                &mut originals,
                &memory,
                step.time(),
                &live,
            )
        };
        let guard = step.transaction.guard();
        let result = execute_selected_transfer_resource_v1(
            &mut step,
            &mut materials,
            &selected,
            &outcome,
            fixture.current.raw().agents(),
            &fixture.roots,
            &mut read,
            &mut parent_read,
            &mut work,
            &mut audits,
            64 << 10,
            &guard,
            &live,
            |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                if matches!(case, TransferResourceFailureCase::OriginalSink) {
                    return Err(BodyMutationError::Invalid(
                        "injected TransferResource original sink failure",
                    ));
                }
                producer.persist_to(&mut |stream: &mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1, cancelled: &mut dyn FnMut() -> bool| {
                    let bytes = usize::try_from(stream.original_bytes()).unwrap();
                    let _fee = memory
                        .reserve(bytes + std::mem::size_of::<Vec<u8>>(), MemoryClass::Ordinary)
                        .map_err(ExecutionError::Memory)?;
                    let mut encoded = Vec::with_capacity(bytes);
                    stream.write_to(&mut encoded)?;
                    assert_eq!(fixture.objects.put(&encoded), stream.original_id());
                    if matches!(case, TransferResourceFailureCase::CancellationDuringOriginal) {
                        stop.store(true, std::sync::atomic::Ordering::Release);
                    }
                    if cancelled() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    Ok(())
                })
            },
        );
        if let Some(expected) = &expected_error {
            match &result {
                Err(BodyMutationError::Original(error)) => {
                    let original=error.downcast_ref::<crate::world::incremental_action_data::transfer_resource::TransferResourceStateErrorV1>().expect("typed original transfer state error");
                    assert_eq!(&original.cause, expected);
                }
                _ => panic!("expected original resource overflow error"),
            }
        }
        result.is_err()
    };
    assert!(failed, "expected {case:?} to reject the transition");
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}

#[test]
fn selected_transfer_resource_foreign_overflow_sink_cancellation_and_work_fail_closed() {
    for case in [
        TransferResourceFailureCase::ForeignRuleOutcome,
        TransferResourceFailureCase::ReceiverOverflow,
        TransferResourceFailureCase::OriginalSink,
        TransferResourceFailureCase::ContinuousWork,
        TransferResourceFailureCase::CancellationDuringOriginal,
    ] {
        run_transfer_resource_failure(case);
    }
}
