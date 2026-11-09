#[path = "tests_storage_challenge_gate.rs"]
mod storage_challenge_gate_tests;
#[path = "tests_storage_replication_failover.rs"]
mod storage_replication_failover_tests;
#[path = "tests_storage_replication_core.rs"]
mod storage_replication_core_tests;
#[path = "tests_storage_replication_high_checkpoint.rs"]
mod storage_replication_high_checkpoint_tests;
#[path = "tests_storage_replication_first_ready_checkpoint.rs"]
mod storage_replication_first_ready_checkpoint_tests;
#[path = "tests_storage_replication_checkpoint_fallback.rs"]
mod storage_replication_checkpoint_fallback_tests;
#[path = "tests_storage_replication_high_checkpoint_retained.rs"]
mod storage_replication_high_checkpoint_retained_tests;
#[path = "tests_storage_replication_live_retained_boundary.rs"]
mod storage_replication_live_retained_boundary_tests;
#[path = "tests_storage_replication_high_checkpoint_timeout.rs"]
mod storage_replication_high_checkpoint_timeout_tests;
#[path = "tests_storage_replication_peer_head.rs"]
mod storage_replication_peer_head_tests;
#[path = "tests_storage_replication_progress.rs"]
mod storage_replication_progress_tests;
#[path = "tests_storage_replication_diagnostics.rs"]
mod storage_replication_diagnostics;
#[path = "tests_storage_replication_recovery.rs"]
mod storage_replication_recovery_tests;

#[test]
fn runtime_network_replication_gap_sync_reports_error_after_retries_exhausted() {
    let world_id = "world-network-gap-retry-exhausted";
    let dir_a = temp_dir("network-gap-retry-a");
    let dir_b = temp_dir("network-gap-retry-b");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let pos_config =
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 89), ("node-b", 90)]);
    let network_impl = Arc::new(TestInMemoryNetwork::default());
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();

    let config_a = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config a")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir_a.clone(), 89));
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(signed_replication_config(dir_b.clone(), 90));

    let mut runtime_a = with_noop_execution_hook(NodeRuntime::new(config_a))
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime_a.start().expect("start a");
    let reached = wait_until(Instant::now() + Duration::from_secs(2), || {
        runtime_a.snapshot().consensus.committed_height >= 3
    });
    let snapshot_a = runtime_a.snapshot();
    assert!(
        reached,
        "sequencer did not reach target height in time: committed_height={} network_committed_height={} last_error={:?}",
        snapshot_a.consensus.committed_height,
        snapshot_a.consensus.network_committed_height,
        snapshot_a.last_error
    );
    let target_height = snapshot_a.consensus.committed_height;
    runtime_a.stop().expect("stop a");

    let request = signed_fetch_commit_request_for_test(world_id, target_height, 89);
    let payload = serde_json::to_vec(&request).expect("encode commit request");
    let response_payload = network
        .request(
            super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL,
            payload.as_slice(),
        )
        .expect("fetch commit");
    let response: super::replication::FetchCommitResponse =
        serde_json::from_slice(&response_payload).expect("decode commit response");
    assert!(response.found, "missing high commit");
    let high_message = response.message.expect("high commit payload");

    let topic = super::network_bridge::default_replication_topic(world_id);
    network_impl.clear_topic(topic.as_str());
    network_impl
        .clear_topic(super::network_bridge::default_consensus_proposal_topic(world_id).as_str());
    network_impl
        .clear_topic(super::network_bridge::default_consensus_attestation_topic(world_id).as_str());
    network_impl
        .clear_topic(super::network_bridge::default_consensus_commit_topic(world_id).as_str());
    let high_payload = serde_json::to_vec(&high_message).expect("encode high message");
    network
        .publish(topic.as_str(), high_payload.as_slice())
        .expect("publish high message");

    let mut runtime_b = NodeRuntime::new(config_b)
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime_b.start().expect("start b");
    network
        .register_handler(
            super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL,
            Box::new(move |_payload| {
                Err(WorldError::NetworkProtocolUnavailable {
                    protocol: "forced-gap-sync-retry-failure".to_string(),
                })
            }),
        )
        .expect("register commit retry-failure handler");
    let errored = wait_until(Instant::now() + Duration::from_secs(3), || {
        runtime_b
            .snapshot()
            .last_error
            .as_deref()
            .map(|reason| {
                reason.contains("gap sync height")
                    && reason.contains("failed after 3 attempts")
                    && reason.contains("attempt 3/3 failed")
            })
            .unwrap_or(false)
    });
    let snapshot_b = runtime_b.snapshot();
    assert!(
        errored,
        "observer did not report gap sync retry exhaustion: committed_height={} network_committed_height={} last_error={:?}",
        snapshot_b.consensus.committed_height,
        snapshot_b.consensus.network_committed_height,
        snapshot_b.last_error
    );

    runtime_b.stop().expect("stop b");
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn replication_gap_sync_backfills_when_consensus_height_already_advanced() {
    let world_id = "world-gap-sync-consensus-ahead";
    let dir_a = temp_dir("gap-sync-consensus-ahead-a");
    let dir_b = temp_dir("gap-sync-consensus-ahead-b");
    let (_, public_key_a) = deterministic_keypair_hex(143);
    let (_, public_key_b) = deterministic_keypair_hex(144);
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let pos_config =
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 143), ("node-b", 144)]);
    let replication_config_a = signed_replication_config(dir_a.clone(), 143)
        .with_remote_writer_allowlist(vec![public_key_b.clone()])
        .expect("allowlist a")
        .with_fetch_requester_allowlist(vec![public_key_b.clone()])
        .expect("fetch allowlist a");
    let replication_config_b = signed_replication_config(dir_b.clone(), 144)
        .with_remote_writer_allowlist(vec![public_key_a.clone()])
        .expect("allowlist b")
        .with_fetch_requester_allowlist(vec![public_key_a.clone()])
        .expect("fetch allowlist b");
    let config_a = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_replication(replication_config_a.clone());
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Storage)
        .expect("config b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(replication_config_b.clone());

    let mut replication_a =
        ReplicationRuntime::new(config_a.replication.as_ref().expect("repl a"), "node-a")
            .expect("runtime a");
    for height in 1..=3 {
        let decision = PosDecision {
            height,
            slot: height,
            epoch: 0,
            proposer_id: "node-a".to_string(),
            status: PosConsensusStatus::Committed,
            block_hash: format!("block-{height}"),
            action_root: empty_action_root(),
            committed_actions: Vec::new(),
            approved_stake: 60,
            rejected_stake: 0,
            required_stake: 40,
            total_stake: 100,
        };
        replication_a
            .build_local_commit_message(
                "node-a",
                world_id,
                1_000 + i64::try_from(height).expect("height fits i64"),
                &decision,
                None,
                None,
            )
            .expect("build local message")
            .expect("message");
    }

    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(TestInMemoryNetwork::default());
    let handle_a = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    register_replication_fetch_handlers(
        &handle_a,
        config_a.replication.as_ref().expect("repl a"),
        world_id,
        &config_a.network_policy,
    )
    .expect("register fetch handlers");

    let handle_b = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let endpoint_b =
        ReplicationNetworkEndpoint::new(&handle_b, world_id, false, &config_b.network_policy)
            .expect("endpoint b");
    let mut replication_b =
        ReplicationRuntime::new(config_b.replication.as_ref().expect("repl b"), "node-b")
            .expect("runtime b");
    let mut engine_b = PosNodeEngine::new(&config_b).expect("engine b");
    engine_b.committed_height = 3;
    engine_b.network_committed_height = 3;
    engine_b.next_height = 4;
    engine_b.last_committed_block_hash = Some("block-3".to_string());

    assert_eq!(
        replication_b
            .latest_persisted_commit_height(world_id)
            .expect("initial persisted height"),
        0
    );
    engine_b
        .sync_missing_replication_commits(
            &endpoint_b,
            "node-b",
            world_id,
            Some(&mut replication_b),
            None,
        )
        .expect("gap sync");

    assert!(replication_b
        .load_commit_message_by_height(world_id, 1)
        .expect("load commit 1")
        .is_some());
    assert!(replication_b
        .load_commit_message_by_height(world_id, 3)
        .expect("load commit 3")
        .is_some());
    assert_eq!(
        replication_b
            .latest_persisted_commit_height(world_id)
            .expect("persisted height after sync"),
        3
    );
    assert_eq!(engine_b.committed_height, 3);
    assert_eq!(engine_b.next_height, 4);

    let store_b = LocalCasStore::new(dir_b.join("store"));
    assert!(
        store_b
            .list_files()
            .expect("list files")
            .iter()
            .any(|item| item.path == "consensus/commits/00000000000000000003.json"),
        "expected synced commit file to be present"
    );

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn proposal_ahead_updates_replication_gap_sync_target_height() {
    let world_id = "world-gap-sync-proposal-ahead";
    let dir_a = temp_dir("gap-sync-proposal-ahead-a");
    let dir_b = temp_dir("gap-sync-proposal-ahead-b");
    let (private_hex_a, public_key_a) = deterministic_keypair_hex(154);
    let (_, public_key_b) = deterministic_keypair_hex(155);
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let pos_config =
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 154), ("node-b", 155)]);
    let replication_config_a = signed_replication_config(dir_a.clone(), 154)
        .with_remote_writer_allowlist(vec![public_key_b.clone()])
        .expect("allowlist a")
        .with_fetch_requester_allowlist(vec![public_key_b.clone()])
        .expect("fetch allowlist a");
    let replication_config_b = signed_replication_config(dir_b.clone(), 155)
        .with_remote_writer_allowlist(vec![public_key_a.clone()])
        .expect("allowlist b")
        .with_fetch_requester_allowlist(vec![public_key_a.clone()])
        .expect("fetch allowlist b");
    let config_a = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_replication(replication_config_a.clone());
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Storage)
        .expect("config b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(replication_config_b.clone());

    let mut replication_a =
        ReplicationRuntime::new(config_a.replication.as_ref().expect("repl a"), "node-a")
            .expect("runtime a");
    for height in 1..=3 {
        let decision = PosDecision {
            height,
            slot: height,
            epoch: 0,
            proposer_id: "node-a".to_string(),
            status: PosConsensusStatus::Committed,
            block_hash: format!("block-{height}"),
            action_root: empty_action_root(),
            committed_actions: Vec::new(),
            approved_stake: 60,
            rejected_stake: 0,
            required_stake: 40,
            total_stake: 100,
        };
        replication_a
            .build_local_commit_message(
                "node-a",
                world_id,
                2_000 + i64::try_from(height).expect("height fits i64"),
                &decision,
                None,
                None,
            )
            .expect("build local message")
            .expect("message");
    }

    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(TestInMemoryNetwork::default());
    let handle_a = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    register_replication_fetch_handlers(
        &handle_a,
        config_a.replication.as_ref().expect("repl a"),
        world_id,
        &config_a.network_policy,
    )
    .expect("register fetch handlers");

    let handle_b = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let endpoint_b =
        ReplicationNetworkEndpoint::new(&handle_b, world_id, false, &config_b.network_policy)
            .expect("endpoint b");
    let mut replication_b =
        ReplicationRuntime::new(config_b.replication.as_ref().expect("repl b"), "node-b")
            .expect("runtime b");
    let mut engine_b = PosNodeEngine::new(&config_b).expect("engine b");

    let signing_key_a = SigningKey::from_bytes(
        &hex::decode(private_hex_a)
            .expect("proposal private decode")
            .try_into()
            .expect("proposal private len"),
    );
    let proposal_signer =
        ConsensusMessageSigner::new(signing_key_a, public_key_a).expect("proposal signer");
    let mut proposal = GossipProposalMessage {
        version: 1,
        world_id: world_id.to_string(),
        node_id: "node-a".to_string(),
        player_id: "node-a".to_string(),
        proposer_id: "node-a".to_string(),
        height: 4,
        slot: 4,
        epoch: 0,
        block_hash: format!("{world_id}:h4:s4:pnode-a"),
        action_root: empty_action_root(),
        actions: Vec::new(),
        proposed_at_ms: 4_000,
        public_key_hex: None,
        signature_hex: None,
    };
    sign_proposal_message(&mut proposal, &proposal_signer).expect("sign proposal");

    engine_b
        .ingest_proposal_message(world_id, &proposal, proposal.slot)
        .expect("ingest proposal");
    assert_eq!(engine_b.network_committed_height, 3);

    engine_b
        .sync_missing_replication_commits(
            &endpoint_b,
            "node-b",
            world_id,
            Some(&mut replication_b),
            None,
        )
        .expect("gap sync");

    assert_eq!(
        replication_b
            .latest_persisted_commit_height(world_id)
            .expect("persisted height after sync"),
        3
    );
    assert_eq!(engine_b.committed_height, 3);
    assert_eq!(engine_b.network_committed_height, 3);
    assert!(replication_b
        .load_commit_message_by_height(world_id, 3)
        .expect("load commit 3")
        .is_some());

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}


#[test]
fn observer_replication_runtime_starts_without_registering_data_service_handlers() {
    let world_id = "world-observer-lane-gate";
    let dir = temp_dir("observer-lane-gate");
    let network_impl = Arc::new(TestInMemoryNetwork::default());
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 91)],
    );
    let config = NodeConfig::new("node-observer", world_id, NodeRole::Observer)
        .expect("config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 92));

    let mut runtime = with_noop_execution_hook(NodeRuntime::new(config))
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime.start().expect("start runtime");

    let commit_request = signed_fetch_commit_request_for_test(world_id, 1, 92);
    let commit_payload = serde_json::to_vec(&commit_request).expect("encode commit request");
    let commit_err = network
        .request(
            super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL,
            commit_payload.as_slice(),
        )
        .expect_err("observer should not serve commit sync protocol");
    assert!(matches!(
        commit_err,
        WorldError::NetworkProtocolUnavailable { .. }
    ));

    let blob_request = signed_fetch_blob_request_for_test("content-hash-1", 92);
    let blob_payload = serde_json::to_vec(&blob_request).expect("encode blob request");
    let blob_err = network
        .request(
            super::replication::REPLICATION_FETCH_BLOB_PROTOCOL,
            blob_payload.as_slice(),
        )
        .expect_err("observer should not serve blob/state protocol");
    assert!(matches!(
        blob_err,
        WorldError::NetworkProtocolUnavailable { .. }
    ));

    runtime.stop().expect("stop runtime");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn storage_validator_core_local_replication_advances_persisted_height() {
    let world_id = "world-storage-validator-local-replication";
    let dir = temp_dir("storage-validator-local-replication");
    let validators = vec![PosValidator {
        validator_id: "node-storage".to_string(),
        stake: 100,
    }];
    let pos_config = signed_pos_config_with_signer_seeds(validators, &[("node-storage", 58)]);
    let config = NodeConfig::new("node-storage", world_id, NodeRole::Storage)
        .expect("config")
        .with_network_policy(NodeNetworkPolicy {
            deployment_mode: oasis7_proto::distributed_dht::PeerDeploymentMode::Private,
            node_role_claim: oasis7_proto::distributed_dht::PeerNodeRole::ValidatorCore,
        })
        .expect("storage validator-core policy")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 58));
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.last_execution_height = 1;
    engine.last_execution_block_hash = Some("exec-block-1".to_string());
    engine.last_execution_state_root = Some("exec-state-1".to_string());

    let mut replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("replication"),
        "node-storage",
    )
    .expect("replication runtime");
    let decision = PosDecision {
        height: 1,
        slot: 0,
        epoch: 0,
        proposer_id: "node-a".to_string(),
        status: PosConsensusStatus::Committed,
        block_hash: "block-1".to_string(),
        action_root: empty_action_root(),
        committed_actions: Vec::new(),
        approved_stake: 100,
        rejected_stake: 0,
        required_stake: 67,
        total_stake: 100,
    };

    engine
        .broadcast_local_replication(
            None,
            None,
            "node-storage",
            world_id,
            1_000,
            &decision,
            Some(&mut replication),
            None,
        )
        .expect("broadcast local replication");

    assert_eq!(
        replication
            .latest_persisted_commit_height(world_id)
            .expect("latest persisted height"),
        1
    );
    assert_eq!(engine.replication_persisted_height, 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn restart_refresh_seeds_replication_cursor_from_durable_writer_state() {
    let dir = temp_dir("restart-refresh-seeds-replication-cursor");
    let world_id = "world-restart-replication-cursor";
    let validators = vec![PosValidator {
        validator_id: "node-a".to_string(),
        stake: 100,
    }];
    let pos_config = signed_pos_config_with_signer_seeds(validators, &[("node-a", 41)]);
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 41));
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let mut replication =
        ReplicationRuntime::new(config.replication.as_ref().expect("replication"), "node-a")
            .expect("replication runtime");

    for height in 1..=3 {
        engine.last_execution_height = height;
        engine.last_execution_block_hash = Some(format!("exec-block-{height}"));
        engine.last_execution_state_root = Some(format!("exec-state-{height}"));
        let decision = PosDecision {
            height,
            slot: height - 1,
            epoch: 0,
            proposer_id: "node-a".to_string(),
            status: PosConsensusStatus::Committed,
            block_hash: format!("block-{height}"),
            action_root: empty_action_root(),
            committed_actions: Vec::new(),
            approved_stake: 100,
            rejected_stake: 0,
            required_stake: 67,
            total_stake: 100,
        };
        engine
            .broadcast_local_replication(
                None,
                None,
                "node-a",
                world_id,
                1_000 + height as i64,
                &decision,
                Some(&mut replication),
                None,
            )
            .expect("broadcast local replication");
    }

    fs::remove_file(
        dir.join("replication_commit_messages")
            .join("00000000000000000001.json"),
    )
    .expect("remove compacted first commit");

    let restarted_replication =
        ReplicationRuntime::new(config.replication.as_ref().expect("replication"), "node-a")
            .expect("restarted replication runtime");
    let mut restarted_engine = PosNodeEngine::new(&config).expect("restarted engine");
    restarted_engine
        .refresh_replication_persisted_height(&restarted_replication, world_id)
        .expect("refresh replication persisted height");

    assert_eq!(restarted_engine.replication_persisted_height, 3);
    assert_eq!(
        restarted_replication
            .latest_persisted_commit_height(world_id)
            .expect("latest persisted height"),
        3
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_remote_replication_ingest_publishes_blob_provider_to_dht() {
    let dir_a = temp_dir("publish-remote-provider-a");
    let dir_b = temp_dir("publish-remote-provider-b");
    let dht = Arc::new(TestReplicaMaintenanceDht::new("peer-seq", "peer-store"));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(TestInMemoryNetwork::default());
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator {
                validator_id: "node-a".to_string(),
                stake: 60,
            },
            PosValidator {
                validator_id: "node-b".to_string(),
                stake: 40,
            },
        ],
        &[("node-a", 95), ("node-b", 96)],
    );
    let config_a = NodeConfig::new(
        "node-a",
        "world-publish-remote-provider",
        NodeRole::Sequencer,
    )
    .expect("config a")
    .with_tick_interval(Duration::from_millis(10))
    .expect("tick a")
    .with_pos_config(pos_config.clone())
    .expect("pos config a")
    .with_auto_attest_all_validators(true)
    .with_replication(signed_replication_config(dir_a.clone(), 95));
    let config_b = NodeConfig::new("node-b", "world-publish-remote-provider", NodeRole::Storage)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(signed_replication_config(dir_b.clone(), 96));

    let mut runtime_a = with_noop_execution_hook(NodeRuntime::new(config_a))
        .with_replication_network(
            NodeReplicationNetworkHandle::new(Arc::clone(&network))
                .with_dht(dht.clone())
                .with_local_provider_id("peer-seq"),
        );
    let mut runtime_b = NodeRuntime::new(config_b).with_replication_network(
        NodeReplicationNetworkHandle::new(Arc::clone(&network))
            .with_dht(dht.clone())
            .with_local_provider_id("peer-store"),
    );

    runtime_a.start().expect("start a");
    runtime_b.start().expect("start b");

    let published = wait_until(Instant::now() + Duration::from_secs(3), || {
        dht.published_records()
            .iter()
            .any(|(_, _, provider_id)| provider_id == "peer-store")
    });
    assert!(
        published,
        "expected storage ingest path to publish peer-store provider, got {:?}",
        dht.published_records()
    );

    runtime_a.stop().expect("stop a");
    runtime_b.stop().expect("stop b");
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn runtime_gossip_replication_with_signature_applies_files() {
    let socket_a = UdpSocket::bind("127.0.0.1:0").expect("bind a");
    let socket_b = UdpSocket::bind("127.0.0.1:0").expect("bind b");
    let addr_a = socket_a.local_addr().expect("addr a");
    let addr_b = socket_b.local_addr().expect("addr b");
    drop(socket_a);
    drop(socket_b);

    let dir_a = temp_dir("signed-repl-a");
    let dir_b = temp_dir("signed-repl-b");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let pos_config =
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 11), ("node-b", 22)]);

    let config_a = NodeConfig::new("node-a", "world-signed", NodeRole::Sequencer)
        .expect("config a")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_auto_attest_all_validators(true)
        .with_gossip_optional(addr_a, vec![addr_b])
        .with_replication(signed_replication_config(dir_a.clone(), 11));
    let config_b = NodeConfig::new("node-b", "world-signed", NodeRole::Observer)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_gossip_optional(addr_b, vec![addr_a])
        .with_replication(signed_replication_config(dir_b.clone(), 22));

    let mut runtime_a = with_noop_execution_hook(NodeRuntime::new(config_a));
    let mut runtime_b = NodeRuntime::new(config_b);
    runtime_b.start().expect("start b");
    let observer_ready = wait_until(Instant::now() + Duration::from_secs(2), || {
        let snapshot = runtime_b.snapshot();
        snapshot.tick_count > 0 && snapshot.last_error.is_none()
    });
    assert!(
        observer_ready,
        "signed observer runtime did not start before sequencer replication: {:?}",
        runtime_b.snapshot()
    );
    runtime_a.start().expect("start a");

    let store_b = LocalCasStore::new(dir_b.join("store"));
    let replicated = wait_until(Instant::now() + Duration::from_secs(3), || {
        store_b
            .list_files()
            .map(|files| {
                files
                    .iter()
                    .any(|item| item.path.starts_with("consensus/commits/"))
            })
            .unwrap_or(false)
    });
    let files = store_b.list_files().expect("list files");
    assert!(
        replicated,
        "expected signed gossip replication to apply commit files, got {files:?}; a={} b={}",
        storage_replication_diagnostics::summary(&runtime_a, &dir_a),
        storage_replication_diagnostics::summary(&runtime_b, &dir_b)
    );

    runtime_a.stop().expect("stop a");
    runtime_b.stop().expect("stop b");

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn runtime_gossip_replication_rejects_unsigned_when_signature_enforced() {
    let socket_a = UdpSocket::bind("127.0.0.1:0").expect("bind a");
    let socket_b = UdpSocket::bind("127.0.0.1:0").expect("bind b");
    let addr_a = socket_a.local_addr().expect("addr a");
    let addr_b = socket_b.local_addr().expect("addr b");
    drop(socket_a);
    drop(socket_b);

    let dir_a = temp_dir("unsigned-a");
    let dir_b = temp_dir("enforced-b");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let pos_config =
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 11), ("node-b", 33)]);

    let config_a = NodeConfig::new("node-a", "world-enforced", NodeRole::Sequencer)
        .expect("config a")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_gossip_optional(addr_a, vec![addr_b])
        .with_replication_root(dir_a.clone())
        .expect("replication a");
    let config_b = NodeConfig::new("node-b", "world-enforced", NodeRole::Observer)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_gossip_optional(addr_b, vec![addr_a])
        .with_replication(signed_replication_config(dir_b.clone(), 33));

    let mut runtime_a = with_noop_execution_hook(NodeRuntime::new(config_a));
    let mut runtime_b = NodeRuntime::new(config_b);
    runtime_a.start().expect("start a");
    runtime_b.start().expect("start b");
    thread::sleep(Duration::from_millis(220));

    runtime_a.stop().expect("stop a");
    runtime_b.stop().expect("stop b");

    let store_b = LocalCasStore::new(dir_b.join("store"));
    let files = store_b.list_files().expect("list files");
    assert!(files.is_empty());

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn runtime_gossip_replication_persists_guard_across_restart() {
    let socket_a = UdpSocket::bind("127.0.0.1:0").expect("bind a");
    let socket_b = UdpSocket::bind("127.0.0.1:0").expect("bind b");
    let addr_a = socket_a.local_addr().expect("addr a");
    let addr_b = socket_b.local_addr().expect("addr b");
    drop(socket_a);
    drop(socket_b);

    let dir_a = temp_dir("restart-a");
    let dir_b = temp_dir("restart-b");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let pos_config =
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 55), ("node-b", 66)]);
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(TestInMemoryNetwork::default());

    let build_config_a = || {
        NodeConfig::new("node-a", "world-restart", NodeRole::Sequencer)
            .expect("config a")
            .with_tick_interval(Duration::from_millis(10))
            .expect("tick a")
            .with_pos_config(pos_config.clone())
            .expect("pos config a")
            .with_auto_attest_all_validators(true)
            .with_gossip_optional(addr_a, vec![addr_b])
            .with_replication(signed_replication_config(dir_a.clone(), 55))
    };
    let build_config_b = || {
        NodeConfig::new("node-b", "world-restart", NodeRole::Observer)
            .expect("config b")
            .with_tick_interval(Duration::from_millis(10))
            .expect("tick b")
            .with_pos_config(pos_config.clone())
            .expect("pos config b")
            .with_gossip_optional(addr_b, vec![addr_a])
            .with_replication(signed_replication_config(dir_b.clone(), 66))
    };

    let mut runtime_a = with_noop_execution_hook(NodeRuntime::new(build_config_a()))
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    let mut runtime_b = NodeRuntime::new(build_config_b())
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime_b.start().expect("start b first");
    let observer_ready = wait_until(Instant::now() + Duration::from_secs(2), || {
        let snapshot = runtime_b.snapshot();
        snapshot.tick_count > 0 && snapshot.last_error.is_none()
    });
    assert!(
        observer_ready,
        "observer runtime did not start before sequencer replication: {:?}",
        runtime_b.snapshot()
    );
    runtime_a.start().expect("start a first");
    let guard_path = dir_b.join("replication_remote_guards.json");
    let guard_ready = wait_until(Instant::now() + Duration::from_secs(10), || {
        fs::read(&guard_path)
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<BTreeMap<String, SingleWriterReplicationGuard>>(&bytes)
                    .ok()
            })
            .and_then(|guards| guards.values().next().cloned())
            .is_some_and(|guard| guard.last_sequence >= 1)
    });
    let snapshot_b_first = runtime_b.snapshot();
    runtime_a.stop().expect("stop a first");
    runtime_b.stop().expect("stop b first");
    assert!(
        guard_ready,
        "replication guard did not persist before first stop: snapshot={:?}",
        snapshot_b_first
    );
    assert!(snapshot_b_first.last_error.is_none());

    let guard_before = serde_json::from_slice::<BTreeMap<String, SingleWriterReplicationGuard>>(
        &fs::read(&guard_path).expect("read guard before"),
    )
    .expect("parse guard before")
    .values()
    .next()
    .cloned()
    .expect("remote guard before");
    assert!(guard_before.last_sequence >= 1);

    let mut runtime_a = with_noop_execution_hook(NodeRuntime::new(build_config_a()))
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    let mut runtime_b = NodeRuntime::new(build_config_b())
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime_b.start().expect("start b second");
    let observer_ready = wait_until(Instant::now() + Duration::from_secs(2), || {
        let snapshot = runtime_b.snapshot();
        snapshot.tick_count > 0 && snapshot.last_error.is_none()
    });
    assert!(
        observer_ready,
        "observer runtime did not restart before sequencer replication: {:?}",
        runtime_b.snapshot()
    );
    runtime_a.start().expect("start a second");
    let writer_before = guard_before
        .writer_id
        .as_deref()
        .expect("writer before")
        .to_string();
    // CI runners can take noticeably longer to resume replication after both
    // nodes restart, so give the persisted remote guard extra time to advance.
    let guard_advanced = wait_until(Instant::now() + Duration::from_secs(20), || {
        fs::read(&guard_path)
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<BTreeMap<String, SingleWriterReplicationGuard>>(&bytes)
                    .ok()
            })
            .and_then(|guards| guards.get(&writer_before).cloned())
            .is_some_and(|guard| guard.last_sequence > guard_before.last_sequence)
    });
    let snapshot_b_second = runtime_b.snapshot();
    let guards_after_wait = fs::read(&guard_path).ok().and_then(|bytes| {
        serde_json::from_slice::<BTreeMap<String, SingleWriterReplicationGuard>>(&bytes).ok()
    });
    runtime_a.stop().expect("stop a second");
    runtime_b.stop().expect("stop b second");
    assert!(snapshot_b_second.last_error.is_none());

    let guard_after = serde_json::from_slice::<BTreeMap<String, SingleWriterReplicationGuard>>(
        &fs::read(&guard_path).expect("read guard after"),
    )
    .expect("parse guard after")
    .get(&writer_before)
    .cloned()
    .expect("remote guard after");
    assert_eq!(guard_after.writer_id, guard_before.writer_id);
    assert!(
        guard_advanced && guard_after.last_sequence > guard_before.last_sequence,
        "replication guard did not advance after restart: before={:?} after={:?} guards_after_wait={:?} snapshot={:?}",
        guard_before,
        guard_after,
        guards_after_wait,
        snapshot_b_second
    );

    let store_b = LocalCasStore::new(dir_b.join("store"));
    let files = store_b.list_files().expect("list files");
    assert!(files.len() >= 2);

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}
