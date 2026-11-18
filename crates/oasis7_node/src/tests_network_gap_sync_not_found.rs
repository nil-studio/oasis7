use super::*;

#[test]
fn runtime_network_replication_gap_sync_not_found_is_non_fatal() {
    let world_id = "world-network-gap-not-found";
    let dir_a = temp_dir("network-gap-not-found-a");
    let dir_b = temp_dir("network-gap-not-found-b");
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
        signed_pos_config_with_signer_seeds(validators, &[("node-a", 87), ("node-b", 88)]);
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
        .with_replication(signed_replication_config(dir_a.clone(), 87));
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(signed_replication_config(dir_b.clone(), 88));

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

    let request = signed_fetch_commit_request_for_test(world_id, target_height, 87);
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

    network
        .register_handler(
            super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL,
            Box::new(move |_payload| {
                let response = super::replication::FetchCommitResponse {
                    found: false,
                    message: None,
                lineage_envelope: None,
                };
                serde_json::to_vec(&response).map_err(|err| {
                    WorldError::DistributedValidationFailed {
                        reason: format!("encode fetch commit response failed: {err}"),
                    }
                })
            }),
        )
        .expect("register commit not found handler");

    let mut runtime_b = NodeRuntime::new(config_b)
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)));
    runtime_b.start().expect("start b");
    thread::sleep(Duration::from_millis(250));

    let snapshot_b = runtime_b.snapshot();
    assert!(
        !snapshot_b
            .last_error
            .as_deref()
            .map(|reason| reason.contains("gap sync height"))
            .unwrap_or(false),
        "not found gap sync should not be reported as fatal error"
    );
    assert!(
        snapshot_b.consensus.committed_height < target_height,
        "observer should keep waiting when target height is not found"
    );
    assert_eq!(
        snapshot_b.consensus.replication_gap_sync_blocked_height,
        Some(1),
        "not found gap sync must expose the first missing height"
    );
    assert!(
        snapshot_b
            .consensus
            .replication_gap_sync_blocked_reason
            .as_deref()
            .map(|reason| reason.contains("missing commit height 1"))
            .unwrap_or(false),
        "not found gap sync must expose a recoverability reason"
    );
    let route_snapshot = snapshot_b
        .consensus
        .replication_gap_sync_repair_attempt_route_snapshot
        .as_ref()
        .expect("gap-sync not-found should expose route observability snapshot");
    assert_eq!(route_snapshot.generic_route_count, 1);
    assert_eq!(route_snapshot.provider_route_count, 0);
    assert_eq!(route_snapshot.generic_retry_route_count, 3);
    assert_eq!(route_snapshot.route_attempt_count, 4);
    assert_eq!(route_snapshot.not_found_route_count, 4);
    assert_eq!(route_snapshot.synced_route_count, 0);
    assert_eq!(route_snapshot.error_route_count, 0);
    assert_eq!(route_snapshot.budget_exhausted_count, 0);
    assert!(
        snapshot_b
            .consensus
            .replication_gap_sync_repair_attempt_summary
            .as_deref()
            .map(|summary| summary.contains("routes_attempted=4"))
            .unwrap_or(false),
        "gap-sync repair summary should include route counter fields, got {:?}",
        snapshot_b
            .consensus
            .replication_gap_sync_repair_attempt_summary
    );

    runtime_b.stop().expect("stop b");
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}
