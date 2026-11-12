//! Genuine FIFO Open -> old Accept -> Settle, independently checked by World.
use super::*;
use crate::world::incremental_body::BodyMutationContext;
#[test]
fn queued_war_contract_lifecycle_preserves_current_finances_limits_cooldown_and_originals() {
    use oasis7_agent_api::world_action::{GeoPos, ResourceKind};
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    for id in ["a", "b", "c", "d"] {
        let mut agent = oasis7_world_state::agent_state::AgentState::new(id, GeoPos::new(0, 0, 0));
        agent
            .resources
            .set(ResourceKind::Electricity, 10000)
            .unwrap();
        agent.resources.set(ResourceKind::Data, 10000).unwrap();
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(agent, 0));
    }
    world.state.world_rules = Some(oasis7_world_state::WorldRulesStateV1::default());
    world.state.gameplay_policy.max_open_contracts_per_agent = 1;
    world.state.gameplay_policy.electricity_tax_bps = 1000;
    world.state.gameplay_policy.power_trade_fee_bps = 200;
    for (id, members) in [
        ("aggressor", vec!["a".into(), "b".into()]),
        ("defender", vec!["c".into(), "d".into()]),
    ] {
        world.state.alliances.insert(
            id.into(),
            oasis7_world_state::AllianceState {
                alliance_id: format!("embedded-{id}"),
                members,
                charter: " original charter ".into(),
                formed_by_agent_id: "a".into(),
                formed_at: 0,
            },
        );
    }
    let open = |creator: &str, id: &str| crate::Action::OpenEconomicContract {
        creator_agent_id: creator.into(),
        contract_id: id.into(),
        counterparty_agent_id: "b".into(),
        fulfillment_kind: oasis7_world_state::EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Electricity,
        settlement_amount: 20,
        reputation_stake: 3,
        expires_at: 100,
        description: " actual contract ".into(),
    };
    let actions = [
        crate::Action::DeclareWar {
            initiator_agent_id: "a".into(),
            war_id: " war-one ".into(),
            aggressor_alliance_id: " aggressor ".into(),
            defender_alliance_id: " defender ".into(),
            objective: " actual objective ".into(),
            intensity: 1,
        },
        crate::Action::DeclareWar {
            initiator_agent_id: "a".into(),
            war_id: "war-two".into(),
            aggressor_alliance_id: "aggressor".into(),
            defender_alliance_id: "defender".into(),
            objective: "active conflict".into(),
            intensity: 1,
        },
        open("missing", "invalid"),
        open("a", "one"),
        crate::Action::AcceptEconomicContract {
            accepter_agent_id: "b".into(),
            contract_id: "one".into(),
        },
        crate::Action::SettleEconomicContract {
            operator_agent_id: "a".into(),
            contract_id: "one".into(),
            success: true,
            notes: " paid settlement ".into(),
        },
        open("a", "two"),
        crate::Action::AcceptEconomicContract {
            accepter_agent_id: "b".into(),
            contract_id: "two".into(),
        },
        crate::Action::SettleEconomicContract {
            operator_agent_id: "a".into(),
            contract_id: "two".into(),
            success: true,
            notes: " pair cooldown ".into(),
        },
        open("a", "three"),
    ];
    let kinds: Vec<_> = actions
        .iter()
        .map(|a| match a {
            crate::Action::DeclareWar { .. } => 0u8,
            crate::Action::AcceptEconomicContract { .. } => 1,
            _ => 2,
        })
        .collect();
    for action in actions {
        world.submit_action(action);
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
        assert!(World::should_emit_action_accepted(&envelope.action));
        let body = oracle.action_to_event(envelope).unwrap();
        oracle.preflight_domain_event(&body).unwrap();
        oracle
            .append_legacy_action_accepted_for_test(envelope)
            .unwrap();
        expected.push(
            oasis7_wasm_abi::encode_canonical_cbor(oracle.journal.events.last().unwrap()).unwrap(),
        );
        oracle
            .append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        expected.push(
            oasis7_wasm_abi::encode_canonical_cbor(oracle.journal.events.last().unwrap()).unwrap(),
        );
        states.push(oracle.state.economic_contracts.clone());
    }
    assert_eq!(oracle.state.economic_contracts.len(), 2);
    assert_eq!(
        oracle.state.economic_contracts["one"].status,
        oasis7_world_state::EconomicContractStatus::Settled
    );
    assert_eq!(
        oracle.state.economic_contracts["two"].status,
        oasis7_world_state::EconomicContractStatus::Accepted
    );
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
    let (orders_source, accepted_orders, current_orders) = {
        use crate::world::incremental_factory_directory_orders::*;
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
        let mut scan = genesis::scan();
        let mut typed = alliance_typed_work();
        let guard = overlay.transaction.guard();
        let source=oasis7_world_state::incremental_world_rule_composite_family::bind_current_agent_orders_v1(fixture.current.native().rules().orders(),&fixture.roots,&reader,&mut overlay,&mut scan,&mut typed,&guard,&live).unwrap();
        let mut work=FactoryOrderDirectoryWorkV1{ids:oasis7_world_state::incremental_world_rule_composite_family::AgentOrderDirectoryWorkV1{max_ids:8,max_id_bytes:4096,comparison_bytes_remaining:1<<24},pending_entries_remaining:100000,merge_comparison_bytes_remaining:1<<24,max_index_text_bytes:4096};
        let guard = overlay.transaction.guard();
        let accepted = build_accepted_factory_order_directory_v1(
            &fixture.runtime,
            &fixture.head,
            &source,
            &reader,
            &mut overlay,
            &mut scan,
            &mut typed,
            &mut work,
            &guard,
            &live,
        )
        .unwrap();
        (source, accepted, work)
    };
    let current_orders = {
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
        let guard = overlay.transaction.guard();
        let mut work = current_orders;
        crate::world::incremental_factory_directory_orders::open_current_factory_order_directory_v1(
            &accepted_orders,
            &orders_source,
            &reader,
            &mut overlay,
            &mut genesis::scan(),
            &mut alliance_typed_work(),
            &mut work,
            &guard,
            &live,
        )
        .unwrap()
    };
    let sources = crate::world::incremental_normal_action_diplomacy::AllianceSourcesV1 {
        agents: fixture.current.raw().agents(),
        diplomacy: fixture.current.native().diplomacy(),
        continuity: fixture.current.native().rules().continuity(),
        organizations: fixture.current.native().rules().organization(),
        native: fixture.current.native().native(),
        orders: &current_orders,
        roots: &fixture.roots,
    };
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
    let shared = || {
        crate::world::incremental_normal_action_diplomacy::AllianceWorkV1{pending_entries_remaining:1000000,index:oasis7_world_state::incremental_diplomacy::DiplomacyIndexWorkV1{cells_remaining:1000000,comparison_bytes_remaining:1<<30,max_rows:64,max_members:128,max_text_bytes:4096,max_pending_rows:64,pending_entries_remaining:1000000},scope_scan:genesis::scan(),scope_cells:1000000,max_text_bytes:4096,max_pending_node_bytes:16<<20,write_cells:1000000,continuity_read:alliance_typed_work(),continuity_write:oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeWriteWorkV1{cells_remaining:1000000,max_pending_node_bytes:16<<20,json:oasis7_world_state::incremental_json_records::JsonWriteWorkV1{cells_remaining:1000000,json_nodes_remaining:1000000,max_json_depth:64,max_pending_node_bytes:16<<20}},controller:crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1{read:crate::world::incremental_industrial_eligibility::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1{cells_remaining:1000000,comparison_bytes_remaining:1<<30,max_text_bytes:4096},typed:alliance_typed_work(),scan:genesis::scan(),revocation:alliance_typed_work(),pending_entries_remaining:1000000}}
    };
    let mut war_work = shared();

    let contract_sources =
        crate::world::incremental_normal_action_diplomacy::contracts::ContractSourcesV1 {
            shared: &sources,
            artifacts: fixture.current.runtime().resources(),
        };
    let scalar = || crate::world::incremental_action_contract_accept::ContractAcceptWorkV1 {
        steps_remaining: 1000000,
        bytes_remaining: 1 << 30,
        pending_entries_remaining: 1000000,
        contract_scan: genesis::scan(),
        cells_remaining: 1000000,
        max_text_bytes: 4096,
        max_pending_node_bytes: 16 << 20,
    };
    let mut accept_work = scalar();
    let mut work=crate::world::incremental_normal_action_diplomacy::contracts::ContractWorkV1{shared:shared(),scalar:scalar(),policy_scan:genesis::scan(),policy:alliance_typed_work(),binding:crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1{read:crate::world::incremental_industrial_eligibility::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1{cells_remaining:1000000,comparison_bytes_remaining:1<<30,max_text_bytes:4096},typed:alliance_typed_work(),scan:genesis::scan(),revocation:alliance_typed_work(),pending_entries_remaining:1000000}};
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
        for ((pair, kind), expected_contracts) in expected.chunks_exact(2).zip(&kinds).zip(&states)
        {
            let mut persist =
                |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    callbacks += 1;
                    producer.persist_to(&mut|stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool|{let n=usize::try_from(stream.original_bytes()).unwrap();let _fee=memory.reserve(n+64,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;let mut bytes=Vec::with_capacity(n);stream.write_to(&mut bytes)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&bytes),stream.original_id());Ok(())})
                };
            let now = step.time();
            let actual = if *kind == 0 {
                let mut completed = None;
                let outcome = rules::evaluate_with_alliance(
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
                        work: rules::NonmovementFixtureWorkV1::Alliance {
                            work: &mut war_work,
                            sources: &sources,
                            common: &common,
                            result: &mut completed,
                        },
                        audit: &mut audit,
                    },
                );
                assert_eq!(outcome.decision.verdict, crate::RuleVerdict::Allow);
                let completed = completed.expect("real DeclareWar FIFO completion");
                completed.validate(step.transaction).unwrap();
                oasis7_wasm_abi::encode_canonical_cbor(completed.event()).unwrap()
            } else if *kind == 1 {
                let (outcome, completed) = rules::evaluate_with_contract_accept(
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
                        work: rules::NonmovementFixtureWorkV1::ContractAccept(&mut accept_work),
                        audit: &mut audit,
                    },
                );
                assert_eq!(outcome.decision.verdict, crate::RuleVerdict::Allow);
                completed.validate(step.transaction).unwrap();
                oasis7_wasm_abi::encode_canonical_cbor(completed.event()).unwrap()
            } else {
                let mut completed = None;
                let outcome = rules::evaluate_with_alliance(
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
                        work: rules::NonmovementFixtureWorkV1::ContractLifecycle {
                            work: &mut work,
                            sources: &contract_sources,
                            common: &common,
                            result: &mut completed,
                        },
                        audit: &mut audit,
                    },
                );
                assert_eq!(outcome.decision.verdict, crate::RuleVerdict::Allow);
                let completed = completed.expect("actual Current Contract completion");
                completed.validate(step.transaction).unwrap();
                oasis7_wasm_abi::encode_canonical_cbor(completed.event()).unwrap()
            };
            assert_eq!(actual, pair[1]);
            for bytes in pair {
                assert_eq!(
                    fixture.objects.original(blake3::hash(bytes).as_bytes()),
                    *bytes
                );
            }
            let mut io = genesis::io();
            let mut keys = genesis::keys();
            let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                root: fixture.roots.state_root(StateNamespaceV1::Business),
                state: &reader,
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
            for id in ["one", "two", "three"] {
                let guard = step.transaction.guard();
                let proof=oasis7_world_state::incremental_diplomacy::open_current_diplomacy_contract_record_v1(id,sources.diplomacy,&fixture.roots,step.transaction,&mut read,&mut genesis::scan(),&mut 1000000,1<<20,&guard,&live).unwrap();
                match (expected_contracts.get(id), proof) {
                    (None, None) => {}
                    (Some(expected), Some(proof)) => {
                        let actual=crate::world::incremental_action_contract_accept::read_current_contract_body_with_work_v1(id,&proof,&mut read,&mut accept_work,&mut |_|Ok(())).unwrap();
                        assert_eq!(&*actual, expected);
                    }
                    _ => panic!("actual Contract presence differs for {id}"),
                }
            }
        }
        // Check the actual post-settlement financial cells, not only emitted DTOs.
        let mut io = genesis::io();
        let mut keys = genesis::keys();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut *step.transaction,
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
        let guard = overlay.transaction.guard();
        let treasury=oasis7_world_state::incremental_inventory_resource_overlay::read_current_world_resource_v1(ResourceKind::Electricity,&mut overlay,&guard,&live).unwrap().unwrap_or(0);
        assert_eq!(
            treasury,
            oracle
                .state
                .resources
                .get(&ResourceKind::Electricity)
                .copied()
                .unwrap_or(0)
        );
        use oasis7_world_state::incremental_world_number_maps::*;
        for id in ["a", "b", "c", "d"] {
            for (field, expected) in [
                (
                    WorldNumberMapFieldV1::ReputationScores,
                    oracle
                        .state
                        .reputation_scores
                        .get(id)
                        .copied()
                        .map(WorldNumberV1::Signed),
                ),
                (
                    WorldNumberMapFieldV1::ReputationWindowStarted,
                    oracle
                        .state
                        .reputation_reward_window_started_at
                        .get(id)
                        .copied()
                        .map(WorldNumberV1::Unsigned),
                ),
                (
                    WorldNumberMapFieldV1::ReputationWindowAccumulated,
                    oracle
                        .state
                        .reputation_reward_window_accumulated
                        .get(id)
                        .copied()
                        .map(WorldNumberV1::Signed),
                ),
            ] {
                assert_eq!(
                    read_world_number_map_overlay_v1(
                        field,
                        WorldNumberMapKeyRefV1::Text(id),
                        &mut overlay
                    )
                    .unwrap(),
                    expected
                );
            }
            for (ordinal, kind) in [(0, ResourceKind::Electricity), (1, ResourceKind::Data)] {
                let guard = overlay.transaction.guard();
                let value=oasis7_world_state::incremental_agent_subtree::read_agent_subtree_scalar_overlay_v1(oasis7_world_state::incremental_agent_subtree::AgentSubtreeQualifiedRequestV1{cell:oasis7_world_state::incremental_agent_subtree::AgentSubtreeScalarRequestV1{agent:id,field:oasis7_world_state::incremental_agent_fields::AgentFieldV1::ResourceAmount,ordinal},qualification:sources.agents},&mut overlay,64,&mut std::io::sink(),&guard,&live).unwrap();
                assert!(
                    matches!(value,Some(oasis7_world_state::incremental_agent_subtree::AgentSubtreeValueV1::I64(n))if n==oracle.state.agents[id].state.resources.get(kind))
                );
            }
        }
        drop(overlay);
        {
            use oasis7_world_state::incremental_diplomacy::*;
            let mut io = genesis::io();
            let mut keys = genesis::keys();
            let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                root: fixture.roots.state_root(StateNamespaceV1::Business),
                state: &reader,
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
            let guard = step.transaction.guard();
            let proof = open_current_diplomacy_relation_record_v1(
                DiplomacyFieldV1::Wars,
                "war-one",
                sources.diplomacy,
                &fixture.roots,
                step.transaction,
                &mut read,
                &mut war_work.index,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            let expected = &oracle.state.wars["war-one"];
            read.root = proof.root();
            let mut get = |column| {
                let mut text = Vec::new();
                let value = read_qualified_diplomacy_scalar_v1(
                    "war-one",
                    DiplomacyFieldV1::Wars,
                    DiplomacyScalarV1::War(column),
                    &proof,
                    &mut read,
                    4096,
                    &mut text,
                )
                .unwrap();
                (value, String::from_utf8(text).unwrap())
            };
            for (column, text) in [
                (WarScalarV1::WarId, &expected.war_id),
                (WarScalarV1::InitiatorAgentId, &expected.initiator_agent_id),
                (
                    WarScalarV1::AggressorAllianceId,
                    &expected.aggressor_alliance_id,
                ),
                (
                    WarScalarV1::DefenderAllianceId,
                    &expected.defender_alliance_id,
                ),
                (WarScalarV1::Objective, &expected.objective),
            ] {
                let (value, actual) = get(column);
                assert!(matches!(value, Some(DiplomacyValueV1::Text { .. })));
                assert_eq!(&actual, text);
            }
            for (column, n) in [
                (WarScalarV1::Intensity, u64::from(expected.intensity)),
                (WarScalarV1::MaxDurationTicks, expected.max_duration_ticks),
                (WarScalarV1::DeclaredAt, expected.declared_at),
            ] {
                assert!(
                    matches!(get(column).0,Some(DiplomacyValueV1::Unsigned(actual))if actual==n)
                );
            }
            for (column, n) in [
                (
                    WarScalarV1::DeclaredMobilizationElectricityCost,
                    expected.declared_mobilization_electricity_cost,
                ),
                (
                    WarScalarV1::DeclaredMobilizationDataCost,
                    expected.declared_mobilization_data_cost,
                ),
                (WarScalarV1::AggressorScore, expected.aggressor_score),
                (WarScalarV1::DefenderScore, expected.defender_score),
            ] {
                assert!(matches!(get(column).0,Some(DiplomacyValueV1::Signed(actual))if actual==n));
            }
            assert!(
                matches!(get(WarScalarV1::Active).0,Some(DiplomacyValueV1::Bool(n))if n==expected.active)
            );
            for column in [
                WarScalarV1::ConcludedAt,
                WarScalarV1::WinnerAllianceId,
                WarScalarV1::LoserAllianceId,
                WarScalarV1::SettlementSummary,
            ] {
                assert!(get(column).0.is_none());
            }
            drop(get);
            assert_eq!(
                read_qualified_diplomacy_length_v1(
                    DiplomacyFieldV1::Wars,
                    "war-one",
                    &proof,
                    &mut read
                )
                .unwrap(),
                expected.participant_outcomes.len() as u64
            );
            read.root = fixture.roots.state_root(StateNamespaceV1::Business);
            let guard = step.transaction.guard();
            assert!(
                open_current_diplomacy_relation_record_v1(
                    DiplomacyFieldV1::Wars,
                    "war-two",
                    sources.diplomacy,
                    &fixture.roots,
                    step.transaction,
                    &mut read,
                    &mut war_work.index,
                    &guard,
                    &live
                )
                .unwrap()
                .is_none()
            );
        }
        let mut io = genesis::io();
        let mut keys = genesis::keys();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut *step.transaction,
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
        let pair = oasis7_world_state::WorldState::economic_contract_pair_key("a", "b");
        assert_eq!(
            read_world_number_map_overlay_v1(
                WorldNumberMapFieldV1::ContractPairLastSuccess,
                WorldNumberMapKeyRefV1::Text(&pair),
                &mut overlay
            )
            .unwrap(),
            oracle
                .state
                .contract_pair_last_success_settled_at
                .get(&pair)
                .copied()
                .map(WorldNumberV1::Unsigned)
        );
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
    assert_eq!(
        sealed
            .record
            .facts
            .iter()
            .filter(|f| f.kind == ExecutionFactKindV1::Event)
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|b| *blake3::hash(b).as_bytes())
            .collect::<Vec<_>>()
    );
}

fn alliance_typed_work()
-> oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeReadWorkV1 {
    oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeReadWorkV1 {
        json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 1000000,
            json_nodes_remaining: 1000000,
            max_depth: 64,
            max_payload_bytes: 1 << 20,
        },
        max_retained_trees: 8192,
        max_tree_heap: 1 << 20,
    }
}
