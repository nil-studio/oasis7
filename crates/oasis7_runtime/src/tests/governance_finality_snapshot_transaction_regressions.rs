use super::super::*;

fn snapshot() -> GovernanceFinalityEpochSnapshot {
    GovernanceFinalityEpochSnapshot {
        epoch_id: 0,
        threshold: 2,
        signer_node_ids: vec![
            "governance.local.finality.signer.1".to_string(),
            "governance.local.finality.signer.2".to_string(),
        ],
        ..GovernanceFinalityEpochSnapshot::default()
    }
}

#[test]
fn finality_snapshot_publication_failure_is_atomic_and_retry_replays() {
    let mut world = World::new();
    let proposed = snapshot();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let root_before = world.current_state_root_hash().expect("initial state root");

    world.fail_next_append_after_publication_prepare_for_test();
    world
        .set_governance_finality_epoch_snapshot(proposed.clone())
        .expect_err("injected set publication failure must abort");
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(
        world.snapshot().last_event_id,
        snapshot_before.last_event_id
    );
    assert_eq!(world.snapshot().event_id_era, snapshot_before.event_id_era);
    assert!(world.governance_finality_epoch_snapshots().is_empty());
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());

    let replay_base = world.snapshot();
    world
        .set_governance_finality_epoch_snapshot(proposed.clone())
        .expect("retry set finality snapshot");
    let committed = world
        .governance_finality_epoch_snapshots()
        .get(&0)
        .cloned()
        .expect("committed epoch snapshot");
    assert_eq!(committed.epoch_id, proposed.epoch_id);
    assert_eq!(
        world.current_state_root_hash().expect("set root"),
        root_before
    );

    let mut replacement = committed.clone();
    replacement.threshold_bps = 9_000;
    let replacement_snapshot_before = world.snapshot();
    let replacement_journal_before = world.journal().clone();
    let replacement_consensus_before = world.tick_consensus_records().to_vec();
    world.fail_next_append_after_publication_prepare_for_test();
    world
        .set_governance_finality_epoch_snapshot(replacement.clone())
        .expect_err("injected replacement publication failure must abort");
    assert_eq!(world.snapshot(), replacement_snapshot_before);
    assert_eq!(world.journal(), &replacement_journal_before);
    assert_eq!(
        world.tick_consensus_records(),
        replacement_consensus_before.as_slice()
    );
    assert_eq!(
        world.governance_finality_epoch_snapshots().get(&0),
        Some(&committed)
    );
    world
        .set_governance_finality_epoch_snapshot(replacement.clone())
        .expect("retry same-epoch replacement");
    let replaced = world
        .governance_finality_epoch_snapshots()
        .get(&0)
        .expect("replaced epoch snapshot");
    assert_eq!(replaced.threshold_bps, replacement.threshold_bps);

    let remove_snapshot_before = world.snapshot();
    let remove_journal_before = world.journal().clone();
    let remove_consensus_before = world.tick_consensus_records().to_vec();
    world.fail_next_append_after_publication_prepare_for_test();
    assert!(!world.remove_governance_finality_epoch_snapshot(0));
    assert_eq!(world.snapshot(), remove_snapshot_before);
    assert_eq!(
        world.snapshot().last_event_id,
        remove_snapshot_before.last_event_id
    );
    assert_eq!(
        world.snapshot().event_id_era,
        remove_snapshot_before.event_id_era
    );
    assert!(world.governance_finality_epoch_snapshots().contains_key(&0));
    assert_eq!(world.journal(), &remove_journal_before);
    assert_eq!(
        world.tick_consensus_records(),
        remove_consensus_before.as_slice()
    );

    assert!(world.remove_governance_finality_epoch_snapshot(0));
    assert!(world.governance_finality_epoch_snapshots().is_empty());
    assert_eq!(
        world.current_state_root_hash().expect("remove root"),
        root_before
    );

    let restored = World::from_snapshot(replay_base, world.journal().clone())
        .expect("replay finality snapshot publication");
    assert!(restored.governance_finality_epoch_snapshots().is_empty());
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.current_state_root_hash().expect("restored root"),
        world.current_state_root_hash().expect("live root")
    );
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
}

#[test]
fn unknown_finality_snapshot_remove_is_a_noop() {
    let mut world = World::new();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    assert!(!world.remove_governance_finality_epoch_snapshot(99));
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
}

fn replay_rejects_finality_snapshot_body(body: WorldEventBody) {
    let world = World::new();
    let mut snapshot = world.snapshot();
    snapshot.journal_commitment.clear();
    let journal = Journal {
        events: vec![WorldEvent {
            id: 1,
            time: 0,
            caused_by: None,
            body,
        }],
    };
    let error = World::from_snapshot(snapshot, journal).expect_err("malformed replay must fail");
    assert!(matches!(error, WorldError::GovernancePolicyInvalid { .. }));
}

#[test]
fn malformed_finality_snapshot_replay_is_rejected() {
    let valid = snapshot();
    replay_rejects_finality_snapshot_body(WorldEventBody::Governance(
        GovernanceEvent::FinalityEpochSnapshotSet {
            snapshot: valid.clone(),
            previous: Some(valid.clone()),
        },
    ));

    let mut noncanonical = valid.clone();
    noncanonical.signer_node_ids = vec![
        " governance.local.finality.signer.1 ".to_string(),
        "governance.local.finality.signer.2".to_string(),
    ];
    replay_rejects_finality_snapshot_body(WorldEventBody::Governance(
        GovernanceEvent::FinalityEpochSnapshotSet {
            snapshot: noncanonical,
            previous: None,
        },
    ));
    replay_rejects_finality_snapshot_body(WorldEventBody::Governance(
        GovernanceEvent::FinalityEpochSnapshotRemoved {
            epoch_id: valid.epoch_id,
            snapshot: valid,
        },
    ));
}
