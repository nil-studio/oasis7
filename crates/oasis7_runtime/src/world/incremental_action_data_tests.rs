//! Child of action-body integration tests: real selected CollectData and Current oracle.
use super::*;
use crate::world::incremental_body_event::materials::test_support::{
    MaterialFixtureLimitsV1, MaterialFixtureV1,
};
use crate::world::{
    World,
    incremental_action_data::{CollectDataWorkV1, execute_selected_collect_data_v1},
    incremental_action_execution::take_pending_action,
    incremental_body::BodyMutationError,
    incremental_body_event::read_fixed,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionTransaction, TransactionLimits,
    },
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_world_state::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1, AgentSubtreeValueV1,
        read_agent_subtree_scalar_overlay_v1,
    },
    incremental_event_fields::{
        MAILBOX_EVENT_REFERENCE_BYTES, MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1,
        mailbox_event_id_v1,
    },
};

fn collect_data_work() -> CollectDataWorkV1 {
    CollectDataWorkV1 {
        steps_remaining: 10_000,
        bytes_remaining: 16 << 20,
        pending_entries_remaining: 100_000,
        max_scalar_bytes: 1024,
    }
}

fn collect_data_audit_work()
-> oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
    oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
        structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
            bytes_remaining: 16 << 20,
            values_remaining: 1_000_000,
            max_depth: 64,
        },
        canonical: genesis::canonical(),
        max_original_bytes: 64 << 10,
        max_heap_bytes: 64 << 20,
    }
}

fn collect_data_world() -> World {
    use oasis7_agent_api::world_action::{GeoPos, ResourceKind};
    let mut world = World::new();
    let mut state =
        oasis7_world_state::agent_state::AgentState::new("collector", GeoPos::new(0, 0, 0));
    state.resources.set(ResourceKind::Electricity, 7).unwrap();
    state.resources.set(ResourceKind::Data, 5).unwrap();
    world
        .state
        .agents
        .insert("collector".into(), crate::AgentCell::new(state, 0));
    world
}

fn current_resource<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    tx: &mut ExecutionTransaction<'_, Reader<'a>>,
    memory: &MemoryBudget,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ordinal: u64,
) -> Option<i64> {
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut overlay = oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1 {
        transaction: tx,
        values: ValueReadContext {
            source: reader,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let guard = overlay.transaction.guard();
    match read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent: "collector",
                field: AgentFieldV1::ResourceAmount,
                ordinal,
            },
            qualification: fixture.current.raw().agents(),
        },
        &mut overlay,
        1024,
        &mut std::io::sink(),
        &guard,
        live,
    )
    .unwrap()
    {
        None => None,
        Some(AgentSubtreeValueV1::I64(value)) => Some(value),
        other => panic!("unexpected Current resource value: {other:?}"),
    }
}

#[test]
fn selected_non_auth_collect_data_matches_world_and_current_resources() {
    use oasis7_agent_api::world_action::ResourceKind;

    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = collect_data_world();
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
    for (electricity_cost, data_amount) in [(3, 4), (5, 2), (0, 2)] {
        world.submit_action(crate::Action::CollectData {
            collector_agent_id: "collector".into(),
            electricity_cost,
            data_amount,
        });
    }
    let expected_inputs: Vec<_> = world
        .pending_actions
        .iter()
        .map(|envelope| {
            *blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(envelope).unwrap()).as_bytes()
        })
        .collect();
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
    let expected_electricity = oracle.state.agents["collector"]
        .state
        .resources
        .get(ResourceKind::Electricity);
    let expected_data = oracle.state.agents["collector"]
        .state
        .resources
        .get(ResourceKind::Data);
    assert!(matches!(
        &expected[0],
        crate::WorldEventBody::Domain(crate::DomainEvent::DataCollected { .. })
    ));
    assert!(expected[1..].iter().all(|body| matches!(
        body,
        crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { .. })
    )));
    let expected_materials = oracle.state.materials.clone();
    let expected_world_materials = oracle.state.material_ledgers[&world_ledger].clone();
    assert_eq!(expected_materials, expected_world_materials);
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
    let mut actual = Vec::new();
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
        let mut work = collect_data_work();
        let mut audits = collect_data_audit_work();
        for (expected_body, expected_original) in expected.iter().zip(&expected_originals) {
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
            let mut persist =
                |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    producer.persist_to(&mut |stream: &mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1, cancelled: &mut dyn FnMut() -> bool| {
                    let bytes = usize::try_from(stream.original_bytes()).unwrap(); assert!(bytes <= 64 << 10);
                    let _fee = memory.reserve(bytes + std::mem::size_of::<Vec<u8>>(), MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                    let mut encoded = Vec::with_capacity(bytes); stream.write_to(&mut encoded)?;
                    assert!(!cancelled()); assert_eq!(fixture.objects.put(&encoded), stream.original_id()); Ok(())
                })
                };
            let now = step.time();
            let (_, result) = rules::evaluate_with_collect_data(
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
                    work: rules::NonmovementFixtureWorkV1::CollectData(&mut work),
                    audit: &mut audits,
                },
            );
            result.validate(step.transaction).unwrap();
            assert_eq!(&result.event().body, expected_body);
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
        let persisted = crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_collect_data(&actual[0]);
        persisted.validate(step.transaction).unwrap();
        assert_eq!(persisted.event(), actual[0].event());
        assert_eq!(
            current_resource(&fixture, &reader, step.transaction, &memory, &live, 0),
            Some(expected_electricity)
        );
        assert_eq!(
            current_resource(&fixture, &reader, step.transaction, &memory, &live, 1),
            Some(expected_data)
        );
        {
            let mut mirror = WorldFieldOverlayContextV1 {
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
                    mirror.material(key).unwrap(),
                    expected_materials.get(key).copied()
                );
                assert_eq!(mirror.ledger(&world_ledger,key).unwrap(),expected_world_materials.get(key).copied().map_or(oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::MaterialAbsent,oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::Value));
            }
            assert_eq!(
                mirror.ledger(&other_ledger, "untouched").unwrap(),
                oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::Value(
                    expected_other
                )
            );
        }
        let key = AgentBodyKeyV1::new("collector").unwrap();
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
        assert_eq!(current.last_active, 1);
        let frontier_key = MailboxKeyV1::new("collector", None).unwrap();
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
        assert_eq!((frontier.head, frontier.tail), (0, 1));
        let item_key = MailboxKeyV1::new("collector", Some(0)).unwrap();
        let reference = read_fixed::<MAILBOX_EVENT_REFERENCE_BYTES, _, _, _, _>(
            step.transaction,
            StateNamespaceV1::Scheduler,
            item_key.as_bytes(),
            step.values,
            step.mutation,
        )
        .unwrap();
        let event_bytes = oasis7_wasm_abi::encode_canonical_cbor(persisted.event()).unwrap();
        assert_eq!(
            mailbox_event_id_v1(&reference).unwrap(),
            *blake3::hash(&event_bytes).as_bytes()
        );
    }

    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, fixture.roots.height + 1).unwrap();
    let input_facts: Vec<_> = sealed
        .record
        .facts
        .iter()
        .filter(|fact| fact.kind == ExecutionFactKindV1::Input)
        .map(|fact| fact.id)
        .collect();
    assert_eq!(input_facts, expected_inputs);
    assert_eq!(
        sealed
            .record
            .facts
            .iter()
            .filter(|fact| fact.kind == ExecutionFactKindV1::Event)
            .count(),
        expected.len()
    );
}

#[derive(Debug, Clone, Copy)]
enum CollectDataFailureCase {
    ForeignRuleOutcome,
    DataOverflow,
    OriginalSink,
    ContinuousWork,
    CancellationDuringOriginal,
}

fn run_collect_data_failure(case: CollectDataFailureCase) {
    use oasis7_agent_api::world_action::ResourceKind;

    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = collect_data_world();
    if matches!(case, CollectDataFailureCase::DataOverflow) {
        world
            .state
            .agents
            .get_mut("collector")
            .unwrap()
            .state
            .resources
            .set(ResourceKind::Data, i64::MAX)
            .unwrap();
    }
    world.submit_action(crate::Action::CollectData {
        collector_agent_id: "collector".into(),
        electricity_cost: 3,
        data_amount: 4,
    });
    if matches!(case, CollectDataFailureCase::ForeignRuleOutcome) {
        world.submit_action(crate::Action::CollectData {
            collector_agent_id: "collector".into(),
            electricity_cost: 1,
            data_amount: 1,
        });
    }
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
        let mut work = collect_data_work();
        if matches!(case, CollectDataFailureCase::ContinuousWork) {
            // Action identity, two Current reads and two Current writes consume
            // all five steps. The next mailbox step must fail before publication.
            work.steps_remaining = 5;
            work.bytes_remaining = 4096;
        }
        let mut audits = collect_data_audit_work();
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
        let outcome = if matches!(case, CollectDataFailureCase::ForeignRuleOutcome) {
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
        let result = execute_selected_collect_data_v1(
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
                if matches!(case, CollectDataFailureCase::OriginalSink) {
                    return Err(BodyMutationError::Invalid(
                        "injected CollectData original sink failure",
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
                    if matches!(case, CollectDataFailureCase::CancellationDuringOriginal) {
                        stop.store(true, std::sync::atomic::Ordering::Release);
                    }
                    if cancelled() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    Ok(())
                })
            },
        );
        result.is_err()
    };
    assert!(failed, "expected {case:?} to reject the transition");
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}

#[test]
fn selected_collect_data_foreign_overflow_sink_cancellation_and_work_fail_closed() {
    for case in [
        CollectDataFailureCase::ForeignRuleOutcome,
        CollectDataFailureCase::DataOverflow,
        CollectDataFailureCase::OriginalSink,
        CollectDataFailureCase::ContinuousWork,
        CollectDataFailureCase::CancellationDuringOriginal,
    ] {
        run_collect_data_failure(case);
    }
}
