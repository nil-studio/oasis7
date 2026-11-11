//! Genuine Root pending FIFO lifecycle execution; independent legacy state and original oracle.
use super::*;
use crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1;
use crate::world::incremental_body::BodyMutationContext;
fn lifecycle_work() -> FactoryLifecycleWorkV1 {
    FactoryLifecycleWorkV1 {
        steps_remaining: 1_000_000,
        bytes_remaining: 64 << 20,
        pending_entries_remaining: 100_000,
        max_scalar_bytes: 64 << 10,
        max_material_entries: 1024,
        factory: oasis7_world_state::incremental_factory_records::FactoryWorkBudgetV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
            scan: genesis::scan(),
        },
        industry: oasis7_world_state::incremental_industry_jobs::IndustryWorkBudgetV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
            scan: genesis::scan(),
        },
        lifecycle:
            oasis7_world_state::incremental_factory_lifecycle_index::FactoryLifecycleIndexWorkV1 {
                cells_remaining: 1_000_000,
                comparison_bytes_remaining: 64 << 20,
                pending_entries_remaining: 100_000,
                max_factories: 16,
                max_recipe_jobs: 16,
                max_text_bytes: 4096,
                max_pending_records: 128,
            },
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
        output_group: oasis7_world_state::incremental_inventory::FactoryOutputLedgerGroupWorkV1 {
            pending_entries_remaining: 100_000,
            max_ledger_bytes: 4096,
        },
    }
}
#[test]
fn queued_factory_lifecycle_matches_legacy_originals_and_prior_current_writes() {
    use oasis7_agent_api::world_action::{GeoPos, ResourceKind};
    let mut world = World::new();
    for id in ["owner", "other"] {
        let mut agent = oasis7_world_state::agent_state::AgentState::new(id, GeoPos::new(0, 0, 0));
        agent.resources.set(ResourceKind::Electricity, 10).unwrap();
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(agent, 0));
    }
    let source = industrial_eligibility::producing_world();
    let mut factory = source.state.factories["assembler"].clone();
    drop(source);
    factory.durability_ppm = 974_999;
    factory.output_ledger = oasis7_world_state::MaterialLedgerId::factory("salvage");
    let mut empty = factory.clone();
    empty.factory_id = "empty".into();
    empty.durability_ppm = 0;
    empty.spec.build_cost.clear();
    empty.output_ledger = oasis7_world_state::MaterialLedgerId::factory("empty");
    world.state.factories.insert("empty".into(), empty);
    world.state.factories.insert("assembler".into(), factory);
    world.state.materials.insert("hardware_part".into(), 100);
    world
        .state
        .material_ledgers
        .entry(oasis7_world_state::MaterialLedgerId::World)
        .or_default()
        .insert("hardware_part".into(), 100);
    for action in [
        crate::Action::MaintainFactory {
            operator_agent_id: "owner".into(),
            factory_id: "assembler".into(),
            parts: 1,
        },
        crate::Action::MaintainFactory {
            operator_agent_id: "owner".into(),
            factory_id: "assembler".into(),
            parts: i64::MAX,
        },
        crate::Action::MaintainFactory {
            operator_agent_id: "owner".into(),
            factory_id: "assembler".into(),
            parts: 1,
        },
        crate::Action::RecycleFactory {
            operator_agent_id: "owner".into(),
            factory_id: "assembler".into(),
        },
        crate::Action::RecycleFactory {
            operator_agent_id: "owner".into(),
            factory_id: "assembler".into(),
        },
        crate::Action::RecycleFactory {
            operator_agent_id: "owner".into(),
            factory_id: "empty".into(),
        },
    ] {
        world.submit_action(action);
    }
    assert_factory_fifo(world);
}

pub(super) fn assert_factory_fifo(world: World) {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let expected_inputs: Vec<_> = world
        .pending_actions
        .iter()
        .map(|e| *blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(e).unwrap()).as_bytes())
        .collect();
    let mut oracle = World::new_with_state(world.state.clone());
    oracle.state.time = 1;
    let mut expected = Vec::new();
    let mut states = Vec::new();
    let mut economy = Vec::new();
    for envelope in &world.pending_actions {
        let body = oracle.action_to_event(envelope).unwrap();
        oracle
            .append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        expected.push(
            oasis7_wasm_abi::encode_canonical_cbor(oracle.journal.events.last().unwrap()).unwrap(),
        );
        states.push(
            oracle
                .state
                .factories
                .get("assembler")
                .map(|f| f.durability_ppm),
        );
        economy.push((
            oracle.state.material_ledgers.clone(),
            oracle
                .state
                .factory_recycle_receipts
                .get("assembler")
                .cloned(),
            oracle.state.industry_progress.clone(),
            oracle.state.retired_factory_ids.contains("assembler"),
        ));
    }
    let final_agents = ["owner", "other"].map(|id| {
        let mailbox = oracle
            .journal
            .events
            .iter()
            .filter(|event| match &event.body {
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::FactoryMaintained {
                        operator_agent_id, ..
                    }
                    | crate::DomainEvent::FactoryRecycled {
                        operator_agent_id, ..
                    },
                ) => operator_agent_id == id,
                _ => false,
            })
            .map(|event| {
                *blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(event).unwrap()).as_bytes()
            })
            .collect::<Vec<_>>();
        (
            id,
            oracle.state.agents[id].last_active,
            oracle.state.agents[id]
                .state
                .resources
                .get(oasis7_agent_api::world_action::ResourceKind::Electricity),
            mailbox,
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
    let mut work = lifecycle_work();
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
        for ((original, states), (ledgers, receipt, progress, retired)) in
            expected.iter().zip(&states).zip(&economy)
        {
            let mut persist =
                |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    callbacks += 1;
                    producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
                    let n=usize::try_from(stream.original_bytes()).unwrap();let _fee=memory.reserve(n+64,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                    let mut b=Vec::with_capacity(n);stream.write_to(&mut b)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&b),stream.original_id());Ok(())})
                };
            let now = step.time();
            let (_, result) = rules::evaluate_with_factory_lifecycle(
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
                    work: rules::NonmovementFixtureWorkV1::FactoryLifecycle(&mut work),
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
            let actual = oasis7_world_state::incremental_factory_records::read_factory_cell_overlay_v1(
                oasis7_world_state::incremental_factory_records::FactoryRecordKeyV1::Factory("assembler"), oasis7_world_state::incremental_factory_records::FactoryCellV1::Column(
                    oasis7_world_state::incremental_factory_records::FactoryColumnV1::FactoryDurabilityPpm),
                &mut overlay, 4096, &mut std::io::sink()).unwrap();
            let expected_cell = states.map_or(
                oasis7_world_state::incremental_factory_records::FactoryCellReadV1::RecordAbsent,
                |n| {
                    oasis7_world_state::incremental_factory_records::FactoryCellReadV1::Value(
                        oasis7_world_state::incremental_factory_records::FactoryValueV1::Signed(n),
                    )
                },
            );
            assert_eq!(actual, expected_cell);
            use oasis7_world_state::incremental_inventory_read::LedgerQuantityV1;
            let oracle_ledgers = [
                oasis7_world_state::MaterialLedgerId::World,
                oasis7_world_state::MaterialLedgerId::factory("salvage"),
                oasis7_world_state::MaterialLedgerId::factory("empty"),
            ]
            .into_iter()
            .chain(ledgers.keys().cloned())
            .collect::<std::collections::BTreeSet<_>>();
            for ledger in oracle_ledgers {
                // Include an absent kind to distinguish a created empty group
                // from a missing group, in addition to every actual member.
                let kinds = ledgers
                    .get(&ledger)
                    .into_iter()
                    .flat_map(|m| m.keys().map(String::as_str))
                    .chain(std::iter::once("not-a-material"));
                for kind in kinds {
                    let expected = match ledgers.get(&ledger) {
                        None => LedgerQuantityV1::LedgerAbsent,
                        Some(group) => group
                            .get(kind)
                            .map_or(LedgerQuantityV1::MaterialAbsent, |n| {
                                LedgerQuantityV1::Value(*n)
                            }),
                    };
                    assert_eq!(
                        overlay.ledger(&ledger, kind).unwrap(),
                        expected,
                        "{ledger:?}/{kind}"
                    );
                }
            }
            assert_eq!(oasis7_world_state::incremental_world_id_sets::world_id_set_contains_overlay_v1(
                oasis7_world_state::incremental_world_id_sets::WorldIdSetFieldV1::RetiredFactories,
                oasis7_world_state::incremental_world_id_sets::WorldIdSetKeyRefV1::Text("assembler"), &mut overlay).unwrap(),*retired);
            drop(overlay);
            read.root = fixture.roots.state_root(StateNamespaceV1::Authorization);
            let guard = step.transaction.guard();
            let actual_receipt=oasis7_world_state::incremental_world_native::recover_current_factory_recycle_receipt_v1(
                "assembler",fixture.current.native().native(),&fixture.roots,step.transaction,&mut read,&mut work.native_scan,&mut work.native_read,&guard,&live).unwrap();
            assert_eq!(actual_receipt.value(), receipt.as_ref());
            drop(actual_receipt);
            read.root = fixture.roots.state_root(StateNamespaceV1::Business);
            let guard = step.transaction.guard();
            let actual_progress=oasis7_world_state::incremental_world_native::recover_current_factory_industry_progress_v1(
                fixture.current.native().native(),&fixture.roots,step.transaction,&mut read,&mut work.native_scan,&mut work.native_read,&guard,&live).unwrap();
            assert_eq!(actual_progress.value(), Some(progress));
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
        expected.len()
    );
}

#[test]
fn lifecycle_scalar_arithmetic_has_independent_integer_oracles() {
    use crate::world::incremental_action_factory_lifecycle::math::{
        maintenance_repair_v1, recycle_stack_amount_v1,
    };
    // Use independently widened integer arithmetic, not the old helper (which
    // now delegates to the same production kernel).
    for current in [
        -9,
        0,
        1,
        24_999,
        974_999,
        975_000,
        999_999,
        1_000_000,
        i64::MAX,
    ] {
        for requested in [i64::MIN, -1, 0, 1, 2, 40, i64::MAX] {
            let clamped = i128::from(current.clamp(0, 1_000_000));
            let expected = if requested <= 0 || clamped == 1_000_000 {
                None
            } else {
                let delta = (i128::from(requested) * 25_000).min(1_000_000 - clamped);
                Some((((delta + 24_999) / 25_000) as i64, (clamped + delta) as i64))
            };
            assert_eq!(
                maintenance_repair_v1(current, requested)
                    .map(|x| (x.consumed_parts, x.durability_ppm)),
                expected,
                "current={current} requested={requested}"
            );
        }
    }
    for cost in [-1, 0, 1, 2, 3, 7, 101, i64::MAX] {
        for durability in [i64::MIN, -1, 0, 1, 500_000, 999_999, 1_000_000, i64::MAX] {
            let value = i128::from(cost.max(0)) * 7 * i128::from(durability.clamp(0, 1_000_000))
                / 10_000_000;
            let expected = (value > 0).then_some(value as i64);
            assert_eq!(
                recycle_stack_amount_v1(cost, durability),
                expected,
                "cost={cost} durability={durability}"
            );
        }
    }
    // Duplicate input kinds retain the original stack boundaries and order;
    // summing before flooring would manufacture an extra unit in this example.
    let input = [("ore", 1), ("water", 3), ("ore", 1)];
    let recovered: Vec<_> = input
        .into_iter()
        .filter_map(|(kind, n)| recycle_stack_amount_v1(n, 1_000_000).map(|n| (kind, n)))
        .collect();
    assert_eq!(recovered, vec![("water", 2)]);
}

#[path = "incremental_action_factory_lifecycle_boundary_tests.rs"]
mod boundary;

#[path = "incremental_action_factory_lifecycle_failure_tests.rs"]
mod failure;
