use super::*;

#[test]
fn runtime_replication_storage_challenge_gate_provider_lookup_timeout_fallback_wrong_blob_degrades()
{
    let dir = temp_dir("challenge-gate-provider-lookup-fallback-wrong");
    let world_id = "world-challenge-provider-lookup-fallback-wrong";
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 121)],
    );

    let seed_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("seed config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("seed tick")
        .with_pos_config(pos_config.clone())
        .expect("seed pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 121));
    seed_six_local_commits(&seed_config);

    let network_impl = Arc::new(ProviderLookupFailureConnectedPeerTrapNetwork::new());
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 121));
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network))
        .with_dht(Arc::new(ProviderLookupFailureDht))
        .with_local_provider_id("node-a");
    let endpoint =
        ReplicationNetworkEndpoint::new(&handle, world_id, false, &config.network_policy)
            .expect("endpoint");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.network_committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.peer_heads.insert(
        "observer-light-peer".to_string(),
        PeerCommittedHead {
            height: 1,
            block_hash: "observer-light-peer-head".to_string(),
            committed_at_ms: 1_234,
            observed_at_ms: 1_234,
            execution_block_hash: None,
            execution_state_root: None,
            action_root: empty_action_root(),
            public_key_hex: None,
            signature_hex: None,
        },
    );
    let replication = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir.clone(), 121),
        "node-a",
    )
    .expect("restart replication runtime");

    let gate_result = engine.enforce_storage_challenge_gate(
        &replication,
        Some(&endpoint),
        "node-a",
        world_id,
        1_234,
    );

    assert!(
        gate_result.is_ok(),
        "provider lookup fallback wrong blob should degrade, not hard-block consensus: {gate_result:?}"
    );
    assert!(
        network_impl.connected_peer_provider_attempts() > 0,
        "storage challenge should try bounded fallback after retryable provider lookup failure"
    );
    let snapshot = engine.snapshot_from_decision(&engine.idle_pending_decision().expect("decision"));
    assert!(
        snapshot
            .storage_challenge_network_degraded_reason
            .as_deref()
            .map(|reason| {
                reason.contains("provider lookup failed")
                    && reason.contains("fallback blob hash mismatch")
            })
            .unwrap_or(false),
        "expected provider lookup failure and fallback mismatch to remain observable as degraded, got {:?}",
        snapshot.storage_challenge_network_degraded_reason
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_replication_storage_challenge_gate_provider_lookup_timeout_fallback_correct_blob_matches()
{
    let dir = temp_dir("challenge-gate-provider-lookup-fallback-correct");
    let world_id = "world-challenge-provider-lookup-fallback-correct";
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 124)],
    );

    let seed_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("seed config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("seed tick")
        .with_pos_config(pos_config.clone())
        .expect("seed pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 124));
    seed_six_local_commits(&seed_config);

    let replication = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir.clone(), 124),
        "node-a",
    )
    .expect("restart replication runtime");
    let mut fallback_blobs = HashMap::<String, Vec<u8>>::new();
    for (_, content_hash) in replication
        .recent_replicated_content_refs(world_id, STORAGE_GATE_NETWORK_SAMPLES_PER_CHECK)
        .expect("load recent replicated refs")
    {
        let blob = replication
            .load_blob_by_hash(content_hash.as_str())
            .expect("load local blob")
            .expect("blob payload");
        fallback_blobs.insert(content_hash, blob);
    }

    let network_impl = Arc::new(ProviderLookupFailureGenericBlobNetwork::new(fallback_blobs));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 124));
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network))
        .with_dht(Arc::new(ProviderLookupFailureDht))
        .with_local_provider_id("node-a");
    let endpoint =
        ReplicationNetworkEndpoint::new(&handle, world_id, false, &config.network_policy)
            .expect("endpoint");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.network_committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.peer_heads.insert(
        "observer-fallback-peer".to_string(),
        PeerCommittedHead {
            height: STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8,
            block_hash: "observer-fallback-peer-head".to_string(),
            committed_at_ms: 1_234,
            observed_at_ms: 1_234,
            execution_block_hash: None,
            execution_state_root: None,
            action_root: empty_action_root(),
            public_key_hex: None,
            signature_hex: None,
        },
    );

    let gate_result = engine.enforce_storage_challenge_gate(
        &replication,
        Some(&endpoint),
        "node-a",
        world_id,
        1_234,
    );

    assert!(
        gate_result.is_ok(),
        "provider lookup fallback correct blob should satisfy storage challenge: {gate_result:?}"
    );
    assert!(
        network_impl.generic_attempts() > 0,
        "storage challenge should use generic fallback after retryable provider lookup failure"
    );
    let snapshot = engine.snapshot_from_decision(&engine.idle_pending_decision().expect("decision"));
    assert_eq!(snapshot.storage_challenge_network_degraded_height, None);
    assert_eq!(snapshot.storage_challenge_network_degraded_reason, None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_replication_storage_challenge_gate_empty_provider_lookup_uses_generic_fallback() {
    let dir = temp_dir("challenge-gate-empty-provider-lookup-fallback");
    let world_id = "world-challenge-empty-provider-lookup-fallback";
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 126)],
    );

    let seed_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("seed config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("seed tick")
        .with_pos_config(pos_config.clone())
        .expect("seed pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 126));
    seed_six_local_commits(&seed_config);

    let replication = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir.clone(), 126),
        "node-a",
    )
    .expect("restart replication runtime");
    let mut fallback_blobs = HashMap::<String, Vec<u8>>::new();
    for (_, content_hash) in replication
        .recent_replicated_content_refs(world_id, STORAGE_GATE_NETWORK_SAMPLES_PER_CHECK)
        .expect("load recent replicated refs")
    {
        let blob = replication
            .load_blob_by_hash(content_hash.as_str())
            .expect("load local blob")
            .expect("blob payload");
        fallback_blobs.insert(content_hash, blob);
    }

    let network_impl = Arc::new(ProviderLookupFailureGenericBlobNetwork::new(fallback_blobs));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 126));
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network))
        .with_dht(Arc::new(EmptyProviderLookupDht))
        .with_local_provider_id("node-a");
    let endpoint =
        ReplicationNetworkEndpoint::new(&handle, world_id, false, &config.network_policy)
            .expect("endpoint");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.network_committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.peer_heads.insert(
        "observer-fallback-peer".to_string(),
        PeerCommittedHead {
            height: STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8,
            block_hash: "observer-empty-provider-fallback-peer-head".to_string(),
            committed_at_ms: 1_234,
            observed_at_ms: 1_234,
            execution_block_hash: None,
            execution_state_root: None,
            action_root: empty_action_root(),
            public_key_hex: None,
            signature_hex: None,
        },
    );

    let gate_result = engine.enforce_storage_challenge_gate(
        &replication,
        Some(&endpoint),
        "node-a",
        world_id,
        1_234,
    );

    assert!(
        gate_result.is_ok(),
        "empty provider lookup should recover through generic fallback: {gate_result:?}"
    );
    assert!(
        network_impl.generic_attempts() > 0,
        "storage challenge should use generic fallback when DHT returns no providers"
    );
    assert!(
        network_impl.generic_attempts() <= STORAGE_GATE_NETWORK_SAMPLES_PER_CHECK,
        "empty provider lookup should use one bounded generic request per successful sample"
    );
    let snapshot = engine.snapshot_from_decision(&engine.idle_pending_decision().expect("decision"));
    assert_eq!(snapshot.storage_challenge_network_degraded_height, None);
    assert_eq!(snapshot.storage_challenge_network_degraded_reason, None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_replication_storage_challenge_gate_empty_provider_lookup_generic_not_found_degrades() {
    let dir = temp_dir("challenge-gate-empty-provider-lookup-not-found");
    let world_id = "world-challenge-empty-provider-lookup-not-found";
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 127)],
    );

    let seed_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("seed config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("seed tick")
        .with_pos_config(pos_config.clone())
        .expect("seed pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 127));
    seed_six_local_commits(&seed_config);

    let network_impl = Arc::new(ProviderLookupFailureGenericBlobNetwork::new(HashMap::new()));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 127));
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network))
        .with_dht(Arc::new(EmptyProviderLookupDht))
        .with_local_provider_id("node-a");
    let endpoint =
        ReplicationNetworkEndpoint::new(&handle, world_id, false, &config.network_policy)
            .expect("endpoint");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.network_committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.peer_heads.insert(
        "observer-fallback-peer".to_string(),
        PeerCommittedHead {
            height: STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8,
            block_hash: "observer-empty-provider-not-found-peer-head".to_string(),
            committed_at_ms: 1_234,
            observed_at_ms: 1_234,
            execution_block_hash: None,
            execution_state_root: None,
            action_root: empty_action_root(),
            public_key_hex: None,
            signature_hex: None,
        },
    );
    let replication = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir.clone(), 127),
        "node-a",
    )
    .expect("restart replication runtime");

    let gate_result = engine.enforce_storage_challenge_gate(
        &replication,
        Some(&endpoint),
        "node-a",
        world_id,
        1_234,
    );

    assert!(
        gate_result.is_ok(),
        "empty provider lookup generic not-found should degrade, not hard-block: {gate_result:?}"
    );
    assert_eq!(
        network_impl.generic_attempts(),
        STORAGE_GATE_NETWORK_SAMPLES_PER_CHECK + STORAGE_GATE_FALLBACK_SAMPLES_PER_CHECK,
        "empty provider lookup generic not-found should spend bounded primary and fallback attempts"
    );
    let snapshot = engine.snapshot_from_decision(&engine.idle_pending_decision().expect("decision"));
    assert_eq!(
        snapshot.storage_challenge_network_degraded_height,
        Some(STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8)
    );
    assert!(
        snapshot
            .storage_challenge_network_degraded_reason
            .as_deref()
            .map(|reason| reason.contains("network blob not found"))
            .unwrap_or(false),
        "expected generic not-found to remain observable as degraded, got {:?}",
        snapshot.storage_challenge_network_degraded_reason
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_replication_storage_challenge_gate_unavailable_primary_uses_old_sample_fallback() {
    let dir = temp_dir("challenge-gate-primary-unavailable-old-fallback");
    let world_id = "world-challenge-primary-unavailable-old-fallback";
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 129)],
    );

    let seed_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("seed config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("seed tick")
        .with_pos_config(pos_config.clone())
        .expect("seed pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 129));
    seed_six_local_commits(&seed_config);

    let replication = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir.clone(), 129),
        "node-a",
    )
    .expect("restart replication runtime");
    let mut fallback_blobs = HashMap::<String, Vec<u8>>::new();
    for (_, content_hash) in replication
        .replicated_content_refs_from_height(world_id, 1, STORAGE_GATE_FALLBACK_SAMPLES_PER_CHECK)
        .expect("load older replicated refs")
        .into_iter()
        .take(2)
    {
        let blob = replication
            .load_blob_by_hash(content_hash.as_str())
            .expect("load old local blob")
            .expect("old blob payload");
        fallback_blobs.insert(content_hash, blob);
    }

    let network_impl = Arc::new(ProviderLookupFailureGenericBlobNetwork::new(fallback_blobs));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 129));
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network))
        .with_dht(Arc::new(EmptyProviderLookupDht))
        .with_local_provider_id("node-a");
    let endpoint =
        ReplicationNetworkEndpoint::new(&handle, world_id, false, &config.network_policy)
            .expect("endpoint");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.network_committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.peer_heads.insert(
        "observer-old-fallback-peer".to_string(),
        PeerCommittedHead {
            height: STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8,
            block_hash: "observer-old-fallback-peer-head".to_string(),
            committed_at_ms: 1_234,
            observed_at_ms: 1_234,
            execution_block_hash: None,
            execution_state_root: None,
            action_root: empty_action_root(),
            public_key_hex: None,
            signature_hex: None,
        },
    );

    let gate_result = engine.enforce_storage_challenge_gate(
        &replication,
        Some(&endpoint),
        "node-a",
        world_id,
        1_234,
    );

    assert!(
        gate_result.is_ok(),
        "unavailable primary samples should recover through old-sample fallback: {gate_result:?}"
    );
    assert!(
        network_impl.generic_attempts() >= 3,
        "storage challenge should probe primary once and enough old fallback samples"
    );
    let snapshot = engine.snapshot_from_decision(&engine.idle_pending_decision().expect("decision"));
    assert_eq!(snapshot.storage_challenge_network_degraded_height, None);
    assert_eq!(snapshot.storage_challenge_network_degraded_reason, None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_replication_storage_challenge_gate_empty_provider_lookup_wrong_generic_blob_hard_fails()
{
    let dir = temp_dir("challenge-gate-empty-provider-lookup-wrong-blob");
    let world_id = "world-challenge-empty-provider-lookup-wrong-blob";
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 128)],
    );

    let seed_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("seed config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("seed tick")
        .with_pos_config(pos_config.clone())
        .expect("seed pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 128));
    seed_six_local_commits(&seed_config);

    let network_impl = Arc::new(ProviderRouteFailureGenericTrapNetwork::new());
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 128));
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network))
        .with_dht(Arc::new(EmptyProviderLookupDht))
        .with_local_provider_id("node-a");
    let endpoint =
        ReplicationNetworkEndpoint::new(&handle, world_id, false, &config.network_policy)
            .expect("endpoint");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.network_committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.peer_heads.insert(
        "observer-fallback-peer".to_string(),
        PeerCommittedHead {
            height: STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8,
            block_hash: "observer-empty-provider-wrong-blob-peer-head".to_string(),
            committed_at_ms: 1_234,
            observed_at_ms: 1_234,
            execution_block_hash: None,
            execution_state_root: None,
            action_root: empty_action_root(),
            public_key_hex: None,
            signature_hex: None,
        },
    );
    let replication = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir.clone(), 128),
        "node-a",
    )
    .expect("restart replication runtime");

    let gate_result = engine.enforce_storage_challenge_gate(
        &replication,
        Some(&endpoint),
        "node-a",
        world_id,
        1_234,
    );

    assert!(
        matches!(gate_result, Err(NodeError::Consensus { .. })),
        "wrong blob from empty-provider generic fallback must hard-fail: {gate_result:?}"
    );
    assert_eq!(
        network_impl.generic_attempts(),
        1,
        "wrong generic blob should stop after the first hard-failure sample"
    );
    assert!(
        format!("{gate_result:?}").contains("network blob hash mismatch"),
        "expected wrong generic blob to remain a hard mismatch, got {gate_result:?}"
    );
    let snapshot = engine.snapshot_from_decision(&engine.idle_pending_decision().expect("decision"));
    assert_eq!(snapshot.storage_challenge_network_degraded_reason, None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_replication_storage_challenge_gate_provider_lookup_timeout_fallback_local_corruption_hard_fails(
) {
    let dir = temp_dir("challenge-gate-provider-lookup-fallback-local-corrupt");
    let world_id = "world-challenge-provider-lookup-fallback-local-corrupt";
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 125)],
    );

    let seed_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("seed config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("seed tick")
        .with_pos_config(pos_config.clone())
        .expect("seed pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 125));
    seed_six_local_commits(&seed_config);

    let replication = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir.clone(), 125),
        "node-a",
    )
    .expect("restart replication runtime");
    let mut fallback_blobs = HashMap::<String, Vec<u8>>::new();
    for (_, content_hash) in replication
        .recent_replicated_content_refs(world_id, STORAGE_GATE_NETWORK_SAMPLES_PER_CHECK)
        .expect("load recent replicated refs")
    {
        let blob = replication
            .load_blob_by_hash(content_hash.as_str())
            .expect("load local blob")
            .expect("blob payload");
        fallback_blobs.insert(content_hash, blob);
    }
    let store = LocalCasStore::new(dir.join("store"));
    for entry in fs::read_dir(store.blobs_dir()).expect("list blobs") {
        let entry = entry.expect("blob entry");
        if entry.file_type().expect("blob type").is_file() {
            fs::write(entry.path(), b"tampered-local-blob").expect("tamper blob");
        }
    }

    let network_impl = Arc::new(ProviderLookupFailureGenericBlobNetwork::new(fallback_blobs));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 125));
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network))
        .with_dht(Arc::new(ProviderLookupFailureDht))
        .with_local_provider_id("node-a");
    let endpoint =
        ReplicationNetworkEndpoint::new(&handle, world_id, false, &config.network_policy)
            .expect("endpoint");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.network_committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.peer_heads.insert(
        "observer-fallback-peer".to_string(),
        PeerCommittedHead {
            height: STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8,
            block_hash: "observer-fallback-peer-head".to_string(),
            committed_at_ms: 1_234,
            observed_at_ms: 1_234,
            execution_block_hash: None,
            execution_state_root: None,
            action_root: empty_action_root(),
            public_key_hex: None,
            signature_hex: None,
        },
    );

    let gate_result = engine.enforce_storage_challenge_gate(
        &replication,
        Some(&endpoint),
        "node-a",
        world_id,
        1_234,
    );

    gate_result.expect_err("local CAS corruption should hard-fail");
    let _ = fs::remove_dir_all(&dir);
}

/// These cases exercise provider fallback against durable local history, not
/// background worker scheduling. Drive the real signed consensus/replication
/// tick synchronously so a loaded host cannot expire an unrelated setup timer.
fn seed_six_local_commits(config: &NodeConfig) {
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
            replication.latest_persisted_commit_height(&config.world_id).unwrap(),
            height,
            "seed must be durably replicated before the provider test"
        );
    }
    assert_eq!(calls.lock().unwrap().len(), 6);
}
