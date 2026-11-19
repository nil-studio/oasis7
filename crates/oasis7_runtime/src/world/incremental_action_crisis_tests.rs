//! Whole native23 GEN, sole Root FIFO, real old Accepted producer and Current oracle.
use super::*;
use crate::world::incremental_action_crisis::CrisisWorkV1;
use crate::world::incremental_body::BodyMutationContext;
fn crisis_work() -> CrisisWorkV1 {
    CrisisWorkV1 {
        steps_remaining: 1_000_000,
        bytes_remaining: 64 << 20,
        pending_entries_remaining: 100_000,
        crisis_scan: genesis::scan(),
        cells_remaining: 1_000_000,
        max_text_bytes: 4096,
        max_pending_node_bytes: 16 << 20,
    }
}
#[test]
fn queued_crisis_preserves_legacy_accepted_originals_current_state_and_mailboxes() {
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
    use oasis7_world_state::gameplay_state::{CrisisState, CrisisStatus};
    for (outer, status, expires, severity) in [
        ("c", CrisisStatus::Active, 1, 2),
        ("failed", CrisisStatus::Active, 1, 0),
        ("old", CrisisStatus::Active, 0, 3),
        ("done", CrisisStatus::Resolved, 9, 4),
        ("unrelated", CrisisStatus::TimedOut, 99, 5),
    ] {
        world.state.crises.insert(
            outer.into(),
            CrisisState {
                crisis_id: format!("embedded-{outer}"),
                kind: "storm".into(),
                severity,
                status,
                opened_at: 0,
                expires_at: expires,
                resolver_agent_id: Some("previous".into()),
                strategy: Some("old".into()),
                success: Some(true),
                impact: 77,
                resolved_at: Some(0),
            },
        );
    }
    world.state.materials.insert("ore".into(), 7);
    world
        .state
        .material_ledgers
        .entry(oasis7_world_state::MaterialLedgerId::World)
        .or_default()
        .insert("ore".into(), 10);
    for (resolver, id, strategy, success) in [
        ("missing", "", "", true),
        ("owner", " ", "", true),
        ("owner", "absent", "", true),
        ("owner", "done", "", true),
        ("owner", "old", "", true),
        ("owner", "c", " ", true),
        ("owner", " c ", " shield ", true),
        ("owner", "c", "again", true),
        ("other", "failed", "retreat", false),
    ] {
        world.submit_action(crate::Action::ResolveCrisis {
            resolver_agent_id: resolver.into(),
            crisis_id: id.into(),
            strategy: strategy.into(),
            success,
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
            oracle.state.crises.clone(),
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
    let mut work = crisis_work();
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
        for (events, (crises, ledgers, materials)) in expected.iter().zip(&states) {
            let mut persist =
                |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    callbacks += 1;
                    producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
                    let n=usize::try_from(stream.original_bytes()).unwrap();let _fee=memory.reserve(n+64,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;let mut bytes=Vec::with_capacity(n);stream.write_to(&mut bytes)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&bytes),stream.original_id());Ok(())
                })
                };
            let now = step.time();
            let (_, result) = rules::evaluate_with_crisis(
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
                    work: rules::NonmovementFixtureWorkV1::Crisis(&mut work),
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
            for (id, expected_crisis) in crises {
                use oasis7_world_state::incremental_diplomacy::*;
                let guard = step.transaction.guard();
                let proof = open_current_diplomacy_crisis_record_v1(
                    id,
                    fixture.current.native().diplomacy(),
                    &fixture.roots,
                    step.transaction,
                    &mut read,
                    &mut work.crisis_scan,
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
                let status = match expected_crisis.status {
                    oasis7_world_state::gameplay_state::CrisisStatus::Active => "active",
                    oasis7_world_state::gameplay_state::CrisisStatus::Resolved => "resolved",
                    oasis7_world_state::gameplay_state::CrisisStatus::TimedOut => "timed_out",
                };
                for (col, text) in [
                    (
                        CrisisScalarV1::CrisisId,
                        Some(expected_crisis.crisis_id.as_str()),
                    ),
                    (CrisisScalarV1::Kind, Some(expected_crisis.kind.as_str())),
                    (CrisisScalarV1::Status, Some(status)),
                    (
                        CrisisScalarV1::ResolverAgentId,
                        expected_crisis.resolver_agent_id.as_deref(),
                    ),
                    (
                        CrisisScalarV1::Strategy,
                        expected_crisis.strategy.as_deref(),
                    ),
                ] {
                    let mut bytes = Vec::new();
                    let actual = read_qualified_diplomacy_scalar_v1(
                        id,
                        DiplomacyFieldV1::Crises,
                        DiplomacyScalarV1::Crisis(col),
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
                        CrisisScalarV1::Severity,
                        Some(DiplomacyValueV1::Unsigned(u64::from(
                            expected_crisis.severity,
                        ))),
                    ),
                    (
                        CrisisScalarV1::OpenedAt,
                        Some(DiplomacyValueV1::Unsigned(expected_crisis.opened_at)),
                    ),
                    (
                        CrisisScalarV1::ExpiresAt,
                        Some(DiplomacyValueV1::Unsigned(expected_crisis.expires_at)),
                    ),
                    (
                        CrisisScalarV1::Success,
                        expected_crisis.success.map(DiplomacyValueV1::Bool),
                    ),
                    (
                        CrisisScalarV1::Impact,
                        Some(DiplomacyValueV1::Signed(expected_crisis.impact)),
                    ),
                    (
                        CrisisScalarV1::ResolvedAt,
                        expected_crisis.resolved_at.map(DiplomacyValueV1::Unsigned),
                    ),
                ] {
                    assert_eq!(
                        read_qualified_diplomacy_scalar_v1(
                            id,
                            DiplomacyFieldV1::Crises,
                            DiplomacyScalarV1::Crisis(col),
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

// Actual fallible Root FIFO boundaries; the successful oracle above is unchanged.
use std::sync::atomic::AtomicBool;
#[derive(Clone, Copy)]
enum Failure {
    StepsExhausted,
    CancelBeforeSelection,
    PersistSentinel,
}
#[derive(Debug)]
struct CrisisPersistSentinel;
impl std::fmt::Display for CrisisPersistSentinel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("crisis-Accepted-original-persist-sentinel")
    }
}
impl std::error::Error for CrisisPersistSentinel {}
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
    world.state.crises.insert(
        "c".into(),
        oasis7_world_state::gameplay_state::CrisisState {
            crisis_id: "embedded-c".into(),
            kind: "storm".into(),
            severity: 2,
            status: oasis7_world_state::gameplay_state::CrisisStatus::Active,
            opened_at: 0,
            expires_at: 10,
            resolver_agent_id: None,
            strategy: None,
            success: None,
            impact: 0,
            resolved_at: None,
        },
    );
    world.submit_action(crate::Action::ResolveCrisis {
        resolver_agent_id: "owner".into(),
        crisis_id: "c".into(),
        strategy: "shield".into(),
        success: true,
    });
    // Confirm that the exact input is legal through the independent legacy owner.
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
        oracle.state.crises["c"].status,
        oasis7_world_state::gameplay_state::CrisisStatus::Resolved
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
    let mut work = crisis_work();
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
                    Box::new(CrisisPersistSentinel),
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
        match rules::try_evaluate_with_crisis(
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
                work: rules::NonmovementFixtureWorkV1::Crisis(&mut work),
                audit: &mut audit,
            },
        ) {
            Err(error) => error,
            Ok(_) => panic!("host failure unexpectedly returned a completed Crisis receipt"),
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
                    ValueTreeError::ResourceLimited("crisis business steps")
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
                cause.downcast_ref::<CrisisPersistSentinel>().is_some(),
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
fn queued_crisis_steps_exhausted_preserves_input_without_accepted_or_result_original() {
    run_failure(Failure::StepsExhausted);
}
#[test]
fn queued_crisis_cancel_before_selection_has_no_input_or_event_and_aborts() {
    run_failure(Failure::CancelBeforeSelection);
}
#[test]
fn queued_crisis_accepted_persist_retains_exact_sentinel_and_has_no_event() {
    run_failure(Failure::PersistSentinel);
}
