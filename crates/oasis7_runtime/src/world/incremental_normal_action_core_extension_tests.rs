//! Genuine Root FIFO, independent old World event oracle; no selected ctor.
use super::*;
use crate::world::incremental_body::BodyMutationContext;
#[test]
fn queued_normal_extensions_match_original_events_body_rows_and_mailboxes() {
    use oasis7_agent_api::world_action::{GeoPos, Observation, ObservedAgent, ResourceKind};
    use oasis7_world_state::agent_state::{CargoEntityEntry, CargoEntityKind};
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    for id in ["owner", "other", "full"] {
        let mut agent = oasis7_world_state::agent_state::AgentState::new(id, GeoPos::new(0, 0, 0));
        agent.resources.set(ResourceKind::Electricity, 10).unwrap();
        if id == "owner" {
            agent.body_state.cargo_entries = vec![
                CargoEntityEntry {
                    entity_id: "if".into(),
                    entity_kind: CargoEntityKind::Mineral,
                    quantity: 0,
                    size_per_unit: 7,
                },
                CargoEntityEntry {
                    entity_id: "if".into(),
                    entity_kind: CargoEntityKind::InterfaceModuleItem,
                    quantity: 1,
                    size_per_unit: 9,
                },
            ];
        }
        if id == "full" {
            agent.body_state.slot_capacity = 128;
        }
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(agent, 0));
    }
    world.state.materials.insert("ore".into(), 7);
    world
        .state
        .material_ledgers
        .entry(oasis7_world_state::MaterialLedgerId::World)
        .or_default()
        .insert("ore".into(), 10);
    for id in ["owner", "missing"] {
        world.submit_action(crate::Action::EmitObservation {
            observation: Observation {
                time: 99,
                agent_id: id.into(),
                pos: GeoPos::new(1, 2, 3),
                visibility_range_cm: 40,
                visible_agents: vec![ObservedAgent {
                    agent_id: "other".into(),
                    pos: GeoPos::new(4, 5, 6),
                    distance_cm: 7,
                }],
            },
        });
    }
    for (agent, item) in [
        ("owner", " "),
        ("owner", "if"),
        ("owner", "if"),
        ("full", "if"),
        ("missing", ""),
    ] {
        world.submit_action(crate::Action::ExpandBodyInterface {
            agent_id: agent.into(),
            interface_module_item_id: item.into(),
        });
    }
    let expected_inputs: Vec<_> = world
        .pending_actions
        .iter()
        .map(|e| *blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(e).unwrap()).as_bytes())
        .collect();
    let mut oracle = World::new();
    oracle.state = world.state.clone();
    oracle.state.time = 1;
    let mut expected = Vec::new();
    let mut states = Vec::new();
    for envelope in &world.pending_actions {
        assert!(!World::should_emit_action_accepted(&envelope.action));
        let body = oracle.action_to_event(envelope).unwrap();
        oracle.preflight_domain_event(&body).unwrap();
        oracle
            .append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        expected.push(
            oasis7_wasm_abi::encode_canonical_cbor(oracle.journal.events.last().unwrap()).unwrap(),
        );
        states.push(
            oracle
                .state
                .agents
                .iter()
                .map(|(id, agent)| (id.clone(), agent.state.body_state.clone()))
                .collect::<std::collections::BTreeMap<_, _>>(),
        );
    }
    let final_agents=["owner","other","full"].map(|id|{
        let events=oracle.journal.events.iter().filter(|event|matches!(&event.body,crate::WorldEventBody::Domain(domain) if domain.agent_id()==Some(id))).collect::<Vec<_>>();
        assert_eq!(events.len(),oracle.state.agents[id].mailbox.len());
        let mailbox=events.iter().map(|event|*blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(*event).unwrap()).as_bytes()).collect::<Vec<_>>();
        (id,oracle.state.agents[id].last_active,oracle.state.agents[id].state.resources.get(ResourceKind::Electricity),mailbox)
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
    let mut sink = fixture.cas.clone();
    let mut write_io = genesis::io();
    let mut canonical = genesis::canonical();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut write_io,
        cancelled: || false,
    };
    let common=oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1::new(100_000_000,4_000_000_000).unwrap();
    let mut work = crate::world::incremental_module_invocation::NormalCoreExtensionWorkV1 {
        common: &common,
        pending_entries_remaining: 100_000,
        max_body_text_bytes: 4096,
    };
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
        for (bytes, state) in expected.iter().zip(&states) {
            let mut persist =
                |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    callbacks += 1;
                    producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool|{
                    let n=usize::try_from(stream.original_bytes()).unwrap();let _fee=memory.reserve(n+64,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                    let mut output=Vec::with_capacity(n);stream.write_to(&mut output)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&output),stream.original_id());Ok(())
                })
                };
            let mut completed = None;
            let now = step.time();
            let outcome = rules::evaluate_with_normal_extension(
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
                    work: rules::NonmovementFixtureWorkV1::NormalExtension {
                        work: work.view(),
                        result: &mut completed,
                    },
                    audit: &mut audit,
                },
            );
            assert_eq!(outcome.decision.verdict, crate::RuleVerdict::Allow);
            let completed = completed.expect("actual new FIFO Completed outslot");
            completed.validate(step.transaction).unwrap();
            assert_eq!(
                oasis7_wasm_abi::encode_canonical_cbor(completed.event()).unwrap(),
                *bytes
            );
            assert_eq!(
                fixture.objects.original(blake3::hash(bytes).as_bytes()),
                *bytes
            );
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
            for (agent, body) in state {
                for (field, value) in [
                    (
                        AgentFieldV1::SlotCapacity,
                        AgentSubtreeValueV1::U16(body.slot_capacity),
                    ),
                    (
                        AgentFieldV1::ExpansionLevel,
                        AgentSubtreeValueV1::U16(body.expansion_level),
                    ),
                    (
                        AgentFieldV1::SlotCount,
                        AgentSubtreeValueV1::U64(body.slots.len() as u64),
                    ),
                    (
                        AgentFieldV1::CargoCount,
                        AgentSubtreeValueV1::U64(body.cargo_entries.len() as u64),
                    ),
                ] {
                    let guard = overlay.transaction.guard();
                    assert_eq!(
                        read_agent_subtree_scalar_overlay_v1(
                            AgentSubtreeQualifiedRequestV1 {
                                cell: AgentSubtreeScalarRequestV1 {
                                    agent,
                                    field,
                                    ordinal: 0
                                },
                                qualification: fixture.current.raw().agents()
                            },
                            &mut overlay,
                            4096,
                            &mut std::io::sink(),
                            &guard,
                            &live
                        )
                        .unwrap(),
                        Some(value)
                    );
                }
                for (ordinal, slot) in body.slots.iter().enumerate() {
                    let guard = overlay.transaction.guard();
                    let mut text = Vec::new();
                    let actual = read_agent_subtree_scalar_overlay_v1(
                        AgentSubtreeQualifiedRequestV1 {
                            cell: AgentSubtreeScalarRequestV1 {
                                agent,
                                field: AgentFieldV1::SlotId,
                                ordinal: ordinal as u64,
                            },
                            qualification: fixture.current.raw().agents(),
                        },
                        &mut overlay,
                        4096,
                        &mut text,
                        &guard,
                        &live,
                    )
                    .unwrap();
                    assert!(actual.is_some());
                    assert_eq!(String::from_utf8(text).unwrap(), slot.slot_id);
                }
            }
        }
        assert_eq!(step.journal_height().unwrap(), expected.len() as u64);
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
    assert_eq!(callbacks, expected.len());
    assert!(
        oasis7_world_state::incremental_pending_action_overlay::peek_pending_action_v1(
            &pending,
            &mut tx,
            &memory,
            &live,
            &mut pending_work.index,
            || false
        )
        .unwrap()
        .is_none()
    );
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
    let expected_events = expected
        .iter()
        .map(|bytes| *blake3::hash(bytes).as_bytes())
        .collect::<Vec<_>>();
    assert_eq!(
        sealed
            .record
            .facts
            .iter()
            .filter(|f| f.kind == ExecutionFactKindV1::Event)
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        expected_events
    );
}
