use super::*;

#[test]
fn runtime_network_replication_gap_sync_fetches_missing_commits() {
    let world_id = "world-network-gap";
    let dir_a = temp_dir("network-gap-a");
    let dir_b = temp_dir("network-gap-b");
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
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 78), ("node-b", 79)]);
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
        .with_replication(signed_replication_config(dir_a.clone(), 78));
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(signed_replication_config(dir_b.clone(), 79));

    let mut runtime_a = with_noop_execution_hook(NodeRuntime::new(config_a))
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime_a.start().expect("start a");
    let start = Instant::now();
    let mut rounds = 0usize;
    let mut last_snapshot = runtime_a.snapshot();
    let reached = wait_until(start + Duration::from_secs(2), || {
        rounds += 1;
        last_snapshot = runtime_a.snapshot();
        last_snapshot.consensus.committed_height >= 3
    });
    assert!(
        reached,
        "sequencer did not reach target height in time: elapsed={:?}, rounds={}, snapshot={:?}",
        start.elapsed(),
        rounds,
        last_snapshot
    );
    let target_height = runtime_a.snapshot().consensus.committed_height;
    runtime_a.stop().expect("stop a");

    let mut commit_map = HashMap::<u64, super::replication::GossipReplicationMessage>::new();
    let mut blob_map = HashMap::<String, Vec<u8>>::new();
    for height in 1..=target_height {
        let request = signed_fetch_commit_request_for_test(world_id, height, 78);
        let payload = serde_json::to_vec(&request).expect("encode commit request");
        let response_payload = network
            .request(
                super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL,
                payload.as_slice(),
            )
            .expect("fetch commit");
        let response: super::replication::FetchCommitResponse =
            serde_json::from_slice(&response_payload).expect("decode commit response");
        assert!(response.found, "missing fetched commit at height {height}");
        let message = response.message.expect("commit payload");
        blob_map.insert(message.record.content_hash.clone(), message.payload.clone());
        commit_map.insert(height, message);
    }
    assert_eq!(commit_map.len() as u64, target_height);
    let high_message = commit_map
        .get(&target_height)
        .cloned()
        .expect("high commit message");

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
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)))
        .with_replication_network_consensus_enabled(false);
    runtime_b.start().expect("start b");

    let commit_map = Arc::new(commit_map);
    let blob_map = Arc::new(blob_map);
    let commit_world_id = world_id.to_string();
    network
        .register_handler(
            super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL,
            Box::new(move |payload| {
                let request =
                    serde_json::from_slice::<super::replication::FetchCommitRequest>(payload)
                        .map_err(|err| WorldError::DistributedValidationFailed {
                            reason: format!("decode fetch commit request failed: {err}"),
                        })?;
                if request.world_id != commit_world_id {
                    return Err(WorldError::DistributedValidationFailed {
                        reason: format!(
                            "world mismatch expected={} actual={}",
                            commit_world_id, request.world_id
                        ),
                    });
                }
                let response = super::replication::FetchCommitResponse {
                    found: commit_map.contains_key(&request.height),
                    message: commit_map.get(&request.height).cloned(),
                    lineage_envelope: None,
                };
                serde_json::to_vec(&response).map_err(|err| {
                    WorldError::DistributedValidationFailed {
                        reason: format!("encode fetch commit response failed: {err}"),
                    }
                })
            }),
        )
        .expect("register commit handler");
    network
        .register_handler(
            super::replication::REPLICATION_FETCH_BLOB_PROTOCOL,
            Box::new(move |payload| {
                let request =
                    serde_json::from_slice::<super::replication::FetchBlobRequest>(payload)
                        .map_err(|err| WorldError::DistributedValidationFailed {
                            reason: format!("decode fetch blob request failed: {err}"),
                        })?;
                let blob = blob_map.get(request.content_hash.as_str()).cloned();
                let response = test_fetch_blob_response(blob.is_some(), blob);
                serde_json::to_vec(&response).map_err(|err| {
                    WorldError::DistributedValidationFailed {
                        reason: format!("encode fetch blob response failed: {err}"),
                    }
                })
            }),
        )
        .expect("register blob handler");

    let sync_start = Instant::now();
    let mut sync_rounds = 0usize;
    let mut sync_snapshot = runtime_b.snapshot();
    let synced = wait_until(sync_start + Duration::from_secs(8), || {
        sync_rounds += 1;
        sync_snapshot = runtime_b.snapshot();
        sync_snapshot.consensus.committed_height >= target_height
    });
    assert!(
        synced,
        "observer did not sync missing commits in time: elapsed={:?}, rounds={}, snapshot={:?}",
        sync_start.elapsed(),
        sync_rounds,
        sync_snapshot
    );

    runtime_b.stop().expect("stop b");
    let snapshot_b = runtime_b.snapshot();
    assert!(snapshot_b.last_error.is_none());
    assert!(snapshot_b.consensus.committed_height >= target_height);

    let store_b = LocalCasStore::new(dir_b.join("store"));
    let files = store_b.list_files().expect("list files");
    assert!(files
        .iter()
        .any(|item| item.path == "consensus/commits/00000000000000000001.json"));
    assert!(files
        .iter()
        .any(|item| { item.path == format!("consensus/commits/{:020}.json", target_height) }));

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}
