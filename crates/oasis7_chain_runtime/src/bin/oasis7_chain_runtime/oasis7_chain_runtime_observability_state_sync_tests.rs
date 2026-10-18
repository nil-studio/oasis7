use super::*;
use oasis7_node::{
    Libp2pReachabilitySnapshot, NodeConsensusSnapshot, NodeNetworkPolicy,
    NodeReachabilityAutoDetection, NodeReplicationGapSyncRouteSnapshot, NodeRole, NodeSnapshot,
    NodeUserMode,
};
use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::ReleaseSecurityPolicy;
use std::collections::BTreeMap;
use std::path::Path;

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_marks_replication_gap_blocked_unhealthy() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 30;
    consensus.replication_persisted_height = 4;
    consensus.replication_gap_sync_blocked_height = Some(5);
    consensus.replication_gap_sync_blocked_reason =
        Some("replication gap sync blocked: missing commit height 5".to_string());
    consensus.replication_gap_sync_repair_attempt_height = Some(5);
    consensus.replication_gap_sync_repair_attempt_summary =
        Some("generic:found=false;peer:p1:found=false".to_string());
    consensus.replication_gap_sync_repair_attempt_route_snapshot =
        Some(NodeReplicationGapSyncRouteSnapshot {
            elapsed_ms: 3_000,
            route_attempt_count: 4,
            synced_route_count: 0,
            not_found_route_count: 0,
            error_route_count: 4,
            generic_route_count: 1,
            provider_route_count: 2,
            generic_retry_route_count: 1,
            budget_exhausted_count: 1,
            last_slow_route_reason: Some("request failed: Timeout".to_string()),
        });
    let snapshot = NodeSnapshot {
        node_id: "node-gap".to_string(),
        player_id: "player-gap".to_string(),
        world_id: "world-gap".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let recommendation = NodeNetworkPolicy::recommend_for_user_mode(
        NodeRole::Sequencer,
        NodeUserMode::PrivateSafe,
        NodeReachabilityAutoDetection::default(),
        false,
    )
    .expect("recommendation");
    let payload = build_chain_status_payload(
        snapshot,
        Path::new("/tmp/execution-world"),
        None,
        None,
        &recommendation,
        None,
        NodeNetworkPolicy {
            deployment_mode: PeerDeploymentMode::Private,
            node_role_claim: PeerNodeRole::ValidatorCore,
        },
        &Libp2pReachabilitySnapshot::default(),
        NodeReachabilityAutoDetection::default(),
        ReleaseSecurityPolicy::default(),
        super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
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
        super::storage_metrics::StorageMetricsSnapshot {
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
        super::observability_tests::sample_wasm_status(),
        None,
        super::ChainTrafficStatus {
            udp_gossip: None,
            libp2p_replication: oasis7_node::Libp2pTrafficMetricsSnapshot::default(),
        },
        super::transfer_submit_api::ChainTransferMetricsStatus {
            tracked_records: 0,
            accepted_count: 0,
            pending_count: 0,
            confirmed_count: 0,
            failed_count: 0,
            timeout_count: 0,
            inflight_count: 0,
            oldest_inflight_age_ms: None,
            recent_confirmation_latency:
                super::transfer_submit_api::ChainTransferLatencySummaryStatus {
                    sample_count: 0,
                    avg_latency_ms: None,
                    max_latency_ms: None,
                    p50_latency_ms: None,
                    p95_latency_ms: None,
                },
        },
        super::ChainReplicationDebugStatus::default(),
    );

    assert!(!payload.ok);
    assert_eq!(payload.observability.status, "critical");
    assert_eq!(payload.observability.replication_state_gap, 6);
    let consensus = &payload.consensus;
    assert_eq!(consensus.replication_gap_sync_blocked_height, Some(5));
    assert!(consensus.state_sync_fallback_required);
    assert!(!consensus.state_sync_snapshot_available);
    assert_eq!(
        consensus.state_sync_trusted_checkpoint_required_height,
        Some(30)
    );
    assert_eq!(
        consensus.replication_gap_sync_repair_attempt_height,
        Some(5)
    );
    assert_eq!(
        consensus
            .replication_gap_sync_repair_attempt_summary
            .as_deref(),
        Some("generic:found=false;peer:p1:found=false")
    );
    let route_snapshot = consensus
        .replication_gap_sync_repair_attempt_route_snapshot
        .as_ref()
        .expect("failed gap-sync route snapshot should be exposed in status payload");
    assert_eq!(route_snapshot.elapsed_ms, 3_000);
    assert_eq!(route_snapshot.route_attempt_count, 4);
    assert_eq!(route_snapshot.error_route_count, 4);
    assert_eq!(route_snapshot.budget_exhausted_count, 1);
    assert_eq!(
        route_snapshot.last_slow_route_reason.as_deref(),
        Some("request failed: Timeout")
    );
    assert!(payload.consensus.consensus_participation_held);
    assert_eq!(
        consensus.consensus_participation_hold_reason.as_deref(),
        Some("replication_gap_sync_blocked_height=5")
    );
    let alert_codes = payload
        .observability
        .alerts
        .iter()
        .map(|alert| alert.code.as_str())
        .collect::<Vec<_>>();
    assert!(alert_codes.contains(&"replication_gap_sync_blocked"));
    assert!(alert_codes.contains(&"consensus_replication_state_gap"));
    assert!(alert_codes.contains(&"consensus_participation_held"));
    assert!(alert_codes.contains(&"state_sync_fallback_required"));
}
