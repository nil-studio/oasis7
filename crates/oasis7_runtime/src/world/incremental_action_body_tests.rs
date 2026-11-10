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
fn body_world() -> World {
    let mut world = World::new();
    world.state.agents.insert(
        "body-agent".into(),
        crate::AgentCell::new(
            oasis7_world_state::agent_state::AgentState::new(
                "body-agent",
                oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
            ),
            0,
        ),
    );
    // Declare a valid fresh body before GEN. AgentState::new's zero/default
    // projection is not a valid candidate for the original body guard.
    world
        .state
        .agents
        .get_mut("body-agent")
        .unwrap()
        .state
        .body_view = crate::agent_state::BodyKernelView {
        mass_kg: 100,
        radius_cm: 10,
        thrust_limit: 100,
        cross_section_cm2: 100,
    };
    world
}
fn body_work() -> BodyActionWorkV1 {
    BodyActionWorkV1 {
        steps_remaining: 10000,
        bytes_remaining: 16 << 20,
        pending_entries_remaining: 100000,
    }
}
#[test]
fn actual_queued_body_actions_match_world_and_read_prior_current_write() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = body_world();
    // Fresh GEN input deliberately violates the legacy/World mirror invariant.
    // The old CorePolicy reducer normalizes it for every queued result below.
    let world_ledger = oasis7_world_state::MaterialLedgerId::World;
    let other_ledger = oasis7_world_state::MaterialLedgerId::Agent("other".into());
    world.state.materials = [("legacy_only".into(), 9), ("conflict".into(), 7)].into();
    world.state.material_ledgers.insert(
        world_ledger.clone(),
        [
            ("zero".into(), 0),
            ("negative".into(), -3),
            ("conflict".into(), -1),
        ]
        .into(),
    );
    world
        .state
        .material_ledgers
        .insert(other_ledger.clone(), [("untouched".into(), -7)].into());
    let initial = world.state.agents["body-agent"].state.body_view.clone();
    let mut first = initial.clone();
    first.mass_kg = initial.mass_kg * 10;
    let mut second = initial.clone();
    second.mass_kg = initial.mass_kg * 100;
    let mut invalid = second.clone();
    invalid.mass_kg = 0;
    for action in [
        crate::Action::EmitBodyAttributes {
            agent_id: "body-agent".into(),
            view: first,
            reason: "first".into(),
        },
        crate::Action::EmitBodyAttributes {
            agent_id: "body-agent".into(),
            view: second.clone(),
            reason: "second".into(),
        },
        crate::Action::EmitBodyAttributes {
            agent_id: "body-agent".into(),
            view: invalid,
            reason: "invalid".into(),
        },
        crate::Action::QueryObservation {
            agent_id: "body-agent".into(),
        },
        crate::Action::BodyAction {
            agent_id: "body-agent".into(),
            kind: "probe".into(),
            payload: serde_json::json!({}),
        },
        crate::Action::QueryObservation {
            agent_id: "missing".into(),
        },
        crate::Action::BodyAction {
            agent_id: "missing".into(),
            kind: "probe".into(),
            payload: serde_json::json!({}),
        },
        crate::Action::EmitBodyAttributes {
            agent_id: "missing".into(),
            view: initial,
            reason: "missing".into(),
        },
    ] {
        world.submit_action(action);
    }
    let expected_inputs: Vec<_> = world
        .pending_actions
        .iter()
        .map(|envelope| {
            let bytes = oasis7_wasm_abi::encode_canonical_cbor(envelope).unwrap();
            assert!(bytes.len() <= 64 << 10);
            *blake3::hash(&bytes).as_bytes()
        })
        .collect();
    // The old World business entry and publication reducer form the independent
    // oracle. Its journal is never used to construct the Current fixture.
    let mut oracle = World::new_with_state(world.state.clone());
    oracle.state.time = 1;
    let mut expected = Vec::new();
    for envelope in &world.pending_actions {
        let body = oracle.action_to_event(envelope).unwrap();
        oracle
            .append_event_for_test(body.clone(), Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        expected.push(body);
    }
    assert!(
        matches!(
            &expected[1],
            crate::WorldEventBody::Domain(crate::DomainEvent::BodyAttributesUpdated { .. })
        ),
        "second update requires the first Current body, not the parent"
    );
    assert!(matches!(
        &expected[2],
        crate::WorldEventBody::Domain(crate::DomainEvent::BodyAttributesRejected { .. })
    ));
    let expected_body = oracle.state.agents["body-agent"].state.body_view.clone();
    let expected_last_active = oracle.state.agents["body-agent"].last_active;
    assert_eq!(expected_body, second);
    let expected_materials = oracle.state.materials.clone();
    let expected_world_materials = oracle.state.material_ledgers[&world_ledger].clone();
    assert_eq!(expected_materials, expected_world_materials);
    assert_eq!(expected_materials.get("zero"), Some(&0));
    assert_eq!(expected_materials.get("negative"), Some(&-3));
    assert!(!expected_materials.contains_key("legacy_only"));
    let expected_other = oracle.state.material_ledgers[&other_ledger]["untouched"];

    let expected_originals: Vec<_> = oracle
        .journal
        .events
        .iter()
        .map(|event| oasis7_wasm_abi::encode_canonical_cbor(event).unwrap())
        .collect();
    assert_eq!(expected_originals.len(), expected.len());
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
        [101; 32],
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
        let mut directory_io = genesis::io();
        let mut directory_keys = genesis::keys();
        let mut directory_ctx = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut directory_io,
            },
            key_work: &mut directory_keys,
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
            &mut directory_ctx,
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
        cancelled: || false,
    };
    let mut write_io = genesis::io();
    let mut sink = fixture.cas.clone();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut write_io,
        cancelled: || false,
    };
    let mut canonical = genesis::canonical();
    let guard = tx.guard();
    {
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
        let mut work = body_work();
        let mut audits = audit_work();
        let mut actual = Vec::new();
        for (expected, expected_original) in expected.iter().zip(&expected_originals) {
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
            let mut persist =
                |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    producer.persist_to(&mut |stream: &mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1, cancelled: &mut dyn FnMut() -> bool| {
                    let n=usize::try_from(stream.original_bytes()).unwrap(); assert!(n<=64<<10);
                    let _fee=memory.reserve(n+std::mem::size_of::<Vec<u8>>(),MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                    let mut bytes=Vec::with_capacity(n); stream.write_to(&mut bytes)?;
                    assert!(!cancelled()); assert_eq!(fixture.objects.put(&bytes),stream.original_id()); Ok(())
                })
                };
            let (_, result) = rules::evaluate_with_body(
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
                rules::NonmovementExecutionFixtureV1 {
                    limits: material_limits,
                    persist: &mut persist,
                    work: rules::NonmovementFixtureWorkV1::Body(&mut work),
                    audit: &mut audits,
                },
            );
            result.validate(step.transaction).unwrap();
            assert_eq!(&result.event().body, expected);
            let actual_original = oasis7_wasm_abi::encode_canonical_cbor(result.event()).unwrap();
            assert_eq!(&actual_original, expected_original);
            assert_eq!(
                fixture
                    .objects
                    .original(blake3::hash(&actual_original).as_bytes()),
                actual_original
            );

            actual.push(result);
        }
        assert_eq!(step.journal_height().unwrap(), expected.len() as u64);
        let key = AgentBodyKeyV1::new("body-agent").unwrap();
        let current = AgentBodyValueV1::from_canonical_bytes(
            &read_fixed::<AGENT_BODY_VALUE_BYTES, _, _, _, _>(
                step.transaction,
                StateNamespaceV1::Business,
                key.as_bytes(),
                step.values,
                step.mutation,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(current.body_view, expected_body);
        assert_eq!(current.last_active, expected_last_active);
        use oasis7_world_state::incremental_event_fields::{
            MAILBOX_EVENT_REFERENCE_BYTES, MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1,
            mailbox_event_id_v1,
        };
        let frontier_key = MailboxKeyV1::new("body-agent", None).unwrap();
        let frontier = MailboxFrontierV1::from_canonical_bytes(
            &read_fixed::<MAILBOX_FRONTIER_BYTES, _, _, _, _>(
                step.transaction,
                StateNamespaceV1::Scheduler,
                frontier_key.as_bytes(),
                step.values,
                step.mutation,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!((frontier.head, frontier.tail), (0, 3));
        for (ordinal, result) in actual.iter().take(3).enumerate() {
            let item = MailboxKeyV1::new("body-agent", Some(ordinal as u64)).unwrap();
            let actual_reference = read_fixed::<MAILBOX_EVENT_REFERENCE_BYTES, _, _, _, _>(
                step.transaction,
                StateNamespaceV1::Scheduler,
                item.as_bytes(),
                step.values,
                step.mutation,
            )
            .unwrap();
            let bytes = oasis7_wasm_abi::encode_canonical_cbor(result.event()).unwrap();
            assert!(bytes.len() <= 64 << 10);
            assert_eq!(
                mailbox_event_id_v1(&actual_reference).unwrap(),
                *blake3::hash(&bytes).as_bytes()
            );
        }

        // This assertion must fail until the actual event sink contributes
        // CorePolicy normalization; an event-only or body-only reducer is not enough.
        {
            let mut materials = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: &reader,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            for key in ["legacy_only", "zero", "negative", "conflict"] {
                assert_eq!(
                    materials.material(key).unwrap(),
                    expected_materials.get(key).copied(),
                    "actual CorePolicy legacy mirror for {key}"
                );
                assert_eq!(materials.ledger(&world_ledger,key).unwrap(),expected_world_materials.get(key).copied().map_or(oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::MaterialAbsent,oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::Value),"actual CorePolicy World ledger for {key}");
            }
            assert_eq!(
                materials.ledger(&other_ledger, "untouched").unwrap(),
                oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::Value(
                    expected_other
                )
            );
        }

        assert!(
            oasis7_world_state::incremental_pending_action_overlay::peek_pending_action_v1(
                &pending,
                step.transaction,
                &memory,
                &live,
                &mut pending_work.index,
                || false
            )
            .unwrap()
            .is_none()
        );
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, fixture.roots.height + 1).unwrap();
    let inputs: Vec<_> = sealed
        .record
        .facts
        .iter()
        .filter(|fact| {
            fact.kind == oasis7_formal_protocol::incremental_execution::ExecutionFactKindV1::Input
        })
        .map(|fact| fact.id)
        .collect();
    assert_eq!(inputs, expected_inputs);
    assert_eq!(
        sealed
            .record
            .facts
            .iter()
            .filter(|fact| fact.kind
                == oasis7_formal_protocol::incremental_execution::ExecutionFactKindV1::Event)
            .count(),
        expected.len()
    );
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
