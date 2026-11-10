//! Genuine full-GEN Root failures, including the generated-state text closure.
use super::*;
use std::sync::atomic::AtomicBool;
#[derive(Clone, Copy)]
enum Failure {
    GeneratedCap,
    CancelBeforeSelection,
    PersistSentinel,
}
#[derive(Debug)]
struct MetaPersistSentinel;
impl std::fmt::Display for MetaPersistSentinel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("meta-Accepted-original-persist-sentinel")
    }
}
impl std::error::Error for MetaPersistSentinel {}
fn run_failure(failure: Failure) {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    for id in ["owner", "other"] {
        let agent = oasis7_world_state::agent_state::AgentState::new(
            id,
            oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        );
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(agent, 0));
    }
    world.submit_action(crate::Action::GrantMetaProgress {
        operator_agent_id: "owner".into(),
        target_agent_id: "other".into(),
        track: "x".into(),
        points: 100,
        achievement_id: None,
    });
    // The original operation is legal and actually produces the 13-byte tier
    // achievement. It is the declared host cap, not a business denial, that
    // must reject the candidate's admission before its Accepted original.
    let mut oracle = World::new();
    oracle.state = world.state.clone();
    oracle.state.time = 1;
    let envelope = world.pending_actions.front().unwrap();
    let body = oracle.action_to_event(envelope).unwrap();
    oracle.preflight_domain_event(&body).unwrap();
    oracle
        .append_legacy_action_accepted_for_test(envelope)
        .unwrap();
    oracle
        .append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)))
        .unwrap();
    assert!(
        oracle.state.meta_progress["other"]
            .achievements
            .iter()
            .any(|id| id == "tier.x.bronze")
    );
    assert_eq!("tier.x.bronze".len(), 13);
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
    let mut work = meta_work();
    if matches!(failure, Failure::GeneratedCap) {
        work.mutation.max_text_bytes = 12;
    }
    let mut audit = transfer_resource_audit_work();
    let mut callbacks = 0;
    let stop = AtomicBool::new(matches!(failure, Failure::CancelBeforeSelection));
    let mut persist =
        |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
            callbacks += 1;
            if matches!(failure, Failure::PersistSentinel) {
                return Err(crate::world::incremental_body::BodyMutationError::Original(
                    Box::new(MetaPersistSentinel),
                ));
            }
            producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
            let n=usize::try_from(stream.original_bytes()).unwrap();let _fee=memory.reserve(n+64,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
            let mut bytes=Vec::with_capacity(n);stream.write_to(&mut bytes)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&bytes),stream.original_id());Ok(())
        })
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
        match rules::try_evaluate_with_meta_progress(
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
                work: rules::NonmovementFixtureWorkV1::MetaProgress(&mut work),
                audit: &mut audit,
            },
        ) {
            Err(error) => error,
            Ok(_) => panic!("host failure unexpectedly returned a completed Meta receipt"),
        }
    };
    let body = error
        .body_error_for_fixture()
        .expect("original Root nested body cause");
    match failure {
        Failure::GeneratedCap => assert!(
            matches!(
                body,
                crate::world::incremental_body::BodyMutationError::Value(
                    ValueTreeError::ResourceLimited("meta generated identifier bytes")
                )
            ),
            "{body:?}"
        ),
        Failure::CancelBeforeSelection => assert!(
            matches!(
                body,
                crate::world::incremental_body::BodyMutationError::Value(ValueTreeError::Cancelled)
            ),
            "{body:?}"
        ),
        Failure::PersistSentinel => match body {
            crate::world::incremental_body::BodyMutationError::Original(cause) => assert!(
                cause.downcast_ref::<MetaPersistSentinel>().is_some(),
                "{cause}"
            ),
            other => panic!("Accepted persistence cause changed: {other:?}"),
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
    assert_eq!(
        (0..tx.ordered_fact_count())
            .filter_map(|i| tx.ordered_fact(i))
            .filter(|fact| fact.kind == ExecutionFactKindV1::Input)
            .count(),
        usize::from(!matches!(failure, Failure::CancelBeforeSelection))
    );
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
#[test]
fn queued_meta_generated_identifier_cap_rejects_before_any_accepted_or_result_original() {
    run_failure(Failure::GeneratedCap);
}
#[test]
fn queued_meta_cancel_before_selection_has_no_input_or_event_and_aborts() {
    run_failure(Failure::CancelBeforeSelection);
}
#[test]
fn queued_meta_accepted_persist_retains_exact_sentinel_and_has_no_event() {
    run_failure(Failure::PersistSentinel);
}
