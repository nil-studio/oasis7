fn assert_stale_dht_checkpoint_provenance(
    replication: &ReplicationRuntime,
    world_id: &str,
    checkpoint_height: u64,
    checkpoint_block_hash: &str,
    checkpoint_state_root: &str,
    replication_dir: &std::path::Path,
) {
    let persisted_checkpoint = replication
        .load_commit_message_by_height(world_id, checkpoint_height)
        .expect("load persisted checkpoint")
        .expect("checkpoint persists after bootstrap");
    let persisted_checkpoint =
        super::replication_state_reconcile::parse_replication_commit_payload(
            persisted_checkpoint.payload.as_slice(),
        )
        .expect("decode persisted checkpoint payload");
    assert_eq!(
        persisted_checkpoint.execution_block_hash.as_deref(),
        Some(checkpoint_block_hash)
    );
    assert_eq!(
        persisted_checkpoint.execution_state_root.as_deref(),
        Some(checkpoint_state_root)
    );

    let receipt_path = replication_dir
        .join("checkpoint-verification")
        .join(format!("{checkpoint_height}.json"));
    let receipt: serde_json::Value = serde_json::from_slice(
        fs::read(&receipt_path)
            .expect("read checkpoint verification receipt")
            .as_slice(),
    )
    .expect("decode checkpoint verification receipt");
    assert!(
        receipt["fetch_observations"]
            .as_array()
            .expect("receipt observations")
            .iter()
            .all(|observation| {
                observation["source"].as_str() == Some("network_fetch")
                    && observation["signed_request"].as_bool() == Some(true)
                    && observation["connected_peer_ids"]
                        .as_array()
                        .is_some_and(|candidates| {
                            candidates
                                .iter()
                                .any(|candidate| candidate.as_str() == Some("node-a"))
                        })
            }),
        "checkpoint receipt must retain signed connected-peer provenance: {receipt}"
    );
}

fn peer_head_checkpoint_before_height_one(
    initial_peer_head: InitialPeerHead,
    initial_checkpoint_fetch_available: bool,
) {
    peer_head_checkpoint_before_height_one_with_stale_dht(
        initial_peer_head,
        initial_checkpoint_fetch_available,
        false,
        false,
    );
}

#[test]
fn fresh_observer_fetches_peer_head_checkpoint_before_first_height_one_execution() {
    peer_head_checkpoint_before_height_one(InitialPeerHead::HighCheckpoint, true);
}

#[test]
fn fresh_observer_defers_height_one_until_transient_peer_head_preflight_can_bootstrap() {
    peer_head_checkpoint_before_height_one(InitialPeerHead::Unavailable, true);
}

#[test]
fn fresh_observer_defers_height_one_until_advertised_checkpoint_fetch_recovers() {
    peer_head_checkpoint_before_height_one(InitialPeerHead::HighCheckpoint, false);
}

#[test]
fn fresh_observer_defers_height_one_until_stale_peer_head_advances_to_high_checkpoint() {
    peer_head_checkpoint_before_height_one(InitialPeerHead::StaleHeightOne, true);
}

#[test]
fn fresh_observer_prefers_connected_high_checkpoint_over_stale_dht_height_one() {
    // Also covers testnet #251 when the signed high-checkpoint closure is available.
    peer_head_checkpoint_before_height_one_with_stale_dht(
        InitialPeerHead::HighCheckpoint,
        true,
        true,
        false,
    );
}

#[test]
fn fresh_observer_defers_height_one_when_signed_checkpoint_is_temporarily_not_found() {
    peer_head_checkpoint_before_height_one_with_stale_dht(
        InitialPeerHead::HighCheckpoint,
        false,
        false,
        true,
    );
}

#[test]
fn fresh_observer_with_observed_high_head_defers_height_one_without_checkpoint_closure() {
    peer_head_checkpoint_before_height_one_with_stale_dht(
        InitialPeerHead::UnavailableWithObservedHigh,
        false,
        false,
        true,
    );
}

fn seed_consistent_high_peer_heads(engine: &mut PosNodeEngine, height: u64) {
    let head = PeerCommittedHead {
        height,
        block_hash: format!("block-{height}"),
        committed_at_ms: 6_164,
        observed_at_ms: 6_200,
        execution_block_hash: Some(format!("exec-block-{height}")),
        execution_state_root: Some(format!("exec-state-{height}")),
        action_root: empty_action_root(),
        public_key_hex: None,
        signature_hex: None,
    };
    engine.peer_heads.insert("node-a".to_string(), head.clone());
    engine.peer_heads.insert("node-c".to_string(), head);
}

#[test]
fn fresh_observer_keeps_height_zero_when_connected_high_heads_have_no_checkpoint_closure() {
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let world_id = "world-live-probe-height-zero-persistence";
    let dir_a = temp_dir("live-probe-height-zero-persistence-a");
    let dir_b = temp_dir("live-probe-height-zero-persistence-b");
    let (_, public_key_a) = deterministic_keypair_hex(188);
    let (_, public_key_b) = deterministic_keypair_hex(189);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator {
                validator_id: "node-a".to_string(),
                stake: 50,
            },
            PosValidator {
                validator_id: "node-c".to_string(),
                stake: 50,
            },
        ],
        &[("node-a", 188), ("node-c", 189)],
    );
    let replication_config_a = signed_replication_config(dir_a.clone(), 188)
        .with_remote_writer_allowlist(vec![public_key_b.clone()])
        .expect("allowlist a")
        .with_fetch_requester_allowlist(vec![public_key_b])
        .expect("fetch allowlist a");
    let replication_config_b = signed_replication_config(dir_b.clone(), 189)
        .with_remote_writer_allowlist(vec![public_key_a.clone()])
        .expect("allowlist b")
        .with_fetch_requester_allowlist(vec![public_key_a])
        .expect("fetch allowlist b");
    let config_a = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_replication(replication_config_a.clone());
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(replication_config_b.clone());
    let mut replication_a =
        ReplicationRuntime::new(config_a.replication.as_ref().expect("repl a"), "node-a")
            .expect("runtime a");
    let height_one_message = replication_a
        .build_local_commit_message(
            "node-a",
            world_id,
            8_100,
            &committed_decision(1),
            Some("peer-exec-block-1"),
            Some("peer-exec-state-1"),
        )
        .expect("build height-one message")
        .expect("height-one message");
    let fetch_protocols = Arc::new(Mutex::new(Vec::new()));
    let head = Arc::new(Mutex::new(super::replication::FetchHeadResponse {
        // The first package poll sees a high connected-provider head, but the
        // checkpoint closure is still unavailable. The second poll models a
        // stale DHT/world-head response masking that same higher signed head.
        found: true,
        head: Some(super::replication::ReplicationHeadSummary {
            world_id: world_id.to_string(),
            height: 64,
            block_hash: "block-64".to_string(),
            state_root: "exec-state-64".to_string(),
            timestamp_ms: 8_164,
        }),
    }));
    let checkpoint_fetch_available = Arc::new(AtomicBool::new(false));
    let checkpoint_fetch_not_found = Arc::new(AtomicBool::new(true));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(PeerHeadCheckpointNetwork {
        inner: Arc::new(TestInMemoryNetwork::default()),
        fetch_protocols,
        head: Arc::clone(&head),
        checkpoint_fetch_available,
        checkpoint_fetch_not_found,
        connected_peer_ids: vec!["node-a".to_string(), "node-c".to_string()],
    });
    register_replication_fetch_handlers(
        &NodeReplicationNetworkHandle::new(Arc::clone(&network)),
        &replication_config_a,
        world_id,
        &config_a.network_policy,
    )
    .expect("register provider handlers");
    let handle_b = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let mut endpoint_b =
        ReplicationNetworkEndpoint::new(&handle_b, world_id, true, &config_b.network_policy)
            .expect("fresh observer endpoint");
    let mut replication_b =
        ReplicationRuntime::new(config_b.replication.as_ref().expect("repl b"), "node-b")
            .expect("runtime b");
    let mut engine_b = PosNodeEngine::new(&config_b).expect("fresh observer engine");
    let high_head = |node_id: &str, public_key_hex: String| PeerCommittedHead {
        height: 64,
        block_hash: "block-64".to_string(),
        committed_at_ms: 8_164,
        observed_at_ms: 8_200,
        execution_block_hash: Some("exec-block-64".to_string()),
        execution_state_root: Some("exec-state-64".to_string()),
        action_root: empty_action_root(),
        public_key_hex: Some(public_key_hex),
        signature_hex: Some(format!("signed-{node_id}-64")),
    };
    engine_b.observe_peer_committed_head(
        "node-a",
        high_head("node-a", deterministic_keypair_hex(188).1),
    );
    engine_b.observe_peer_committed_head(
        "node-c",
        high_head("node-c", deterministic_keypair_hex(189).1),
    );
    assert_eq!(
        engine_b.peer_heads.len(),
        2,
        "two signed high peer heads observed"
    );
    assert!(engine_b.peer_heads.values().all(|head| head.height == 64));
    let mut execution_hook = BootstrapBeforeIncrementalHook {
        installed: Vec::new(),
        incremental_commits: Vec::new(),
        rollback_heights: Vec::new(),
    };
    endpoint_b
        .publish_replication(&height_one_message)
        .expect("publish height-one tail");

    engine_b
        .tick(
            "node-b",
            world_id,
            8_300,
            None,
            Some(&mut replication_b),
            Some(&mut endpoint_b),
            None,
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect("unresolved high checkpoint must not execute height one");

    assert_eq!(
        engine_b.committed_height, 0,
        "observer remains at height zero"
    );
    assert_eq!(engine_b.replication_persisted_height, 0);
    assert!(
        execution_hook.incremental_commits.is_empty(),
        "height-one execution must remain deferred while checkpoint closure is unresolved: {:?}",
        execution_hook.incremental_commits
    );
    assert!(
        execution_hook.rollback_heights.is_empty(),
        "no unsigned or unavailable height-zero rollback may be attempted: {:?}",
        execution_hook.rollback_heights
    );
    *head
        .lock()
        .expect("downgrade world head for stale DHT retry") =
        super::replication::FetchHeadResponse {
            found: true,
            head: Some(super::replication::ReplicationHeadSummary {
                world_id: world_id.to_string(),
                height: 1,
                block_hash: "block-1".to_string(),
                state_root: "peer-exec-state-1".to_string(),
                timestamp_ms: 8_100,
            }),
        };
    endpoint_b
        .publish_replication(&height_one_message)
        .expect("republish height-one tail after stale DHT downgrade");
    engine_b
        .tick(
            "node-b",
            world_id,
            8_400,
            None,
            Some(&mut replication_b),
            Some(&mut endpoint_b),
            None,
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect("unresolved high checkpoint must remain fail-closed on retry");
    assert_eq!(
        engine_b.committed_height, 0,
        "observer must remain at height zero across retries"
    );
    assert_eq!(engine_b.replication_persisted_height, 0);
    assert!(
        execution_hook.incremental_commits.is_empty(),
        "height-one execution must not occur on a later retry: {:?}",
        execution_hook.incremental_commits
    );
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn fresh_observer_restart_keeps_height_zero_when_high_peer_heads_are_not_repopulated() {
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let world_id = "world-live-probe-height-zero-restart";
    let dir_a = temp_dir("live-probe-height-zero-restart-a");
    let dir_b = temp_dir("live-probe-height-zero-restart-b");
    let (_, public_key_a) = deterministic_keypair_hex(190);
    let (_, public_key_b) = deterministic_keypair_hex(191);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator {
                validator_id: "node-a".to_string(),
                stake: 50,
            },
            PosValidator {
                validator_id: "node-c".to_string(),
                stake: 50,
            },
        ],
        &[("node-a", 190), ("node-c", 191)],
    );
    let replication_config_a = signed_replication_config(dir_a.clone(), 190)
        .with_remote_writer_allowlist(vec![public_key_b.clone()])
        .expect("allowlist a")
        .with_fetch_requester_allowlist(vec![public_key_b])
        .expect("fetch allowlist a");
    let replication_config_b = signed_replication_config(dir_b.clone(), 191)
        .with_remote_writer_allowlist(vec![public_key_a.clone()])
        .expect("allowlist b")
        .with_fetch_requester_allowlist(vec![public_key_a])
        .expect("fetch allowlist b");
    let config_a = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_replication(replication_config_a.clone());
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(replication_config_b.clone());
    let mut replication_a =
        ReplicationRuntime::new(config_a.replication.as_ref().expect("repl a"), "node-a")
            .expect("runtime a");
    let height_one_message = replication_a
        .build_local_commit_message(
            "node-a",
            world_id,
            9_100,
            &committed_decision(1),
            Some("peer-exec-block-1"),
            Some("peer-exec-state-1"),
        )
        .expect("build height-one message")
        .expect("height-one message");
    let head = Arc::new(Mutex::new(super::replication::FetchHeadResponse {
        found: true,
        head: Some(super::replication::ReplicationHeadSummary {
            world_id: world_id.to_string(),
            height: 64,
            block_hash: "block-64".to_string(),
            state_root: "exec-state-64".to_string(),
            timestamp_ms: 9_164,
        }),
    }));
    let fetch_protocols = Arc::new(Mutex::new(Vec::new()));
    let checkpoint_fetch_available = Arc::new(AtomicBool::new(false));
    let checkpoint_fetch_not_found = Arc::new(AtomicBool::new(true));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(PeerHeadCheckpointNetwork {
        inner: Arc::new(TestInMemoryNetwork::default()),
        fetch_protocols,
        head: Arc::clone(&head),
        checkpoint_fetch_available,
        checkpoint_fetch_not_found,
        connected_peer_ids: vec!["node-a".to_string(), "node-c".to_string()],
    });
    register_replication_fetch_handlers(
        &NodeReplicationNetworkHandle::new(Arc::clone(&network)),
        &replication_config_a,
        world_id,
        &config_a.network_policy,
    )
    .expect("register provider handlers");
    let handle_b = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let mut endpoint_b =
        ReplicationNetworkEndpoint::new(&handle_b, world_id, true, &config_b.network_policy)
            .expect("fresh observer endpoint");
    let mut replication_b =
        ReplicationRuntime::new(config_b.replication.as_ref().expect("repl b"), "node-b")
            .expect("runtime b");
    let mut engine_b = PosNodeEngine::new(&config_b).expect("fresh observer engine");
    let high_head = |public_key_hex: String| PeerCommittedHead {
        height: 64,
        block_hash: "block-64".to_string(),
        committed_at_ms: 9_164,
        observed_at_ms: 9_200,
        execution_block_hash: Some("exec-block-64".to_string()),
        execution_state_root: Some("exec-state-64".to_string()),
        action_root: empty_action_root(),
        public_key_hex: Some(public_key_hex),
        signature_hex: Some("signed-high-head-64".to_string()),
    };
    engine_b.observe_peer_committed_head("node-a", high_head(deterministic_keypair_hex(190).1));
    engine_b.observe_peer_committed_head("node-c", high_head(deterministic_keypair_hex(191).1));
    let mut execution_hook = BootstrapBeforeIncrementalHook {
        installed: Vec::new(),
        incremental_commits: Vec::new(),
        rollback_heights: Vec::new(),
    };
    endpoint_b
        .publish_replication(&height_one_message)
        .expect("publish height-one tail");
    engine_b
        .tick(
            "node-b",
            world_id,
            9_300,
            None,
            Some(&mut replication_b),
            Some(&mut endpoint_b),
            None,
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect("high unresolved checkpoint must establish zero-height hold");
    assert_eq!(engine_b.committed_height, 0);
    assert!(execution_hook.incremental_commits.is_empty());

    // Persist only the durable state; peer_heads and retry markers are
    // intentionally process-local and disappear across restart.
    let state_store = super::pos_state_store::PosNodeStateStore::from_replication(
        config_b.replication.as_ref().expect("repl b config"),
    );
    state_store
        .save_engine_state(&engine_b)
        .expect("persist zero-height high-network snapshot");
    let persisted_snapshot = state_store
        .load()
        .expect("load persisted snapshot")
        .expect("persisted snapshot");
    assert_eq!(persisted_snapshot.committed_height, 0);
    assert_eq!(persisted_snapshot.network_committed_height, 64);

    let mut restarted_engine = PosNodeEngine::new(&config_b).expect("restarted engine");
    restarted_engine
        .restore_state_snapshot(persisted_snapshot, Some(9_400))
        .expect("restore persisted high network snapshot");
    assert_eq!(restarted_engine.committed_height, 0);
    assert_eq!(restarted_engine.network_committed_height, 64);
    assert!(
        restarted_engine.peer_heads.is_empty(),
        "peer heads are not durable in the current snapshot"
    );
    *head.lock().expect("downgrade world head for restart") =
        super::replication::FetchHeadResponse {
            found: true,
            head: Some(super::replication::ReplicationHeadSummary {
                world_id: world_id.to_string(),
                height: 1,
                block_hash: "block-1".to_string(),
                state_root: "peer-exec-state-1".to_string(),
                timestamp_ms: 9_100,
            }),
        };
    let mut restarted_replication = ReplicationRuntime::new(
        config_b.replication.as_ref().expect("repl b restart"),
        "node-b",
    )
    .expect("restart replication runtime");
    let mut restarted_hook = BootstrapBeforeIncrementalHook {
        installed: Vec::new(),
        incremental_commits: Vec::new(),
        rollback_heights: Vec::new(),
    };
    endpoint_b
        .publish_replication(&height_one_message)
        .expect("publish stale height-one tail after restart");
    let result = restarted_engine.tick(
        "node-b",
        world_id,
        9_500,
        None,
        Some(&mut restarted_replication),
        Some(&mut endpoint_b),
        None,
        Vec::new(),
        Some(&mut restarted_hook),
    );
    assert!(
        result.is_ok(),
        "restart must remain fail-closed at height zero, got {result:?}"
    );
    assert_eq!(restarted_engine.committed_height, 0);
    assert!(restarted_hook.incremental_commits.is_empty());
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn fresh_observer_does_not_reenter_height_one_after_high_checkpoint_retry_becomes_stale() {
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let _probe_nonce = CheckpointProbeNonceGuard::install();
    let world_id = "world-live-probe-height-one-reentry-after-retry";
    let dir_a = temp_dir("live-probe-height-one-reentry-after-retry-a");
    let dir_b = temp_dir("live-probe-height-one-reentry-after-retry-b");
    let dir_c = temp_dir("live-probe-height-one-reentry-after-retry-c");
    let (_, public_key_a) = deterministic_keypair_hex(192);
    let (_, public_key_b) = deterministic_keypair_hex(193);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator {
                validator_id: "node-a".to_string(),
                stake: 50,
            },
            PosValidator {
                validator_id: "node-c".to_string(),
                stake: 50,
            },
        ],
        &[("node-a", 192), ("node-c", 193)],
    );
    let replication_config_a = signed_replication_config(dir_a.clone(), 192)
        .with_remote_writer_allowlist(vec![public_key_b.clone()])
        .expect("allowlist a")
        .with_fetch_requester_allowlist(vec![public_key_b])
        .expect("fetch allowlist a");
    let replication_config_b = signed_replication_config(dir_b.clone(), 193)
        .with_remote_writer_allowlist(vec![public_key_a.clone()])
        .expect("allowlist b")
        .with_fetch_requester_allowlist(vec![public_key_a.clone()])
        .expect("fetch allowlist b");
    let replication_config_c = signed_replication_config(dir_c.clone(), 193)
        .with_remote_writer_allowlist(vec![public_key_a.clone()])
        .expect("allowlist c")
        .with_fetch_requester_allowlist(vec![public_key_a.clone()])
        .expect("fetch allowlist c");
    let config_a = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_replication(replication_config_a.clone());
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_pos_config(pos_config.clone())
        .expect("pos config b")
        .with_require_execution_on_commit(true)
        .with_replication(replication_config_b.clone());
    let config_c = NodeConfig::new("node-c", world_id, NodeRole::Sequencer)
        .expect("config c")
        .with_pos_config(pos_config)
        .expect("pos config c")
        .with_replication(replication_config_c);
    let checkpoint_height = 41_620;
    let checkpoint_block_hash = format!("exec-block-{checkpoint_height}");
    let checkpoint_state_root = format!("exec-state-{checkpoint_height}");
    let mut replication_a =
        ReplicationRuntime::new(config_a.replication.as_ref().expect("repl a"), "node-a")
            .expect("runtime a");
    let height_one_message = replication_a
        .build_local_commit_message(
            "node-a",
            world_id,
            10_100,
            &committed_decision(1),
            Some("peer-exec-block-1"),
            Some("peer-exec-state-1"),
        )
        .expect("build early height-one message")
        .expect("early height-one message");
    replication_a
        .build_local_commit_message_with_checkpoint(
            "node-a",
            world_id,
            10_164,
            &committed_decision(checkpoint_height),
            Some(checkpoint_block_hash.as_str()),
            Some(checkpoint_state_root.as_str()),
            Some(checkpoint_bundle(
                checkpoint_height,
                checkpoint_block_hash.as_str(),
                checkpoint_state_root.as_str(),
            )),
        )
        .expect("build high checkpoint closure")
        .expect("high checkpoint message");
    let engine_a = PosNodeEngine::new(&config_a).expect("node-a lineage authority engine");
    let engine_c = PosNodeEngine::new(&config_c).expect("node-c lineage authority engine");
    super::storage_replication_live_retained_boundary_tests::attach_production_lineage_envelope(
        &mut replication_a,
        world_id,
        checkpoint_height,
        CheckpointLineageHeadV1 {
            height: checkpoint_height,
            block_hash: format!("block-{checkpoint_height}"),
            state_root: checkpoint_state_root.clone(),
            execution_block_hash: checkpoint_block_hash.clone(),
            execution_state_root: checkpoint_state_root.clone(),
        },
        &[&engine_a, &engine_c],
    );
    let fetch_protocols = Arc::new(Mutex::new(Vec::new()));
    let head = Arc::new(Mutex::new(super::replication::FetchHeadResponse {
        found: true,
        head: Some(super::replication::ReplicationHeadSummary {
            world_id: world_id.to_string(),
            height: checkpoint_height,
            block_hash: format!("block-{checkpoint_height}"),
            state_root: checkpoint_state_root.clone(),
            timestamp_ms: 10_164,
        }),
    }));
    let checkpoint_fetch_available = Arc::new(AtomicBool::new(false));
    let checkpoint_fetch_not_found = Arc::new(AtomicBool::new(true));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(PeerHeadCheckpointNetwork {
        inner: Arc::new(TestInMemoryNetwork::default()),
        fetch_protocols,
        head: Arc::clone(&head),
        checkpoint_fetch_available: Arc::clone(&checkpoint_fetch_available),
        checkpoint_fetch_not_found: Arc::clone(&checkpoint_fetch_not_found),
        connected_peer_ids: vec!["node-a".to_string(), "node-c".to_string()],
    });
    register_replication_fetch_handlers(
        &NodeReplicationNetworkHandle::new(Arc::clone(&network)),
        &replication_config_a,
        world_id,
        &config_a.network_policy,
    )
    .expect("register high-checkpoint provider handlers");
    let handle_b = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let mut endpoint_b =
        ReplicationNetworkEndpoint::new(&handle_b, world_id, true, &config_b.network_policy)
            .expect("fresh strict observer endpoint");
    let mut replication_b =
        ReplicationRuntime::new(config_b.replication.as_ref().expect("repl b"), "node-b")
            .expect("fresh observer runtime");
    let mut engine_b = PosNodeEngine::new(&config_b).expect("fresh observer engine");
    let high_head = |node_id: &str, public_key_hex: String| PeerCommittedHead {
        height: checkpoint_height,
        block_hash: format!("block-{checkpoint_height}"),
        committed_at_ms: 10_164,
        observed_at_ms: 10_200,
        execution_block_hash: Some(checkpoint_block_hash.clone()),
        execution_state_root: Some(checkpoint_state_root.clone()),
        action_root: empty_action_root(),
        public_key_hex: Some(public_key_hex),
        signature_hex: Some(format!("signed-{node_id}-{checkpoint_height}")),
    };
    // Two connected validators have the same signed high head. Keep this
    // validated peer-head cache separate from network_committed_height to model
    // the live probe's stale world-head downgrade after a failed high probe.
    engine_b.peer_heads.insert(
        "node-a".to_string(),
        high_head("node-a", deterministic_keypair_hex(192).1),
    );
    engine_b.peer_heads.insert(
        "node-c".to_string(),
        high_head("node-c", deterministic_keypair_hex(193).1),
    );
    assert_eq!(engine_b.peer_heads.len(), 2);
    assert!(engine_b.peer_heads.values().all(|head| head.height == checkpoint_height));
    let mut execution_hook = BootstrapBeforeIncrementalHook {
        installed: Vec::new(),
        incremental_commits: Vec::new(),
        rollback_heights: Vec::new(),
    };

    endpoint_b
        .publish_replication(&height_one_message)
        .expect("publish early height-one candidate");
    engine_b
        .tick(
            "node-b",
            world_id,
            10_300,
            None,
            Some(&mut replication_b),
            Some(&mut endpoint_b),
            None,
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect("first high checkpoint probe must hold the observer");
    assert_eq!(engine_b.committed_height, 0);
    assert_eq!(engine_b.replication_persisted_height, 0);
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(
        engine_b.fresh_observer_checkpoint_bootstrap_retry_pending,
        "high checkpoint probe must establish retry authority"
    );

    // The authorized high checkpoint closure becomes available after the
    // transient miss, but the world-head probe now returns an early candidate.
    // Receipt publication must still precede any height-one replay.
    checkpoint_fetch_available.store(true, Ordering::SeqCst);
    checkpoint_fetch_not_found.store(false, Ordering::SeqCst);

    // The next poll receives a stale height-one candidate while the cached
    // validator heads remain higher; the recovered closure must be retried
    // before any height-one replay.
    *head.lock().expect("downgrade world head for stale retry") =
        super::replication::FetchHeadResponse {
            found: true,
            head: Some(super::replication::ReplicationHeadSummary {
                world_id: world_id.to_string(),
                height: 1,
                block_hash: "block-1".to_string(),
                state_root: "peer-exec-state-1".to_string(),
                timestamp_ms: 10_100,
            }),
        };
    endpoint_b
        .publish_replication(&height_one_message)
        .expect("publish reentered stale height-one candidate");
    let result = engine_b
        .tick(
            "node-b",
            world_id,
            10_600,
            None,
            Some(&mut replication_b),
            Some(&mut endpoint_b),
            None,
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect("stale height-one candidate must retry the recovered checkpoint");
    assert_eq!(result.consensus_snapshot.committed_height, checkpoint_height);
    assert!(
        execution_hook.incremental_commits.is_empty(),
        "height-one execution must remain deferred while checkpoint retry is authoritative: {:?}",
        execution_hook.incremental_commits
    );
    assert!(execution_hook.rollback_heights.is_empty());
    assert_eq!(execution_hook.installed, vec![checkpoint_height]);
    assert_eq!(engine_b.committed_height, checkpoint_height);
    assert_eq!(engine_b.replication_persisted_height, checkpoint_height);
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
    let _ = fs::remove_dir_all(&dir_c);
}

struct PackageProbeHeightOneFailureHook {
    incremental_commits: Vec<u64>,
    rollback_heights: Vec<u64>,
}

impl NodeExecutionHook for PackageProbeHeightOneFailureHook {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        self.incremental_commits.push(context.height);
        Err(format!(
            "{} at height {}",
            EXECUTION_MISSING_PREDECESSOR_RECORD_SIGNATURE, context.height
        ))
    }

    fn restore_to_height(&mut self, _world_id: &str, height: u64) -> Result<bool, String> {
        self.rollback_heights.push(height);
        Ok(false)
    }
}

#[test]
fn fresh_observer_package_probe_keeps_height_zero_after_probe_window_candidate_ingest() {
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let _probe_nonce = CheckpointProbeNonceGuard::install();
    let world_id = "world-live-package-probe-height-one-reentry";
    let dir_a = temp_dir("live-package-probe-height-one-reentry-a");
    let dir_b = temp_dir("live-package-probe-height-one-reentry-b");
    let dir_c = temp_dir("live-package-probe-height-one-reentry-c");
    let (_, public_key_a) = deterministic_keypair_hex(204);
    let (_, public_key_b) = deterministic_keypair_hex(205);
    let (_, public_key_c) = deterministic_keypair_hex(206);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator {
                validator_id: "node-a".to_string(),
                stake: 50,
            },
            PosValidator {
                validator_id: "node-c".to_string(),
                stake: 50,
            },
        ],
        &[("node-a", 204), ("node-c", 206)],
    );
    let replication_config_a = signed_replication_config(dir_a.clone(), 204)
        .with_remote_writer_allowlist(vec![public_key_b.clone()])
        .expect("allowlist a")
        .with_fetch_requester_allowlist(vec![public_key_b.clone()])
        .expect("fetch allowlist a");
    let replication_config_b = signed_replication_config(dir_b.clone(), 205)
        .with_remote_writer_allowlist(vec![public_key_a.clone(), public_key_c.clone()])
        .expect("allowlist b")
        .with_fetch_requester_allowlist(vec![public_key_a.clone(), public_key_c.clone()])
        .expect("fetch allowlist b");
    let replication_config_c = signed_replication_config(dir_c.clone(), 206)
        .with_remote_writer_allowlist(vec![public_key_b.clone()])
        .expect("allowlist c")
        .with_fetch_requester_allowlist(vec![public_key_b])
        .expect("fetch allowlist c");
    let config_a = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_replication(replication_config_a.clone());
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_pos_config(pos_config.clone())
        .expect("pos config b")
        .with_require_execution_on_commit(true)
        .with_replication(replication_config_b.clone());
    let config_c = NodeConfig::new("node-c", world_id, NodeRole::Sequencer)
        .expect("config c")
        .with_pos_config(pos_config)
        .expect("pos config c")
        .with_replication(replication_config_c.clone());
    let mut replication_a =
        ReplicationRuntime::new(config_a.replication.as_ref().expect("repl a"), "node-a")
            .expect("runtime a");
    let mut replication_c =
        ReplicationRuntime::new(config_c.replication.as_ref().expect("repl c"), "node-c")
            .expect("runtime c");
    let height_one_a = replication_a
        .build_local_commit_message(
            "node-a",
            world_id,
            12_100,
            &committed_decision(1),
            Some("peer-exec-block-1"),
            Some("peer-exec-state-1"),
        )
        .expect("build node-a height-one candidate")
        .expect("node-a height-one candidate");
    let height_one_c = replication_c
        .build_local_commit_message(
            "node-c",
            world_id,
            12_101,
            &PosDecision {
                proposer_id: "node-c".to_string(),
                ..committed_decision(1)
            },
            Some("peer-exec-block-1"),
            Some("peer-exec-state-1"),
        )
        .expect("build node-c height-one candidate")
        .expect("node-c height-one candidate");
    let checkpoint_height = 43_340;
    let fetch_protocols = Arc::new(Mutex::new(Vec::new()));
    let head = Arc::new(Mutex::new(super::replication::FetchHeadResponse {
        found: true,
        head: Some(super::replication::ReplicationHeadSummary {
            world_id: world_id.to_string(),
            // The DHT and connected world-head route remain stale at height
            // one even though the observer's validated peer-head cache below
            // contains two consistent high validator heads.
            height: 1,
            block_hash: "block-1".to_string(),
            state_root: "peer-exec-state-1".to_string(),
            timestamp_ms: 12_100,
        }),
    }));
    let checkpoint_fetch_available = Arc::new(AtomicBool::new(false));
    let checkpoint_fetch_not_found = Arc::new(AtomicBool::new(true));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(PeerHeadCheckpointNetwork {
        inner: Arc::new(TestInMemoryNetwork::default()),
        fetch_protocols,
        head: Arc::clone(&head),
        checkpoint_fetch_available,
        checkpoint_fetch_not_found,
        connected_peer_ids: vec!["node-a".to_string(), "node-c".to_string()],
    });
    // The package probe talks to both connected validators. The final handler
    // registration keeps both connected candidates available while the test
    // delivers a height-one tail only after the checkpoint probe window.
    register_replication_fetch_handlers(
        &NodeReplicationNetworkHandle::new(Arc::clone(&network)),
        &replication_config_a,
        world_id,
        &config_a.network_policy,
    )
    .expect("register node-a provider handlers");
    register_replication_fetch_handlers(
        &NodeReplicationNetworkHandle::new(Arc::clone(&network)),
        &replication_config_c,
        world_id,
        &config_c.network_policy,
    )
    .expect("register node-c provider handlers");
    let handle_b = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let mut endpoint_b =
        ReplicationNetworkEndpoint::new(&handle_b, world_id, true, &config_b.network_policy)
            .expect("fresh observer endpoint");
    let mut replication_b =
        ReplicationRuntime::new(config_b.replication.as_ref().expect("repl b"), "node-b")
            .expect("fresh observer runtime");
    let mut engine_b = PosNodeEngine::new(&config_b).expect("fresh observer engine");
    let high_head = |node_id: &str, public_key_hex: String| PeerCommittedHead {
        height: checkpoint_height,
        block_hash: format!("block-{checkpoint_height}"),
        committed_at_ms: 12_164,
        observed_at_ms: 12_200,
        execution_block_hash: Some(format!("exec-block-{checkpoint_height}")),
        execution_state_root: Some(format!("exec-state-{checkpoint_height}")),
        action_root: empty_action_root(),
        public_key_hex: Some(public_key_hex),
        signature_hex: Some(format!("signed-{node_id}-{checkpoint_height}")),
    };
    engine_b.peer_heads.insert(
        "node-a".to_string(),
        high_head("node-a", deterministic_keypair_hex(204).1),
    );
    engine_b.peer_heads.insert(
        "node-c".to_string(),
        high_head("node-c", deterministic_keypair_hex(206).1),
    );
    assert_eq!(engine_b.peer_heads.len(), 2);
    assert!(engine_b
        .peer_heads
        .values()
        .all(|head| head.height == checkpoint_height));
    let mut execution_hook = PackageProbeHeightOneFailureHook {
        incremental_commits: Vec::new(),
        rollback_heights: Vec::new(),
    };

    // Probe ordering: both validators have the same signed high head in the
    // validated cache, but the stale world-head route yields height one and
    // checkpoint closure is unavailable. The first poll establishes the
    // low-head confirmation while no candidate is yet queued.
    engine_b
        .tick(
            "node-b",
            world_id,
            12_300,
            None,
            Some(&mut replication_b),
            Some(&mut endpoint_b),
            None,
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect("initial unavailable checkpoint probe must hold height zero");
    assert_eq!(engine_b.committed_height, 0);
    assert_eq!(engine_b.replication_persisted_height, 0);
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(execution_hook.rollback_heights.is_empty());

    // Let the probe window elapse with no checkpoint receipt. This clears the
    // one-poll low-head confirmation while preserving the cached high quorum.
    engine_b
        .tick(
            "node-b",
            world_id,
            312_300,
            None,
            Some(&mut replication_b),
            Some(&mut endpoint_b),
            None,
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect("300s unavailable checkpoint probe must hold height zero");
    assert_eq!(engine_b.committed_height, 0);
    assert_eq!(engine_b.replication_persisted_height, 0);
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(execution_hook.rollback_heights.is_empty());
    assert!(!dir_b
        .join("checkpoint-verification")
        .join(format!("{checkpoint_height}.json"))
        .exists());

    // A height-one candidate arrives only after the probe window. Current
    // code overwrites the high cached heads while ingesting it and can
    // re-enter incremental execution without a verified checkpoint receipt.
    endpoint_b
        .publish_replication(&height_one_a)
        .expect("publish node-a height-one candidate after probe window");
    let result = engine_b.tick(
        "node-b",
        world_id,
        312_301,
        None,
        Some(&mut replication_b),
        Some(&mut endpoint_b),
        None,
        Vec::new(),
        Some(&mut execution_hook),
    );
    assert!(
        result.is_ok(),
        "fresh package observer must stay at height zero: result={result:?} incremental={:?} rollback={:?}",
        execution_hook.incremental_commits,
        execution_hook.rollback_heights
    );
    let snapshot = result.expect("height-zero package probe snapshot");
    assert_eq!(snapshot.consensus_snapshot.committed_height, 0);
    assert_eq!(engine_b.committed_height, 0);
    assert_eq!(engine_b.replication_persisted_height, 0);
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(execution_hook.rollback_heights.is_empty());

    // A second height-one candidate arrives in the next gossip window. The
    // first candidate has already overwritten one cached high head; replaying
    // this candidate must still remain fail-closed while the other connected
    // high head has no verified checkpoint receipt.
    endpoint_b
        .publish_replication(&height_one_c)
        .expect("publish node-c height-one candidate after probe window");
    let retry_result = engine_b.tick(
        "node-b",
        world_id,
        312_302,
        None,
        Some(&mut replication_b),
        Some(&mut endpoint_b),
        None,
        Vec::new(),
        Some(&mut execution_hook),
    );
    assert!(
        retry_result.is_ok(),
        "fresh package observer must stay at height zero on second candidate: result={retry_result:?} incremental={:?} rollback={:?}",
        execution_hook.incremental_commits,
        execution_hook.rollback_heights
    );
    let retry_snapshot = retry_result.expect("height-zero second-candidate snapshot");
    assert_eq!(retry_snapshot.consensus_snapshot.committed_height, 0);
    assert_eq!(engine_b.committed_height, 0);
    assert_eq!(engine_b.replication_persisted_height, 0);
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(execution_hook.rollback_heights.is_empty());

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
    let _ = fs::remove_dir_all(&dir_c);
}
