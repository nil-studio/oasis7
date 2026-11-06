use super::*;
use oasis7_agent_api::world_action::GeoPos;
pub(super) fn audit_work()
-> oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
    oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
        structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
            bytes_remaining: 8 << 20,
            values_remaining: 100000,
            max_depth: 32,
        },
        canonical: genesis::canonical(),
        max_original_bytes: 64 << 10,
        max_heap_bytes: 32 << 20,
    }
}
fn accepted_audit_fixture(actor_present: bool, corrupt: u8) {
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    if actor_present {
        let agent =
            oasis7_world_state::agent_state::AgentState::new("queued-alice", GeoPos::new(1, 2, 3));
        world
            .state
            .agents
            .insert("queued-alice".into(), crate::AgentCell::new(agent, 0));
    }
    world.submit_action(crate::Action::ClaimAgent {
        claimer_agent_id: "queued-alice".into(),
        target_agent_id: "target".into(),
    });
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
        [92; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &tx, &memory).unwrap();
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
    let selected = crate::world::incremental_action_execution::take_pending_action(
        &mut pending,
        &mut tx,
        &mut read,
        &mut pending_work,
        &mut originals,
        &live,
    )
    .unwrap()
    .unwrap();
    selected.validate(&tx).unwrap();
    if corrupt == 1 {
        let key = oasis7_world_state::incremental_agent_fields::AgentFieldKeyV1::new(
            "queued-alice",
            oasis7_world_state::incremental_agent_fields::AgentFieldV1::Identity,
            0,
        )
        .unwrap();
        tx.write(StateNamespaceV1::Business, key.as_bytes(), None)
            .unwrap();
    } else if corrupt == 2 {
        let key = oasis7_world_state::incremental_agent_body::AgentBodyKeyV1::new("queued-alice")
            .unwrap();
        tx.write(StateNamespaceV1::Business, key.as_bytes(), None)
            .unwrap();
    }
    let mut cas = fixture.cas.clone();
    let mut io = genesis::io();
    let mut canonical = genesis::canonical();
    let mut mutation = BodyMutationContext {
        sink: &mut cas,
        memory: &memory,
        io: &mut io,
        cancelled: || false,
    };
    let mut persisted = Vec::new();
    let guard = tx.guard();
    let mut audit_work = audit_work();
    let initial_values = audit_work.structure.values_remaining;
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
    let mut pending_entries = 10000;
    let mut actor_read = crate::world::incremental_action_execution::AcceptedActorReadContextV1 {
        roots: &fixture.roots,
        agents: fixture.current.raw().agents(),
        current: &mut read,
        parent: &mut parent_read,
        pending_entries: &mut pending_entries,
        live: &live,
    };
    let owned_result = {
        let mut step = crate::world::incremental_step::IwsStepContext::begin_counted(
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
        step.append_selected_accepted(&selected, &mut actor_read, 64 << 10, &mut audit_work,
            |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                producer.persist_to(&mut |stream: &mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
                    let length = usize::try_from(stream.original_bytes()).unwrap();
                    assert!(length <= 64 << 10);
                    let _fee = memory.reserve(length, MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                    let mut bytes = Vec::with_capacity(length);
                    stream.write_to(&mut bytes)?;
                    assert_eq!(*blake3::hash(&bytes).as_bytes(), stream.original_id());
                    assert_eq!(fixture.objects.put(&bytes), stream.original_id());
                    persisted.push(bytes);
                    Ok(())
                })
            }
        )
    };
    if corrupt != 0 {
        assert!(
            owned_result.is_err(),
            "partial Current agent must be rejected before audit persistence"
        );
        assert!(persisted.is_empty());
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
        return;
    }
    let owned = owned_result.unwrap().unwrap();
    owned.validate(&tx).unwrap();
    let token =
        crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_action_audit(
            &owned,
        );
    token.validate(&tx).unwrap();
    assert!(std::ptr::eq(token.event(), owned.event()));
    let unrelated = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [98; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    assert!(
        token.validate(&unrelated).is_err(),
        "Accepted audit token must reject another same-parent transaction"
    );

    assert!(audit_work.structure.values_remaining < initial_values);
    let expected = crate::WorldEvent {
        id: 1,
        time: 1,
        caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
        body: crate::WorldEventBody::Domain(crate::DomainEvent::ActionAccepted {
            action_id: selected.envelope().id,
            action_kind: "action.gameplay.claim_agent".into(),
            actor_id: "queued-alice".into(),
            eta_ticks: 0,
            notes: vec!["accepted_for_gameplay_processing".into()],
        }),
    };
    assert_eq!(
        oasis7_wasm_abi::encode_canonical_cbor(owned.event()).unwrap(),
        oasis7_wasm_abi::encode_canonical_cbor(&expected).unwrap()
    );
    assert_eq!(
        persisted,
        vec![oasis7_wasm_abi::encode_canonical_cbor(&expected).unwrap()]
    );
    assert_eq!(counter.journal_height(&tx).unwrap(), 1);
    // A later rejected outcome shares the same continuous typed audit budget.
    // It appends no second actor mailbox entry.
    let values_after_accepted = audit_work.structure.values_remaining;
    let guard = tx.guard();
    let rejected = {
        let mut step = crate::world::incremental_step::IwsStepContext::begin_counted(
            &mut tx,
            1,
            &guard,
            &live,
            &reader,
            &mut mutation,
            &mut canonical,
            &counter,
        )
        .unwrap();
        step.append_selected_rejection_owned(&selected, crate::RejectReason::AgentNotFound { agent_id: "target".into() }, 64 << 10, &mut audit_work,
            |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                producer.persist_to(&mut |stream: &mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
                    let len = usize::try_from(stream.original_bytes()).unwrap(); assert!(len <= 64 << 10);
                    let _fee = memory.reserve(len, MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                    let mut bytes = Vec::with_capacity(len); stream.write_to(&mut bytes)?;
                    assert_eq!(fixture.objects.put(&bytes), stream.original_id()); Ok(())
                })
            }
        ).unwrap()
    };
    rejected.validate(&tx).unwrap();
    let rejected_token =
        crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_action_audit(
            &rejected,
        );
    rejected_token.validate(&tx).unwrap();
    assert!(std::ptr::eq(rejected_token.event(), rejected.event()));
    assert!(
        rejected_token.validate(&unrelated).is_err(),
        "Rejected audit token must reject another same-parent transaction"
    );
    token.validate(&tx).unwrap(); // Later real same-transaction contributions do not invalidate the token.

    assert!(audit_work.structure.values_remaining < values_after_accepted);
    assert_eq!(counter.journal_height(&tx).unwrap(), 2);
    use oasis7_world_state::incremental_event_fields::{
        MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1,
    };
    let key = MailboxKeyV1::new("queued-alice", None).unwrap();
    if actor_present {
        let mut read_io = genesis::io();
        let mut reader_values = ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        };
        let root = reader_values
            .load_root(
                &tx.read(StateNamespaceV1::Scheduler, key.as_bytes())
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        let mut bytes = [0; MAILBOX_FRONTIER_BYTES];
        reader_values
            .read_range(
                &root,
                0,
                MAILBOX_FRONTIER_BYTES as u64,
                &mut std::io::Cursor::new(bytes.as_mut_slice()),
                &mut || false,
            )
            .unwrap();
        let frontier = MailboxFrontierV1::from_canonical_bytes(&bytes).unwrap();
        assert_eq!(frontier.tail, 1);
        let item = MailboxKeyV1::new("queued-alice", Some(0)).unwrap();
        assert!(
            tx.read(StateNamespaceV1::Scheduler, item.as_bytes())
                .unwrap()
                .is_some()
        );
    } else {
        assert!(
            tx.read(StateNamespaceV1::Scheduler, key.as_bytes())
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn actual_accepted_audit_routes_original_to_existing_actor_mailbox() {
    accepted_audit_fixture(true, 0);
}
#[test]
fn actual_accepted_audit_missing_actor_persists_without_mailbox() {
    accepted_audit_fixture(false, 0);
}

#[test]
fn actual_accepted_audit_rejects_current_identity_deletion_before_sink() {
    accepted_audit_fixture(true, 1);
}
#[test]
fn actual_accepted_audit_rejects_current_body_deletion_before_sink() {
    accepted_audit_fixture(true, 2);
}
