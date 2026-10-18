//! Mechanical retention/restore tests use explicit cfg(test) parent tokens.
//! They do not attest external authority or provider trust.
use super::*;
#[test]
fn formal_tick_prefix_keeps_bounded_hot_tail_restores_and_queries_original_records() {
    let mut world = World::new();
    world.initialize_empty_formal_history().unwrap();
    let nodes = SharedMemoryNodes::default();
    attach_test_parent(&mut world, Arc::new(nodes.clone()));
    world.step().unwrap();
    let original = world.latest_tick_consensus_record().unwrap().clone();
    for _ in 0..74 {
        attach_test_parent(&mut world, Arc::new(nodes.clone()));
        world.step().unwrap();
        let plan = world.prepare_formal_retention_candidate().unwrap();
        persist_retention_delta(&nodes, &plan);
        world.apply_formal_retention_plan(&plan).unwrap();
    }
    assert_eq!(world.tick_consensus_record_count(), 75);
    assert_eq!(world.tick_consensus_records().len(), 32);
    assert_eq!(
        world.read_tick_consensus_record(1).unwrap(),
        Some(original.clone())
    );
    assert_eq!(
        world
            .read_tick_consensus_at_tick(original.block.header.tick)
            .unwrap(),
        Some(original.clone())
    );
    let snapshot = world.snapshot();
    assert_eq!(snapshot.tick_consensus_archived_record_count, 43);
    assert_eq!(snapshot.tick_consensus_total_record_count, 75);
    let bytes = serde_cbor::to_vec(&snapshot).unwrap();
    let parent = VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(
        world.state.retained_history_root.clone().unwrap(),
        &bytes,
    );
    assert!(World::from_snapshot(snapshot.clone(), world.journal.clone()).is_err());
    let restored = World::from_verified_formal_snapshot(
        snapshot.clone(),
        world.journal.clone(),
        parent.clone(),
        Arc::new(nodes.clone()),
    )
    .unwrap();
    assert_eq!(restored.tick_consensus_records().len(), 32);
    assert_eq!(
        restored.read_tick_consensus_record(1).unwrap(),
        Some(original)
    );
    assert!(
        World::from_verified_formal_snapshot(
            snapshot.clone(),
            world.journal.clone(),
            parent.clone(),
            Arc::new(EmptyNodes)
        )
        .is_err()
    );
    assert!(
        World::from_verified_formal_snapshot(
            snapshot,
            world.journal.clone(),
            parent,
            Arc::new(CorruptNodes)
        )
        .is_err()
    );
}
#[test]
fn formal_tick_prefix_rejects_rewritten_qualified_record_before_archive() {
    let mut world = World::new();
    world.initialize_empty_formal_history().unwrap();
    let nodes = SharedMemoryNodes::default();
    for _ in 0..35 {
        world.step().unwrap();
    }
    attach_test_parent(&mut world, Arc::new(nodes.clone()));
    let parent = world
        .formal_history_reader
        .as_ref()
        .unwrap()
        .parent
        .clone()
        .unwrap();
    world.step().unwrap();
    world.tick_consensus_records[0]
        .certificate
        .signatures
        .clear();
    assert!(
        world
            .prepare_formal_tick_consensus_archive(&parent, &nodes)
            .is_err()
    );
}
