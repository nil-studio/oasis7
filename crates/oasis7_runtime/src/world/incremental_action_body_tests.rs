//! Child of module_execution_tests; genuine pending originals and Current oracle.
use super::*;
use crate::world::incremental_body_event::materials::test_support::{
    MaterialFixtureLimitsV1, MaterialFixtureV1,
};
use crate::world::{
    incremental_action_body::*, incremental_action_execution::take_pending_action,
    incremental_body_event::read_fixed, incremental_step::IwsStepContext,
};
use oasis7_world_state::incremental_agent_body::{
    AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1,
};
#[path = "incremental_action_body_queued_case.rs"]
pub(in crate::world) mod queued_case;
use queued_case::{body_work, body_world};
#[path = "incremental_material_normalization_transfer_tests.rs"]
mod material_transfer;
#[path = "incremental_action_body_tests_rules.rs"]
pub(super) mod rules;
pub(super) fn audit_work()
-> oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
    oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
        structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
            bytes_remaining: 16 << 20,
            values_remaining: 1000000,
            max_depth: 64,
        },
        canonical: genesis::canonical(),
        max_original_bytes: 64 << 10,
        max_heap_bytes: 64 << 20,
    }
}

#[test]
fn actual_queued_body_actions_match_world_and_read_prior_current_write() {
    queued_case::run();
}

#[test]
fn selected_body_actions_reject_foreign_stale_cross_action_cancel_and_work_exhaustion() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = body_world();
    let view = world.state.agents["body-agent"].state.body_view.clone();
    world.submit_action(crate::Action::EmitBodyAttributes {
        agent_id: "body-agent".into(),
        view,
        reason: "unchanged".into(),
    });
    world.submit_action(crate::Action::QueryObservation {
        agent_id: "body-agent".into(),
    });
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
    for case in 0..5 {
        let cancelled = std::cell::Cell::new(false);
        let mut originals = Originals {
            objects: fixture.objects.clone(),
            memory: &memory,
        };
        let mut tx = ExecutionTransaction::begin(
            fixture.parent.clone(),
            [110 + case; 32],
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
            let mut ctx = WorldFieldOverlayContextV1 {
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
                &mut ctx,
                &mut rules::directory_work(),
                &live,
            )
            .unwrap()
        };
        let mut pending_work = genesis::current_work(&fixture.roots).runtime.pending;
        let guard = tx.guard();
        let mut pending=oasis7_world_state::incremental_pending_action_overlay::begin_pending_action_current_v1(fixture.current.runtime().pending(),&fixture.roots,&mut tx,&memory,&guard,&live,&mut pending_work.index,||false).unwrap();
        let mut read_io = genesis::io();
        let mut read_keys = genesis::keys();
        let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
            root: fixture.roots.state_root(StateNamespaceV1::Business),
            state: &reader,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || cancelled.get(),
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
            cancelled: || cancelled.get(),
        };
        let mut write_io = genesis::io();
        let mut sink = fixture.cas.clone();
        let mut mutation = BodyMutationContext {
            sink: &mut sink,
            memory: &memory,
            io: &mut write_io,
            cancelled: || cancelled.get(),
        };
        let mut canonical = genesis::canonical();
        let guard = tx.guard();
        {
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
            let now = step.time();
            let outcome = rules::outcome(
                &fixture,
                &reader,
                step.transaction,
                &selected,
                &directory,
                &counter,
                &mut originals,
                &memory,
                now,
                &live,
            );
            let mut work = body_work();
            let mut audits = audit_work();
            if case == 0 {
                let mut foreign = ExecutionTransaction::begin(
                    fixture.parent.clone(),
                    [125; 32],
                    &reader,
                    &memory,
                    TransactionLimits {
                        max_reads:
                            oasis7_formal_protocol::incremental_execution::EXECUTION_MAX_READS,
                        max_writes: 1024,
                        max_facts: 64,
                        max_pending_node_bytes: 16 << 20,
                    },
                )
                .unwrap();
                let fc =
                    WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &foreign, &memory)
                        .unwrap();
                let mut fio = genesis::io();
                let mut fsink = fixture.cas.clone();
                let mut fm = BodyMutationContext {
                    sink: &mut fsink,
                    memory: &memory,
                    io: &mut fio,
                    cancelled: || false,
                };
                let mut canonical = genesis::canonical();
                let fg = foreign.guard();
                {
                    let mut foreign_step = IwsStepContext::begin_counted(
                        &mut foreign,
                        0,
                        &fg,
                        &live,
                        &reader,
                        &mut fm,
                        &mut canonical,
                        &fc,
                    )
                    .unwrap();
                    let fg = foreign_step.transaction.guard();
                    assert!(
                        execute_selected_body_action_v1(
                            &mut foreign_step,
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
                            &fg,
                            &live,
                            |_| Err(BodyMutationError::Invalid("foreign must not publish"))
                        )
                        .is_err()
                    );
                }
                let fg = foreign.guard();
                assert!(foreign.seal(&fg, &live, fixture.roots.height + 1).is_err());
                continue;
            }
            let second = if case == 4 {
                Some(
                    take_pending_action(
                        &mut pending,
                        step.transaction,
                        &mut read,
                        &mut pending_work,
                        &mut originals,
                        &live,
                    )
                    .unwrap()
                    .unwrap(),
                )
            } else {
                None
            };
            let guard = step.transaction.guard();
            match case {
                1 => step
                    .transaction
                    .write(
                        StateNamespaceV1::Effects,
                        b"body-action-stale-guard",
                        Some([77; 32]),
                    )
                    .unwrap(),
                2 => cancelled.set(true),
                3 => work.steps_remaining = 0,
                _ => {}
            }
            let result = execute_selected_body_action_v1(
                &mut step,
                &mut materials,
                second.as_ref().unwrap_or(&selected),
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
                |_| {
                    Err(BodyMutationError::Invalid(
                        "failed body action must not publish",
                    ))
                },
            );
            assert!(result.is_err());
            match (case, result.err().unwrap()) {
                (1, BodyMutationError::Execution(ExecutionError::StaleGuard)) => {}
                (2, BodyMutationError::Value(ValueTreeError::Cancelled)) => {}
                (3, BodyMutationError::Value(ValueTreeError::ResourceLimited(_))) => {}
                (4, BodyMutationError::Invalid("rule outcome selected action")) => {}
                (_, error) => panic!("unexpected body failure {case}: {error:?}"),
            }
        }
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, fixture.roots.height + 1).is_err());
    }
}

#[path = "incremental_action_data_tests.rs"]
mod collect_data;

#[path = "incremental_action_transfer_resource_tests.rs"]
mod transfer_resource;
