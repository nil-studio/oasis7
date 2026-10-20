use oasis7_node::{NodeConsensusSnapshot, NodePeerCommittedHead, NodeRole, NodeSnapshot};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use std::collections::BTreeMap;

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_marks_storage_challenge_network_degraded_not_ready() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 42;
    consensus.network_committed_height = 42;
    consensus.replication_persisted_height = 42;
    consensus.known_peer_heads = 1;
    consensus.storage_challenge_network_degraded_height = Some(42);
    consensus.storage_challenge_network_degraded_reason = Some(
        "storage challenge network degraded: required_matches=2 successful_matches=0 latest_reason=provider lookup failed"
            .to_string(),
    );
    consensus.peer_heads = vec![NodePeerCommittedHead {
        node_id: "storage-peer".to_string(),
        validator_id: None,
        height: 42,
        block_hash: "storage-peer-block".to_string(),
        committed_at_ms: 1_700_000_000_000,
        observed_at_ms: 1_700_000_000_000,
        execution_block_hash: Some("storage-peer-execution".to_string()),
        execution_state_root: Some("storage-peer-state".to_string()),
    }];
    let snapshot = NodeSnapshot {
        node_id: "sequencer-storage-challenge-degraded".to_string(),
        player_id: "player-storage-challenge-degraded".to_string(),
        world_id: "world-storage-challenge-degraded".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let network_head =
        super::status_payload::build_network_head_status(&snapshot, 1_700_000_000_000, None);
    let policy = super::status_payload::readiness_policy(&snapshot, None);
    let observability = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &super::storage_metrics::StorageMetricsSnapshot {
            storage_profile: "dev_local".to_string(),
            effective_budget: StorageProfileConfig::from(StorageProfile::DevLocal),
            bytes_by_dir: BTreeMap::new(),
            blob_counts: BTreeMap::new(),
            ref_count: 0,
            pin_count: 0,
            retained_heights: Vec::new(),
            checkpoint_count: 0,
            replay_summary: super::storage_metrics::StorageReplaySummary::default(),
            orphan_blob_count: 0,
            last_gc_at_ms: None,
            last_gc_result: "not_available".to_string(),
            last_gc_error: None,
            degraded_reason: None,
        },
        &super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
            enabled: false,
            metrics_available: true,
            report_dir: String::new(),
            report_count: 0,
            latest_epoch_index: 0,
            latest_report_observed_at_unix_ms: 0,
            latest_total_distributed_points: 0,
            latest_minted_record_count: 0,
            cumulative_minted_record_count: 0,
            distfs_total_checks: 0,
            distfs_failed_checks: 0,
            distfs_failure_ratio: 0.0,
            settlement_apply_attempts_total: 0,
            settlement_apply_failures_total: 0,
            settlement_apply_failure_ratio: 0.0,
            invariant_ok: true,
            last_error: None,
        },
        &super::ChainReplicationDebugStatus {
            local_peer_id: "peer-local".to_string(),
            connected_peers: vec!["storage-peer".to_string()],
            peer_healths: vec![super::ChainPeerHealthStatus {
                peer_id: "storage-peer".to_string(),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["static_bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            }],
            registered_protocols: vec![
                "/aw/node/replication/fetch-blob/1.0.0".to_string(),
                "/aw/node/replication/fetch-commit/1.0.0".to_string(),
            ],
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::new(),
            connection_events: Vec::new(),
            recent_errors: Vec::new(),
        },
        &network_head,
        &super::observability_tests::sample_observability_p2p_status(),
        &policy,
        None,
        1_700_000_000_000,
    );
    let readiness = super::status_payload::build_readiness_status(&observability, policy.clone());

    assert_eq!(observability.status, "warn");
    assert!(!observability.ready);
    assert!(observability.storage_challenge_network_degraded);
    assert!(observability.alerts.iter().any(|alert| {
        alert.code == "storage_challenge_network_degraded"
            && alert.summary.contains("provider lookup failed")
    }));
    assert_eq!(readiness.status, "not_ready");
    assert!(
        readiness
            .failed_gates
            .iter()
            .any(|gate| gate == "storage_challenge_network_degraded")
    );
}
