use super::super::*;
use super::pos;
use oasis7_agent_api::world_action::ResourceKind;

#[test]
fn register_and_move_agent() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(1, 1),
    });
    world.step().unwrap();

    let agent = world.state().agents.get("agent-1").unwrap();
    assert_eq!(agent.state.pos, pos(1, 1));
    assert_eq!(world.journal().len(), 2);
}

#[test]
fn snapshot_and_replay() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
    let snapshot = world.snapshot();

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(2, 2),
    });
    world.step().unwrap();

    let journal = world.journal().clone();
    let restored = World::from_snapshot(snapshot, journal).unwrap();
    assert_eq!(restored.state(), world.state());
}

#[test]
fn rejects_invalid_actions() {
    let mut world = World::new();
    let action_id = world.submit_action(Action::MoveAgent {
        agent_id: "missing".to_string(),
        to: pos(1, 1),
    });
    world.step().unwrap();

    let event = world.journal().events.last().unwrap();
    match &event.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id: id,
            reason,
        }) => {
            assert_eq!(*id, action_id);
            assert!(matches!(reason, RejectReason::AgentNotFound { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn scheduler_round_robin() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-2".to_string(),
        pos: pos(1, 1),
    });
    world.step().unwrap();

    let first = world.schedule_next().unwrap();
    assert_eq!(first.agent_id, "agent-1");
    let second = world.schedule_next().unwrap();
    assert_eq!(second.agent_id, "agent-2");
    assert!(world.schedule_next().is_none());
}

#[test]
fn new_world_migrates_legacy_world_materials_into_material_ledgers() {
    let mut state = WorldState::default();
    state.material_ledgers.clear();
    state.materials.insert("iron_ingot".to_string(), 7);

    let world = World::new_with_state(state);

    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::world(), "iron_ingot"),
        7
    );
    assert_eq!(world.material_balance("iron_ingot"), 7);
}

#[test]
fn action_id_rolls_over_into_next_era() {
    let mut world = World::new();
    let mut snapshot = world.snapshot();
    snapshot.next_action_id = u64::MAX;
    snapshot.action_id_era = 7;

    world = World::from_snapshot(snapshot, world.journal().clone()).expect("restore");

    let first_id = world.submit_action(Action::RegisterAgent {
        agent_id: "agent-max".to_string(),
        pos: pos(0, 0),
    });
    let second_id = world.submit_action(Action::RegisterAgent {
        agent_id: "agent-wrap".to_string(),
        pos: pos(1, 1),
    });

    assert_eq!(first_id, u64::MAX);
    assert_eq!(second_id, 1);
    let rolled = world.snapshot();
    assert_eq!(rolled.action_id_era, 8);
    assert_eq!(rolled.next_action_id, 2);
}

#[test]
fn event_id_rolls_over_into_next_era() {
    let mut world = World::new();
    let mut snapshot = world.snapshot();
    snapshot.last_event_id = u64::MAX;
    snapshot.event_id_era = 3;

    world = World::from_snapshot(snapshot, world.journal().clone()).expect("restore");

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-max".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("step 1");
    assert_eq!(world.journal().events.last().expect("event").id, u64::MAX);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-wrap".to_string(),
        pos: pos(1, 1),
    });
    world.step().expect("step 2");
    assert_eq!(world.journal().events.last().expect("event").id, 1);

    let rolled = world.snapshot();
    assert_eq!(rolled.event_id_era, 4);
    assert_eq!(rolled.last_event_id, 1);
}

#[test]
fn persisted_max_event_id_does_not_double_advance_stored_era() {
    let world = World::new();
    let mut snapshot = world.snapshot();
    snapshot.journal_len = 1;
    snapshot.last_event_id = u64::MAX;
    snapshot.event_id_era = 8;
    snapshot.journal_commitment.clear();
    let mut journal = Journal::new();
    journal.append(WorldEvent {
        id: u64::MAX,
        time: 0,
        caused_by: None,
        body: WorldEventBody::SnapshotCreated(crate::SnapshotMeta { journal_len: 1 }),
    });

    let restored = World::from_snapshot(snapshot, journal).expect("restore max-id checkpoint");
    assert_eq!(restored.snapshot().event_id_era, 8);
}

#[test]
fn adjust_resource_balance_rejects_overflow() {
    let mut world = World::new();
    world.set_resource_balance(ResourceKind::Data, i64::MAX - 1);
    let err = world
        .adjust_resource_balance(ResourceKind::Data, 9)
        .expect_err("overflow should be rejected");

    assert!(
        matches!(err, WorldError::ResourceBalanceInvalid { .. }),
        "unexpected error: {err:?}"
    );
    assert_eq!(world.resource_balance(ResourceKind::Data), i64::MAX - 1);
}

#[test]
fn emit_resource_transfer_overflow_keeps_balances_atomic() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "from".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "to".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agents");
    world
        .set_agent_resource_balance("from", ResourceKind::Data, 10)
        .expect("seed from data");
    world
        .set_agent_resource_balance("to", ResourceKind::Data, i64::MAX)
        .expect("seed to data at boundary");
    world.submit_action(Action::GrantDataAccess {
        owner_agent_id: "from".to_string(),
        grantee_agent_id: "to".to_string(),
    });
    world.step().expect("grant data access");
    let events_before = world.journal().len();

    world.submit_action(Action::EmitResourceTransfer {
        from_agent_id: "from".to_string(),
        to_agent_id: "to".to_string(),
        kind: ResourceKind::Data,
        amount: 1,
    });
    let err = world.step().expect_err("transfer overflow must fail");
    assert!(
        matches!(err, WorldError::ResourceBalanceInvalid { .. }),
        "unexpected error: {err:?}"
    );

    assert_eq!(
        world
            .agent_resource_balance("from", ResourceKind::Data)
            .expect("query from balance"),
        10
    );
    assert_eq!(
        world
            .agent_resource_balance("to", ResourceKind::Data)
            .expect("query to balance"),
        i64::MAX
    );
    assert_eq!(world.journal().len(), events_before);
}

#[test]
fn pending_actions_are_bounded_and_track_evictions() {
    let mut world = World::new().with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_pending_actions: 1,
        ..WorldRuntimeMemoryLimits::default()
    });

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-a".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-b".to_string(),
        pos: pos(1, 1),
    });
    assert_eq!(world.pending_actions_len(), 1);
    assert_eq!(
        world.runtime_backpressure_stats().pending_actions_evicted,
        1
    );

    world.step().expect("step");
    assert!(!world.state().agents.contains_key("agent-a"));
    assert!(world.state().agents.contains_key("agent-b"));
}

#[test]
fn journal_events_are_bounded_and_track_evictions() {
    let mut world = World::new().with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_journal_events: 2,
        ..WorldRuntimeMemoryLimits::default()
    });

    for index in 0..3 {
        world.submit_action(Action::RegisterAgent {
            agent_id: format!("agent-{index}"),
            pos: pos(index as i64, index as i64),
        });
        world.step().expect("step");
    }

    assert_eq!(world.journal().events.len(), 2);
    assert_eq!(world.journal().events[0].id, 2);
    assert_eq!(world.runtime_backpressure_stats().journal_events_evicted, 1);
}

#[test]
fn tick_consensus_records_chain_and_verify_across_steps() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("step 1");
    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(1, 1),
    });
    world.step().expect("step 2");

    let records = world.tick_consensus_records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].block.header.parent_hash, "genesis");
    assert_eq!(
        records[1].block.header.parent_hash,
        records[0].certificate.block_hash
    );
    assert_eq!(records[0].block.header.tick, 1);
    assert_eq!(records[1].block.header.tick, 2);
    assert!(records[0].block.event_count > 0);
    assert!(records[1].block.event_count > 0);

    world
        .verify_tick_consensus_chain()
        .expect("verify consensus chain");
}

#[test]
fn tick_consensus_records_include_empty_tick() {
    let mut world = World::new();
    world.step().expect("step without actions");

    let records = world.tick_consensus_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].block.header.tick, 1);
    assert_eq!(records[0].block.event_count, 0);
    assert!(records[0].block.ordered_event_ids.is_empty());
    world
        .verify_tick_consensus_chain()
        .expect("verify consensus chain");
}

#[test]
fn from_snapshot_replay_rebuilds_missing_tick_consensus_records() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("step 1");
    let snapshot = world.snapshot();
    assert_eq!(snapshot.tick_consensus_records.len(), 1);

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(2, 2),
    });
    world.step().expect("step 2");
    let journal = world.journal().clone();

    let restored = World::from_snapshot(snapshot, journal).expect("restore from snapshot");
    assert_eq!(restored.tick_consensus_records().len(), 2);
    restored
        .verify_tick_consensus_chain()
        .expect("verify rebuilt chain");
}

#[test]
fn tick_consensus_rejects_non_authoritative_submission_after_authority_commit() {
    let mut world = World::new();
    world
        .bind_node_identity("relay.node.1", "relay-public-key")
        .expect("bind relay identity");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("step");

    let err = world
        .record_tick_consensus_propagation_for_tick(1, "relay.node.1")
        .expect_err("non-authoritative overwrite must be rejected");
    assert!(
        matches!(err, WorldError::DistributedValidationFailed { .. }),
        "unexpected error: {err:?}"
    );

    let audit = world
        .tick_consensus_rejection_audit_events()
        .last()
        .expect("rejection audit event");
    assert_eq!(audit.tick, 1);
    assert_eq!(audit.attempted_source, "relay.node.1");
    assert_eq!(
        audit.attempted_role,
        TickConsensusSubmissionRole::Propagation
    );
    assert_eq!(
        audit.existing_role,
        Some(TickConsensusSubmissionRole::Authority)
    );
    assert!(
        audit
            .reason
            .contains("non-authoritative submission rejected"),
        "unexpected audit reason: {}",
        audit.reason
    );
}

#[test]
fn tick_consensus_rejects_authority_submission_from_unconfigured_source() {
    let mut world = World::new();
    world
        .bind_node_identity("authority.alt", "authority-alt-public-key")
        .expect("bind alt authority identity");
    world.step().expect("step");

    let err = world
        .record_tick_consensus_authority_for_tick(1, "authority.alt")
        .expect_err("unconfigured authority source must be rejected");
    assert!(
        matches!(err, WorldError::DistributedValidationFailed { .. }),
        "unexpected error: {err:?}"
    );
    let audit = world
        .tick_consensus_rejection_audit_events()
        .last()
        .expect("rejection audit event");
    assert_eq!(audit.attempted_role, TickConsensusSubmissionRole::Authority);
    assert_eq!(audit.attempted_source, "authority.alt");
    assert!(
        audit
            .reason
            .contains("authority submission source mismatch"),
        "unexpected audit reason: {}",
        audit.reason
    );
}

#[test]
fn tick_consensus_authority_source_can_be_reconfigured_for_new_commits() {
    let mut world = World::new();
    world
        .bind_node_identity("authority.next", "authority-next-public-key")
        .expect("bind authority source");
    world
        .set_tick_consensus_authority_source("authority.next")
        .expect("set authority source");

    world.step().expect("step");
    let record = world
        .latest_tick_consensus_record()
        .expect("tick consensus record");
    assert_eq!(record.block.header.tick, 1);
    assert_eq!(record.certificate.authority_source, "authority.next");
    assert_eq!(
        record.certificate.submission_role,
        TickConsensusSubmissionRole::Authority
    );
    assert!(
        record.certificate.signatures.contains_key("authority.next"),
        "authority signature is missing"
    );
}

#[test]
fn tick_consensus_propagation_conflict_requires_authority_adjudication() {
    let mut world = World::new();
    world
        .bind_node_identity("relay.node.1", "relay-public-key-1")
        .expect("bind relay 1");
    world
        .record_tick_consensus_propagation_for_tick(0, "relay.node.1")
        .expect("seed propagation record");
    world
        .bind_node_identity("relay.node.2", "relay-public-key-2")
        .expect("bind relay 2 to perturb state root");

    let err = world
        .record_tick_consensus_propagation_for_tick(0, "relay.node.2")
        .expect_err("propagation conflict must be rejected");
    assert!(
        matches!(err, WorldError::DistributedValidationFailed { .. }),
        "unexpected error: {err:?}"
    );
    let audit = world
        .tick_consensus_rejection_audit_events()
        .last()
        .expect("rejection audit event");
    assert_eq!(
        audit.attempted_role,
        TickConsensusSubmissionRole::Propagation
    );
    assert_eq!(
        audit.existing_role,
        Some(TickConsensusSubmissionRole::Propagation)
    );
    assert!(
        audit.reason.contains("requires authoritative adjudication"),
        "unexpected audit reason: {}",
        audit.reason
    );
}

#[test]
fn tick_consensus_authority_can_replace_propagation_record_after_conflict() {
    let mut world = World::new();
    world
        .bind_node_identity("relay.node.1", "relay-public-key-1")
        .expect("bind relay");
    world
        .record_tick_consensus_propagation_for_tick(0, "relay.node.1")
        .expect("seed propagation record");
    world
        .bind_node_identity("relay.node.2", "relay-public-key-2")
        .expect("bind relay 2 to perturb state root");

    let authority_source = world.tick_consensus_authority_source().to_string();
    world
        .record_tick_consensus_authority_for_tick(0, authority_source.as_str())
        .expect("authority should adjudicate and replace");

    let record = world
        .tick_consensus_records()
        .iter()
        .find(|record| record.block.header.tick == 0)
        .expect("tick 0 consensus record");
    assert_eq!(
        record.certificate.submission_role,
        TickConsensusSubmissionRole::Authority
    );
    assert_eq!(record.certificate.authority_source, authority_source);
}
