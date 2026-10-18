use super::super::*;

fn local_guardians() -> Vec<String> {
    vec![
        "governance.local.finality.signer.1".to_string(),
        "governance.local.finality.signer.2".to_string(),
    ]
}

#[test]
fn emergency_brake_publication_failure_is_atomic_and_retry_replays() {
    let mut world = World::new();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let root_before = world.current_state_root_hash().expect("initial state root");

    world.fail_next_append_after_publication_prepare_for_test();
    world
        .activate_emergency_brake("guardian-1", "incident", 4, local_guardians())
        .expect_err("injected publication failure must abort emergency-brake activation");
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(
        world.snapshot().last_event_id,
        snapshot_before.last_event_id
    );
    assert_eq!(world.snapshot().event_id_era, snapshot_before.event_id_era);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(world.governance_emergency_brake_until_tick(), None);

    let activation_base = world.snapshot();
    world
        .activate_emergency_brake("guardian-1", "incident", 4, local_guardians())
        .expect("retry emergency-brake activation");
    assert_eq!(world.governance_emergency_brake_until_tick(), Some(4));
    assert_eq!(
        world.current_state_root_hash().expect("activation root"),
        root_before
    );
    world
        .activate_emergency_brake("guardian-1", "still incident", 2, local_guardians())
        .expect("shorter emergency-brake activation");
    assert_eq!(world.governance_emergency_brake_until_tick(), Some(4));

    let release_snapshot_before = world.snapshot();
    let release_journal_before = world.journal().clone();
    let release_consensus_before = world.tick_consensus_records().to_vec();
    world.fail_next_append_after_publication_prepare_for_test();
    world
        .release_emergency_brake("guardian-2", "retry release", local_guardians())
        .expect_err("injected publication failure must abort emergency-brake release");
    assert_eq!(world.snapshot(), release_snapshot_before);
    assert_eq!(
        world.snapshot().last_event_id,
        release_snapshot_before.last_event_id
    );
    assert_eq!(
        world.snapshot().event_id_era,
        release_snapshot_before.event_id_era
    );
    assert_eq!(world.journal(), &release_journal_before);
    assert_eq!(
        world.tick_consensus_records(),
        release_consensus_before.as_slice()
    );
    assert_eq!(world.governance_emergency_brake_until_tick(), Some(4));

    world
        .release_emergency_brake("guardian-2", "resolved", local_guardians())
        .expect("release emergency brake");
    assert_eq!(world.governance_emergency_brake_until_tick(), None);
    assert_eq!(
        world.current_state_root_hash().expect("release root"),
        root_before
    );

    let restored = World::from_snapshot(activation_base, world.journal().clone())
        .expect("replay emergency-brake publication");
    assert_eq!(restored.snapshot(), world.snapshot());
    assert_eq!(restored.journal(), world.journal());
}
