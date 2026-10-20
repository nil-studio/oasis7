use oasis7_node::{NodeConsensusSnapshot, NodeRole, NodeSnapshot};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use std::collections::BTreeMap;

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_zeroes_replication_gap_when_replication_disabled() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 0;
    let snapshot = NodeSnapshot {
        node_id: "node-no-replication".to_string(),
        player_id: "player-no-replication".to_string(),
        world_id: "world-no-replication".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: false,
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
    let status = super::status_payload::build_chain_node_observability_status(
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
        &super::ChainReplicationDebugStatus::default(),
        &network_head,
        &super::observability_tests::sample_observability_p2p_status(),
        &policy,
        None,
        1_700_000_000_000,
    );

    assert_eq!(status.status, "ok");
    assert!(!status.replication_enabled);
    assert_eq!(status.replication_persisted_height, 0);
    assert_eq!(status.replication_state_gap, 0);
    assert!(
        !status
            .alerts
            .iter()
            .any(|alert| alert.code == "consensus_replication_state_gap")
    );
}
