//! Authenticated Root consumer with independent old-World originals and per-action nonce oracle.
use super::*;
use crate::world::incremental_body_event::materials::test_support::MaterialFixtureLimitsV1;
use crate::world::{
    World, incremental_action_data::CollectDataWorkV1,
    incremental_action_execution::take_pending_action, incremental_body_event::read_fixed,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionTransaction, TransactionLimits,
    },
    incremental_state::StateNamespaceV1,
    incremental_value::ValueReadContext,
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
fn selected_authenticated_collect_data_matches_world_originals_resources_and_nonce() {
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
    let key = |seed: u8| {
        hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                .verifying_key()
                .to_bytes(),
        )
    };
    let pk = key(7);
    let duplicate_key = key(8);
    let unbound_key = key(9);
    for (outer, agent, public_key) in [
        ("outer-different-from-player", "collector", pk.as_str()),
        ("duplicate-a", "collector", duplicate_key.as_str()),
        ("duplicate-b", "receiver", duplicate_key.as_str()),
    ] {
        world.state.starter_oc_claims.insert(
            outer.into(),
            oasis7_world_state::gameplay_state::StarterOcClaimState {
                agent_id: agent.into(),
                player_id: "player".into(),
                public_key: Some(public_key.into()),
                ..Default::default()
            },
        );
    }
    for (seed, public_key, collector, nonce, bad) in [
        (9, unbound_key.as_str(), "collector", 1, false),
        (7, pk.as_str(), "collector", 1, true),
        (7, pk.as_str(), "collector", 0, false),
        (7, pk.as_str(), "collector", 1, false),
        (7, pk.as_str(), "collector", 1, false),
        (7, pk.as_str(), "collector", 2, false),
        (7, pk.as_str(), "receiver", 3, false),
        (8, duplicate_key.as_str(), "collector", 1, false),
        (7, pk.as_str(), "collector", 3, false),
    ] {
        let signature = if bad {
            "invalid".into()
        } else {
            oasis7_proto::collect_data_auth::sign_authorization(
                oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
                1,
                1,
                "player",
                public_key,
                nonce,
                &hex::encode([seed; 32]),
            )
            .unwrap()
        };
        world.submit_action(crate::Action::CollectDataAuthenticated {
            collector_agent_id: collector.into(),
            electricity_cost: 1,
            data_amount: 1,
            player_id: "player".into(),
            public_key: public_key.into(),
            nonce,
            signature,
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
    let mut expected_nonces = Vec::new();
    for envelope in &world.pending_actions {
        let body = oracle.action_to_event(envelope).unwrap();
        oracle
            .append_event_for_test(body.clone(), Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        expected.push(body);
        expected_nonces.push(
            oracle
                .state
                .authenticated_collect_data_last_nonces
                .get("player")
                .and_then(|m| m.get(&pk))
                .copied(),
        );
    }
    let expected_electricity = oracle.state.agents["collector"]
        .state
        .resources
        .get(ResourceKind::Electricity);
    let expected_data = oracle.state.agents["collector"]
        .state
        .resources
        .get(ResourceKind::Data);
    assert_eq!(
        expected
            .iter()
            .filter(|body| matches!(
                body,
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::DataCollectedAuthenticated { .. }
                )
            ))
            .count(),
        3
    );
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
        let mut auth_work=crate::world::incremental_action_data::auth_current::CollectDataAuthorizationWorkV1 {
            signature:crate::world::incremental_action_data::auth_signature::CollectDataSignatureWorkV1 {checks_remaining:18,bytes_remaining:4<<20},
            claims:oasis7_world_state::incremental_world_native::StarterBindingIndexWorkV1 {
                max_rows:256,max_row_text_bytes:64<<10,max_pending_rows:1024,steps_remaining:100_000,
                bytes_remaining:64<<20,comparison_bytes_remaining:64<<20,moved_slots_remaining:1_000_000,pending_entries_remaining:100_000},
            native:genesis::current_work(&fixture.roots).native.native,scan:genesis::scan()};
        for ((expected_body, expected_original), expected_nonce) in expected
            .iter()
            .zip(&expected_originals)
            .zip(&expected_nonces)
        {
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
                    work: rules::NonmovementFixtureWorkV1::AuthenticatedCollectData(
                        &mut work,
                        &mut auth_work,
                    ),
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

            {
                let mut nonce_read = WorldFieldOverlayContextV1 {
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
                let nonce=oasis7_world_state::incremental_authenticated_nonces::read_authenticated_nonce_overlay_v1("player",&pk,&mut nonce_read).unwrap();
                assert_eq!(nonce,expected_nonce.map(oasis7_world_state::incremental_authenticated_nonces::AuthenticatedNonceV1::Value)
                    .unwrap_or(oasis7_world_state::incremental_authenticated_nonces::AuthenticatedNonceV1::PlayerAbsent));
            }
            actual.push(result);
        }
        assert_eq!(step.journal_height().unwrap(), expected.len() as u64);
        let persisted = crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_collect_data(&actual[3]);
        persisted.validate(step.transaction).unwrap();
        assert_eq!(persisted.event(), actual[3].event());
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
        assert_eq!((frontier.head, frontier.tail), (0, 3));
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

#[path = "incremental_collect_data_authenticated_overflow_tests.rs"]
mod overflow;
