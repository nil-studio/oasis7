use super::*;

/// These cases exercise provider fallback against durable local history, not
/// background worker scheduling. Drive the real signed consensus/replication
/// tick synchronously so a loaded host cannot expire an unrelated setup timer.
pub(super) fn seed_six_local_commits(config: &NodeConfig) {
    let mut engine = PosNodeEngine::new(config).expect("seed engine");
    let mut replication = super::replication::ReplicationRuntime::new(
        config.replication.as_ref().expect("seed replication config"),
        &config.node_id,
    )
    .expect("seed replication runtime");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut hook = RecordingExecutionHook::new(calls.clone());
    for height in 1_u64..=6 {
        let now = 1_000 + (height - 1) * config.pos_config.slot_duration_ms;
        engine
            .tick(
                &config.node_id,
                &config.world_id,
                i64::try_from(now).expect("fixture timestamp"),
                None,
                Some(&mut replication),
                None,
                None,
                vec![],
                Some(&mut hook),
            )
            .expect("seed signed local commit");
        assert_eq!(engine.committed_height, height);
        assert_eq!(
            replication
                .latest_persisted_commit_height(&config.world_id)
                .unwrap(),
            height,
            "seed must be durably replicated before the provider test"
        );
    }
    assert_eq!(calls.lock().unwrap().len(), 6);

    drop(replication);
    let reopened = super::replication::ReplicationRuntime::new(
        config.replication.as_ref().expect("seed replication config"),
        &config.node_id,
    )
    .expect("reopen seed replication runtime");
    assert_eq!(
        reopened
            .latest_persisted_commit_height(&config.world_id)
            .expect("reopened seed height"),
        6
    );
    for height in 1_u64..=6 {
        let message = reopened
            .load_commit_message_by_height(&config.world_id, height)
            .expect("read reopened seed commit")
            .unwrap_or_else(|| panic!("seed commit {height} must survive reopening"));
        let payload = parse_replication_commit_payload(&message.payload)
            .expect("decode reopened seed commit");
        assert_eq!(payload.height, height);
        assert_eq!(payload.world_id, config.world_id);
        assert_eq!(message.node_id, config.node_id);
        let blob = reopened
            .load_blob_by_hash(&message.record.content_hash)
            .expect("read reopened seed blob")
            .expect("seed commit blob must survive reopening");
        assert_eq!(blob, message.payload);
    }
}
