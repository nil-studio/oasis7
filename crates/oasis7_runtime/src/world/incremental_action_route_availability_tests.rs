//! Real Root FIFO route availability owner, independent legacy event and Current oracle.
use super::*;
use crate::world::incremental_action_route_availability::RouteAvailabilityWorkV1;
use crate::world::incremental_body::BodyMutationContext;
use oasis7_world_state::incremental_logistics_records::{
    LogisticsCellReadV1, LogisticsCellV1, LogisticsColumnV1, LogisticsRecordKeyV1,
    LogisticsRecordKindV1, LogisticsValueV1, LogisticsWorkBudgetV1, read_logistics_cell_overlay_v1,
};
fn route_work() -> RouteAvailabilityWorkV1 {
    RouteAvailabilityWorkV1 {
        steps_remaining: 10_000,
        bytes_remaining: 16 << 20,
        pending_entries_remaining: 100_000,
        max_scalar_bytes: 64 << 10,
        logistics: LogisticsWorkBudgetV1 {
            cells_remaining: 100_000,
            max_pending_node_bytes: 16 << 20,
            scan: genesis::scan(),
        },
    }
}
#[test]
fn queued_route_availability_matches_legacy_originals_and_prior_current_writes() {
    use oasis7_agent_api::world_action::{GeoPos, ResourceKind};
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    for id in ["owner", "other"] {
        let mut agent = oasis7_world_state::agent_state::AgentState::new(id, GeoPos::new(0, 0, 0));
        agent.resources.set(ResourceKind::Electricity, 10).unwrap();
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(agent, 0));
    }
    for (id, owner) in [("r1", "owner"), ("r2", "other")] {
        world.state.logistics_routes.insert(
            id.into(),
            oasis7_world_state::LogisticsRouteV1 {
                route_id: id.into(),
                from_ledger: oasis7_world_state::MaterialLedgerId::agent("owner"),
                to_ledger: oasis7_world_state::MaterialLedgerId::agent("other"),
                kind: "water".into(),
                distance_km: 2,
                priority: oasis7_agent_api::world_action::MaterialTransitPriority::Standard,
                owner_agent_id: owner.into(),
                available: true,
                capacity_units: 10,
                reserved_capacity_units: 2,
                tariff_electricity_per_unit: 3,
            },
        );
    }
    for (actor, id, available) in [
        ("missing", "absent", true),
        ("other", "r1", false),
        ("owner", "absent", false),
        ("owner", "r1", false),
        ("owner", "r1", false),
        ("owner", "r1", true),
        ("other", "r2", false),
        ("owner", "r2", true),
        ("missing", "r1", true),
    ] {
        world.submit_action(crate::Action::SetLogisticsRouteAvailability {
            requester_agent_id: actor.into(),
            route_id: id.into(),
            available,
        });
    }
    let expected_inputs: Vec<_> = world
        .pending_actions
        .iter()
        .map(|e| *blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(e).unwrap()).as_bytes())
        .collect();
    let mut oracle = World::new_with_state(world.state.clone());
    oracle.state.time = 1;
    let mut expected = Vec::new();
    let mut availability = Vec::new();
    for envelope in &world.pending_actions {
        let body = oracle.action_to_event(envelope).unwrap();
        oracle
            .append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        expected.push(
            oasis7_wasm_abi::encode_canonical_cbor(oracle.journal.events.last().unwrap()).unwrap(),
        );
        availability.push([
            oracle.state.logistics_routes["r1"].available,
            oracle.state.logistics_routes["r2"].available,
        ]);
    }
    let final_agents = ["owner", "other"].map(|id| {
        let routed: Vec<_> = oracle.journal.events.iter().filter(|event| matches!(
            &event.body, crate::WorldEventBody::Domain(crate::DomainEvent::LogisticsRouteAvailabilityChanged { requester_agent_id, .. }) if requester_agent_id == id
        )).collect();
        let mailbox = &oracle.state.agents[id].mailbox;
        assert_eq!(routed.len(), mailbox.len());
        for (event, domain) in routed.iter().zip(mailbox.iter()) {
            assert_eq!(&event.body, &crate::WorldEventBody::Domain(domain.clone()));
        }
        (
            id,
            oracle.state.agents[id].last_active,
            oracle.state.agents[id]
                .state
                .resources
                .get(ResourceKind::Electricity),
            routed.iter().map(|event| *blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(*event).unwrap()).as_bytes()).collect::<Vec<_>>(),
        )
    });
    let material_limits = MaterialFixtureLimitsV1::for_state(&world.state);
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    drop(oracle);
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
    let read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
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
    let mut sink = fixture.cas.clone();
    let mut write_io = genesis::io();
    let mut canonical = genesis::canonical();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut write_io,
        cancelled: || false,
    };
    let mut work = route_work();
    let mut audit = transfer_resource_audit_work();
    let mut callbacks = 0;
    {
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
        for (original, states) in expected.iter().zip(&availability) {
            let mut persist =
                |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    callbacks += 1;
                    producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
                    let n=usize::try_from(stream.original_bytes()).unwrap();let _fee=memory.reserve(n+64,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                    let mut b=Vec::with_capacity(n);stream.write_to(&mut b)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&b),stream.original_id());Ok(())})
                };
            let now = step.time();
            let (_, result) = rules::evaluate_with_route_availability(
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
                    work: rules::NonmovementFixtureWorkV1::RouteAvailability(&mut work),
                    audit: &mut audit,
                },
            );
            result.validate(step.transaction).unwrap();
            let bytes = oasis7_wasm_abi::encode_canonical_cbor(result.event()).unwrap();
            assert_eq!(&bytes, original);
            assert_eq!(
                fixture.objects.original(blake3::hash(&bytes).as_bytes()),
                bytes
            );
            for (id, state) in ["r1", "r2"].into_iter().zip(states) {
                let mut overlay = WorldFieldOverlayContextV1 {
                    transaction: step.transaction,
                    values: ValueReadContext {
                        source: &reader,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: 4096,
                    record_bytes: 16 << 20,
                    take_state_error: || reader.cas.error(),
                    cancelled: || false,
                };
                let actual = read_logistics_cell_overlay_v1(
                    LogisticsRecordKeyV1 {
                        kind: LogisticsRecordKindV1::Route,
                        id,
                    },
                    LogisticsCellV1::Column(LogisticsColumnV1::Available),
                    &mut overlay,
                    64,
                    &mut std::io::sink(),
                )
                .unwrap();
                assert_eq!(
                    actual,
                    LogisticsCellReadV1::Value(LogisticsValueV1::Bool(*state))
                );
            }
        }
        assert_eq!(step.journal_height().unwrap(), 9);
        for (id, last_active, electricity, mailbox) in final_agents {
            let key = oasis7_world_state::incremental_agent_body::AgentBodyKeyV1::new(id).unwrap();
            let body =
                oasis7_world_state::incremental_agent_body::AgentBodyValueV1::from_canonical_bytes(
                    &read_fixed::<
                        { oasis7_world_state::incremental_agent_body::AGENT_BODY_VALUE_BYTES },
                        _,
                        _,
                        _,
                        _,
                    >(
                        step.transaction,
                        StateNamespaceV1::Business,
                        key.as_bytes(),
                        step.values,
                        step.mutation,
                    )
                    .unwrap(),
                )
                .unwrap();
            assert_eq!(body.last_active, last_active);
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: step.transaction,
                values: ValueReadContext {
                    source: &reader,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: 4096,
                record_bytes: 16 << 20,
                take_state_error: || reader.cas.error(),
                cancelled: || false,
            };
            let guard = overlay.transaction.guard();
            let resource = read_agent_subtree_scalar_overlay_v1(
                AgentSubtreeQualifiedRequestV1 {
                    cell: AgentSubtreeScalarRequestV1 {
                        agent: id,
                        field: AgentFieldV1::ResourceAmount,
                        ordinal: 0,
                    },
                    qualification: fixture.current.raw().agents(),
                },
                &mut overlay,
                1024,
                &mut std::io::sink(),
                &guard,
                &live,
            )
            .unwrap();
            assert_eq!(resource, Some(AgentSubtreeValueV1::I64(electricity)));
            let front = MailboxKeyV1::new(id, None).unwrap();
            let frontier = MailboxFrontierV1::from_canonical_bytes(
                &read_fixed::<MAILBOX_FRONTIER_BYTES, _, _, _, _>(
                    step.transaction,
                    StateNamespaceV1::Scheduler,
                    front.as_bytes(),
                    step.values,
                    step.mutation,
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(frontier.tail - frontier.head, mailbox.len() as u64);
            for (offset, event) in mailbox.iter().enumerate() {
                let key = MailboxKeyV1::new(id, Some(frontier.head + offset as u64)).unwrap();
                let reference = read_fixed::<MAILBOX_EVENT_REFERENCE_BYTES, _, _, _, _>(
                    step.transaction,
                    StateNamespaceV1::Scheduler,
                    key.as_bytes(),
                    step.values,
                    step.mutation,
                )
                .unwrap();
                assert_eq!(mailbox_event_id_v1(&reference).unwrap(), *event);
            }
        }
    }
    assert_eq!(callbacks, 9);
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(
        sealed
            .record
            .facts
            .iter()
            .filter(|f| f.kind == ExecutionFactKindV1::Input)
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        expected_inputs
    );
    assert_eq!(
        sealed
            .record
            .facts
            .iter()
            .filter(|f| f.kind == ExecutionFactKindV1::Event)
            .count(),
        9
    );
}
