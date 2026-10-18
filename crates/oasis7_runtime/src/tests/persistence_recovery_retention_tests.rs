use super::*;

#[test]
fn snapshot_retention_policy_prunes_old_entries() {
    let mut world = World::new();
    world.set_snapshot_retention(SnapshotRetentionPolicy { max_snapshots: 1 });

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
    let snap1 = world.create_snapshot().unwrap();

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(3, 3),
    });
    world.step().unwrap();
    let snap2 = world.create_snapshot().unwrap();

    assert_eq!(world.snapshot_catalog().records.len(), 1);
    let last_record = &world.snapshot_catalog().records[0];
    assert_eq!(last_record.snapshot_hash, util::hash_json(&snap2).unwrap());
    assert_ne!(last_record.snapshot_hash, util::hash_json(&snap1).unwrap());
}

#[test]
fn snapshot_file_pruning_removes_old_files() {
    let mut world = World::new();
    world.set_snapshot_retention(SnapshotRetentionPolicy { max_snapshots: 1 });

    let dir = std::env::temp_dir().join(format!(
        "oasis7-snapshots-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    world.save_snapshot_to_dir(&dir).unwrap();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
    world.save_snapshot_to_dir(&dir).unwrap();

    let snapshots_dir = dir.join("snapshots");
    let file_count = fs::read_dir(&snapshots_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .count();
    assert_eq!(file_count, 1);

    let _ = fs::remove_dir_all(&dir);
}
