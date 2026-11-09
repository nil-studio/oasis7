use super::*;

#[test]
fn runtime_replication_storage_challenge_gate_prefers_dht_blob_providers() {
    let dir = temp_dir("challenge-gate-provider-selection");
    let network_impl = Arc::new(ProviderAwareTestNetwork::new(
        dir.clone(),
        "storage-provider-1",
    ));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let dht = Arc::new(TestReplicaMaintenanceDht::new(
        "storage-provider-1",
        "storage-provider-1",
    ));
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 93)],
    );
    let config = NodeConfig::new(
        "node-a",
        "world-challenge-provider-selection",
        NodeRole::Sequencer,
    )
    .expect("config")
    .with_tick_interval(Duration::from_millis(10))
    .expect("tick")
    .with_pos_config(pos_config)
    .expect("pos config")
    .with_auto_attest_all_validators(true)
    .with_replication(signed_replication_config(dir.clone(), 93));
    let mut runtime = with_noop_execution_hook(NodeRuntime::new(config)).with_replication_network(
        NodeReplicationNetworkHandle::new(Arc::clone(&network))
            .with_dht(dht)
            .with_local_provider_id("node-a"),
    );

    runtime.start().expect("start runtime");
    let advanced = wait_until(Instant::now() + Duration::from_secs(2), || {
        runtime.snapshot().consensus.committed_height >= 4
    });
    let snapshot = runtime.snapshot();
    let attempts = network_impl.provider_attempts();
    assert!(
        advanced,
        "runtime did not continue committing when provider-aware fetch-blob was available: committed_height={} network_committed_height={} last_error={:?} attempts={attempts:?}",
        snapshot.consensus.committed_height,
        snapshot.consensus.network_committed_height,
        snapshot.last_error
    );

    assert!(
        attempts.iter().any(|providers| {
            providers
                .iter()
                .any(|provider| provider == "storage-provider-1")
        }),
        "expected storage challenge gate to request fetch-blob with DHT providers, attempts={attempts:?}"
    );

    assert!(
        !snapshot
            .last_error
            .as_deref()
            .map(|reason| reason.contains("storage challenge gate"))
            .unwrap_or(false),
        "runtime should not report storage challenge gate failure when provider-aware fetch-blob succeeds: {:?}",
        snapshot.last_error
    );

    runtime.stop().expect("stop runtime");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_local_replication_publishes_blob_provider_to_dht() {
    let dir = temp_dir("publish-local-provider");
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(TestInMemoryNetwork::default());
    let dht = Arc::new(TestReplicaMaintenanceDht::new("peer-local", "peer-local"));
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 94)],
    );
    let config = NodeConfig::new(
        "node-a",
        "world-publish-local-provider",
        NodeRole::Sequencer,
    )
    .expect("config")
    .with_tick_interval(Duration::from_millis(10))
    .expect("tick")
    .with_pos_config(pos_config)
    .expect("pos config")
    .with_auto_attest_all_validators(true)
    .with_replication(signed_replication_config(dir.clone(), 94));
    let mut runtime = with_noop_execution_hook(NodeRuntime::new(config)).with_replication_network(
        NodeReplicationNetworkHandle::new(Arc::clone(&network))
            .with_dht(dht.clone())
            .with_local_provider_id("peer-local"),
    );

    runtime.start().expect("start runtime");
    let published = wait_until(Instant::now() + Duration::from_secs(2), || {
        !dht.published_records().is_empty()
    });
    assert!(published, "expected local commit to publish blob provider");

    let published_records = dht.published_records();
    assert!(
        published_records
            .iter()
            .any(|(_, _, provider_id)| provider_id == "peer-local"),
        "expected published provider id peer-local, got {published_records:?}"
    );

    runtime.stop().expect("stop runtime");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_replication_storage_challenge_gate_falls_back_after_provider_route_unavailable() {
    let dir = temp_dir("challenge-gate-provider-fallback");
    let network_impl = Arc::new(ProviderFallbackTestNetwork::new(dir.clone()));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let dht = Arc::new(
        TestReplicaMaintenanceDht::new("storage-provider-1", "node-a")
            .with_source_provider_for_all_lookups(),
    );
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 97)],
    );
    let config = NodeConfig::new(
        "node-a",
        "world-challenge-provider-fallback",
        NodeRole::Sequencer,
    )
    .expect("config")
    .with_tick_interval(Duration::from_millis(10))
    .expect("tick")
    .with_pos_config(pos_config)
    .expect("pos config")
    .with_auto_attest_all_validators(true)
    .with_replication(signed_replication_config(dir.clone(), 97));
    let mut runtime = with_noop_execution_hook(NodeRuntime::new(config)).with_replication_network(
        NodeReplicationNetworkHandle::new(Arc::clone(&network))
            .with_dht(dht)
            .with_local_provider_id("node-a"),
    );

    runtime.start().expect("start runtime");
    let advanced = wait_until(Instant::now() + Duration::from_secs(2), || {
        runtime.snapshot().consensus.committed_height >= 4
    });
    let snapshot = runtime.snapshot();
    let provider_attempts = network_impl.provider_attempts();
    let generic_attempts = network_impl.generic_attempts();
    assert!(
        advanced,
        "runtime did not continue committing after provider-route degraded mode: committed_height={} network_committed_height={} last_error={:?} provider_attempts={provider_attempts:?} generic_attempts={generic_attempts}",
        snapshot.consensus.committed_height,
        snapshot.consensus.network_committed_height,
        snapshot.last_error
    );
    assert!(
        provider_attempts.iter().any(|providers| {
            providers
                .iter()
                .any(|provider| provider == "storage-provider-1")
        }),
        "expected storage challenge gate to try DHT-selected provider before fallback: {provider_attempts:?}"
    );
    assert!(
        generic_attempts > 0,
        "storage challenge should fall back to generic lane after retryable provider route unavailability"
    );
    assert!(
        !snapshot
            .last_error
            .as_deref()
            .map(|reason| reason.contains("storage challenge gate"))
            .unwrap_or(false),
        "runtime should not hard-fail storage challenge gate when provider route is unavailable: {:?}",
        snapshot.last_error
    );
    assert!(
        snapshot
            .consensus
            .storage_challenge_network_degraded_height
            .is_none(),
        "retryable provider route unavailability should recover through generic fallback: {:?}",
        snapshot.consensus.storage_challenge_network_degraded_reason
    );

    runtime.stop().expect("stop runtime");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn runtime_replication_storage_challenge_gate_degrades_after_provider_route_not_found() {
    let dir = temp_dir("challenge-gate-provider-not-found");
    let world_id = "world-challenge-provider-not-found";
    let network_impl = Arc::new(ProviderNotFoundFallbackTestNetwork::new(dir.clone()));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let dht = Arc::new(
        TestReplicaMaintenanceDht::new("storage-provider-1", "node-a")
            .with_source_provider_for_all_lookups(),
    );
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 112)],
    );
    let seed_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("seed config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("seed tick")
        .with_pos_config(pos_config.clone())
        .expect("seed pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir.clone(), 112));
    seed_six_local_commits(&seed_config);

    let replication = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir.clone(), 112),
        "node-a",
    )
    .expect("restart replication runtime");
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_replication(signed_replication_config(dir.clone(), 112));
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network))
        .with_dht(dht)
        .with_local_provider_id("node-a");
    let endpoint =
        ReplicationNetworkEndpoint::new(&handle, world_id, false, &config.network_policy)
            .expect("endpoint");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    engine.committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.network_committed_height = STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8;
    engine.peer_heads.insert(
        "storage-provider-1".to_string(),
        PeerCommittedHead {
            height: STORAGE_GATE_NETWORK_WARMUP_HEIGHT + 8,
            block_hash: "provider-not-found-fallback-peer-head".to_string(),
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
    let snapshot = engine.snapshot_from_decision(&engine.idle_pending_decision().expect("decision"));
    let provider_attempts = network_impl.provider_attempts();
    let generic_attempts = network_impl.generic_attempts();
    assert!(
        gate_result.is_ok(),
        "provider route not-found should degrade, not hard-block: {gate_result:?}"
    );
    assert!(
        provider_attempts.iter().any(|providers| {
            providers
                .iter()
                .any(|provider| provider == "storage-provider-1")
        }),
        "expected storage challenge gate to try DHT-selected provider before degraded mode: {provider_attempts:?}"
    );
    assert!(
        generic_attempts == 0,
        "storage challenge should not retry generic lane after not-found provider response"
    );
    assert!(
        snapshot.storage_challenge_network_degraded_height.is_some(),
        "provider not-found should remain observable as degraded: {:?}",
        snapshot.storage_challenge_network_degraded_reason
    );

    let _ = fs::remove_dir_all(&dir);
}
