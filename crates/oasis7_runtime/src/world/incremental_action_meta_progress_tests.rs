//! Whole native23 GEN, sole Root FIFO, real old Accepted producer and Current oracle.
use super::*;
use crate::world::incremental_action_meta_progress::MetaProgressWorkV1;
use crate::world::incremental_body::BodyMutationContext;
fn meta_work() -> MetaProgressWorkV1 {
    MetaProgressWorkV1 {
        steps_remaining: 1_000_000,
        bytes_remaining: 64 << 20,
        pending_entries_remaining: 100_000,
        native_scan: genesis::scan(),
        native_read: oasis7_world_state::incremental_world_native::WorldNativeSelectedReadWorkV1 {
            json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 1_000_000,
                json_nodes_remaining: 1_000_000,
                max_depth: 64,
                max_payload_bytes: 16 << 20,
            },
            max_retained_trees: 4096,
            max_tree_heap: 16 << 20,
        },
        native_write: oasis7_world_state::incremental_world_state_genesis::TypedRecordWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
            json: oasis7_world_state::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 1_000_000,
                json_nodes_remaining: 1_000_000,
                max_json_depth: 64,
                max_pending_node_bytes: 16 << 20,
            },
        },
        records_remaining: 100_000,
        mutation: oasis7_world_state::incremental_world_native::MetaProgressMutationWorkV1 {
            steps_remaining: 1_000_000,
            comparison_bytes_remaining: 64 << 20,
            max_entries: 1024,
            max_text_bytes: 4096,
        },
    }
}
#[test]
fn queued_meta_progress_preserves_legacy_accepted_originals_current_state_and_mailboxes() {
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
    let original = oasis7_world_state::MetaProgressState {
        agent_id: "embedded-other".into(),
        track_points: std::collections::BTreeMap::from([
            ("science".into(), 19),
            ("untouched".into(), i64::MIN),
        ]),
        total_points: 7,
        achievements: vec!["z".into(), "a".into(), "a".into()],
        unlocked_tiers: std::collections::BTreeMap::from([(
            "untouched".into(),
            vec!["bronze".into()],
        )]),
        last_granted_at: 0,
    };
    world
        .state
        .meta_progress
        .insert("other".into(), original.clone());
    world
        .state
        .meta_progress
        .insert("unrelated".into(), original);
    world.state.materials.insert("ore".into(), 7);
    world
        .state
        .material_ledgers
        .entry(oasis7_world_state::MaterialLedgerId::World)
        .or_default()
        .insert("ore".into(), 10);
    for (op, target, track, points, achievement) in [
        ("missing", "absent", "", 0, None),
        ("owner", "absent", "", 0, None),
        ("owner", "other", " ", 0, None),
        ("owner", "other", "science", 0, Some("")),
        ("owner", "other", "science", 1, Some(" ")),
        ("owner", "other", " science ", 2, Some(" a ")),
        ("owner", "other", "science", 29, None),
        ("owner", "other", "science", 50, Some("a")),
        ("owner", "other", "science", -i64::MAX, None),
        ("owner", "other", "science", i64::MAX, None),
        ("owner", "other", "saturation", i64::MAX, Some("a")),
        ("owner", "other", "saturation", i64::MAX, None),
        ("owner", "owner", "self", -5, None),
        ("owner", "owner", "self", i64::MIN, None),
    ] {
        world.submit_action(crate::Action::GrantMetaProgress {
            operator_agent_id: op.into(),
            target_agent_id: target.into(),
            track: track.into(),
            points,
            achievement_id: achievement.map(str::to_owned),
        });
    }
    let expected_inputs: Vec<_> = world
        .pending_actions
        .iter()
        .map(|e| *blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(e).unwrap()).as_bytes())
        .collect();
    // Keep the raw initial dual-material representation: new_with_state would
    // migrate it before the real old Accepted producer is exercised.
    let mut oracle = World::new();
    oracle.state = world.state.clone();
    oracle.state.time = 1;
    let mut expected = Vec::new();
    let mut states = Vec::new();
    for envelope in &world.pending_actions {
        let before = oracle.journal.events.len();
        let body = oracle.action_to_event(envelope).unwrap();
        oracle.preflight_domain_event(&body).unwrap();
        oracle
            .append_legacy_action_accepted_for_test(envelope)
            .unwrap();
        oracle
            .append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        let events = &oracle.journal.events[before..];
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0].body,
            crate::WorldEventBody::Domain(crate::DomainEvent::ActionAccepted { .. })
        ));
        expected.push(
            events
                .iter()
                .map(|e| oasis7_wasm_abi::encode_canonical_cbor(e).unwrap())
                .collect::<Vec<_>>(),
        );
        states.push((
            oracle.state.meta_progress.clone(),
            oracle.state.material_ledgers.clone(),
            oracle.state.materials.clone(),
        ));
    }
    let final_agents=["owner","other"].map(|id| {
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
    let mut work = meta_work();
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
        for (events, (progress, ledgers, materials)) in expected.iter().zip(&states) {
            let mut persist =
                |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    callbacks += 1;
                    producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
                    let n=usize::try_from(stream.original_bytes()).unwrap();let _fee=memory.reserve(n+64,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;let mut bytes=Vec::with_capacity(n);stream.write_to(&mut bytes)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&bytes),stream.original_id());Ok(())
                })
                };
            let now = step.time();
            let (_, result) = rules::evaluate_with_meta_progress(
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
                    work: rules::NonmovementFixtureWorkV1::MetaProgress(&mut work),
                    audit: &mut audit,
                },
            );
            result.validate(step.transaction).unwrap();
            assert_eq!(
                oasis7_wasm_abi::encode_canonical_cbor(result.event()).unwrap(),
                events[1]
            );
            for bytes in events {
                assert_eq!(
                    fixture.objects.original(blake3::hash(bytes).as_bytes()),
                    *bytes
                );
            }
            for id in ["owner", "other", "unrelated"] {
                let guard = step.transaction.guard();
                let actual =
                    oasis7_world_state::incremental_world_native::recover_current_meta_progress_v1(
                        id,
                        fixture.current.native().native(),
                        &fixture.roots,
                        step.transaction,
                        &mut read,
                        &mut work.native_scan,
                        &mut work.native_read,
                        &guard,
                        &live,
                    )
                    .unwrap();
                assert_eq!(actual.value(), progress.get(id), "Current meta outer={id}");
            }
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
            for (ledger, balances) in ledgers {
                for (kind, amount) in balances {
                    assert_eq!(
                        overlay.ledger(ledger, kind).unwrap(),
                        oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::Value(
                            *amount
                        )
                    );
                }
            }
            for (kind, amount) in materials {
                assert_eq!(overlay.material(kind).unwrap(), Some(*amount));
            }
        }
        assert_eq!(step.journal_height().unwrap(), (expected.len() * 2) as u64);
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
    assert_eq!(callbacks, expected.len() * 2);
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
        .flatten()
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

#[path = "incremental_action_meta_progress_failure_tests.rs"]
mod failure;
