//! Genuine FIFO Grant/Revoke -> Root -> independent old World oracle.
use super::*;
fn data_access_work() -> crate::world::incremental_action_data::data_access::DataAccessWorkV1 {
    crate::world::incremental_action_data::data_access::DataAccessWorkV1 { steps_remaining:100_000,bytes_remaining:64<<20,pending_entries_remaining:100_000,controls:oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsWorkV1 {records_remaining:100_000,index_steps_remaining:100_000,max_record_bytes:16<<20} }
}
fn assert_permissions<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    tx: &mut ExecutionTransaction<'_, Reader<'a>>,
    memory: &MemoryBudget,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    expected: &[(String, Option<std::collections::BTreeSet<String>>); 2],
) {
    use oasis7_world_state::{
        incremental_authorization_mapsets::{
            AuthorizationMapSetFieldV1, AuthorizationMembershipV1,
            read_authorization_membership_overlay_v1,
        },
        incremental_world_collection_controls::read_current_data_access_group_size_v1,
    };
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: reader,
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
    let mut work = data_access_work();
    for (subject, members) in expected {
        let guard = tx.guard();
        let count = read_current_data_access_group_size_v1(
            fixture.current.native().controls(),
            &fixture.roots,
            subject,
            tx,
            &mut read,
            &mut work.controls,
            &mut work.pending_entries_remaining,
            &guard,
            live,
        )
        .unwrap();
        assert_eq!(count, members.as_ref().map(|m| m.len() as u64));
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: ValueReadContext {
                source: reader,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        for member in ["sender", "receiver", "far"] {
            let actual = read_authorization_membership_overlay_v1(
                AuthorizationMapSetFieldV1::DataAccessPermissions,
                subject,
                member,
                &mut overlay,
            )
            .unwrap();
            let expected = match members {
                None => AuthorizationMembershipV1::GroupAbsent,
                Some(set) if set.contains(member) => AuthorizationMembershipV1::Present,
                Some(_) => AuthorizationMembershipV1::MemberAbsent,
            };
            assert_eq!(actual, expected);
        }
    }
}
#[test]
fn queued_data_access_actions_match_world_and_current_permissions() {
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
    let actions = [
        crate::Action::RevokeDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "receiver".into(),
        },
        crate::Action::GrantDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "receiver".into(),
        },
        crate::Action::GrantDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "receiver".into(),
        },
        crate::Action::GrantDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "far".into(),
        },
        crate::Action::RevokeDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "receiver".into(),
        },
        crate::Action::RevokeDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "far".into(),
        },
        crate::Action::RevokeDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "receiver".into(),
        },
        crate::Action::GrantDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "sender".into(),
        },
        crate::Action::RevokeDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "sender".into(),
        },
        crate::Action::GrantDataAccess {
            owner_agent_id: "missing".into(),
            grantee_agent_id: "missing2".into(),
        },
        crate::Action::RevokeDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "missing".into(),
        },
        crate::Action::RevokeDataAccess {
            owner_agent_id: "missing".into(),
            grantee_agent_id: "receiver".into(),
        },
        crate::Action::GrantDataAccess {
            owner_agent_id: "far".into(),
            grantee_agent_id: "receiver".into(),
        },
        crate::Action::RevokeDataAccess {
            owner_agent_id: "far".into(),
            grantee_agent_id: "receiver".into(),
        },
        crate::Action::GrantDataAccess {
            owner_agent_id: "sender".into(),
            grantee_agent_id: "receiver".into(),
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
    let mut expected_permissions = Vec::new();
    for envelope in &world.pending_actions {
        let body = oracle.action_to_event(envelope).unwrap();
        oracle
            .append_event_for_test(body.clone(), Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        expected.push(body);
        expected_permissions.push(["sender", "far"].map(|id| {
            (
                id.to_string(),
                oracle.state.data_access_permissions.get(id).cloned(),
            )
        }));
    }
    let expected_balances: [(String, i64, i64); 3] = ["sender", "receiver", "far"].map(|id| {
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
    assert!(expected[..7].iter().all(|body| matches!(
        body,
        crate::WorldEventBody::Domain(
            crate::DomainEvent::DataAccessGranted { .. }
                | crate::DomainEvent::DataAccessRevoked { .. }
        )
    )));
    assert!(expected[7..12].iter().all(|body| matches!(
        body,
        crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { .. })
    )));
    let expected_mailboxes: [(String, Vec<crate::DomainEvent>); 3] = ["sender", "receiver", "far"]
        .map(|id| {
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
        let mut work = data_access_work();
        let mut audits = transfer_resource_audit_work();
        for ((expected_body, expected_original), permissions) in expected
            .iter()
            .zip(&expected_originals)
            .zip(&expected_permissions)
        {
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
            let (_, result) = rules::evaluate_with_data_access(
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
                    work: rules::NonmovementFixtureWorkV1::DataAccess(&mut work),
                    audit: &mut audits,
                },
            );
            assert_permissions(
                &fixture,
                &reader,
                step.transaction,
                &memory,
                &live,
                permissions,
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
                    .filter(|result| match &result.event().body {
                        crate::WorldEventBody::Domain(
                            crate::DomainEvent::DataAccessGranted {
                                owner_agent_id,
                                grantee_agent_id,
                            }
                            | crate::DomainEvent::DataAccessRevoked {
                                owner_agent_id,
                                grantee_agent_id,
                            },
                        ) => owner_agent_id == agent || grantee_agent_id == agent,
                        _ => false,
                    })
                    .nth(index)
                    .unwrap();
                assert_eq!(
                    &result.event().body,
                    &crate::WorldEventBody::Domain(expected_domain.clone())
                );
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
