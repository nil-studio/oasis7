#![expect(clippy::too_many_arguments)]

use super::execution_bridge::{
    ExecutionBridgeCommitTimingSnapshot, snapshot_execution_bridge_commit_timing,
};
use super::p2p_status::peer_reachability_as_str;
use super::runtime_authority::RuntimeAuthorityBinding;
use super::runtime_status_util::{consensus_status_to_string, now_unix_ms};
use super::storage_metrics;
use super::traffic_status::ChainTrafficStatus;
use super::wasm_status::ChainWasmStatus;
use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7::simulator::RuntimePerfSnapshot;
use oasis7_node::{
    Libp2pReachabilitySnapshot, NodeNetworkPolicy, NodeReachabilityAutoDetection, NodeSnapshot,
    NodeUserModeRecommendation,
};
use oasis7_runtime::ReleaseSecurityPolicy;
use serde::Serialize;
use std::path::Path;
#[path = "status_payload_chain_proof.rs"]
mod status_payload_chain_proof;
#[path = "status_payload_consensus.rs"]
mod status_payload_consensus;
#[path = "status_payload_module_tick_routing.rs"]
mod status_payload_module_tick_routing;
#[path = "status_payload_network_head.rs"]
mod status_payload_network_head;
#[path = "status_payload_network_tier.rs"]
mod status_payload_network_tier;
#[path = "status_payload_publication.rs"]
mod status_payload_publication;
#[path = "status_payload_runtime_errors.rs"]
mod status_payload_runtime_errors;
#[path = "status_payload_validator_provider.rs"]
mod status_payload_validator_provider;
use status_payload_chain_proof::build_chain_proof_status;
use status_payload_consensus::{
    ChainConsensusStatus, ChainPendingConsensusActionsStatus, ChainPendingProposalStatus,
};
use status_payload_module_tick_routing::{
    ChainModuleTickRoutingStatus, build_module_tick_routing_status,
};
pub(super) use status_payload_network_head::{
    ChainConsensusNetworkHeadStatus, ChainReadinessPolicyStatus, applied_slashing_receipt_hashes,
    build_network_head_status, pending_slashing_intent_count, readiness_policy,
};
#[cfg(test)]
pub(super) use status_payload_publication::publication_lifecycle_rejection_reason;
use status_payload_publication::{
    enforce_retained_publication_proof, push_publication_or_divergence_alert,
};
use status_payload_runtime_errors::push_runtime_error_alerts;
pub(super) use status_payload_validator_provider::{
    ChainStatusResponse, build_chain_provider_status, build_chain_validator_status,
};
#[path = "status_payload_observability.rs"]
mod status_payload_observability;
#[path = "status_payload_state_sync.rs"]
mod status_payload_state_sync;
#[cfg(test)]
pub(super) use status_payload_observability::RuntimePerfGateTier;
use status_payload_observability::{
    ChainFinalityLatencyStatus, ChainInboundTimingRejectionsStatus,
    ChainP2pPathObservabilityStatus, build_path_observability_status,
};
pub(crate) use status_payload_observability::{
    ChainP2pTransportTransition, ChainP2pTransportTransitionCounters, build_liveness_status,
    build_runtime_perf_observability_status,
    build_runtime_perf_snapshot_from_execution_bridge_timing, classify_transport_stability,
    observability_status_for_alerts, observability_summary_for_alerts,
    push_local_chain_ahead_alert, push_observability_alert, reachability_policy_ok,
    runtime_perf_gate_tier_from_manifest, sequencer_head_publication_pending_summary,
};
use status_payload_state_sync::{
    consensus_participation_hold_reason, state_sync_fallback_reason,
    state_sync_trusted_checkpoint_required_height,
};
#[path = "status_payload_world_resource.rs"]
mod status_payload_world_resource;
use status_payload_world_resource::build_world_resource_status_with_authoritative_execution;
#[path = "status_payload_p2p.rs"]
mod status_payload_p2p;
pub(super) use status_payload_p2p::{
    ChainP2pStatus, build_chain_p2p_status, build_readiness_status, build_sync_status,
};
#[path = "status_payload_lifecycle.rs"]
mod status_payload_lifecycle;
pub(super) use status_payload_lifecycle::{
    ChainLivenessStatus, ChainReadinessStatus, ChainSyncStatus,
};

const TRANSPORT_STABILITY_MIN_SCORE: u8 = 70;
const CONSENSUS_FINALITY_LATENCY_BUDGET_MS: i64 = 1_000;
const CONSENSUS_FINALITY_LATENCY_MIN_SAMPLES: usize = 4;
const TRANSFER_LIFECYCLE_DEGRADED_MIN_SAMPLES: usize = 4;
const TRANSFER_LIFECYCLE_DEGRADED_RATIO_PPM: u64 = 50_000;
const MODULE_TICK_SLOW_ROUTE_MIN_SAMPLES: u64 = 4;
const MODULE_TICK_SLOW_ROUTE_RATIO_PPM: u64 = 50_000;
const UDP_GOSSIP_SEND_FAILURE_MIN_ATTEMPTS: u64 = 4;
const UDP_GOSSIP_SEND_FAILURE_RATIO_PPM: u64 = 50_000;
#[derive(Debug, Serialize)]
pub(super) struct ChainNodeObservabilityStatus {
    pub(super) status: String,
    pub(super) summary: String,
    pub(super) ready: bool,
    pub(super) path_observability: ChainP2pPathObservabilityStatus,
    pub(super) connected_peer_count: usize,
    pub(super) active_peer_count: usize,
    pub(super) candidate_peer_count: usize,
    pub(super) suspect_peer_count: usize,
    pub(super) blocked_peer_count: usize,
    pub(super) peer_with_issues_count: usize,
    pub(super) known_peer_heads: usize,
    pub(super) network_head_available: bool,
    pub(super) network_height_lag: u64,
    pub(super) transport_stable: bool,
    pub(super) transport_stability_score: u8,
    pub(super) reachability_policy_ok: bool,
    pub(super) misbehavior_evidence_count: usize,
    pub(super) slashing_intent_count: usize,
    pub(super) pending_slashing_intent_count: usize,
    pub(super) slashing_receipt_count: usize,
    pub(super) applied_slashing_receipt_count: usize,
    pub(super) quarantined_validator_count: usize,
    pub(super) slashable_stake_total: u64,
    pub(super) replication_enabled: bool,
    pub(super) replication_persisted_height: u64,
    pub(super) replication_state_gap: u64,
    pub(super) replication_gap_sync_blocked_height: Option<u64>,
    pub(super) storage_challenge_network_degraded: bool,
    pub(super) recent_replication_error_count: usize,
    pub(super) storage_degraded: bool,
    pub(super) reward_runtime_degraded: bool,
    pub(super) runtime_perf_available: bool,
    pub(super) runtime_perf_health: String,
    pub(super) runtime_perf_bottleneck: String,
    pub(super) runtime_perf_degraded: bool,
    pub(super) alerts: Vec<ChainNodeObservabilityAlert>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChainReplicationTransportStability {
    pub(super) stable: bool,
    pub(super) score: u8,
    pub(super) recent_error_count: usize,
    pub(super) blocking_error_count: usize,
    pub(super) connection_closed_count: usize,
    pub(super) insufficient_peers_count: usize,
    pub(super) timeout_count: usize,
    pub(super) protocol_error_count: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct ChainNodeObservabilityAlert {
    pub(super) severity: String,
    pub(super) code: String,
    pub(super) summary: String,
}

#[cfg(test)]
pub(super) fn build_chain_node_observability_status(
    snapshot: &NodeSnapshot,
    storage_metrics: &storage_metrics::StorageMetricsSnapshot,
    reward_runtime_metrics: &super::reward_runtime_worker::RewardRuntimeMetricsSnapshot,
    replication: &super::ChainReplicationDebugStatus,
    network_head: &ChainConsensusNetworkHeadStatus,
    p2p: &ChainP2pStatus,
    policy: &ChainReadinessPolicyStatus,
    runtime_perf: Option<&RuntimePerfSnapshot>,
    observed_at_unix_ms: i64,
) -> ChainNodeObservabilityStatus {
    build_chain_node_observability_status_with_transactions(
        snapshot,
        storage_metrics,
        reward_runtime_metrics,
        replication,
        network_head,
        p2p,
        policy,
        runtime_perf,
        None,
        None,
        None,
        observed_at_unix_ms,
    )
}

fn build_chain_node_observability_status_with_transactions(
    snapshot: &NodeSnapshot,
    storage_metrics: &storage_metrics::StorageMetricsSnapshot,
    reward_runtime_metrics: &super::reward_runtime_worker::RewardRuntimeMetricsSnapshot,
    replication: &super::ChainReplicationDebugStatus,
    network_head: &ChainConsensusNetworkHeadStatus,
    p2p: &ChainP2pStatus,
    policy: &ChainReadinessPolicyStatus,
    runtime_perf: Option<&RuntimePerfSnapshot>,
    transactions: Option<&super::transfer_submit_api::ChainTransferMetricsStatus>,
    module_tick_routing: Option<&ChainModuleTickRoutingStatus>,
    wasm: Option<&ChainWasmStatus>,
    observed_at_unix_ms: i64,
) -> ChainNodeObservabilityStatus {
    let connected_peer_count = replication.connected_peers.len();
    let mut active_peer_count = 0usize;
    let mut candidate_peer_count = 0usize;
    let mut suspect_peer_count = 0usize;
    let mut blocked_peer_count = 0usize;
    for health in &replication.peer_healths {
        match health.status.as_str() {
            "active" => active_peer_count += 1,
            "candidate" => candidate_peer_count += 1,
            "suspect" => suspect_peer_count += 1,
            "blocked" => blocked_peer_count += 1,
            _ => {}
        }
    }
    let peer_with_issues_count = replication
        .peer_healths
        .iter()
        .filter(|health| !health.issues.is_empty())
        .count();
    let active_peer_available = active_peer_count > 0 || connected_peer_count > 0;
    let known_peer_heads = snapshot.consensus.known_peer_heads;
    let network_head_available = network_head.decision == "ready"
        && match network_head.source.as_str() {
            "peer_quorum" | "peer_single" => true,
            "self_only" => !snapshot.replication_enabled || active_peer_available,
            _ => false,
        };
    let network_height_lag = snapshot
        .consensus
        .network_committed_height
        .saturating_sub(snapshot.consensus.committed_height);
    let replication_state_gap = if snapshot.replication_enabled {
        snapshot
            .consensus
            .committed_height
            .saturating_sub(snapshot.consensus.replication_persisted_height)
    } else {
        0
    };
    let consensus_participation_hold_reason = consensus_participation_hold_reason(
        snapshot,
        network_height_lag,
        replication_state_gap,
        policy.max_network_height_lag,
    );
    let state_sync_fallback_reason = state_sync_fallback_reason(
        snapshot,
        replication_state_gap,
        network_height_lag,
        policy.max_network_height_lag,
    );
    let recent_replication_error_count = replication.recent_errors.len();
    let transport_stability = classify_transport_stability(replication);
    let blocking_replication_error_count = transport_stability.blocking_error_count;
    let reachability_policy_ok = reachability_policy_ok(snapshot, p2p, active_peer_count, policy);
    let path_observability = build_path_observability_status(p2p, observed_at_unix_ms);
    let storage_degraded = storage_metrics.degraded_reason.is_some()
        || matches!(storage_metrics.last_gc_result.as_str(), "failed");
    let storage_challenge_network_degraded = snapshot
        .consensus
        .storage_challenge_network_degraded_height
        .is_some();
    let reward_runtime_degraded = reward_runtime_metrics.enabled
        && (!reward_runtime_metrics.metrics_available
            || !reward_runtime_metrics.invariant_ok
            || reward_runtime_metrics.last_error.is_some());
    let slashable_stake_total = snapshot
        .consensus
        .misbehavior_evidence
        .iter()
        .map(|evidence| evidence.slashable_stake)
        .sum::<u64>();
    let mut alerts = Vec::new();
    if let Some(metrics) = module_tick_routing.and_then(|status| status.metrics.as_ref()) {
        let metric_u64 = |key: &str| {
            metrics
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        };
        let last_missing_invocation_count = metric_u64("last_missing_invocation_count");
        let oldest_overdue_ticks = metric_u64("oldest_overdue_ticks");
        if last_missing_invocation_count > 0 || oldest_overdue_ticks > 0 {
            push_observability_alert(
                &mut alerts,
                "warn",
                "module_tick_routing_degraded",
                format!(
                    "module tick routing degraded: last_missing_invocation_count={last_missing_invocation_count} oldest_overdue_ticks={oldest_overdue_ticks}"
                ),
            );
        }
    }
    if let Some(metrics) = module_tick_routing.and_then(|status| status.live_metrics.as_ref()) {
        let metric_u64 = |key: &str| {
            metrics
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        };
        let routing_count = metric_u64("routing_count");
        let slow_route_count = metrics
            .get("duration_buckets")
            .and_then(|buckets| buckets.get("ge_100ms"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let slow_route_ratio_ppm = slow_route_count
            .saturating_mul(1_000_000)
            .checked_div(routing_count)
            .unwrap_or(0);
        if routing_count >= MODULE_TICK_SLOW_ROUTE_MIN_SAMPLES
            && slow_route_ratio_ppm >= MODULE_TICK_SLOW_ROUTE_RATIO_PPM
        {
            push_observability_alert(
                &mut alerts,
                "warn",
                "module_tick_routing_degraded",
                format!(
                    "module tick routing degraded: sustained_slow_routes={slow_route_count} routing_count={routing_count} slow_route_ratio_ppm={slow_route_ratio_ppm} slow_route_budget_ms=100"
                ),
            );
        }
    }
    if let Some(reason) = wasm.and_then(|status| {
        status
            .degraded_reason
            .as_deref()
            .or(status.build.degraded_reason.as_deref())
    }) {
        push_observability_alert(
            &mut alerts,
            "warn",
            "wasm_observability_degraded",
            format!("WASM observability degraded: {reason}"),
        );
    }
    let finality_latency = &snapshot.consensus.recent_finality_latency;
    if finality_latency.sample_count >= CONSENSUS_FINALITY_LATENCY_MIN_SAMPLES
        && finality_latency
            .p95_latency_ms
            .is_some_and(|p95_ms| p95_ms > CONSENSUS_FINALITY_LATENCY_BUDGET_MS)
    {
        push_observability_alert(
            &mut alerts,
            "warn",
            "consensus_finality_latency_degraded",
            format!(
                "consensus finality latency degraded: sample_count={} finality_p95_ms={} finality_budget_ms={}",
                finality_latency.sample_count,
                finality_latency.p95_latency_ms.unwrap_or_default(),
                CONSENSUS_FINALITY_LATENCY_BUDGET_MS
            ),
        );
    }
    if let Some(transactions) = transactions {
        let failure_count = transactions
            .failed_count
            .saturating_add(transactions.timeout_count);
        let failure_ratio_ppm = if transactions.tracked_records > 0 {
            failure_count
                .saturating_mul(1_000_000)
                .saturating_div(transactions.tracked_records) as u64
        } else {
            0
        };
        if transactions.tracked_records >= TRANSFER_LIFECYCLE_DEGRADED_MIN_SAMPLES
            && failure_ratio_ppm >= TRANSFER_LIFECYCLE_DEGRADED_RATIO_PPM
        {
            let dominant_error_code = if transactions.timeout_count >= transactions.failed_count {
                "transfer_timeout"
            } else {
                "transfer_failed"
            };
            push_observability_alert(
                &mut alerts,
                "warn",
                "transfer_lifecycle_degraded",
                format!(
                    "transfer lifecycle degraded: failure_ratio_ppm={failure_ratio_ppm} dominant_error_code={dominant_error_code}"
                ),
            );
        }
    }
    push_runtime_error_alerts(&mut alerts, snapshot);
    if network_height_lag > policy.max_network_height_lag {
        push_observability_alert(
            &mut alerts,
            "warn",
            "consensus_network_lag",
            format!(
                "network committed height is ahead by {network_height_lag}; allowed lag is {}",
                policy.max_network_height_lag
            ),
        );
    }
    push_publication_or_divergence_alert(
        &mut alerts,
        snapshot,
        network_head,
        policy,
        observed_at_unix_ms,
    );
    if network_height_lag > 0
        && snapshot
            .consensus
            .last_committed_at_ms
            .map(|last_ms| observed_at_unix_ms.saturating_sub(last_ms).max(0))
            .map(|age_ms| age_ms > policy.sync_stalled_after_ms)
            .unwrap_or(false)
    {
        push_observability_alert(
            &mut alerts,
            "critical",
            "consensus_sync_stalled",
            format!(
                "sync lag remains {} and last commit age exceeded {}ms",
                network_height_lag, policy.sync_stalled_after_ms
            ),
        );
    }
    if snapshot.replication_enabled
        && network_head.required_peer_count > 0
        && network_head.fresh_peer_count == 0
    {
        push_observability_alert(
            &mut alerts,
            "warn",
            "consensus_peer_head_unavailable",
            "network head is unknown because no peer committed heads are visible".to_string(),
        );
    }
    if network_head.stale_peer_count > 0 {
        push_observability_alert(
            &mut alerts,
            "warn",
            "consensus_peer_head_stale",
            format!(
                "{} peer committed head(s) exceeded freshness ttl {}ms",
                network_head.stale_peer_count, network_head.freshness_ttl_ms
            ),
        );
    }
    if snapshot.replication_enabled
        && policy.quorum_mode != "stake_weighted"
        && network_head.fresh_peer_count > 0
        && network_head.fresh_peer_count < network_head.required_peer_count
    {
        push_observability_alert(
            &mut alerts,
            "warn",
            "consensus_peer_head_quorum_missing",
            format!(
                "fresh peer head quorum missing: observed={} required={}",
                network_head.fresh_peer_count, network_head.required_peer_count
            ),
        );
    }
    if network_head.conflicting_peer_count > 0 {
        push_observability_alert(
            &mut alerts,
            "critical",
            "consensus_peer_head_conflict",
            format!(
                "{} fresh peer head(s) conflict at the same height",
                network_head.conflicting_peer_count
            ),
        );
    }
    if policy.quorum_mode == "stake_weighted" && !network_head.stake_quorum_met {
        push_observability_alert(
            &mut alerts,
            "critical",
            "consensus_stake_quorum_missing",
            format!(
                "stake-weighted head quorum missing: observed_stake={} required_stake={} total_stake={}",
                network_head.observed_stake, network_head.required_stake, network_head.total_stake
            ),
        );
    }
    if policy.quorum_mode == "count_fallback_stake_unavailable" {
        push_observability_alert(
            &mut alerts,
            "critical",
            "consensus_stake_quorum_unavailable",
            "mainnet validator readiness requires stake-weighted peer head quorum, but validator stake snapshot is unavailable".to_string(),
        );
    }
    if !snapshot.consensus.misbehavior_evidence.is_empty() {
        push_observability_alert(
            &mut alerts,
            "critical",
            "consensus_misbehavior_evidence_present",
            format!(
                "consensus misbehavior evidence present: count={} quarantined_validators={} slashable_stake_total={}",
                snapshot.consensus.misbehavior_evidence.len(),
                snapshot.consensus.quarantined_validators.len(),
                slashable_stake_total
            ),
        );
    }
    if !snapshot.consensus.quarantined_validators.is_empty() {
        push_observability_alert(
            &mut alerts,
            "critical",
            "consensus_validator_quarantined",
            format!(
                "{} validator(s) are quarantined from peer-head quorum",
                snapshot.consensus.quarantined_validators.len()
            ),
        );
    }
    let applied_slashing_receipts = applied_slashing_receipt_hashes(snapshot);
    let applied_slashing_receipt_count = applied_slashing_receipts.len();
    let pending_slashing_intent_count = pending_slashing_intent_count(snapshot);
    if pending_slashing_intent_count > 0 {
        push_observability_alert(
            &mut alerts,
            "critical",
            "consensus_slashing_intent_pending",
            format!(
                "{pending_slashing_intent_count} consensus slashing intent(s) are pending governance identity penalty submission"
            ),
        );
    }
    if let Some(height) = snapshot.consensus.replication_gap_sync_blocked_height {
        let reason = snapshot
            .consensus
            .replication_gap_sync_blocked_reason
            .clone()
            .unwrap_or_else(|| format!("replication gap sync blocked at height {height}"));
        push_observability_alert(
            &mut alerts,
            "critical",
            "replication_gap_sync_blocked",
            reason,
        );
    }
    if let Some(height) = snapshot.consensus.storage_challenge_network_degraded_height {
        let reason = snapshot
            .consensus
            .storage_challenge_network_degraded_reason
            .clone()
            .unwrap_or_else(|| {
                format!("storage challenge network degraded at committed height {height}")
            });
        push_observability_alert(
            &mut alerts,
            "warn",
            "storage_challenge_network_degraded",
            reason,
        );
    }
    if snapshot.replication_enabled && replication_state_gap > 0 {
        push_observability_alert(
            &mut alerts,
            "critical",
            "consensus_replication_state_gap",
            format!(
                "consensus committed height {} is ahead of contiguous replication persisted height {} by {}",
                snapshot.consensus.committed_height,
                snapshot.consensus.replication_persisted_height,
                replication_state_gap
            ),
        );
    }
    if let Some(reason) = consensus_participation_hold_reason.as_ref() {
        push_observability_alert(
            &mut alerts,
            "critical",
            "consensus_participation_held",
            format!("local consensus participation is held until verified sync recovers: {reason}"),
        );
    }
    if let Some(reason) = state_sync_fallback_reason.as_ref() {
        let severity = if storage_metrics.checkpoint_count > 0 {
            "warn"
        } else {
            "critical"
        };
        push_observability_alert(
            &mut alerts,
            severity,
            "state_sync_fallback_required",
            reason.clone(),
        );
    }
    if suspect_peer_count > 0 || blocked_peer_count > 0 || peer_with_issues_count > 0 {
        push_observability_alert(
            &mut alerts,
            "warn",
            "replication_peer_health_degraded",
            format!(
                "peer health degraded: suspect={suspect_peer_count}, blocked={blocked_peer_count}, peers_with_issues={peer_with_issues_count}"
            ),
        );
    }
    if snapshot.replication_enabled && !active_peer_available {
        push_observability_alert(
            &mut alerts,
            "warn",
            "replication_no_connected_peers",
            "replication has no active or connected peers".to_string(),
        );
    }
    if blocking_replication_error_count > 0 {
        push_observability_alert(
            &mut alerts,
            "warn",
            "replication_recent_errors",
            format!(
                "replication reported {blocking_replication_error_count} recent blocking transport/protocol errors"
            ),
        );
    }
    if !transport_stability.stable {
        push_observability_alert(
            &mut alerts,
            "warn",
            "replication_transport_unstable",
            format!(
                "replication transport stability score {} below threshold {}: connection_closed={}, insufficient_peers={}, timeout={}, protocol_errors={}",
                transport_stability.score,
                TRANSPORT_STABILITY_MIN_SCORE,
                transport_stability.connection_closed_count,
                transport_stability.insufficient_peers_count,
                transport_stability.timeout_count,
                transport_stability.protocol_error_count
            ),
        );
    }
    if !reachability_policy_ok {
        push_observability_alert(
            &mut alerts,
            "warn",
            "p2p_reachability_degraded",
            format!(
                "p2p reachability does not satisfy role policy: role={} detected={:?} autonat={} public_port={} relay_available={}",
                snapshot.role.as_str(),
                p2p.detected_reachability,
                p2p.autonat_status,
                p2p.public_port_reachability,
                p2p.relay_available
            ),
        );
    }
    if policy.tier == "mainnet" && policy.slashing_policy == "evidence_only_readiness_gate" {
        let evidence_count = snapshot.consensus.misbehavior_evidence.len();
        push_observability_alert(
            &mut alerts,
            "warn",
            "mainnet_slashing_evidence_only",
            format!(
                "mainnet slashing policy is evidence-only in readiness and does not execute protocol penalties; evidence_count={evidence_count}, slashable_stake_total={slashable_stake_total}"
            ),
        );
    }
    if storage_degraded {
        let reason = storage_metrics
            .degraded_reason
            .clone()
            .or_else(|| {
                (storage_metrics.last_gc_result == "failed")
                    .then(|| "latest GC result is failed".to_string())
            })
            .unwrap_or_else(|| "storage reported degraded state".to_string());
        push_observability_alert(
            &mut alerts,
            "warn",
            "storage_degraded",
            format!("storage degraded: {reason}"),
        );
    }
    if reward_runtime_degraded {
        let reason = reward_runtime_metrics
            .last_error
            .clone()
            .unwrap_or_else(|| {
                if !reward_runtime_metrics.metrics_available {
                    "reward runtime metrics unavailable".to_string()
                } else if !reward_runtime_metrics.invariant_ok {
                    "reward runtime invariant failed".to_string()
                } else {
                    "reward runtime degraded".to_string()
                }
            });
        push_observability_alert(
            &mut alerts,
            "warn",
            "reward_runtime_degraded",
            format!("reward runtime degraded: {reason}"),
        );
    }
    if network_head.source == "authority_activation_planned" {
        push_observability_alert(
            &mut alerts,
            "critical",
            "authority_activation_planned",
            "manifest policy is planned and unsupported; prototype readiness cannot activate it"
                .into(),
        );
    }
    let runtime_perf_observability =
        build_runtime_perf_observability_status(runtime_perf, &mut alerts);

    let status = observability_status_for_alerts(alerts.as_slice());
    let ready = status != "critical"
        && !storage_degraded
        && (!snapshot.replication_enabled || network_head_available)
        && !storage_challenge_network_degraded
        && network_head.decision == "ready"
        && transport_stability.stable
        && reachability_policy_ok
        && network_height_lag <= policy.max_network_height_lag
        && policy.quorum_mode != "count_fallback_stake_unavailable";
    ChainNodeObservabilityStatus {
        status,
        summary: observability_summary_for_alerts(alerts.as_slice()),
        ready,
        path_observability,
        connected_peer_count,
        active_peer_count,
        candidate_peer_count,
        suspect_peer_count,
        blocked_peer_count,
        peer_with_issues_count,
        known_peer_heads,
        network_head_available,
        network_height_lag,
        transport_stable: transport_stability.stable,
        transport_stability_score: transport_stability.score,
        reachability_policy_ok,
        misbehavior_evidence_count: snapshot.consensus.misbehavior_evidence.len(),
        slashing_intent_count: snapshot.consensus.slashing_intents.len(),
        pending_slashing_intent_count,
        slashing_receipt_count: snapshot.consensus.slashing_receipts.len(),
        applied_slashing_receipt_count,
        quarantined_validator_count: snapshot.consensus.quarantined_validators.len(),
        slashable_stake_total,
        replication_enabled: snapshot.replication_enabled,
        replication_persisted_height: snapshot.consensus.replication_persisted_height,
        replication_state_gap,
        replication_gap_sync_blocked_height: snapshot.consensus.replication_gap_sync_blocked_height,
        storage_challenge_network_degraded,
        recent_replication_error_count,
        storage_degraded,
        reward_runtime_degraded,
        runtime_perf_available: runtime_perf_observability.available,
        runtime_perf_health: runtime_perf_observability.health,
        runtime_perf_bottleneck: runtime_perf_observability.bottleneck,
        runtime_perf_degraded: runtime_perf_observability.degraded,
        alerts,
    }
}

#[cfg(test)]
pub(super) fn build_chain_status_payload(
    snapshot: NodeSnapshot,
    execution_world_dir: &Path,
    execution_records_dir: Option<&Path>,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
    live_p2p_recommendation: &NodeUserModeRecommendation,
    applied_effective_user_mode: Option<String>,
    effective_p2p_policy: NodeNetworkPolicy,
    live_snapshot: &Libp2pReachabilitySnapshot,
    p2p_detection: NodeReachabilityAutoDetection,
    release_security_policy: ReleaseSecurityPolicy,
    reward_runtime_metrics: super::reward_runtime_worker::RewardRuntimeMetricsSnapshot,
    storage_metrics: storage_metrics::StorageMetricsSnapshot,
    wasm: ChainWasmStatus,
    runtime_perf: Option<RuntimePerfSnapshot>,
    traffic: ChainTrafficStatus,
    transactions: super::transfer_submit_api::ChainTransferMetricsStatus,
    replication: super::ChainReplicationDebugStatus,
) -> ChainStatusResponse {
    let derived_storage_root = execution_records_dir
        .and_then(Path::parent)
        .map(|runtime_root| runtime_root.join("store"));
    build_chain_status_payload_with_storage_root(
        snapshot,
        execution_world_dir,
        execution_records_dir,
        derived_storage_root.as_deref(),
        loaded_network_tier_manifest,
        live_p2p_recommendation,
        applied_effective_user_mode,
        effective_p2p_policy,
        live_snapshot,
        p2p_detection,
        release_security_policy,
        reward_runtime_metrics,
        storage_metrics,
        wasm,
        runtime_perf,
        traffic,
        transactions,
        replication,
    )
}

#[cfg(test)]
pub(super) fn build_chain_status_payload_with_storage_root(
    snapshot: NodeSnapshot,
    execution_world_dir: &Path,
    execution_records_dir: Option<&Path>,
    execution_storage_root: Option<&Path>,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
    live_p2p_recommendation: &NodeUserModeRecommendation,
    applied_effective_user_mode: Option<String>,
    effective_p2p_policy: NodeNetworkPolicy,
    live_snapshot: &Libp2pReachabilitySnapshot,
    p2p_detection: NodeReachabilityAutoDetection,
    release_security_policy: ReleaseSecurityPolicy,
    reward_runtime_metrics: super::reward_runtime_worker::RewardRuntimeMetricsSnapshot,
    storage_metrics: storage_metrics::StorageMetricsSnapshot,
    wasm: ChainWasmStatus,
    runtime_perf: Option<RuntimePerfSnapshot>,
    traffic: ChainTrafficStatus,
    transactions: super::transfer_submit_api::ChainTransferMetricsStatus,
    replication: super::ChainReplicationDebugStatus,
) -> ChainStatusResponse {
    build_chain_status_payload_with_storage_root_and_authority(
        snapshot,
        execution_world_dir,
        execution_records_dir,
        execution_storage_root,
        loaded_network_tier_manifest,
        live_p2p_recommendation,
        applied_effective_user_mode,
        effective_p2p_policy,
        live_snapshot,
        p2p_detection,
        release_security_policy,
        reward_runtime_metrics,
        storage_metrics,
        wasm,
        runtime_perf,
        traffic,
        transactions,
        replication,
        None,
    )
}

pub(super) fn build_chain_status_payload_with_storage_root_and_authority(
    snapshot: NodeSnapshot,
    execution_world_dir: &Path,
    execution_records_dir: Option<&Path>,
    execution_storage_root: Option<&Path>,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
    live_p2p_recommendation: &NodeUserModeRecommendation,
    applied_effective_user_mode: Option<String>,
    effective_p2p_policy: NodeNetworkPolicy,
    live_snapshot: &Libp2pReachabilitySnapshot,
    p2p_detection: NodeReachabilityAutoDetection,
    release_security_policy: ReleaseSecurityPolicy,
    reward_runtime_metrics: super::reward_runtime_worker::RewardRuntimeMetricsSnapshot,
    storage_metrics: storage_metrics::StorageMetricsSnapshot,
    wasm: ChainWasmStatus,
    runtime_perf: Option<RuntimePerfSnapshot>,
    traffic: ChainTrafficStatus,
    transactions: super::transfer_submit_api::ChainTransferMetricsStatus,
    replication: super::ChainReplicationDebugStatus,
    authority_binding: Option<&RuntimeAuthorityBinding>,
) -> ChainStatusResponse {
    let observed_at_unix_ms = now_unix_ms();
    let p2p = build_chain_p2p_status(
        live_p2p_recommendation,
        applied_effective_user_mode,
        effective_p2p_policy,
        live_snapshot,
        p2p_detection,
    );
    let clamped_elapsed_ms = |prior_ms: i64| -> Option<i64> {
        observed_at_unix_ms
            .checked_sub(prior_ms)
            .map(|age_ms| age_ms.max(0))
    };
    let last_status = snapshot
        .consensus
        .last_status
        .map(consensus_status_to_string);
    let readiness_policy = readiness_policy(&snapshot, loaded_network_tier_manifest);
    let network_head =
        build_network_head_status(&snapshot, observed_at_unix_ms, loaded_network_tier_manifest);
    let module_tick_routing = build_module_tick_routing_status(execution_world_dir);
    let mut observability = build_chain_node_observability_status_with_transactions(
        &snapshot,
        &storage_metrics,
        &reward_runtime_metrics,
        &replication,
        &network_head,
        &p2p,
        &readiness_policy,
        runtime_perf.as_ref(),
        Some(&transactions),
        Some(&module_tick_routing),
        Some(&wasm),
        observed_at_unix_ms,
    );
    enforce_retained_publication_proof(
        &snapshot,
        &network_head,
        &readiness_policy,
        execution_world_dir,
        execution_records_dir,
        observed_at_unix_ms,
        &mut observability,
    );
    if let Some(gossip) = traffic.udp_gossip.as_ref() {
        let outbound = &gossip.totals.outbound;
        if outbound.attempted_datagrams >= UDP_GOSSIP_SEND_FAILURE_MIN_ATTEMPTS
            && outbound.failure_ratio_ppm >= UDP_GOSSIP_SEND_FAILURE_RATIO_PPM
        {
            let dominant_error = gossip
                .by_error_kind
                .iter()
                .max_by_key(|(_, count)| *count)
                .map(|(kind, count)| format!(" dominant_error={kind} count={count}"))
                .unwrap_or_default();
            push_observability_alert(
                &mut observability.alerts,
                "warn",
                "udp_gossip_send_failures",
                format!(
                    "udp gossip send failures: attempted={} succeeded={} failed={} failure_ratio_ppm={}{}",
                    outbound.attempted_datagrams,
                    outbound.succeeded_datagrams,
                    outbound.failed_datagrams,
                    outbound.failure_ratio_ppm,
                    dominant_error,
                ),
            );
            observability.status = observability_status_for_alerts(observability.alerts.as_slice());
            observability.summary =
                observability_summary_for_alerts(observability.alerts.as_slice());
        }
    }
    let consensus_participation_hold_reason = consensus_participation_hold_reason(
        &snapshot,
        observability.network_height_lag,
        observability.replication_state_gap,
        readiness_policy.max_network_height_lag,
    );
    let consensus_participation_held = consensus_participation_hold_reason.is_some();
    let state_sync_fallback_reason = state_sync_fallback_reason(
        &snapshot,
        observability.replication_state_gap,
        observability.network_height_lag,
        readiness_policy.max_network_height_lag,
    );
    let state_sync_trusted_checkpoint_required_height =
        state_sync_trusted_checkpoint_required_height(
            &snapshot,
            observability.replication_state_gap,
            observability.network_height_lag,
            readiness_policy.max_network_height_lag,
        );
    let state_sync_fallback_required = state_sync_fallback_reason.is_some();
    let state_sync_snapshot_available = storage_metrics.checkpoint_count > 0;
    let liveness = build_liveness_status(&snapshot);
    let readiness = build_readiness_status(&observability, readiness_policy.clone());
    let sync = build_sync_status(
        &network_head,
        observability.network_height_lag,
        &readiness_policy,
        &snapshot,
        observed_at_unix_ms,
    );
    let last_commit_age_ms = snapshot
        .consensus
        .last_committed_at_ms
        .and_then(clamped_elapsed_ms);
    let slashable_stake_total = snapshot
        .consensus
        .misbehavior_evidence
        .iter()
        .map(|evidence| evidence.slashable_stake)
        .sum::<u64>();
    let pending_slashing_intent_count = pending_slashing_intent_count(&snapshot);
    let applied_slashing_receipt_count = applied_slashing_receipt_hashes(&snapshot).len();
    let world_resource = build_world_resource_status_with_authoritative_execution(
        &snapshot,
        execution_world_dir,
        execution_records_dir,
        execution_storage_root,
        loaded_network_tier_manifest,
    );
    let chain_proof = build_chain_proof_status(execution_records_dir, execution_storage_root);
    let validator = build_chain_validator_status(&snapshot, authority_binding);
    let provider = build_chain_provider_status(
        &snapshot,
        &world_resource,
        &chain_proof,
        &storage_metrics,
        &replication,
        authority_binding,
    );
    let execution_bridge_commit_timing = snapshot_execution_bridge_commit_timing();
    let pending_proposal = snapshot
        .consensus
        .pending_proposal
        .as_ref()
        .map(|proposal| ChainPendingProposalStatus {
            height: proposal.height,
            slot: proposal.slot,
            epoch: proposal.epoch,
            proposer_id: proposal.proposer_id.clone(),
            opened_at_ms: proposal.opened_at_ms,
            age_ms: clamped_elapsed_ms(proposal.opened_at_ms).unwrap_or(0),
            action_count: proposal.action_count,
            action_payload_bytes: proposal.action_payload_bytes,
            attestation_count: proposal.attestation_count,
            approved_stake: proposal.approved_stake,
            rejected_stake: proposal.rejected_stake,
            required_stake: proposal.required_stake,
            total_stake: proposal.total_stake,
            approval_progress_bps: proposal.approval_progress_bps,
            rejection_progress_bps: proposal.rejection_progress_bps,
            remaining_approval_stake: proposal.remaining_approval_stake,
            status: consensus_status_to_string(proposal.status),
        });

    ChainStatusResponse {
        ok: observability.ready,
        observed_at_unix_ms,
        node_id: snapshot.node_id,
        world_id: snapshot.world_id,
        role: snapshot.role.as_str().to_string(),
        running: snapshot.running,
        liveness,
        readiness,
        sync,
        worker_poll_count: snapshot.tick_count,
        tick_count: snapshot.tick_count,
        last_tick_unix_ms: snapshot.last_tick_unix_ms,
        network_tier: loaded_network_tier_manifest
            .map(status_payload_network_tier::build_network_tier_status),
        world_resource,
        consensus: ChainConsensusStatus {
            slot: snapshot.consensus.slot,
            epoch: snapshot.consensus.epoch,
            ticks_per_slot: snapshot.consensus.ticks_per_slot,
            tick_phase: snapshot.consensus.tick_phase,
            proposal_tick_phase: snapshot.consensus.proposal_tick_phase,
            last_observed_slot: snapshot.consensus.last_observed_slot,
            missed_slot_count: snapshot.consensus.missed_slot_count,
            last_observed_tick: snapshot.consensus.last_observed_tick,
            missed_tick_count: snapshot.consensus.missed_tick_count,
            adaptive_tick_scheduler_enabled: snapshot.consensus.adaptive_tick_scheduler_enabled,
            latest_height: snapshot.consensus.latest_height,
            committed_height: snapshot.consensus.committed_height,
            last_committed_at_ms: snapshot.consensus.last_committed_at_ms,
            last_commit_age_ms,
            network_committed_height: snapshot.consensus.network_committed_height,
            replication_enabled: snapshot.replication_enabled,
            replication_persisted_height: snapshot.consensus.replication_persisted_height,
            replication_gap_sync_blocked_height: snapshot
                .consensus
                .replication_gap_sync_blocked_height,
            replication_gap_sync_blocked_reason: snapshot
                .consensus
                .replication_gap_sync_blocked_reason
                .clone(),
            replication_gap_sync_repair_attempt_height: snapshot
                .consensus
                .replication_gap_sync_repair_attempt_height,
            replication_gap_sync_repair_attempt_summary: snapshot
                .consensus
                .replication_gap_sync_repair_attempt_summary
                .clone(),
            replication_gap_sync_repair_attempt_route_snapshot: snapshot
                .consensus
                .replication_gap_sync_repair_attempt_route_snapshot
                .clone(),
            storage_challenge_network_degraded_height: snapshot
                .consensus
                .storage_challenge_network_degraded_height,
            storage_challenge_network_degraded_reason: snapshot
                .consensus
                .storage_challenge_network_degraded_reason
                .clone(),
            state_sync_fallback_required,
            state_sync_snapshot_available,
            state_sync_trusted_checkpoint_required_height,
            state_sync_fallback_reason,
            consensus_participation_held,
            consensus_participation_hold_reason,
            recent_finality_latency: ChainFinalityLatencyStatus {
                sample_count: snapshot.consensus.recent_finality_latency.sample_count,
                avg_latency_ms: snapshot.consensus.recent_finality_latency.avg_latency_ms,
                max_latency_ms: snapshot.consensus.recent_finality_latency.max_latency_ms,
                p50_latency_ms: snapshot.consensus.recent_finality_latency.p50_latency_ms,
                p95_latency_ms: snapshot.consensus.recent_finality_latency.p95_latency_ms,
            },
            pending_proposal,
            pending_consensus_actions: ChainPendingConsensusActionsStatus {
                queued_action_count: snapshot
                    .consensus
                    .pending_consensus_actions
                    .queued_action_count,
                queued_payload_bytes: snapshot
                    .consensus
                    .pending_consensus_actions
                    .queued_payload_bytes,
                reserved_requeue_action_count: snapshot
                    .consensus
                    .pending_consensus_actions
                    .reserved_requeue_action_count,
                reserved_requeue_payload_bytes: snapshot
                    .consensus
                    .pending_consensus_actions
                    .reserved_requeue_payload_bytes,
                available_capacity: snapshot
                    .consensus
                    .pending_consensus_actions
                    .available_capacity,
                max_capacity: snapshot.consensus.pending_consensus_actions.max_capacity,
                submit_buffer_action_count: snapshot
                    .consensus
                    .pending_consensus_actions
                    .submit_buffer_action_count,
                submit_buffer_payload_bytes: snapshot
                    .consensus
                    .pending_consensus_actions
                    .submit_buffer_payload_bytes,
                submit_buffer_max_capacity: snapshot
                    .consensus
                    .pending_consensus_actions
                    .submit_buffer_max_capacity,
            },
            inbound_timing_rejections: ChainInboundTimingRejectionsStatus {
                proposal_future_slot: snapshot.consensus.inbound_rejected_proposal_future_slot,
                proposal_stale_slot: snapshot.consensus.inbound_rejected_proposal_stale_slot,
                attestation_future_slot: snapshot
                    .consensus
                    .inbound_rejected_attestation_future_slot,
                attestation_stale_slot: snapshot.consensus.inbound_rejected_attestation_stale_slot,
                attestation_epoch_mismatch: snapshot
                    .consensus
                    .inbound_rejected_attestation_epoch_mismatch,
                last_reason: snapshot.consensus.last_inbound_timing_reject_reason.clone(),
            },
            last_status,
            last_block_hash: snapshot.consensus.last_block_hash,
            last_execution_height: snapshot.consensus.last_execution_height,
            last_execution_block_hash: snapshot.consensus.last_execution_block_hash,
            last_execution_state_root: snapshot.consensus.last_execution_state_root,
            known_peer_heads: snapshot.consensus.known_peer_heads,
            validator_set_hash: snapshot.consensus.validator_set_hash.clone(),
            validator_stake_root: snapshot.consensus.validator_stake_root.clone(),
            validator_stake_proof_count: snapshot.consensus.validator_stake_proofs.len(),
            misbehavior_evidence_count: snapshot.consensus.misbehavior_evidence.len(),
            slashing_intent_count: snapshot.consensus.slashing_intents.len(),
            pending_slashing_intent_count,
            slashing_receipt_count: snapshot.consensus.slashing_receipts.len(),
            applied_slashing_receipt_count,
            quarantined_validator_count: snapshot.consensus.quarantined_validators.len(),
            slashable_stake_total,
            network_head,
        },
        chain_proof,
        consensus_progress_observer_error: snapshot
            .consensus_progress_observer_error
            .map(|error| error.message),
        last_error: snapshot.last_error,
        execution_world_dir: execution_world_dir.display().to_string(),
        p2p,
        observability,
        release_security_policy,
        reward_runtime: reward_runtime_metrics,
        storage: storage_metrics,
        wasm,
        runtime_perf,
        traffic,
        transactions,
        replication,
        execution_bridge_commit_timing,
        module_tick_routing,
        validator,
        provider,
    }
}
