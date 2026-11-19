//! Whole native23 GEN, sole Root FIFO, real old Accepted producer and Current oracle.
use super::*;
use crate::world::incremental_action_contract_accept::ContractAcceptWorkV1;
use crate::world::incremental_body::BodyMutationContext;
fn contract_accept_work() -> ContractAcceptWorkV1 {
    ContractAcceptWorkV1 {
        steps_remaining: 1_000_000,
        bytes_remaining: 64 << 20,
        pending_entries_remaining: 100_000,
        contract_scan: genesis::scan(),
        cells_remaining: 1_000_000,
        max_text_bytes: 4096,
        max_pending_node_bytes: 16 << 20,
    }
}
fn run_contract_oracle(material_mode: u8) {
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
    use oasis7_world_state::gameplay_state::{
        EconomicContractFulfillmentKind, EconomicContractState, EconomicContractStatus,
    };
    for (outer, status, expires, counterparty, fulfillment) in [
        (
            "c",
            EconomicContractStatus::Open,
            1,
            "owner",
            EconomicContractFulfillmentKind::AtomicExchange,
        ),
        (
            "service",
            EconomicContractStatus::Open,
            1,
            "other",
            EconomicContractFulfillmentKind::Service,
        ),
        (
            "old",
            EconomicContractStatus::Open,
            0,
            "owner",
            EconomicContractFulfillmentKind::AtomicExchange,
        ),
        (
            "wrong",
            EconomicContractStatus::Open,
            0,
            "other",
            EconomicContractFulfillmentKind::AtomicExchange,
        ),
        (
            "accepted",
            EconomicContractStatus::Accepted,
            0,
            "other",
            EconomicContractFulfillmentKind::AtomicExchange,
        ),
        (
            "settled",
            EconomicContractStatus::Settled,
            0,
            "other",
            EconomicContractFulfillmentKind::AtomicExchange,
        ),
        (
            "expired",
            EconomicContractStatus::Expired,
            0,
            "other",
            EconomicContractFulfillmentKind::AtomicExchange,
        ),
        (
            "unrelated",
            EconomicContractStatus::Open,
            99,
            "missing",
            EconomicContractFulfillmentKind::AtomicExchange,
        ),
    ] {
        world.state.economic_contracts.insert(
            outer.into(),
            EconomicContractState {
                contract_id: format!("embedded-{outer}"),
                creator_agent_id: "absent-creator".into(),
                counterparty_agent_id: counterparty.into(),
                fulfillment_kind: fulfillment,
                settlement_kind: ResourceKind::Data,
                settlement_amount: 37,
                reputation_stake: 9,
                expires_at: expires,
                description: "original description".into(),
                status,
                accepted_at: Some(44),
                settled_at: Some(55),
                settlement_success: Some(false),
                transfer_amount: 17,
                tax_amount: 3,
                settlement_notes: Some("keep original notes".into()),
            },
        );
    }
    if material_mode == 2 {
        world
            .state
            .material_ledgers
            .remove(&oasis7_world_state::MaterialLedgerId::World);
        assert!(
            !world
                .state
                .material_ledgers
                .contains_key(&oasis7_world_state::MaterialLedgerId::World)
        );
    }
    if material_mode < 2 {
        world.state.materials.insert("ore".into(), 7);
    }
    if material_mode == 0 {
        world
            .state
            .material_ledgers
            .entry(oasis7_world_state::MaterialLedgerId::World)
            .or_default()
            .insert("ore".into(), 10);
    }
    if material_mode == 1 {
        world.state.material_ledgers.insert(
            oasis7_world_state::MaterialLedgerId::World,
            Default::default(),
        );
    }
    world
        .state
        .material_ledgers
        .entry(oasis7_world_state::MaterialLedgerId::agent("other"))
        .or_default()
        .insert("untouched".into(), 13);
    let cases = if material_mode == 0 {
        vec![
            ("missing", ""),
            ("owner", " "),
            ("owner", "absent"),
            ("owner", "accepted"),
            ("owner", "settled"),
            ("owner", "expired"),
            ("owner", "wrong"),
            ("owner", "old"),
            ("owner", " c "),
            ("owner", "c"),
            ("other", "service"),
        ]
    } else {
        vec![("owner", "c")]
    };
    for (accepter, id) in cases {
        world.submit_action(crate::Action::AcceptEconomicContract {
            accepter_agent_id: accepter.into(),
            contract_id: id.into(),
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
            oracle.state.economic_contracts.clone(),
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
    if material_mode == 2 {
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
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
        assert_eq!(
            overlay
                .ledger(
                    &oasis7_world_state::MaterialLedgerId::World,
                    "__absent_probe__"
                )
                .unwrap(),
            oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::LedgerAbsent
        );
    }
    let mut sink = fixture.cas.clone();
    let mut write_io = genesis::io();
    let mut canonical = genesis::canonical();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut write_io,
        cancelled: || false,
    };
    let mut work = contract_accept_work();
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
        for (events, (contracts, ledgers, materials)) in expected.iter().zip(&states) {
            let mut persist =
                |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    callbacks += 1;
                    producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
                    let n=usize::try_from(stream.original_bytes()).unwrap();let _fee=memory.reserve(n+64,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;let mut bytes=Vec::with_capacity(n);stream.write_to(&mut bytes)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&bytes),stream.original_id());Ok(())
                })
                };
            let now = step.time();
            let (_, result) = rules::evaluate_with_contract_accept(
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
                    work: rules::NonmovementFixtureWorkV1::ContractAccept(&mut work),
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
            for (id, expected_contract) in contracts {
                use oasis7_world_state::incremental_diplomacy::*;
                let guard = step.transaction.guard();
                let proof = open_current_diplomacy_contract_record_v1(
                    id,
                    fixture.current.native().diplomacy(),
                    &fixture.roots,
                    step.transaction,
                    &mut read,
                    &mut work.contract_scan,
                    &mut work.cells_remaining,
                    4096,
                    &guard,
                    &live,
                )
                .unwrap()
                .unwrap();
                let mut io = genesis::io();
                let mut keys = genesis::keys();
                let mut child =
                    oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                        root: proof.root(),
                        state: &reader,
                        values: ValueReadContext {
                            source: &reader,
                            memory: read.values.memory,
                            class: MemoryClass::Ordinary,
                            io: &mut io,
                        },
                        key_work: &mut keys,
                        semantic_bytes: 4096,
                        record_bytes: 16 << 20,
                        take_state_error: || reader.cas.error(),
                        cancelled: || false,
                    };
                let status = match expected_contract.status {
                    EconomicContractStatus::Open => "open",
                    EconomicContractStatus::Accepted => "accepted",
                    EconomicContractStatus::Settled => "settled",
                    EconomicContractStatus::Expired => "expired",
                };
                let fulfillment = match expected_contract.fulfillment_kind {
                    EconomicContractFulfillmentKind::AtomicExchange => "atomic_exchange",
                    EconomicContractFulfillmentKind::Service => "service",
                };
                let kind = match expected_contract.settlement_kind {
                    ResourceKind::Electricity => "electricity",
                    ResourceKind::Data => "data",
                };
                for (col, text) in [
                    (
                        ContractScalarV1::ContractId,
                        Some(expected_contract.contract_id.as_str()),
                    ),
                    (
                        ContractScalarV1::CreatorAgentId,
                        Some(expected_contract.creator_agent_id.as_str()),
                    ),
                    (
                        ContractScalarV1::CounterpartyAgentId,
                        Some(expected_contract.counterparty_agent_id.as_str()),
                    ),
                    (ContractScalarV1::FulfillmentKind, Some(fulfillment)),
                    (ContractScalarV1::SettlementKind, Some(kind)),
                    (
                        ContractScalarV1::Description,
                        Some(expected_contract.description.as_str()),
                    ),
                    (ContractScalarV1::Status, Some(status)),
                    (
                        ContractScalarV1::SettlementNotes,
                        expected_contract.settlement_notes.as_deref(),
                    ),
                ] {
                    let mut bytes = Vec::new();
                    let actual = read_qualified_diplomacy_scalar_v1(
                        id,
                        DiplomacyFieldV1::EconomicContracts,
                        DiplomacyScalarV1::Contract(col),
                        &proof,
                        &mut child,
                        4096,
                        &mut bytes,
                    )
                    .unwrap();
                    assert_eq!(actual.is_some(), text.is_some(), "outer={id}");
                    assert_eq!(String::from_utf8(bytes).unwrap(), text.unwrap_or(""));
                }
                for (col, value) in [
                    (
                        ContractScalarV1::SettlementAmount,
                        Some(DiplomacyValueV1::Signed(
                            expected_contract.settlement_amount,
                        )),
                    ),
                    (
                        ContractScalarV1::ReputationStake,
                        Some(DiplomacyValueV1::Signed(expected_contract.reputation_stake)),
                    ),
                    (
                        ContractScalarV1::ExpiresAt,
                        Some(DiplomacyValueV1::Unsigned(expected_contract.expires_at)),
                    ),
                    (
                        ContractScalarV1::AcceptedAt,
                        expected_contract
                            .accepted_at
                            .map(DiplomacyValueV1::Unsigned),
                    ),
                    (
                        ContractScalarV1::SettledAt,
                        expected_contract.settled_at.map(DiplomacyValueV1::Unsigned),
                    ),
                    (
                        ContractScalarV1::SettlementSuccess,
                        expected_contract
                            .settlement_success
                            .map(DiplomacyValueV1::Bool),
                    ),
                    (
                        ContractScalarV1::TransferAmount,
                        Some(DiplomacyValueV1::Signed(expected_contract.transfer_amount)),
                    ),
                    (
                        ContractScalarV1::TaxAmount,
                        Some(DiplomacyValueV1::Signed(expected_contract.tax_amount)),
                    ),
                ] {
                    assert_eq!(
                        read_qualified_diplomacy_scalar_v1(
                            id,
                            DiplomacyFieldV1::EconomicContracts,
                            DiplomacyScalarV1::Contract(col),
                            &proof,
                            &mut child,
                            4096,
                            &mut std::io::sink()
                        )
                        .unwrap(),
                        value,
                        "outer={id}"
                    );
                }
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
            assert_eq!(
                overlay
                    .ledger(
                        &oasis7_world_state::MaterialLedgerId::World,
                        "__absent_probe__"
                    )
                    .unwrap(),
                oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::MaterialAbsent
            );
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

#[test]
fn queued_contract_accept_full_oracle_world_ledger_wins() {
    run_contract_oracle(0);
}
#[test]
fn queued_contract_accept_full_oracle_legacy_material_fallback() {
    run_contract_oracle(1);
}
#[test]
fn queued_contract_accept_full_oracle_both_empty_creates_world_group() {
    run_contract_oracle(2);
}

// Fallible real Root FIFO boundaries.
use std::sync::atomic::AtomicBool;
#[derive(Clone, Copy)]
enum Failure {
    StepsExhausted,
    CancelBeforeSelection,
    PersistSentinel,
}
#[derive(Debug)]
struct ContractAcceptPersistSentinel;
impl std::fmt::Display for ContractAcceptPersistSentinel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("contract-accept-Accepted-original-persist-sentinel")
    }
}
impl std::error::Error for ContractAcceptPersistSentinel {}
fn run_failure(failure: Failure) {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    for id in ["owner", "other"] {
        let agent = oasis7_world_state::agent_state::AgentState::new(
            id,
            oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        );
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(agent, 0));
    }
    world.state.economic_contracts.insert(
        "c".into(),
        oasis7_world_state::gameplay_state::EconomicContractState {
            contract_id: "embedded-c".into(),
            creator_agent_id: "absent-creator".into(),
            counterparty_agent_id: "owner".into(),
            fulfillment_kind:
                oasis7_world_state::gameplay_state::EconomicContractFulfillmentKind::AtomicExchange,
            settlement_kind: oasis7_agent_api::world_action::ResourceKind::Data,
            settlement_amount: 5,
            reputation_stake: 1,
            expires_at: 10,
            description: "contract".into(),
            status: oasis7_world_state::gameplay_state::EconomicContractStatus::Open,
            accepted_at: None,
            settled_at: None,
            settlement_success: None,
            transfer_amount: 0,
            tax_amount: 0,
            settlement_notes: None,
        },
    );
    world.submit_action(crate::Action::AcceptEconomicContract {
        accepter_agent_id: "owner".into(),
        contract_id: "c".into(),
    });
    // The independent legacy action/publication confirms this input is legal.
    let mut oracle = World::new();
    oracle.state = world.state.clone();
    oracle.state.time = 1;
    let envelope = world.pending_actions.front().unwrap();
    let body = oracle.action_to_event(envelope).unwrap();
    oracle.preflight_domain_event(&body).unwrap();
    oracle
        .append_legacy_action_accepted_for_test(envelope)
        .unwrap();
    oracle
        .append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)))
        .unwrap();
    assert_eq!(
        oracle.state.economic_contracts["c"].status,
        oasis7_world_state::gameplay_state::EconomicContractStatus::Accepted
    );
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
    let mut sink = fixture.cas.clone();
    let mut write_io = genesis::io();
    let mut canonical = genesis::canonical();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut write_io,
        cancelled: || false,
    };
    let mut work = contract_accept_work();
    if matches!(failure, Failure::StepsExhausted) {
        work.steps_remaining = 0;
    }
    let mut audit = transfer_resource_audit_work();
    let mut callbacks = 0;
    let stop = AtomicBool::new(matches!(failure, Failure::CancelBeforeSelection));
    let mut persist =
        |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
            callbacks += 1;
            if matches!(failure, Failure::PersistSentinel) {
                return Err(crate::world::incremental_body::BodyMutationError::Original(
                    Box::new(ContractAcceptPersistSentinel),
                ));
            }
            producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
            let n=usize::try_from(stream.original_bytes()).unwrap();let _fee=memory.reserve(n+64,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
            let mut bytes=Vec::with_capacity(n);stream.write_to(&mut bytes)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&bytes),stream.original_id());Ok(())
        })
        };
    let error = {
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
        let now = step.time();
        match rules::try_evaluate_with_contract_accept(
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
            matches!(failure, Failure::CancelBeforeSelection).then_some(&stop),
            rules::NonmovementExecutionFixtureV1 {
                limits: material_limits,
                persist: &mut persist,
                work: rules::NonmovementFixtureWorkV1::ContractAccept(&mut work),
                audit: &mut audit,
            },
        ) {
            Err(error) => error,
            Ok(_) => {
                panic!("host failure unexpectedly returned a completed ContractAccept receipt")
            }
        }
    };
    let body = error
        .body_error_for_fixture()
        .expect("original Root nested body cause");
    match failure {
        Failure::StepsExhausted => assert!(
            matches!(
                body,
                crate::world::incremental_body::BodyMutationError::Value(
                    ValueTreeError::ResourceLimited("contract_accept business steps")
                )
            ),
            "{body:?}"
        ),
        Failure::CancelBeforeSelection => assert!(
            matches!(
                body,
                crate::world::incremental_body::BodyMutationError::Value(ValueTreeError::Cancelled)
            ),
            "{body:?}"
        ),
        Failure::PersistSentinel => match body {
            crate::world::incremental_body::BodyMutationError::Original(cause) => assert!(
                cause
                    .downcast_ref::<ContractAcceptPersistSentinel>()
                    .is_some(),
                "{cause}"
            ),
            other => panic!("Accepted persistence cause changed: {other:?}"),
        },
    }
    assert_eq!(
        callbacks,
        usize::from(matches!(failure, Failure::PersistSentinel))
    );
    assert_eq!(
        (0..tx.ordered_fact_count())
            .filter_map(|i| tx.ordered_fact(i))
            .filter(|fact| fact.kind == ExecutionFactKindV1::Event)
            .count(),
        0
    );
    assert_eq!(
        (0..tx.ordered_fact_count())
            .filter_map(|i| tx.ordered_fact(i))
            .filter(|fact| fact.kind == ExecutionFactKindV1::Input)
            .count(),
        usize::from(!matches!(failure, Failure::CancelBeforeSelection))
    );
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
#[test]
fn queued_contract_accept_steps_exhausted_preserves_input_without_accepted_or_result_original() {
    run_failure(Failure::StepsExhausted);
}
#[test]
fn queued_contract_accept_cancel_before_selection_has_no_input_or_event_and_aborts() {
    run_failure(Failure::CancelBeforeSelection);
}
#[test]
fn queued_contract_accept_accepted_persist_retains_exact_sentinel_and_has_no_event() {
    run_failure(Failure::PersistSentinel);
}
