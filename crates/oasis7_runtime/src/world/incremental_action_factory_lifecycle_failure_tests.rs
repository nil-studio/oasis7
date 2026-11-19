//! Real fallible Root FIFO: preserve original causes and abort without result facts.
use super::*;
use std::sync::atomic::AtomicBool;
#[derive(Clone, Copy)]
enum Failure {
    OldState,
    CancelBeforeSelection,
    PersistSentinel,
}
#[derive(Debug)]
struct PersistSentinel;
impl std::fmt::Display for PersistSentinel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("factory-original-persist-sentinel")
    }
}
impl std::error::Error for PersistSentinel {}
fn base_world() -> World {
    let mut world = World::new();
    let agent = oasis7_world_state::agent_state::AgentState::new(
        "owner",
        oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    );
    world
        .state
        .agents
        .insert("owner".into(), crate::AgentCell::new(agent, 0));
    let source = industrial_eligibility::producing_world();
    let mut factory = source.state.factories["assembler"].clone();
    factory.output_ledger = oasis7_world_state::MaterialLedgerId::World;
    factory.spec.build_cost = vec![oasis7_world_state::MaterialStack::new("ore", 3)];
    world.state.factories.insert("assembler".into(), factory);
    world.submit_action(crate::Action::RecycleFactory {
        operator_agent_id: "owner".into(),
        factory_id: "assembler".into(),
    });
    world
}
fn run_failure(world: World, failure: Failure) {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut oracle = World::new();
    oracle.state = world.state.clone();
    oracle.state.time = 1;
    let envelope = world.pending_actions.front().unwrap();
    let body = oracle.action_to_event(envelope).unwrap();
    let old = oracle.append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)));
    let expected = match failure {
        Failure::OldState => {
            let error = old.expect_err("old publication must fail, not produce ActionRejected");
            assert!(matches!(
                error,
                crate::WorldError::ResourceBalanceInvalid { .. }
            ));
            Some(format!("{error:?}"))
        }
        _ => {
            old.unwrap();
            None
        }
    };
    drop(oracle);
    let material_limits = MaterialFixtureLimitsV1::for_state(&world.state);
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut originals = Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [141; 32],
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
    let mut sink = fixture.cas.clone();
    let mut write_io = genesis::io();
    let mut canonical = genesis::canonical();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut write_io,
        cancelled: || false,
    };
    let mut work = lifecycle_work();
    let mut audit = transfer_resource_audit_work();
    let mut callbacks = 0;
    let stop = AtomicBool::new(matches!(failure, Failure::CancelBeforeSelection));
    let mut persist =
        |_producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
            callbacks += 1;
            Err(crate::world::incremental_body::BodyMutationError::Original(
                Box::new(PersistSentinel),
            ))
        };
    let error = {
        let guard = tx.guard();
        let step = IwsStepContext::begin_counted(
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
        let now = step.time();
        match rules::try_evaluate_with_factory_lifecycle(
            &fixture,
            &reader,
            step.transaction,
            &mut pending,
            &mut pending_work,
            &directory,
            &counter,
            &mut originals,
            &memory,
            now,
            &live,
            matches!(failure, Failure::CancelBeforeSelection).then_some(&stop),
            rules::NonmovementExecutionFixtureV1 {
                limits: material_limits,
                persist: &mut persist,
                work: rules::NonmovementFixtureWorkV1::FactoryLifecycle(&mut work),
                audit: &mut audit,
            },
        ) {
            Err(error) => error,
            Ok(_) => panic!("fallible genuine FIFO unexpectedly returned completed receipt"),
        }
    };
    let body = error
        .body_error_for_fixture()
        .unwrap_or_else(|| panic!("actual Root business/selection body cause: {error:?}"));
    match failure {
        Failure::OldState => match body {
            crate::world::incremental_body::BodyMutationError::Original(cause) => {
                assert_eq!(cause.to_string(), expected.unwrap())
            }
            other => panic!("old typed state error changed classification: {other:?}"),
        },
        Failure::CancelBeforeSelection => assert!(
            matches!(
                body,
                crate::world::incremental_body::BodyMutationError::Value(ValueTreeError::Cancelled)
            ),
            "{body:?}"
        ),
        Failure::PersistSentinel => match body {
            crate::world::incremental_body::BodyMutationError::Original(cause) => {
                assert!(cause.downcast_ref::<PersistSentinel>().is_some(), "{cause}")
            }
            other => panic!("persist original cause replaced: {other:?}"),
        },
    }
    assert_eq!(
        callbacks,
        usize::from(matches!(failure, Failure::PersistSentinel))
    );
    assert_eq!(
        (0..tx.ordered_fact_count())
            .filter_map(|i| tx.ordered_fact(i))
            .filter(|fact| fact.kind == ExecutionFactKindV1::Event)
            .count(),
        0
    );
    let inputs = (0..tx.ordered_fact_count())
        .filter_map(|i| tx.ordered_fact(i))
        .filter(|fact| fact.kind == ExecutionFactKindV1::Input)
        .count();
    assert_eq!(
        inputs,
        usize::from(!matches!(failure, Failure::CancelBeforeSelection))
    );
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
#[test]
fn fifo_factory_receipt_conflict_precedes_retired_replay() {
    for retired in [false, true] {
        let mut world = base_world();
        world.state.factory_recycle_receipts.insert(
            "assembler".into(),
            oasis7_world_state::FactoryRecycleReceiptV1 {
                operator_agent_id: "different-owner".into(),
                factory_id: "assembler".into(),
                recycle_ledger: oasis7_world_state::MaterialLedgerId::World,
                recovered: vec![],
                durability_ppm: 0,
            },
        );
        if retired {
            world.state.retired_factory_ids.insert("assembler".into());
        }
        run_failure(world, Failure::OldState);
    }
}
#[test]
fn fifo_factory_raw_preflight_overflow_preserves_old_typed_error() {
    let mut world = base_world();
    world
        .state
        .material_ledgers
        .entry(oasis7_world_state::MaterialLedgerId::World)
        .or_default()
        .insert("ore".into(), i64::MAX);
    world.state.materials.insert("ore".into(), i64::MAX);
    run_failure(world, Failure::OldState);
}
#[test]
fn fifo_factory_normalized_add_overflow_preserves_later_old_typed_error() {
    let mut world = base_world();
    // Raw World is empty, so preflight accepts; old normalization then selects
    // legacy MAX and the actual addition fails at its distinct later boundary.
    world.state.materials.insert("ore".into(), i64::MAX);
    run_failure(world, Failure::OldState);
}
#[test]
fn fifo_factory_cancel_before_selection_has_no_input_or_event_and_aborts() {
    run_failure(base_world(), Failure::CancelBeforeSelection);
}
#[test]
fn fifo_factory_original_persist_retains_exact_sentinel_and_aborts() {
    run_failure(base_world(), Failure::PersistSentinel);
}
