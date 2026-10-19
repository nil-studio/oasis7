use super::*;

struct FailingCheckpointExportHook {
    commits: Arc<Mutex<Vec<u64>>>,
    restores: Arc<Mutex<Vec<(String, u64)>>>,
    restore_result: bool,
}

impl NodeExecutionHook for FailingCheckpointExportHook {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        self.commits
            .lock()
            .expect("record commits")
            .push(context.height);
        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: format!("exec-block-{}", context.height),
            execution_state_root: format!("exec-state-{}", context.height),
        })
    }

    fn restore_to_height(&mut self, world_id: &str, height: u64) -> Result<bool, String> {
        self.restores
            .lock()
            .expect("record restores")
            .push((world_id.to_string(), height));
        Ok(self.restore_result)
    }

    fn export_checkpoint_bundle(
        &mut self,
        height: u64,
    ) -> Result<Option<NodeExecutionCheckpointBundle>, String> {
        Err(format!("injected checkpoint export failure at height {height}"))
    }
}

#[derive(Clone)]
struct CommitPublishFailingNetwork {
    inner: TestInMemoryNetwork,
    fail_topic: String,
}

impl CommitPublishFailingNetwork {
    fn new(world_id: &str) -> Self {
        Self {
            inner: TestInMemoryNetwork::default(),
            fail_topic: super::network_bridge::default_consensus_commit_topic(world_id),
        }
    }
}

impl oasis7_proto::distributed_net::DistributedNetwork<WorldError>
    for CommitPublishFailingNetwork
{
    fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), WorldError> {
        if topic == self.fail_topic {
            return Err(WorldError::DistributedValidationFailed {
                reason: "injected consensus commit publish failure".to_string(),
            });
        }
        self.inner.publish(topic, payload)
    }

    fn subscribe(&self, topic: &str) -> Result<NetworkSubscription, WorldError> {
        self.inner.subscribe(topic)
    }

    fn request(&self, protocol: &str, payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        self.inner.request(protocol, payload)
    }

    fn register_handler(
        &self,
        protocol: &str,
        handler: oasis7_proto::distributed_net::NetworkHandler<WorldError>,
    ) -> Result<(), WorldError> {
        self.inner.register_handler(protocol, handler)
    }
}

#[test]
fn non_proposer_committed_decision_does_not_persist_local_replication() {
    let world_id = "world-non-proposer-replication-guard";
    let dir = temp_dir("non-proposer-replication-guard");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 100,
        },
        PosValidator {
            validator_id: "node-c".to_string(),
            stake: 100,
        },
    ];
    let pos_config = signed_pos_config_with_signer_seeds(
        validators.clone(),
        &[("node-a", 31), ("node-b", 32), ("node-c", 33)],
    );
    let probe_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("probe config")
        .with_pos_config(pos_config.clone())
        .expect("probe pos config");
    let probe_engine = PosNodeEngine::new(&probe_config).expect("probe engine");
    let slot = 0;
    let expected_proposer = probe_engine
        .expected_proposer(slot)
        .expect("expected proposer");
    let non_proposer = validators
        .iter()
        .map(|validator| validator.validator_id.as_str())
        .find(|validator_id| *validator_id != expected_proposer)
        .expect("non proposer");
    let signer_seed = match non_proposer {
        "node-a" => 31,
        "node-b" => 32,
        "node-c" => 33,
        other => panic!("unexpected validator {other}"),
    };

    let config = NodeConfig::new(non_proposer, world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), signer_seed));
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.last_execution_height = 1;
    engine.last_execution_block_hash = Some("exec-block-1".to_string());
    engine.last_execution_state_root = Some("exec-state-1".to_string());
    engine.network_committed_height = 2;
    engine.last_replication_gap_sync_blocked_height = Some(1);
    engine.last_replication_gap_sync_blocked_reason = Some("stale blocked height".to_string());

    let mut replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("replication"),
        non_proposer,
    )
    .expect("replication runtime");
    let decision = PosDecision {
        height: 1,
        slot,
        epoch: 0,
            proposer_id: "node-a".to_string(),
        status: PosConsensusStatus::Committed,
        block_hash: "block-1".to_string(),
        action_root: empty_action_root(),
        committed_actions: Vec::new(),
        approved_stake: 200,
        rejected_stake: 0,
        required_stake: 201,
        total_stake: 300,
    };

    engine
        .broadcast_local_replication(
            None,
            None,
            non_proposer,
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
        0
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn non_proposer_missing_remote_replication_retains_pending_commit() {
    let world_id = "world-non-proposer-missing-remote-replication";
    let dir = temp_dir("non-proposer-missing-remote-replication");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 100,
        },
    ];
    let config = NodeConfig::new("node-b", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(signed_pos_config_with_signer_seeds(
            validators,
            &[("node-a", 41), ("node-b", 42)],
        ))
        .expect("pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 42));
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.pending = Some(PendingProposal {
        height: 1,
        slot: 0,
        epoch: 0,
        opened_at_ms: 100,
        proposer_id: "node-a".to_string(),
        block_hash: "remote-block-1".to_string(),
        action_root: empty_action_root(),
        committed_actions: Vec::new(),
        attestations: std::collections::BTreeMap::new(),
        approved_stake: 100,
        rejected_stake: 0,
        status: PosConsensusStatus::Pending,
    });
    let mut replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("replication"),
        "node-b",
    )
    .expect("replication runtime");

    let result = engine
        .tick(
            &config.node_id,
            &config.world_id,
            1_000,
            None,
            Some(&mut replication),
            None,
            None,
            Vec::new(),
            None,
        )
        .expect("tick");

    assert_eq!(engine.committed_height, 0);
    assert_eq!(engine.replication_persisted_height, 0);
    let pending = engine.pending.as_ref().expect("pending retained");
    assert_eq!(pending.height, 1);
    assert_eq!(pending.proposer_id, "node-a");
    assert_eq!(
        result.consensus_snapshot.last_status,
        Some(PosConsensusStatus::Pending)
    );
    assert_eq!(
        result
            .consensus_snapshot
            .last_inbound_timing_reject_reason
            .as_deref(),
        Some("drop remote committed height 1 without matching persisted replication commit")
    );
    assert_eq!(
        replication
            .latest_persisted_commit_height(world_id)
            .expect("latest persisted height"),
        0
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn non_proposer_mismatched_remote_replication_retains_pending_commit() {
    let world_id = "world-non-proposer-mismatched-remote-replication";
    let dir = temp_dir("non-proposer-mismatched-remote-replication");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 100,
        },
    ];
    let config = NodeConfig::new("node-b", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(signed_pos_config_with_signer_seeds(
            validators,
            &[("node-a", 41), ("node-b", 42)],
        ))
        .expect("pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 42));
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.pending = Some(PendingProposal {
        height: 1,
        slot: 0,
        epoch: 0,
        opened_at_ms: 100,
        proposer_id: "node-a".to_string(),
        block_hash: "remote-block-1".to_string(),
        action_root: empty_action_root(),
        committed_actions: Vec::new(),
        attestations: std::collections::BTreeMap::new(),
        approved_stake: 100,
        rejected_stake: 0,
        status: PosConsensusStatus::Pending,
    });
    let mut replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("replication"),
        "node-b",
    )
    .expect("replication runtime");
    let mismatched_decision = PosDecision {
        height: 1,
        slot: 0,
        epoch: 0,
        proposer_id: "node-a".to_string(),
        status: PosConsensusStatus::Committed,
        block_hash: "different-block-1".to_string(),
        action_root: empty_action_root(),
        committed_actions: Vec::new(),
        approved_stake: 100,
        rejected_stake: 0,
        required_stake: 101,
        total_stake: 200,
    };
    replication
        .build_local_commit_message(
            "node-a",
            world_id,
            900,
            &mismatched_decision,
            None,
            None,
        )
        .expect("persist mismatched commit")
        .expect("mismatched commit");

    let result = engine
        .tick(
            &config.node_id,
            &config.world_id,
            1_000,
            None,
            Some(&mut replication),
            None,
            None,
            Vec::new(),
            None,
        )
        .expect("tick");

    assert_eq!(engine.committed_height, 0);
    assert_eq!(engine.replication_persisted_height, 0);
    let pending = engine.pending.as_ref().expect("pending retained");
    assert_eq!(pending.height, 1);
    assert_eq!(pending.block_hash, "remote-block-1");
    assert_eq!(
        result
            .consensus_snapshot
            .last_inbound_timing_reject_reason
            .as_deref(),
        Some("drop remote committed height 1 without matching persisted replication commit")
    );
    assert_eq!(
        replication
            .latest_persisted_commit_height(world_id)
            .expect("latest persisted height"),
        1
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn proposer_local_replication_advances_persisted_height_without_network_endpoint() {
    let world_id = "world-local-replication-persisted-height";
    let dir = temp_dir("local-replication-persisted-height");
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 100,
        },
        PosValidator {
            validator_id: "node-c".to_string(),
            stake: 100,
        },
    ];
    let pos_config = signed_pos_config_with_signer_seeds(
        validators.clone(),
        &[("node-a", 31), ("node-b", 32), ("node-c", 33)],
    );
    let probe_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("probe config")
        .with_pos_config(pos_config.clone())
        .expect("probe pos config");
    let probe_engine = PosNodeEngine::new(&probe_config).expect("probe engine");
    let slot = 0;
    let proposer = probe_engine
        .expected_proposer(slot)
        .expect("expected proposer");
    let signer_seed = match proposer.as_str() {
        "node-a" => 31,
        "node-b" => 32,
        "node-c" => 33,
        other => panic!("unexpected validator {other}"),
    };

    let config = NodeConfig::new(proposer.as_str(), world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), signer_seed));
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.last_execution_height = 1;
    engine.last_execution_block_hash = Some("exec-block-1".to_string());
    engine.last_execution_state_root = Some("exec-state-1".to_string());

    let mut replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("replication"),
        proposer.as_str(),
    )
    .expect("replication runtime");
    let decision = PosDecision {
        height: 1,
        slot,
        epoch: 0,
            proposer_id: "node-a".to_string(),
        status: PosConsensusStatus::Committed,
        block_hash: "block-1".to_string(),
        action_root: empty_action_root(),
        committed_actions: Vec::new(),
        approved_stake: 200,
        rejected_stake: 0,
        required_stake: 201,
        total_stake: 300,
    };

    engine
        .broadcast_local_replication(
            None,
            None,
            proposer.as_str(),
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
    assert_eq!(engine.last_replication_gap_sync_blocked_height, None);
    assert_eq!(engine.last_replication_gap_sync_blocked_reason, None);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn proposer_tick_rolls_back_local_execution_when_replication_export_fails() {
    let world_id = "world-local-replication-rollback";
    let dir = temp_dir("local-replication-rollback");
    let validators = vec![PosValidator {
        validator_id: "node-a".to_string(),
        stake: 100,
    }];
    let pos_config = signed_pos_config_with_signer_seeds(validators, &[("node-a", 61)]);
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 61));
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let mut replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("replication"),
        "node-a",
    )
    .expect("replication runtime");
    let commits = Arc::new(Mutex::new(Vec::new()));
    let restores = Arc::new(Mutex::new(Vec::new()));
    let mut execution_hook = FailingCheckpointExportHook {
        commits: Arc::clone(&commits),
        restores: Arc::clone(&restores),
        restore_result: true,
    };

    let err = engine
        .tick(
            &config.node_id,
            &config.world_id,
            1_000,
            None,
            Some(&mut replication),
            None,
            None,
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect_err("replication export failure should fail the tick");

    assert!(
        format!("{err:?}").contains("injected checkpoint export failure at height 1"),
        "unexpected error: {err:?}"
    );
    assert_eq!(commits.lock().expect("commits").as_slice(), &[1]);
    assert_eq!(
        restores.lock().expect("restores").as_slice(),
        &[(world_id.to_string(), 0)]
    );
    assert_eq!(engine.last_execution_height, 0);
    assert_eq!(engine.last_execution_block_hash, None);
    assert_eq!(engine.last_execution_state_root, None);
    assert_eq!(engine.committed_height, 0);
    assert_eq!(engine.replication_persisted_height, 0);
    assert!(
        replication
            .load_commit_message_by_height(world_id, 1)
            .expect("load commit message")
            .is_none()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn proposer_tick_fails_closed_when_initial_execution_rollback_record_is_missing() {
    let world_id = "world-local-replication-rollback-missing-zero";
    let dir = temp_dir("local-replication-rollback-missing-zero");
    let validators = vec![PosValidator {
        validator_id: "node-a".to_string(),
        stake: 100,
    }];
    let pos_config = signed_pos_config_with_signer_seeds(validators, &[("node-a", 62)]);
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 62));
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let mut replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("replication"),
        "node-a",
    )
    .expect("replication runtime");
    let commits = Arc::new(Mutex::new(Vec::new()));
    let restores = Arc::new(Mutex::new(Vec::new()));
    let mut execution_hook = FailingCheckpointExportHook {
        commits: Arc::clone(&commits),
        restores: Arc::clone(&restores),
        restore_result: false,
    };

    let err = engine
        .tick(
            &config.node_id,
            &config.world_id,
            1_000,
            None,
            Some(&mut replication),
            None,
            None,
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect_err("missing initial rollback record should fail closed");

    let err_text = format!("{err:?}");
    assert!(
        err_text.contains("rollback record for height 0 is unavailable"),
        "unexpected error: {err:?}"
    );
    assert_eq!(commits.lock().expect("commits").as_slice(), &[1]);
    assert_eq!(
        restores.lock().expect("restores").as_slice(),
        &[(world_id.to_string(), 0)]
    );
    assert_eq!(engine.last_execution_height, 0);
    assert_eq!(engine.last_execution_block_hash, None);
    assert_eq!(engine.last_execution_state_root, None);
    assert_eq!(engine.committed_height, 0);
    assert_eq!(engine.replication_persisted_height, 0);
    assert!(
        replication
            .load_commit_message_by_height(world_id, 1)
            .expect("load commit message")
            .is_none()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn proposer_tick_keeps_consensus_and_execution_aligned_when_post_apply_commit_publish_fails() {
    let world_id = "world-post-apply-commit-publish-failure";
    let validators = vec![PosValidator {
        validator_id: "node-a".to_string(),
        stake: 100,
    }];
    let pos_config = signed_pos_config_with_signer_seeds(validators, &[("node-a", 63)]);
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let network: Arc<dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync> =
        Arc::new(CommitPublishFailingNetwork::new(world_id));
    let handle = NodeReplicationNetworkHandle::new(network);
    let mut consensus_endpoint =
        ConsensusNetworkEndpoint::new(&handle, world_id, false, &config.network_policy)
            .expect("consensus endpoint");
    let commits = Arc::new(Mutex::new(Vec::new()));
    let restores = Arc::new(Mutex::new(Vec::new()));
    let mut execution_hook = FailingCheckpointExportHook {
        commits: Arc::clone(&commits),
        restores: Arc::clone(&restores),
        restore_result: true,
    };

    let err = engine
        .tick(
            &config.node_id,
            &config.world_id,
            1_000,
            None,
            None,
            None,
            Some(&mut consensus_endpoint),
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect_err("commit publish failure should fail the tick");

    assert!(
        format!("{err:?}").contains("injected consensus commit publish failure"),
        "unexpected error: {err:?}"
    );
    assert_eq!(commits.lock().expect("commits").as_slice(), &[1]);
    assert!(
        restores.lock().expect("restores").is_empty(),
        "post-apply publish failure must not roll execution back after consensus advanced"
    );
    assert_eq!(engine.committed_height, 1);
    assert_eq!(engine.last_execution_height, 1);
    assert_eq!(engine.next_height, 2);
    assert_eq!(engine.last_execution_block_hash.as_deref(), Some("exec-block-1"));
    assert_eq!(engine.last_execution_state_root.as_deref(), Some("exec-state-1"));
}

#[cfg(unix)]
#[test]
fn latest_persisted_height_does_not_read_unrelated_historical_payloads() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("latest-height-unrelated-history");
    let world_id = "world-latest-height-history";
    let config = signed_replication_config(dir.clone(), 91);
    let mut runtime = ReplicationRuntime::new(&config, "node-a").expect("runtime");
    for height in 1..=2 {
        let decision = PosDecision {
            height,
            slot: height,
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
        runtime
            .build_local_commit_message("node-a", world_id, 1_000, &decision, None, None)
            .expect("build commit")
            .expect("commit");
    }
    let historical = dir
        .join("replication_commit_messages")
        .join("00000000000000000001.json");
    let permissions = fs::metadata(&historical).expect("metadata").permissions();
    fs::set_permissions(&historical, fs::Permissions::from_mode(0)).expect("deny old payload read");
    // Root bypasses Unix file permissions; this fixture requires an unprivileged runner.
    if fs::read(&historical).is_ok() {
        fs::set_permissions(&historical, permissions).expect("restore permissions");
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    let latest = runtime.latest_persisted_commit_height(world_id);
    fs::set_permissions(&historical, permissions).expect("restore permissions");
    assert_eq!(
        latest.expect("latest height must not inspect unrelated payloads"),
        2
    );
    let newest = dir
        .join("replication_commit_messages")
        .join("00000000000000000002.json");
    let permissions = fs::metadata(&newest)
        .expect("newest metadata")
        .permissions();
    fs::set_permissions(&newest, fs::Permissions::from_mode(0)).expect("deny newest read");
    let inaccessible_head = runtime.latest_persisted_commit_height(world_id);
    fs::set_permissions(&newest, permissions).expect("restore newest permissions");
    let _ = fs::remove_dir_all(&dir);
    assert!(
        inaccessible_head.is_err(),
        "the selected head must still be readable and validated"
    );
}
