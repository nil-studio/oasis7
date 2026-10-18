use super::super::*;

fn proposal_manifest() -> Manifest {
    Manifest {
        version: 2,
        content: serde_json::json!({ "name": "approval-queue-transaction" }),
    }
}

fn shadowed_proposal_world() -> (World, ProposalId) {
    shadowed_proposal_world_with_limit(None)
}

fn shadowed_proposal_world_with_limit(max_journal_events: Option<usize>) -> (World, ProposalId) {
    let mut world = max_journal_events.map_or_else(World::new, |max_journal_events| {
        World::new().with_runtime_memory_limits(WorldRuntimeMemoryLimits {
            max_journal_events,
            ..WorldRuntimeMemoryLimits::default()
        })
    });
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            timelock_ticks: 2,
            activation_delay_epochs: 3,
            epoch_length_ticks: 5,
            ..GovernanceExecutionPolicy::default()
        })
        .expect("governance timing policy");
    let proposal_id = world
        .propose_manifest_update(proposal_manifest(), "alice")
        .expect("proposal");
    world.shadow_proposal(proposal_id).expect("shadow proposal");
    (world, proposal_id)
}

fn assert_consensus_counters(world: &World) {
    let snapshot = world.snapshot();
    assert_eq!(
        snapshot.tick_consensus_total_record_count,
        world.tick_consensus_records().len() as u64
    );
    assert_eq!(snapshot.tick_consensus_archived_record_count, 0);
}

fn assert_unchanged(
    world: &World,
    snapshot: &Snapshot,
    journal: &Journal,
    backpressure: &WorldRuntimeBackpressureStats,
    consensus: &[TickConsensusRecord],
) {
    assert_eq!(&world.snapshot(), snapshot);
    assert_eq!(world.journal(), journal);
    assert_eq!(world.runtime_backpressure_stats(), backpressure);
    assert_eq!(world.tick_consensus_records(), consensus);
}

#[test]
fn approval_queue_publication_failure_is_atomic_and_retry_matches_sequential_success() {
    let (mut world, proposal_id) = shadowed_proposal_world();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let proposal_before = world.proposals().get(&proposal_id).cloned();

    world.fail_next_append_after_publication_prepare_for_test();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect_err("injected approval/queue publication failure must abort");
    assert_unchanged(
        &world,
        &snapshot_before,
        &journal_before,
        &backpressure_before,
        consensus_before.as_slice(),
    );
    assert_eq!(
        world.proposals().get(&proposal_id).cloned(),
        proposal_before
    );

    let replay_base = world.snapshot();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect("retry approval and queue");
    let proposal = world
        .proposals()
        .get(&proposal_id)
        .expect("queued proposal");
    let manifest_hash = util::hash_json(&proposal.manifest).expect("manifest hash");
    assert_eq!(
        proposal.status,
        ProposalStatus::Approved {
            manifest_hash: manifest_hash.clone(),
            approver: "bob".to_string(),
        }
    );
    assert_eq!(proposal.queued_at_tick, Some(0));
    assert_eq!(proposal.not_before_tick, Some(2));
    assert_eq!(proposal.activate_epoch, Some(3));
    assert_eq!(proposal.timelock_ticks, 2);
    assert_eq!(
        world
            .journal()
            .events
            .iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert!(matches!(
        &world.journal().events[2].body,
        WorldEventBody::Governance(GovernanceEvent::Approved {
            proposal_id: event_proposal_id,
            approver: event_approver,
            decision: ProposalDecision::Approve,
        }) if *event_proposal_id == proposal_id && event_approver == "bob"
    ));
    assert!(matches!(
        &world.journal().events[3].body,
        WorldEventBody::Governance(GovernanceEvent::Queued {
            proposal_id: event_proposal_id,
            manifest_hash: event_manifest_hash,
            queued_at_tick: 0,
            not_before_tick: 2,
            activate_epoch: 3,
            timelock_ticks: 2,
        }) if *event_proposal_id == proposal_id && event_manifest_hash == &manifest_hash
    ));
    assert_eq!(
        world.runtime_backpressure_stats(),
        &WorldRuntimeBackpressureStats::default()
    );
    let consensus = world
        .tick_consensus_records()
        .last()
        .expect("approval consensus");
    assert_eq!(consensus.block.ordered_event_ids, vec![1, 2, 3, 4]);
    assert_eq!(consensus.block.event_count, 4);
    let state_root = world.current_state_root_hash().expect("approval root");
    assert_eq!(consensus.block.header.state_root, state_root);
    assert_eq!(
        consensus.block.execution_digest.state_projection_hash,
        state_root
    );
    assert_consensus_counters(&world);

    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay approval and queue");
    assert_eq!(restored.proposals(), world.proposals());
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_consensus_counters(&restored);
}

#[test]
fn approval_queue_preserves_addressed_snapshot_key_and_replays_mismatch() {
    let (seed, proposal_id) = shadowed_proposal_world();
    let mut snapshot = seed.snapshot();
    let proposal = snapshot
        .proposals
        .remove(&proposal_id)
        .expect("snapshot proposal");
    let addressed_id = proposal_id + 100;
    snapshot.proposals.insert(addressed_id, proposal);
    let mut world = World::from_snapshot(snapshot, seed.journal().clone())
        .expect("restore mismatched proposal key");
    let replay_base = world.snapshot();

    world
        .approve_proposal(addressed_id, "bob", ProposalDecision::Approve)
        .expect("approve mismatched proposal key");
    assert!(!world.proposals().contains_key(&proposal_id));
    assert_eq!(
        world
            .proposals()
            .get(&addressed_id)
            .map(|proposal| proposal.id),
        Some(proposal_id)
    );

    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay mismatched proposal key");
    assert_eq!(restored.proposals(), world.proposals());
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
}

#[test]
fn approval_queue_handles_event_id_rollover_and_replay() {
    let seed = World::new();
    let mut snapshot = seed.snapshot();
    snapshot.last_event_id = u64::MAX - 1;
    snapshot.event_id_era = 7;
    let mut world =
        World::from_snapshot(snapshot, seed.journal().clone()).expect("restore rollover fixture");
    let proposal_id = world
        .propose_manifest_update(proposal_manifest(), "alice")
        .expect("rollover proposal");
    world.shadow_proposal(proposal_id).expect("rollover shadow");
    let replay_base = world.snapshot();

    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect("rollover approval and queue");
    assert_eq!(
        world
            .journal()
            .events
            .iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        vec![u64::MAX, 1, 2, 3]
    );
    assert_eq!(world.snapshot().event_id_era, 8);
    let consensus = world
        .tick_consensus_records()
        .last()
        .expect("rollover consensus");
    assert_eq!(consensus.block.ordered_event_ids, vec![u64::MAX, 1, 2, 3]);
    assert_eq!(consensus.block.event_count, 4);

    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay rollover approval and queue");
    assert_eq!(restored.proposals(), world.proposals());
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_eq!(restored.snapshot().event_id_era, 8);
}

#[test]
fn approval_queue_retention_one_matches_sequential_eviction_and_consensus() {
    let (mut world, proposal_id) = shadowed_proposal_world_with_limit(Some(1));
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let consensus_before = world.tick_consensus_records().to_vec();

    world.fail_next_append_after_publication_prepare_for_test();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect_err("retention-limited failure must abort");
    assert_unchanged(
        &world,
        &snapshot_before,
        &journal_before,
        &backpressure_before,
        consensus_before.as_slice(),
    );

    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect("retention-limited retry");
    assert_eq!(world.runtime_backpressure_stats().journal_events_evicted, 3);
    assert_eq!(world.journal().events.len(), 1);
    assert_eq!(world.journal().events[0].id, 4);
    let proposal = world
        .proposals()
        .get(&proposal_id)
        .expect("queued proposal");
    let manifest_hash = util::hash_json(&proposal.manifest).expect("manifest hash");
    assert_eq!(
        proposal.status,
        ProposalStatus::Approved {
            manifest_hash: manifest_hash.clone(),
            approver: "bob".to_string(),
        }
    );
    assert_eq!(proposal.queued_at_tick, Some(0));
    assert_eq!(proposal.not_before_tick, Some(2));
    assert_eq!(proposal.activate_epoch, Some(3));
    assert_eq!(proposal.timelock_ticks, 2);
    assert!(matches!(
        &world.journal().events[0].body,
        WorldEventBody::Governance(GovernanceEvent::Queued {
            proposal_id: event_proposal_id,
            manifest_hash: event_manifest_hash,
            queued_at_tick: 0,
            not_before_tick: 2,
            activate_epoch: 3,
            timelock_ticks: 2,
        }) if *event_proposal_id == proposal_id && event_manifest_hash == &manifest_hash
    ));
    let consensus = world
        .tick_consensus_records()
        .last()
        .expect("retention consensus");
    assert_eq!(consensus.block.ordered_event_ids, vec![4]);
    assert_eq!(consensus.block.event_count, 1);
    let state_root = world.current_state_root_hash().expect("retention root");
    assert_eq!(consensus.block.header.state_root, state_root);
    assert_eq!(
        consensus.block.execution_digest.state_projection_hash,
        state_root
    );
    assert_consensus_counters(&world);
}

#[test]
fn rejected_approval_is_one_event_and_replays_exactly() {
    let (mut world, proposal_id) = shadowed_proposal_world();
    let replay_base = world.snapshot();
    world
        .approve_proposal(
            proposal_id,
            "bob",
            ProposalDecision::Reject {
                reason: "policy mismatch".to_string(),
            },
        )
        .expect("reject proposal");

    let proposal = world
        .proposals()
        .get(&proposal_id)
        .expect("rejected proposal");
    assert_eq!(
        proposal.status,
        ProposalStatus::Rejected {
            reason: "policy mismatch".to_string()
        }
    );
    assert!(proposal.queued_at_tick.is_none());
    assert!(proposal.not_before_tick.is_none());
    assert!(proposal.activate_epoch.is_none());
    assert_eq!(proposal.timelock_ticks, 0);
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Governance(GovernanceEvent::Approved {
            approver,
            decision: ProposalDecision::Reject { reason },
            ..
        })) if approver == "bob" && reason == "policy mismatch"
    ));
    let consensus = world
        .tick_consensus_records()
        .last()
        .expect("rejection consensus");
    assert_eq!(consensus.block.ordered_event_ids, vec![1, 2, 3]);
    assert_eq!(consensus.block.event_count, 3);
    assert_consensus_counters(&world);

    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay rejected proposal");
    assert_eq!(restored.proposals(), world.proposals());
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_consensus_counters(&restored);
}
