//! Child of action-body tests: genuine FIFO/Rule sources and independent old-World oracle.
//! Direct Emit fixtures exercise the business handler after admission; these
//! tests do not grant direct users access to internal Emit actions.
use super::*;
use crate::world::incremental_body_event::materials::test_support::{
    MaterialFixtureLimitsV1, MaterialFixtureV1,
};
use crate::world::{
    World,
    incremental_action_data::transfer_resource::{
        TransferResourceWorkV1, execute_selected_transfer_resource_v1,
    },
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

fn transfer_resource_work() -> TransferResourceWorkV1 {
    TransferResourceWorkV1 {
        steps_remaining: 10_000,
        bytes_remaining: 16 << 20,
        pending_entries_remaining: 100_000,
        max_scalar_bytes: 1024,
    }
}

fn transfer_resource_audit_work()
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

fn transfer_resource_world() -> World {
    use oasis7_agent_api::world_action::{GeoPos, ResourceKind};
    let mut world = World::new();
    for (id, pos) in [
        ("sender", GeoPos::new(0, 0, 0)),
        ("receiver", GeoPos::new(0, 0, 0)),
        ("far", GeoPos::new(1, 0, 0)),
    ] {
        let mut state = oasis7_world_state::agent_state::AgentState::new(id, pos);
        if id == "sender" {
            state.resources.set(ResourceKind::Electricity, 7).unwrap();
            state.resources.set(ResourceKind::Data, 5).unwrap();
        }
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(state, 0));
    }
    world
        .state
        .data_access_permissions
        .insert("sender".into(), ["receiver".into()].into());
    world
}

fn current_resource<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    tx: &mut ExecutionTransaction<'_, Reader<'a>>,
    memory: &MemoryBudget,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    agent: &str,
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
                agent,
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
fn selected_transfer_resource_matches_world_balances_mailboxes_and_native_rejections() {
    use oasis7_agent_api::world_action::ResourceKind;

    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = transfer_resource_world();
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
    use crate::Action::{EmitResourceTransfer, TransferResource};
    let actions = [
        EmitResourceTransfer {
            from_agent_id: "sender".into(),
            to_agent_id: "receiver".into(),
            kind: ResourceKind::Electricity,
            amount: 3,
        },
        EmitResourceTransfer {
            from_agent_id: "sender".into(),
            to_agent_id: "sender".into(),
            kind: ResourceKind::Electricity,
            amount: 2,
        },
        EmitResourceTransfer {
            from_agent_id: "sender".into(),
            to_agent_id: "receiver".into(),
            kind: ResourceKind::Data,
            amount: 2,
        },
        EmitResourceTransfer {
            from_agent_id: "receiver".into(),
            to_agent_id: "sender".into(),
            kind: ResourceKind::Data,
            amount: 1,
        },
        TransferResource {
            from_agent_id: "missing".into(),
            to_agent_id: "receiver".into(),
            kind: ResourceKind::Electricity,
            amount: 0,
        },
        EmitResourceTransfer {
            from_agent_id: "missing".into(),
            to_agent_id: "receiver".into(),
            kind: ResourceKind::Electricity,
            amount: 0,
        },
        EmitResourceTransfer {
            from_agent_id: "sender".into(),
            to_agent_id: "receiver".into(),
            kind: ResourceKind::Electricity,
            amount: 99,
        },
        TransferResource {
            from_agent_id: "sender".into(),
            to_agent_id: "receiver".into(),
            kind: ResourceKind::Electricity,
            amount: 1,
        },
        EmitResourceTransfer {
            from_agent_id: "sender".into(),
            to_agent_id: "missing".into(),
            kind: ResourceKind::Electricity,
            amount: 1,
        },
        EmitResourceTransfer {
            from_agent_id: "sender".into(),
            to_agent_id: "far".into(),
            kind: ResourceKind::Electricity,
            amount: 1,
        },
    ];
    for action in actions {
        world.submit_action(action);
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
    let expected_balances: [(String, i64, i64); 2] = ["sender", "receiver"].map(|id| {
        (
            id.into(),
            oracle.state.agents[id]
                .state
                .resources
                .get(ResourceKind::Electricity),
            oracle.state.agents[id]
                .state
                .resources
                .get(ResourceKind::Data),
        )
    });
    assert!(expected[..3].iter().all(|body| matches!(
        body,
        crate::WorldEventBody::Domain(crate::DomainEvent::ResourceTransferred { .. })
    )));
    assert!(expected[3..].iter().all(|body| matches!(
        body,
        crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { .. })
    )));
    let expected_mailboxes: [(String, Vec<crate::DomainEvent>); 2] =
        ["sender", "receiver"].map(|id| {
            (
                id.into(),
                oracle.state.agents[id].mailbox.iter().cloned().collect(),
            )
        });
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
        let mut work = transfer_resource_work();
        let mut audits = transfer_resource_audit_work();
        for (expected_body, expected_original) in expected.iter().zip(&expected_originals) {
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
            let (_, result) = rules::evaluate_with_transfer_resource(
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
                rules::NonmovementExecutionFixtureV1 {
                    limits: material_limits,
                    persist: &mut persist,
                    work: rules::NonmovementFixtureWorkV1::TransferResource(&mut work),
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
        for (agent, electricity, data) in &expected_balances {
            assert_eq!(
                current_resource(
                    &fixture,
                    &reader,
                    step.transaction,
                    &memory,
                    &live,
                    agent,
                    0
                ),
                (*electricity != 0).then_some(*electricity)
            );
            assert_eq!(
                current_resource(
                    &fixture,
                    &reader,
                    step.transaction,
                    &memory,
                    &live,
                    agent,
                    1
                ),
                (*data != 0).then_some(*data)
            );
        }
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
        for (agent, mailbox) in &expected_mailboxes {
            let key = AgentBodyKeyV1::new(agent).unwrap();
            let body = AgentBodyValueV1::from_canonical_bytes(
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
            assert_eq!(body.last_active, 1);
            let frontier = MailboxKeyV1::new(agent, None).unwrap();
            let frontier = MailboxFrontierV1::from_canonical_bytes(
                &read_fixed::<MAILBOX_FRONTIER_BYTES, _, _, _, _>(
                    step.transaction,
                    StateNamespaceV1::Scheduler,
                    frontier.as_bytes(),
                    step.values,
                    step.mutation,
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!((frontier.head, frontier.tail), (0, mailbox.len() as u64));
            for (index, expected_domain) in mailbox.iter().enumerate() {
                let result = actual
                    .iter()
                    .find(|result| {
                        &result.event().body
                            == &crate::WorldEventBody::Domain(expected_domain.clone())
                    })
                    .unwrap();
                let key = MailboxKeyV1::new(agent, Some(index as u64)).unwrap();
                let reference = read_fixed::<MAILBOX_EVENT_REFERENCE_BYTES, _, _, _, _>(
                    step.transaction,
                    StateNamespaceV1::Scheduler,
                    key.as_bytes(),
                    step.values,
                    step.mutation,
                )
                .unwrap();
                let bytes = oasis7_wasm_abi::encode_canonical_cbor(result.event()).unwrap();
                assert_eq!(
                    mailbox_event_id_v1(&reference).unwrap(),
                    *blake3::hash(&bytes).as_bytes()
                );
            }
        }
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

#[path = "incremental_action_transfer_resource_failure_tests.rs"]
mod failure_tests;

#[test]
fn root_empty_fifo_cancellation_is_not_normal_exhaustion() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let world = World::new();
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
    let before = tx.ordered_fact_count();
    let mut work = transfer_resource_work();
    let mut audit = transfer_resource_audit_work();
    let mut persist = |_: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| -> Result<(), BodyMutationError> { panic!("empty FIFO cannot publish an Event") };
    rules::cancelled_empty_transfer_probe(
        &fixture,
        &reader,
        &mut tx,
        &mut pending,
        &mut pending_work,
        &directory,
        &counter,
        &mut originals,
        &memory,
        &live,
        rules::NonmovementExecutionFixtureV1 {
            limits: material_limits,
            persist: &mut persist,
            work: rules::NonmovementFixtureWorkV1::TransferResource(&mut work),
            audit: &mut audit,
        },
    );
    assert_eq!(tx.ordered_fact_count(), before);
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}

#[path = "incremental_action_data_access_current_tests.rs"]
mod data_access_current;

#[path = "incremental_action_data_access_tests.rs"]
mod data_access_actions;

#[path = "incremental_collect_data_auth_signature_tests.rs"]
mod auth_signature_tests;

#[path = "incremental_starter_binding_integration_tests.rs"]
mod starter_binding_integration;

#[path = "incremental_collect_data_current_authorization_tests.rs"]
mod auth_current_tests;

#[path = "incremental_collect_data_authenticated_tests.rs"]
mod authenticated_data_tests;

#[path = "incremental_action_factory_lifecycle_tests.rs"]
mod factory_lifecycle_actions;
#[path = "incremental_action_route_availability_tests.rs"]
mod route_availability;

#[path = "incremental_diplomacy_integration_tests.rs"]
mod diplomacy_integration;

#[path = "incremental_action_meta_progress_tests.rs"]
mod meta_progress_actions;

#[path = "incremental_action_crisis_tests.rs"]
mod crisis_actions;

#[path = "incremental_action_contract_accept_tests.rs"]
mod contract_accept_actions;

#[path = "incremental_normal_action_core_extension_tests.rs"]
mod normal_extension;

#[path = "incremental_normal_action_logistics_tests.rs"]
mod logistics_registration;
