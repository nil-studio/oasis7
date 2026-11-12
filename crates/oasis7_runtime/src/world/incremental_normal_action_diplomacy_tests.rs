//! Genuine four-Alliance FIFO and independent Accepted+result World oracle.
use super::*;
use crate::world::incremental_body::BodyMutationContext;
#[test]
fn queued_alliance_lifecycle_preserves_outer_risk_war_rejections_and_exact_world_originals() {
    use oasis7_agent_api::world_action::{GeoPos, ResourceKind};
    use oasis7_world_state::MaterialLedgerId as Ledger;
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    for id in ["a", "b", "c", "w", "x"] {
        let mut agent = oasis7_world_state::agent_state::AgentState::new(id, GeoPos::new(0, 0, 0));
        agent.resources.set(ResourceKind::Electricity, 10).unwrap();
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(agent, 0));
    }
    let mut world_rules = oasis7_world_state::WorldRulesStateV1::default();
    world_rules.organization_cases.insert(
        "case-outer".into(),
        oasis7_world_state::world_rules::OrganizationCaseV1 {
            id: "case-embedded".into(),
            organization_id: "risk-org".into(),
            founder_id: "w".into(),
            members: Default::default(),
            policy_version: 1,
            revision: 0,
            phase: oasis7_world_state::world_rules::OrganizationProtectionPhaseV1::Protected,
            disposition: oasis7_world_state::world_rules::OrganizationDispositionPhaseV1::Frozen,
            notified_at: Some(0),
            protected_at: Some(0),
            grants: Default::default(),
            requests: Default::default(),
            stage_receipts: Vec::new(),
        },
    );
    world.state.world_rules = Some(world_rules);
    for id in ["war-a", "war-b"] {
        world.state.alliances.insert(
            id.into(),
            oasis7_world_state::AllianceState {
                alliance_id: format!("embedded-{id}"),
                members: vec!["w".into(), "x".into()],
                charter: "unrelated charter".into(),
                formed_by_agent_id: "w".into(),
                formed_at: 0,
            },
        );
    }
    world.state.wars.insert(
        "war-outer".into(),
        oasis7_world_state::WarState {
            war_id: "war-embedded".into(),
            initiator_agent_id: "w".into(),
            aggressor_alliance_id: "war-a".into(),
            defender_alliance_id: "war-b".into(),
            objective: "unrelated objective".into(),
            intensity: 1,
            active: true,
            declared_mobilization_electricity_cost: 0,
            declared_mobilization_data_cost: 0,
            max_duration_ticks: 8,
            aggressor_score: 0,
            defender_score: 0,
            concluded_at: None,
            winner_alliance_id: None,
            loser_alliance_id: None,
            settlement_summary: None,
            participant_outcomes: Vec::new(),
            declared_at: 0,
        },
    );
    world.state.materials.insert("ore".into(), 7);
    world
        .state
        .material_ledgers
        .entry(Ledger::World)
        .or_default()
        .insert("ore".into(), 10);
    for action in [
        crate::Action::JoinAlliance {
            operator_agent_id: "missing".into(),
            alliance_id: "risk-org".into(),
            member_agent_id: "c".into(),
        },
        crate::Action::DissolveAlliance {
            operator_agent_id: "w".into(),
            alliance_id: "war-a".into(),
            reason: "".into(),
        },
        crate::Action::FormAlliance {
            proposer_agent_id: "a".into(),
            alliance_id: " group ".into(),
            members: vec!["b".into(), "a".into(), "b".into(), " ".into()],
            charter: " charter ".into(),
        },
        crate::Action::JoinAlliance {
            operator_agent_id: "a".into(),
            alliance_id: "group".into(),
            member_agent_id: "c".into(),
        },
        crate::Action::LeaveAlliance {
            operator_agent_id: "a".into(),
            alliance_id: "group".into(),
            member_agent_id: "c".into(),
        },
        crate::Action::DissolveAlliance {
            operator_agent_id: "a".into(),
            alliance_id: "group".into(),
            reason: " finished ".into(),
        },
        crate::Action::DissolveAlliance {
            operator_agent_id: "a".into(),
            alliance_id: "group".into(),
            reason: "again".into(),
        },
    ] {
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
        states.push(oracle.state.alliances.clone());
    }
    assert_eq!(
        states.last().unwrap().len(),
        2,
        "group is deleted; unrelated alliances remain"
    );
    let final_agents=["a","b","c","w","x"].map(|id|{
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
    let mut work=crate::world::incremental_normal_action_diplomacy::AllianceWorkV1{pending_entries_remaining:1000000,index:oasis7_world_state::incremental_diplomacy::DiplomacyIndexWorkV1{cells_remaining:1000000,comparison_bytes_remaining:1<<30,max_rows:64,max_members:128,max_text_bytes:4096,max_pending_rows:64,pending_entries_remaining:1000000},scope_scan:genesis::scan(),scope_cells:1000000,max_text_bytes:4096,max_pending_node_bytes:16<<20,write_cells:1000000,continuity_read:alliance_typed_work(),continuity_write:oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeWriteWorkV1{cells_remaining:1000000,max_pending_node_bytes:16<<20,json:oasis7_world_state::incremental_json_records::JsonWriteWorkV1{cells_remaining:1000000,json_nodes_remaining:1000000,max_json_depth:64,max_pending_node_bytes:16<<20}},controller:crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1{read:crate::world::incremental_industrial_eligibility::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1{cells_remaining:1000000,comparison_bytes_remaining:1<<30,max_text_bytes:4096},typed:alliance_typed_work(),scan:genesis::scan(),revocation:alliance_typed_work(),pending_entries_remaining:1000000}};
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
        for (pair, state) in expected.chunks_exact(2).zip(&states) {
            let mut persist =
                |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    callbacks += 1;
                    producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool|{
                    let n=usize::try_from(stream.original_bytes()).unwrap();let _fee=memory.reserve(n+64,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                    let mut output=Vec::with_capacity(n);stream.write_to(&mut output)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&output),stream.original_id());Ok(())})
                };
            let mut completed = None;
            let now = step.time();
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
                        work: &mut work,
                        sources: &sources,
                        common: &common,
                        result: &mut completed,
                    },
                    audit: &mut audit,
                },
            );
            assert_eq!(outcome.decision.verdict, crate::RuleVerdict::Allow);
            let completed = completed.expect("actual Alliance FIFO completion");
            completed.validate(step.transaction).unwrap();
            assert_eq!(
                oasis7_wasm_abi::encode_canonical_cbor(completed.event()).unwrap(),
                pair[1]
            );
            assert_eq!(
                fixture.objects.original(blake3::hash(&pair[1]).as_bytes()),
                pair[1]
            );
            for bytes in pair {
                assert_eq!(
                    fixture.objects.original(blake3::hash(bytes).as_bytes()),
                    *bytes
                );
            }
            let mut auth_io = genesis::io();
            let mut auth_keys = genesis::keys();
            let mut current = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                root: fixture.roots.state_root(StateNamespaceV1::Business),
                state: &reader,
                values: ValueReadContext {
                    source: &reader,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut auth_io,
                },
                key_work: &mut auth_keys,
                semantic_bytes: 4096,
                record_bytes: 16 << 20,
                take_state_error: || reader.cas.error(),
                cancelled: || false,
            };
            use oasis7_world_state::incremental_diplomacy::*;
            for outer in ["group", "war-a", "war-b"] {
                let guard = step.transaction.guard();
                let proof = open_current_diplomacy_relation_record_v1(
                    DiplomacyFieldV1::Alliances,
                    outer,
                    sources.diplomacy,
                    &fixture.roots,
                    step.transaction,
                    &mut current,
                    &mut work.index,
                    &guard,
                    &live,
                )
                .unwrap();
                let Some(expected) = state.get(outer) else {
                    assert!(proof.is_none());
                    continue;
                };
                let proof = proof.unwrap();
                let mut child =
                    oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                        root: proof.root(),
                        state: &reader,
                        values: ValueReadContext {
                            source: &reader,
                            memory: &memory,
                            class: MemoryClass::Ordinary,
                            io: &mut *current.values.io,
                        },
                        key_work: &mut *current.key_work,
                        semantic_bytes: 4096,
                        record_bytes: 16 << 20,
                        take_state_error: || reader.cas.error(),
                        cancelled: || false,
                    };
                for (column, text) in [
                    (AllianceScalarV1::AllianceId, expected.alliance_id.as_str()),
                    (AllianceScalarV1::Charter, expected.charter.as_str()),
                    (
                        AllianceScalarV1::FormedByAgentId,
                        expected.formed_by_agent_id.as_str(),
                    ),
                ] {
                    let mut output = Vec::new();
                    assert!(matches!(
                        read_qualified_diplomacy_scalar_v1(
                            outer,
                            DiplomacyFieldV1::Alliances,
                            DiplomacyScalarV1::Alliance(column),
                            &proof,
                            &mut child,
                            4096,
                            &mut output
                        )
                        .unwrap(),
                        Some(DiplomacyValueV1::Text { .. })
                    ));
                    assert_eq!(output, text.as_bytes());
                }
                assert!(
                    matches!(read_qualified_diplomacy_scalar_v1(outer,DiplomacyFieldV1::Alliances,DiplomacyScalarV1::Alliance(AllianceScalarV1::FormedAt),&proof,&mut child,64,&mut std::io::sink()).unwrap(),Some(DiplomacyValueV1::Unsigned(n))if n==expected.formed_at)
                );
                assert_eq!(
                    read_qualified_diplomacy_length_v1(
                        DiplomacyFieldV1::Alliances,
                        outer,
                        &proof,
                        &mut child
                    )
                    .unwrap(),
                    expected.members.len() as u64
                );
                for (ordinal, text) in expected.members.iter().enumerate() {
                    let mut output = Vec::new();
                    assert!(matches!(
                        read_qualified_diplomacy_member_v1(
                            DiplomacyMemberReadV1 {
                                field: DiplomacyFieldV1::Alliances,
                                id: outer,
                                ordinal: ordinal as u64,
                                column: DiplomacyMemberColumnV1::AllianceMember,
                                qualification: &proof,
                                max_payload_bytes: 4096
                            },
                            &mut child,
                            &mut output
                        )
                        .unwrap(),
                        Some(DiplomacyValueV1::Text { .. })
                    ));
                    assert_eq!(output, text.as_bytes());
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
