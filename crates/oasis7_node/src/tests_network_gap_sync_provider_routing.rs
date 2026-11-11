use std::fs;

use oasis7_proto::distributed_checkpoint_lineage::{
    CheckpointLineageCheckpointV1, CheckpointLineageEnvelopeV1, CheckpointLineageHeadV1,
};

use super::*;

#[derive(Clone)]
struct TimeoutThenProviderUnavailableFetchCommitNetwork {
    connected_peer_ids: Vec<String>,
    generic_attempts: Arc<Mutex<usize>>,
    provider_attempts: Arc<Mutex<Vec<Vec<String>>>>,
}

#[derive(Clone)]
struct BootstrapPriorityFetchCommitNetwork {
    known_peer_ids: Vec<String>,
    static_bootstrap_peer_ids: Vec<String>,
    provider_attempts: Arc<Mutex<Vec<Vec<String>>>>,
    successful_provider: String,
    successful_response: Arc<Mutex<super::replication::FetchCommitResponse>>,
}

impl oasis7_proto::distributed_net::DistributedNetwork<WorldError>
    for BootstrapPriorityFetchCommitNetwork
{
    fn publish(&self, _topic: &str, _payload: &[u8]) -> Result<(), WorldError> {
        Ok(())
    }

    fn subscribe(&self, topic: &str) -> Result<NetworkSubscription, WorldError> {
        Ok(NetworkSubscription::new(
            topic.to_string(),
            Arc::new(Mutex::new(HashMap::new())),
        ))
    }

    fn request(&self, protocol: &str, _payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        Err(WorldError::NetworkProtocolUnavailable {
            protocol: format!("libp2p-replication no connected providers for protocol {protocol}"),
        })
    }

    fn known_peer_ids(&self) -> Vec<String> {
        self.known_peer_ids.clone()
    }

    fn configured_static_bootstrap_peer_ids(&self) -> Vec<String> {
        self.static_bootstrap_peer_ids.clone()
    }

    fn request_with_providers(
        &self,
        protocol: &str,
        _payload: &[u8],
        providers: &[String],
    ) -> Result<Vec<u8>, WorldError> {
        self.provider_attempts
            .lock()
            .expect("lock provider attempts")
            .push(providers.to_vec());
        if protocol != super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL {
            return Err(WorldError::NetworkProtocolUnavailable {
                protocol: protocol.to_string(),
            });
        }
        if providers == [self.successful_provider.clone()] {
            return serde_json::to_vec(
                &*self
                    .successful_response
                    .lock()
                    .expect("lock successful bootstrap response"),
            )
            .map_err(|err| {
                WorldError::DistributedValidationFailed {
                    reason: format!("encode successful bootstrap fetch-commit response failed: {err}"),
                }
            });
        }
        Err(WorldError::NetworkProtocolUnavailable {
            protocol: format!("libp2p-replication no connected providers for protocol {protocol}"),
        })
    }

    fn register_handler(
        &self,
        _protocol: &str,
        _handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        Ok(())
    }
}

impl oasis7_proto::distributed_net::DistributedNetwork<WorldError>
    for TimeoutThenProviderUnavailableFetchCommitNetwork
{
    fn publish(&self, _topic: &str, _payload: &[u8]) -> Result<(), WorldError> {
        Ok(())
    }

    fn subscribe(&self, topic: &str) -> Result<NetworkSubscription, WorldError> {
        Ok(NetworkSubscription::new(
            topic.to_string(),
            Arc::new(Mutex::new(HashMap::new())),
        ))
    }

    fn request(&self, protocol: &str, _payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        *self.generic_attempts.lock().expect("lock generic attempts") += 1;
        Err(WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrTimeout,
            message: protocol.to_string(),
            retryable: true,
        })
    }

    fn connected_peer_ids(&self) -> Vec<String> {
        self.connected_peer_ids.clone()
    }

    fn request_with_providers(
        &self,
        protocol: &str,
        _payload: &[u8],
        providers: &[String],
    ) -> Result<Vec<u8>, WorldError> {
        self.provider_attempts
            .lock()
            .expect("lock provider attempts")
            .push(providers.to_vec());
        Err(WorldError::NetworkProtocolUnavailable {
            protocol: format!("libp2p-replication no connected providers for protocol {protocol}"),
        })
    }

    fn register_handler(
        &self,
        _protocol: &str,
        _handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        Ok(())
    }
}

fn route_test_lineage_envelope() -> CheckpointLineageEnvelopeV1 {
    CheckpointLineageEnvelopeV1 {
        schema_version: 1,
        claim_boundary: "oasis7.checkpoint_lineage.v1".to_string(),
        world_id: "world-gap-sync-fetch-commit-lineage-route".to_string(),
        checkpoint: CheckpointLineageCheckpointV1 {
            height: 64,
            block_hash: "block-64".to_string(),
            state_root: "state-64".to_string(),
            execution_block_hash: "execution-block-64".to_string(),
            execution_state_root: "execution-state-64".to_string(),
            descriptor_digest: "descriptor-digest-64".to_string(),
            manifest_size: 1,
        },
        head: CheckpointLineageHeadV1 {
            height: 80,
            block_hash: "block-80".to_string(),
            state_root: "state-80".to_string(),
            execution_block_hash: "execution-block-80".to_string(),
            execution_state_root: "execution-state-80".to_string(),
        },
        validator_set_hash: "validator-set".to_string(),
        total_stake: 1,
        required_stake: 1,
        round_id: "round-64".to_string(),
        votes: Vec::new(),
    }
}

#[test]
fn high_checkpoint_fetch_does_not_stop_at_found_provider_without_lineage_sidecar() {
    let dir_remote = temp_dir("gap-sync-fetch-commit-lineage-route-remote");
    let dir_local = temp_dir("gap-sync-fetch-commit-lineage-route-local");
    let world_id = "world-gap-sync-fetch-commit-lineage-route";
    let generic_attempts = Arc::new(Mutex::new(0usize));
    let provider_attempts = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let peer_responses = Arc::new(Mutex::new(HashMap::new()));
    let network: Arc<dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync> =
        Arc::new(network_gap_sync_tests::PeerDirectedFetchCommitTestNetwork {
            // The generic route models the first connected validator: it has
            // the commit but not the authenticated checkpoint sidecar.
            generic_response: super::replication::FetchCommitResponse {
                found: true,
                message: None,
                lineage_envelope: None,
            },
            generic_unsupported: false,
            peer_responses: Arc::clone(&peer_responses),
            connected_peer_ids: vec!["validator-204".to_string(), "validator-205".to_string()],
            generic_attempts: Arc::clone(&generic_attempts),
            provider_attempts: Arc::clone(&provider_attempts),
        });
    let (_, _, endpoint, message) = network_gap_sync_tests::build_fetch_commit_success_cache_fixture(
        world_id,
        dir_remote.as_path(),
        dir_local.as_path(),
        210,
        211,
        Arc::clone(&network),
    );
    peer_responses
        .lock()
        .expect("lock provider responses")
        .extend([
            (
                "validator-204".to_string(),
                super::replication::FetchCommitResponse {
                    found: true,
                    message: None,
                    lineage_envelope: None,
                },
            ),
            (
                "validator-205".to_string(),
                super::replication::FetchCommitResponse {
                    found: true,
                    message: Some(message),
                    lineage_envelope: Some(route_test_lineage_envelope()),
                },
            ),
        ]);
    let request = signed_fetch_commit_request_for_test(world_id, 64, 211);

    let response = endpoint
        .request_fetch_commit_for_high_checkpoint_probe(&request)
        .expect("high checkpoint route should continue after a sidecar-less provider");
    assert!(
        response.response.lineage_envelope.is_some(),
        "high checkpoint route must select a provider carrying authenticated lineage"
    );
    assert_eq!(
        provider_attempts
            .lock()
            .expect("lock provider attempts")
            .as_slice(),
        &[
            vec!["validator-204".to_string()],
            vec!["validator-205".to_string()],
        ],
        "route must try the second validator after a sidecar-less found response"
    );

    let _ = fs::remove_dir_all(&dir_remote);
    let _ = fs::remove_dir_all(&dir_local);
}

#[test]
fn gap_sync_fetch_commit_caps_peer_route_sweep_after_generic_not_found() {
    const MAX_EXPECTED_PEER_ROUTE_ATTEMPTS: usize = 8;

    let dir_remote = temp_dir("gap-sync-fetch-commit-peer-route-cap-remote");
    let dir_local = temp_dir("gap-sync-fetch-commit-peer-route-cap-local");
    let world_id = "world-gap-sync-fetch-commit-peer-route-cap";
    let generic_attempts = Arc::new(Mutex::new(0usize));
    let provider_attempts = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let peer_responses = Arc::new(Mutex::new(HashMap::new()));
    let connected_peer_ids = (0..64)
        .map(|index| format!("peer-{index:02}"))
        .collect::<Vec<_>>();
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(network_gap_sync_tests::PeerDirectedFetchCommitTestNetwork {
        generic_response: super::replication::FetchCommitResponse {
            found: false,
            message: None,
        lineage_envelope: None,
        },
        generic_unsupported: false,
        peer_responses,
        connected_peer_ids,
        generic_attempts: Arc::clone(&generic_attempts),
        provider_attempts: Arc::clone(&provider_attempts),
    });
    let (_, _, endpoint, _) = network_gap_sync_tests::build_fetch_commit_success_cache_fixture(
        world_id,
        dir_remote.as_path(),
        dir_local.as_path(),
        146,
        147,
        Arc::clone(&network),
    );
    let request = signed_fetch_commit_request_for_test(world_id, 1, 147);

    let response = endpoint
        .request_fetch_commit_for_gap_sync(&request)
        .expect("bounded peer sweep should return not-found instead of exhausting every peer");
    assert!(
        !response.response.found,
        "test fixture intentionally has no peer with the requested commit"
    );

    let attempts = provider_attempts
        .lock()
        .expect("lock provider attempts")
        .clone();
    assert!(
        attempts.len() <= MAX_EXPECTED_PEER_ROUTE_ATTEMPTS,
        "gap-sync peer-directed fallback must be bounded; attempted {} peer routes: {:?}",
        attempts.len(),
        attempts
    );
    assert_eq!(
        *generic_attempts.lock().expect("lock generic attempts"),
        4,
        "bounded peer sweep should still preserve the existing generic retry budget"
    );

    let _ = fs::remove_dir_all(&dir_remote);
    let _ = fs::remove_dir_all(&dir_local);
}

#[test]
fn gap_sync_fetch_commit_single_probe_preserves_generic_timeout_over_provider_gap() {
    let dir_remote = temp_dir("gap-sync-fetch-commit-single-probe-timeout-remote");
    let dir_local = temp_dir("gap-sync-fetch-commit-single-probe-timeout-local");
    let world_id = "world-gap-sync-fetch-commit-single-probe-timeout";
    let generic_attempts = Arc::new(Mutex::new(0usize));
    let provider_attempts = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(TimeoutThenProviderUnavailableFetchCommitNetwork {
        connected_peer_ids: vec!["peer-a".to_string()],
        generic_attempts: Arc::clone(&generic_attempts),
        provider_attempts: Arc::clone(&provider_attempts),
    });
    let (_, _, endpoint, _) = network_gap_sync_tests::build_fetch_commit_success_cache_fixture(
        world_id,
        dir_remote.as_path(),
        dir_local.as_path(),
        146,
        147,
        Arc::clone(&network),
    );
    let request = signed_fetch_commit_request_for_test(world_id, 1, 147);

    let err = match endpoint.request_fetch_commit_for_gap_sync_single_probe(&request) {
        Ok(_) => panic!("single probe should return the generic timeout"),
        Err(err) => err,
    };
    let reason = err.to_string();
    assert!(
        reason.contains("kind=timeout")
            && reason.contains(super::replication::REPLICATION_FETCH_COMMIT_PROTOCOL),
        "expected generic timeout to remain actionable, got {reason}"
    );
    assert!(
        !reason.contains("no connected providers"),
        "provider route gap should not overwrite a prior connected-peer timeout: {reason}"
    );
    assert_eq!(
        *generic_attempts.lock().expect("lock generic attempts"),
        1,
        "single probe should keep one generic attempt"
    );
    assert_eq!(
        provider_attempts
            .lock()
            .expect("lock provider attempts")
            .as_slice(),
        &[vec!["peer-a".to_string()]],
        "single probe should still attempt the provider route for diagnostics"
    );

    let _ = fs::remove_dir_all(&dir_remote);
    let _ = fs::remove_dir_all(&dir_local);
}

#[test]
fn fresh_checkpoint_fetch_prioritizes_static_validators_after_peer_head_discovery() {
    let dir_remote = temp_dir("fresh-checkpoint-static-validator-priority-remote");
    let dir_local = temp_dir("fresh-checkpoint-static-validator-priority-local");
    let world_id = "world-fresh-checkpoint-static-validator-priority";
    let provider_attempts = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let successful_provider = "validator-204".to_string();
    let stale_discovery_peers = (0..8)
        .map(|index| format!("dht-stale-{index:02}"))
        .collect::<Vec<_>>();
    let mut known_peer_ids = stale_discovery_peers;
    known_peer_ids.extend(["validator-204".to_string(), "validator-205".to_string()]);
    let successful_response = Arc::new(Mutex::new(super::replication::FetchCommitResponse {
        found: false,
        message: None,
    lineage_envelope: None,
    }));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(BootstrapPriorityFetchCommitNetwork {
        known_peer_ids,
        static_bootstrap_peer_ids: vec!["validator-204".to_string(), "validator-205".to_string()],
        provider_attempts: Arc::clone(&provider_attempts),
        successful_provider: successful_provider.clone(),
        successful_response: Arc::clone(&successful_response),
    });
    let (_, _, endpoint, message) = network_gap_sync_tests::build_fetch_commit_success_cache_fixture(
        world_id,
        dir_remote.as_path(),
        dir_local.as_path(),
        208,
        209,
        Arc::clone(&network),
    );

    *successful_response
        .lock()
        .expect("install signed validator response") = super::replication::FetchCommitResponse {
        found: true,
        message: Some(message),
        lineage_envelope: None,
    };
    let request = signed_fetch_commit_request_for_test(world_id, 1, 209);

    let response = endpoint
        .request_fetch_commit_for_gap_sync_single_probe(&request)
        .expect("fresh observer must fetch its checkpoint from the static validator whose signed head was selected");

    assert!(response.response.found, "static validator must supply the signed checkpoint commit");
    assert_eq!(
        provider_attempts.lock().expect("lock provider attempts").as_slice(),
        &[vec![successful_provider]],
        "checkpoint fetch must prioritize static validators before stale discovery candidates"
    );

    let _ = fs::remove_dir_all(&dir_remote);
    let _ = fs::remove_dir_all(&dir_local);
}

#[test]
fn runtime_network_replication_gap_sync_prefers_dht_blob_providers() {
    let world_id = "world-network-gap-provider-selection";
    let dir_a = temp_dir("network-gap-provider-selection-a");
    let dir_b = temp_dir("network-gap-provider-selection-b");
    let validators = vec![PosValidator {
        validator_id: "node-a".to_string(),
        stake: 100,
    }];
    let pos_config = signed_pos_config_with_signer_seeds(validators, &[("node-a", 98)]);
    let network_impl = Arc::new(ProviderAwareTestNetwork::new(
        dir_a.clone(),
        "storage-provider-1",
    ));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();

    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(signed_replication_config(dir_b.clone(), 99));

    let mut commit_map = HashMap::<u64, super::replication::GossipReplicationMessage>::new();
    let mut replication_runtime_a = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir_a.clone(), 98),
        "node-a",
    )
    .expect("open replication runtime a");
    let target_height = 2;
    for height in 1..=target_height {
        let decision = PosDecision {
            height,
            slot: height.saturating_sub(1),
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
        let message = replication_runtime_a
            .build_local_commit_message(
                "node-a",
                world_id,
                1_000 + height as i64,
                &decision,
                None,
                None,
            )
            .expect("build local commit message")
            .expect("commit payload");
        commit_map.insert(height, message);
    }
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

    let dht = Arc::new(TestReplicaMaintenanceDht::new(
        "storage-provider-1",
        "observer-provider",
    ));
    for message in commit_map.values() {
        dht.seed_provider(message.record.content_hash.as_str(), "storage-provider-1");
    }
    let mut runtime_b = NodeRuntime::new(config_b)
        .with_replication_network(
            NodeReplicationNetworkHandle::new(Arc::clone(&network))
                .with_dht(dht)
                .with_local_provider_id("observer-provider"),
        )
        .with_replication_network_consensus_enabled(false);
    runtime_b.start().expect("start b");

    let commit_map = Arc::new(commit_map);
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

    let synced = wait_until(Instant::now() + Duration::from_secs(3), || {
        runtime_b.snapshot().consensus.committed_height >= target_height
    });
    let snapshot_b = runtime_b.snapshot();
    let attempts = network_impl.provider_attempts();
    assert!(
        synced,
        "observer did not sync missing commits with provider-aware gap sync: committed_height={} network_committed_height={} last_error={:?} attempts={attempts:?}",
        snapshot_b.consensus.committed_height,
        snapshot_b.consensus.network_committed_height,
        snapshot_b.last_error
    );
    assert!(
        attempts.iter().any(|providers| {
            providers
                .iter()
                .any(|provider| provider == "storage-provider-1")
        }),
        "expected gap sync fetch-blob to use DHT-selected providers, attempts={attempts:?}"
    );

    runtime_b.stop().expect("stop b");
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn runtime_network_replication_gap_sync_blocks_without_generic_fallback_after_provider_route_unavailable() {
    let world_id = "world-network-gap-provider-fallback";
    let dir_a = temp_dir("network-gap-provider-fallback-a");
    let dir_b = temp_dir("network-gap-provider-fallback-b");
    let validators = vec![PosValidator {
        validator_id: "node-a".to_string(),
        stake: 100,
    }];
    let pos_config = signed_pos_config_with_signer_seeds(validators, &[("node-a", 100)]);
    let network_impl = Arc::new(ProviderFallbackTestNetwork::new(dir_a.clone()));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();

    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(signed_replication_config(dir_b.clone(), 101));

    // 本例验证 observer 的 provider 路由；同步准备签名历史，避免依赖后台出块时序。
    let target_height = 3;
    let mut generated_commits = HashMap::new();
    {
        let mut replication_runtime_a = super::replication::ReplicationRuntime::new(
            &signed_replication_config(dir_a.clone(), 100),
            "node-a",
        )
        .expect("open replication fixture writer");
        for height in 1..=target_height {
            let decision = PosDecision {
                height,
                slot: height.saturating_sub(1),
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
            let execution_block_hash = format!("exec-block-{height:020}");
            let execution_state_root = format!("exec-state-{height:020}");
            let message = replication_runtime_a
                .build_local_commit_message(
                    "node-a",
                    world_id,
                    1_000 + height as i64,
                    &decision,
                    Some(execution_block_hash.as_str()),
                    Some(execution_state_root.as_str()),
                )
                .expect("build local commit message")
                .expect("commit payload");
            generated_commits.insert(height, message);
        }
    }

    let mut commit_map = HashMap::<u64, super::replication::GossipReplicationMessage>::new();
    let replication_runtime_a = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir_a.clone(), 100),
        "node-a",
    )
    .expect("open replication runtime a");
    for height in 1..=target_height {
        let message = replication_runtime_a
            .load_commit_message_by_height(world_id, height)
            .expect("load commit by height")
            .expect("commit payload");
        assert_eq!(
            Some(&message),
            generated_commits.get(&height),
            "reopened commit must preserve the signed fixture"
        );
        let blob = super::replication::load_blob_from_root(
            dir_a.as_path(),
            message.record.content_hash.as_str(),
        )
        .expect("load persisted fixture blob");
        assert_eq!(blob.as_deref(), Some(message.payload.as_slice()));
        commit_map.insert(height, message);
    }
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

    let dht = Arc::new(TestReplicaMaintenanceDht::new(
        "storage-provider-1",
        "observer-provider",
    ));
    let mut runtime_b = NodeRuntime::new(config_b)
        .with_replication_network(
            NodeReplicationNetworkHandle::new(Arc::clone(&network))
                .with_dht(dht)
                .with_local_provider_id("observer-provider"),
        )
        .with_replication_network_consensus_enabled(false);
    runtime_b.start().expect("start b");

    let commit_map = Arc::new(commit_map);
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

    let blocked = wait_until(Instant::now() + Duration::from_secs(3), || {
        runtime_b
            .snapshot()
            .consensus
            .replication_gap_sync_blocked_height
            .is_some()
    });
    let snapshot_b = runtime_b.snapshot();
    let provider_attempts = network_impl.provider_attempts();
    let generic_attempts = network_impl.generic_attempts();
    assert!(
        blocked,
        "observer did not expose provider-route gap-sync block: committed_height={} network_committed_height={} last_error={:?} provider_attempts={provider_attempts:?} generic_attempts={generic_attempts}",
        snapshot_b.consensus.committed_height,
        snapshot_b.consensus.network_committed_height,
        snapshot_b.last_error
    );
    assert_eq!(
        snapshot_b.consensus.replication_gap_sync_blocked_height,
        Some(1),
        "provider route must block the first missing commit"
    );
    assert!(
        provider_attempts.iter().any(|providers| {
            providers
                .iter()
                .any(|provider| provider == "storage-provider-1")
        }),
        "expected gap sync to try DHT-selected provider before blocking: {provider_attempts:?}"
    );
    assert!(
        provider_attempts.len() <= 1,
        "gap sync should not repeatedly spend request budget on the same unavailable provider route: {provider_attempts:?}"
    );
    assert!(
        generic_attempts == 0,
        "gap sync fetch-blob should not fall back to generic non-provider requests"
    );
    assert!(
        snapshot_b
            .consensus
            .replication_gap_sync_blocked_reason
            .as_deref()
            .map(|reason| reason.contains("provider route blocked"))
            .unwrap_or(false),
        "expected provider route block reason, got {:?}",
        snapshot_b.consensus.replication_gap_sync_blocked_reason
    );

    runtime_b.stop().expect("stop b");
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn runtime_network_replication_gap_sync_blocks_without_generic_fallback_after_provider_route_not_found() {
    let world_id = "world-network-gap-provider-not-found";
    let dir_a = temp_dir("network-gap-provider-not-found-a");
    let dir_b = temp_dir("network-gap-provider-not-found-b");
    let validators = vec![PosValidator {
        validator_id: "node-a".to_string(),
        stake: 100,
    }];
    let pos_config = signed_pos_config_with_signer_seeds(validators, &[("node-a", 110)]);
    let network_impl = Arc::new(ProviderNotFoundFallbackTestNetwork::new(dir_a.clone()));
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
        .with_replication(signed_replication_config(dir_a.clone(), 110));
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_replication(signed_replication_config(dir_b.clone(), 111));

    let mut runtime_a = with_noop_execution_hook(NodeRuntime::new(config_a));
    runtime_a.start().expect("start a");
    let reached = wait_until(Instant::now() + Duration::from_secs(2), || {
        runtime_a.snapshot().consensus.committed_height >= 3
    });
    assert!(reached, "sequencer did not reach target height in time");
    let target_height = runtime_a.snapshot().consensus.committed_height;
    runtime_a.stop().expect("stop a");

    let mut commit_map = HashMap::<u64, super::replication::GossipReplicationMessage>::new();
    let replication_runtime_a = super::replication::ReplicationRuntime::new(
        &signed_replication_config(dir_a.clone(), 110),
        "node-a",
    )
    .expect("open replication runtime a");
    for height in 1..=target_height {
        let message = replication_runtime_a
            .load_commit_message_by_height(world_id, height)
            .expect("load commit by height")
            .expect("commit payload");
        commit_map.insert(height, message);
    }
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

    let dht = Arc::new(TestReplicaMaintenanceDht::new(
        "storage-provider-1",
        "observer-provider",
    ));
    let mut runtime_b = NodeRuntime::new(config_b)
        .with_replication_network(
            NodeReplicationNetworkHandle::new(Arc::clone(&network))
                .with_dht(dht)
                .with_local_provider_id("observer-provider"),
        )
        .with_replication_network_consensus_enabled(false);
    runtime_b.start().expect("start b");

    let commit_map = Arc::new(commit_map);
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

    let blocked = wait_until(Instant::now() + Duration::from_secs(3), || {
        runtime_b
            .snapshot()
            .consensus
            .replication_gap_sync_blocked_height
            .is_some()
    });
    let snapshot_b = runtime_b.snapshot();
    let provider_attempts = network_impl.provider_attempts();
    let generic_attempts = network_impl.generic_attempts();
    assert!(
        blocked,
        "observer did not expose provider-route not-found gap-sync block: committed_height={} network_committed_height={} last_error={:?} provider_attempts={provider_attempts:?} generic_attempts={generic_attempts}",
        snapshot_b.consensus.committed_height,
        snapshot_b.consensus.network_committed_height,
        snapshot_b.last_error
    );
    assert!(
        provider_attempts.iter().any(|providers| {
            providers
                .iter()
                .any(|provider| provider == "storage-provider-1")
        }),
        "expected gap sync to try DHT-selected provider before blocking: {provider_attempts:?}"
    );
    assert!(
        provider_attempts.len() <= 1,
        "gap sync should not repeatedly spend request budget on the same not-found provider route: {provider_attempts:?}"
    );
    assert!(
        generic_attempts == 0,
        "gap sync fetch-blob should not retry on generic non-provider lane after provider not-found"
    );
    assert!(
        snapshot_b
            .consensus
            .replication_gap_sync_blocked_reason
            .as_deref()
            .map(|reason| reason.contains("provider route blocked"))
            .unwrap_or(false),
        "expected provider route block reason, got {:?}",
        snapshot_b.consensus.replication_gap_sync_blocked_reason
    );

    runtime_b.stop().expect("stop b");
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}
