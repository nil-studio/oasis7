//! Real GEN/Current oracle for the selected direct MaterialTransfer lane.
//! This file is intended as a child of incremental_action_body_tests so it
//! shares that suite's genuine Rule and fixture machinery.
use super::*;
use crate::world::{
    incremental_action_execution::take_pending_action,
    incremental_body::{BodyMutationContext, BodyMutationError},
    incremental_body_event::materials::test_support::{MaterialFixtureLimitsV1, MaterialFixtureV1},
    incremental_material_normalization::transfer::{
        MaterialTransferWorkV1, execute_selected_material_transfer_v1,
    },
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction, TransactionLimits},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_abi::{MaterialDefaultPriority, MaterialProfileV1, MaterialTransportLossClass};
use oasis7_world_state::{
    MaterialLedgerId,
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_event_fields::{MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1},
    incremental_inventory::AgentLedgerTransferWorkV1,
    incremental_inventory_read::LedgerQuantityV1,
    incremental_material_records::MaterialProfileCurrentReadWorkV1,
    incremental_projection::{FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS},
    incremental_semantic_key::{SemanticKeyBudget, hash_semantic_key_v1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};

fn transfer_audit_work()
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

fn transfer_world(destination_balance: Option<i64>) -> World {
    let mut world = World::new();
    for agent in ["sender", "receiver"] {
        let state = oasis7_world_state::agent_state::AgentState::new(
            agent,
            oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        );
        world
            .state
            .agents
            .insert(agent.into(), crate::AgentCell::new(state, 0));
    }
    world.state.material_profiles.insert(
        "water".into(),
        MaterialProfileV1 {
            kind: "water".into(),
            tier: 1,
            category: "liquid".into(),
            stack_limit: 1000,
            transport_loss_class: MaterialTransportLossClass::Medium,
            decay_bps_per_tick: 0,
            // The static keyword fallback makes water urgent. The actual
            // Current profile must override it with Standard in this test.
            default_priority: MaterialDefaultPriority::Standard,
        },
    );
    world.state.material_ledgers.insert(
        MaterialLedgerId::agent("sender"),
        [("water".into(), 8)].into(),
    );
    if let Some(balance) = destination_balance {
        world.state.material_ledgers.insert(
            MaterialLedgerId::agent("receiver"),
            [("water".into(), balance)].into(),
        );
    }
    world
}

fn corrupt_current_water_profile<M: MerkleNodeReader>(
    tx: &mut ExecutionTransaction<'_, M>,
    memory: &MemoryBudget,
) {
    let field = WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|field| {
            field.owner == FieldOwner::WorldState
                && field.name == "material_profiles"
                && field.shape == FieldShape::Map
                && field.destination == FieldDestination::State(StateNamespaceV1::Business)
        })
        .expect("material profile field registration");
    let mut semantic_work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let semantic = hash_semantic_key_v1(
        "water",
        memory,
        MemoryClass::Ordinary,
        &mut semantic_work,
        || false,
    )
    .unwrap();
    let key =
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Entry, Some(semantic.digest)).unwrap();
    tx.write(StateNamespaceV1::Business, key.as_bytes(), Some([0xff; 32]))
        .unwrap();
}

#[test]
fn selected_direct_material_transfer_matches_world_and_current_owner_sinks() {
    run_selected_transfer_case(3, false, None);
}

#[test]
fn insufficient_transfer_precedes_corrupt_current_profile_priority() {
    run_selected_transfer_case(9, true, None);
}

#[test]
fn selected_destination_overflow_matches_world_preflight_and_aborts_seal() {
    run_selected_transfer_case(1, false, Some(i64::MAX));
}

fn run_selected_transfer_case(
    amount: i64,
    corrupt_profile: bool,
    destination_balance: Option<i64>,
) {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = transfer_world(destination_balance);
    assert_eq!(
        world.state.material_ledgers[&MaterialLedgerId::agent("sender")]["water"],
        8
    );
    if let Some(balance) = destination_balance {
        assert_eq!(
            world.state.material_ledgers[&MaterialLedgerId::agent("receiver")]["water"],
            balance
        );
    }
    let action = crate::Action::TransferMaterial {
        requester_agent_id: "sender".into(),
        from_ledger: MaterialLedgerId::agent("sender"),
        to_ledger: MaterialLedgerId::agent("receiver"),
        kind: "water".into(),
        amount,
        distance_km: 0,
        priority: None,
        route_id: None,
        route_ids: Vec::new(),
        auto_reroute: false,
    };
    world.submit_action(action);
    let envelope = world.pending_actions[0].clone();
    let mut oracle = World::new_with_state(world.state.clone());
    oracle.state.time = 1;
    let legacy_result = match oracle.action_to_event(&envelope) {
        Ok(body) => oracle
            .append_event_for_test(body.clone(), Some(crate::CausedBy::Action(envelope.id)))
            .map(|_| body),
        Err(error) => Err(error),
    };
    let (expected_body, expected_error, expected_balances) = match legacy_result {
        Ok(body) => {
            let expected_balance = |ledger: &MaterialLedgerId| {
                oracle
                    .state
                    .material_ledgers
                    .get(ledger)
                    .and_then(|materials| materials.get("water"))
                    .copied()
                    .map(LedgerQuantityV1::Value)
                    .unwrap_or(LedgerQuantityV1::LedgerAbsent)
            };
            (
                Some(body),
                None,
                Some((
                    expected_balance(&MaterialLedgerId::agent("sender")),
                    expected_balance(&MaterialLedgerId::agent("receiver")),
                )),
            )
        }
        Err(error) => (None, Some(error), None),
    };
    if amount == 9 {
        let body = expected_body.as_ref().expect("legacy insufficiency event");
        assert!(matches!(
            body,
            crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                reason: crate::RejectReason::InsufficientMaterial {
                    material_kind,
                    requested: 9,
                    available: 8,
                },
                ..
            }) if material_kind == "water"
        ));
    } else if destination_balance.is_some() {
        assert!(matches!(
            expected_error.as_ref(),
            Some(crate::WorldError::ResourceBalanceInvalid { reason })
                if reason.starts_with("material transfer add preflight failed: material balance overflow: kind=water")
        ));
    } else {
        assert!(matches!(
            expected_body.as_ref().expect("legacy transfer event"),
            crate::WorldEventBody::Domain(crate::DomainEvent::MaterialTransferred {
                distance_km: 0,
                priority: oasis7_agent_api::world_action::MaterialTransitPriority::Standard,
                ..
            })
        ));
    };
    let expected_original = oracle
        .journal
        .events
        .last()
        .map(|event| oasis7_wasm_abi::encode_canonical_cbor(event).unwrap());
    let expected_last_active = oracle.state.agents["sender"].last_active;
    let expected_mailbox_tail = u64::try_from(oracle.state.agents["sender"].mailbox.len()).unwrap();
    drop(oracle);

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
        [173; 32],
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
    let counter = crate::world::incremental_world_event_counter::WorldEventCounterV1::begin(
        &fixture.runtime,
        &fixture.head,
        &tx,
        &memory,
    )
    .unwrap();
    let directory = {
        let mut io = genesis::io();
        let mut keys = genesis::keys();
        let mut overlay =
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1 {
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
    let mut overflow_aborted = false;
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
        assert_eq!(selected.envelope().id, envelope.id);
        let outcome = if expected_error.is_some() {
            Some(rules::outcome(
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
            ))
        } else {
            None
        };
        // Build the accepted invocation directory and consume the pending
        // action from the pristine parent first. Then corrupt only the Current
        // profile pointer so the insufficient-balance path proves it does not
        // read profile priority ahead of the legacy insufficiency decision.
        if corrupt_profile {
            corrupt_current_water_profile(step.transaction, &memory);
        }
        let guard = step.transaction.guard();
        let mut audit = transfer_audit_work();
        let mut work = MaterialTransferWorkV1 {
            steps_remaining: 10_000,
            bytes_remaining: 16 << 20,
            profile: MaterialProfileCurrentReadWorkV1 {
                scan: genesis::scan(),
                cells_remaining: 100_000,
                max_payload_bytes: 1 << 20,
            },
            inventory: AgentLedgerTransferWorkV1 {
                steps_remaining: 10_000,
                bytes_remaining: 16 << 20,
                pending_entries_remaining: 100_000,
            },
            receipt: oasis7_world_state::incremental_material_records::MaterialWorkBudgetV1 {
                cells_remaining: 100_000,
                max_pending_node_bytes: 16 << 20,
                scan: genesis::scan(),
            },
        };
        let mut persisted_original_count = 0usize;
        let mut persist =
            |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                persisted_original_count += 1;
                producer.persist_to(&mut |stream: &mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1, cancelled: &mut dyn FnMut() -> bool| {
                    let bytes = usize::try_from(stream.original_bytes()).unwrap();
                    assert!(bytes <= 64 << 10);
                    let _fee = memory
                        .reserve(bytes + std::mem::size_of::<Vec<u8>>(), MemoryClass::Ordinary)
                        .map_err(ExecutionError::Memory)?;
                    let mut encoded = Vec::with_capacity(bytes);
                    stream.write_to(&mut encoded)?;
                    assert!(!cancelled());
                    assert_eq!(fixture.objects.put(&encoded), stream.original_id());
                    Ok(())
                })
            };
        let result = if let Some(outcome) = outcome.as_ref() {
            execute_selected_material_transfer_v1(
                &mut step,
                &mut materials,
                &selected,
                outcome,
                fixture.current.raw().agents(),
                fixture.current.native().material(),
                &mut read,
                &mut parent_read,
                &mut work,
                &mut audit,
                64 << 10,
                &guard,
                &live,
                &mut persist,
            )
        } else {
            let (_, completed) = rules::evaluate_with_material_transfer(
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
                rules::NonmovementExecutionFixtureV1 {
                    limits: material_limits,
                    persist: &mut persist,
                    work: rules::NonmovementFixtureWorkV1::MaterialTransfer(&mut work),
                    audit: &mut audit,
                },
            );
            Ok(completed)
        };
        if let Some(expected_error) = expected_error.as_ref() {
            let actual_error = match result {
                Err(error) => error,
                Ok(_) => panic!("Current overflow must fail before event setup"),
            };
            let BodyMutationError::Original(actual_error) = actual_error else {
                panic!("Current overflow must preserve the typed legacy error");
            };
            let crate::WorldError::ResourceBalanceInvalid { reason } = expected_error else {
                panic!("legacy oracle must report resource balance overflow");
            };
            assert_eq!(actual_error.to_string(), reason.as_str());
            assert!(work.profile.cells_remaining < 100_000);
            assert_eq!(persisted_original_count, 0);
            overflow_aborted = true;
        } else {
            let result = result.expect("Current success/rejection must complete");
            assert_eq!(
                &result.event().body,
                expected_body.as_ref().expect("legacy event oracle")
            );
            result.validate(step.transaction).unwrap();
            let actual_original = oasis7_wasm_abi::encode_canonical_cbor(result.event()).unwrap();
            assert_eq!(Some(&actual_original), expected_original.as_ref());
            assert_eq!(
                fixture
                    .objects
                    .original(blake3::hash(&actual_original).as_bytes()),
                actual_original
            );
            if amount == 3 {
                let persisted = crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_material_transfer(&result);
                persisted.validate(step.transaction).unwrap();
                assert_eq!(persisted.event(), result.event());
            }
            assert_eq!(step.journal_height().unwrap(), 1);
            assert_eq!(persisted_original_count, 1);

            let (expected_sender, expected_receiver) =
                expected_balances.expect("legacy event balances");
            let from = MaterialLedgerId::agent("sender");
            let to = MaterialLedgerId::agent("receiver");
            let mut mirror =
                oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1 {
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
            assert_eq!(mirror.ledger(&from, "water").unwrap(), expected_sender);
            assert_eq!(mirror.ledger(&to, "water").unwrap(), expected_receiver);
            let body_key = AgentBodyKeyV1::new("sender").unwrap();
            let body = AgentBodyValueV1::from_canonical_bytes(
                &read_fixed::<AGENT_BODY_VALUE_BYTES, _, _, _, _>(
                    step.transaction,
                    StateNamespaceV1::Business,
                    body_key.as_bytes(),
                    step.values,
                    step.mutation,
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(body.last_active, expected_last_active);
            let frontier_key = MailboxKeyV1::new("sender", None).unwrap();
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
            assert_eq!(frontier.tail, expected_mailbox_tail);
        }
    }
    if overflow_aborted {
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, fixture.roots.height + 1),
            Err(ExecutionError::Aborted)
        ));
    } else {
        assert!(expected_error.is_none());
    }
}
