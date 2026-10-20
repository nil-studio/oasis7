use std::collections::BTreeSet;

use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7::simulator::{
    RuntimePerfBottleneck, RuntimePerfHealth, RuntimePerfSeriesSnapshot, RuntimePerfSnapshot,
};
use oasis7_node::NodeRole;
use oasis7_node::NodeSnapshot;
use serde::Serialize;

use super::super::publication_lifecycle::{
    SEQUENCER_HEAD_PUBLICATION_GRACE_MS, has_complete_publication_quorum_at_height,
};

use super::ExecutionBridgeCommitTimingSnapshot;
use super::{
    ChainConsensusNetworkHeadStatus, ChainLivenessStatus, ChainNodeObservabilityAlert,
    ChainP2pStatus, ChainReadinessPolicyStatus, ChainReplicationTransportStability,
    TRANSPORT_STABILITY_MIN_SCORE,
};

const EXECUTION_BRIDGE_RUNTIME_PERF_BUDGET_MS: f64 = 1_000.0;
const EXECUTION_BRIDGE_RUNTIME_PERF_STEADY_WINDOW_SAMPLES: usize = 128;
const EXECUTION_BRIDGE_RUNTIME_PERF_CRITICAL_MIN_SAMPLES: u64 = 32;
const EXECUTION_BRIDGE_RUNTIME_PERF_CRITICAL_OVER_BUDGET_RATIO_PPM: u64 = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimePerfGateTier {
    Strict,
    LowResourceValidatorV1,
}

impl RuntimePerfGateTier {
    fn steady_window_limits(self) -> (f64, u64, f64) {
        match self {
            Self::Strict => (1_000.0, 1, 1_250.0),
            Self::LowResourceValidatorV1 => (1_500.0, 60, 3_000.0),
        }
    }
}

pub(crate) fn runtime_perf_gate_tier_from_manifest(
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
    node_role: NodeRole,
) -> RuntimePerfGateTier {
    match (
        loaded_network_tier_manifest.map(|loaded| loaded.manifest.tier.as_str()),
        node_role,
    ) {
        (Some("public_testnet"), NodeRole::Storage) => RuntimePerfGateTier::LowResourceValidatorV1,
        _ => RuntimePerfGateTier::Strict,
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainP2pPathObservabilityStatus {
    pub(crate) selected_path_kind: Option<String>,
    pub(crate) selected_path_age_ms: Option<i64>,
    pub(crate) active_direct_path_count: usize,
    pub(crate) active_hole_punch_path_count: usize,
    pub(crate) active_relay_path_count: usize,
    pub(crate) transition_count: u64,
    pub(crate) transitions: ChainP2pPathTransitionCountersStatus,
    pub(crate) last_transition: Option<ChainP2pPathTransitionStatus>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainP2pPathTransitionCountersStatus {
    pub(crate) direct_to_hole_punched: u64,
    pub(crate) direct_to_relay_reserved: u64,
    pub(crate) hole_punched_to_direct: u64,
    pub(crate) hole_punched_to_relay_reserved: u64,
    pub(crate) relay_reserved_to_direct: u64,
    pub(crate) relay_reserved_to_hole_punched: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainP2pPathTransitionStatus {
    pub(crate) from_kind: Option<String>,
    pub(crate) to_kind: Option<String>,
    pub(crate) age_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ChainP2pTransportTransitionCounters {
    pub(crate) direct_to_hole_punched: u64,
    pub(crate) direct_to_relay_reserved: u64,
    pub(crate) hole_punched_to_direct: u64,
    pub(crate) hole_punched_to_relay_reserved: u64,
    pub(crate) relay_reserved_to_direct: u64,
    pub(crate) relay_reserved_to_hole_punched: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChainP2pTransportTransition {
    pub(crate) from_kind: Option<String>,
    pub(crate) to_kind: Option<String>,
    pub(crate) at_unix_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainInboundTimingRejectionsStatus {
    pub(crate) proposal_future_slot: u64,
    pub(crate) proposal_stale_slot: u64,
    pub(crate) attestation_future_slot: u64,
    pub(crate) attestation_stale_slot: u64,
    pub(crate) attestation_epoch_mismatch: u64,
    pub(crate) last_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainFinalityLatencyStatus {
    pub(crate) sample_count: usize,
    pub(crate) avg_latency_ms: Option<i64>,
    pub(crate) max_latency_ms: Option<i64>,
    pub(crate) p50_latency_ms: Option<i64>,
    pub(crate) p95_latency_ms: Option<i64>,
}

pub(crate) fn build_path_observability_status(
    p2p: &ChainP2pStatus,
    observed_at_unix_ms: i64,
) -> ChainP2pPathObservabilityStatus {
    let selected_path_age_ms = p2p
        .active_transport_kind_since_unix_ms
        .and_then(|since_ms| observed_at_unix_ms.checked_sub(since_ms))
        .map(|age_ms| age_ms.max(0));
    ChainP2pPathObservabilityStatus {
        selected_path_kind: p2p.active_transport_kind.clone(),
        selected_path_age_ms,
        active_direct_path_count: p2p.active_direct_path_count,
        active_hole_punch_path_count: p2p.active_hole_punch_path_count,
        active_relay_path_count: p2p.active_relay_path_count,
        transition_count: p2p.transport_transition_count,
        transitions: ChainP2pPathTransitionCountersStatus {
            direct_to_hole_punched: p2p.transport_transitions.direct_to_hole_punched,
            direct_to_relay_reserved: p2p.transport_transitions.direct_to_relay_reserved,
            hole_punched_to_direct: p2p.transport_transitions.hole_punched_to_direct,
            hole_punched_to_relay_reserved: p2p
                .transport_transitions
                .hole_punched_to_relay_reserved,
            relay_reserved_to_direct: p2p.transport_transitions.relay_reserved_to_direct,
            relay_reserved_to_hole_punched: p2p
                .transport_transitions
                .relay_reserved_to_hole_punched,
        },
        last_transition: p2p.last_transport_transition.as_ref().map(|transition| {
            ChainP2pPathTransitionStatus {
                from_kind: transition.from_kind.clone(),
                to_kind: transition.to_kind.clone(),
                age_ms: transition
                    .at_unix_ms
                    .and_then(|at_ms| observed_at_unix_ms.checked_sub(at_ms))
                    .map(|age_ms| age_ms.max(0)),
            }
        }),
    }
}

pub(crate) fn push_observability_alert(
    alerts: &mut Vec<ChainNodeObservabilityAlert>,
    severity: &str,
    code: &str,
    summary: String,
) {
    alerts.push(ChainNodeObservabilityAlert {
        severity: severity.to_string(),
        code: code.to_string(),
        summary,
    });
}

pub(crate) struct ChainRuntimePerfObservabilityStatus {
    pub(crate) available: bool,
    pub(crate) health: String,
    pub(crate) bottleneck: String,
    pub(crate) degraded: bool,
}

pub(crate) fn build_runtime_perf_observability_status(
    runtime_perf: Option<&RuntimePerfSnapshot>,
    alerts: &mut Vec<ChainNodeObservabilityAlert>,
) -> ChainRuntimePerfObservabilityStatus {
    let health = runtime_perf
        .map(|snapshot| snapshot.health.as_str().to_string())
        .unwrap_or_else(|| "unavailable".to_string());
    let bottleneck = runtime_perf
        .map(|snapshot| snapshot.bottleneck.as_str().to_string())
        .unwrap_or_else(|| "none".to_string());
    let degraded = runtime_perf
        .map(|snapshot| {
            matches!(
                snapshot.health,
                RuntimePerfHealth::Warn | RuntimePerfHealth::Critical
            )
        })
        .unwrap_or(false);

    if let Some(runtime_perf) = runtime_perf.filter(|_| degraded) {
        let severity = if runtime_perf.health == RuntimePerfHealth::Critical {
            "critical"
        } else {
            "warn"
        };
        push_observability_alert(
            alerts,
            severity,
            "runtime_perf_degraded",
            format!(
                "runtime performance degraded: health={} bottleneck={} tick_p95_ms={:.2} decision_p95_ms={:.2} action_execution_p95_ms={:.2} callback_p95_ms={:.2}",
                runtime_perf.health.as_str(),
                runtime_perf.bottleneck.as_str(),
                runtime_perf.tick.p95_ms,
                runtime_perf.decision.p95_ms,
                runtime_perf.action_execution.p95_ms,
                runtime_perf.callback.p95_ms
            ),
        );
    }

    if let Some(llm_api) = runtime_perf
        .map(|snapshot| &snapshot.llm_api)
        .filter(|llm_api| {
            llm_api.has_samples()
                && ((llm_api.budget_ms > 0.0 && llm_api.p95_ms > llm_api.budget_ms)
                    || llm_api.over_budget_ratio_ppm >= 50_000)
        })
    {
        push_observability_alert(
            alerts,
            "warn",
            "llm_api_perf_degraded",
            format!(
                "LLM API performance degraded: llm_api_p95_ms={:.2} llm_api_budget_ms={:.2} llm_api_over_budget_ratio_ppm={}",
                llm_api.p95_ms, llm_api.budget_ms, llm_api.over_budget_ratio_ppm
            ),
        );
    }

    ChainRuntimePerfObservabilityStatus {
        available: runtime_perf.is_some(),
        health,
        bottleneck,
        degraded,
    }
}

pub(crate) fn build_runtime_perf_snapshot_from_execution_bridge_timing(
    timing: &ExecutionBridgeCommitTimingSnapshot,
    gate_tier: RuntimePerfGateTier,
) -> Option<RuntimePerfSnapshot> {
    if timing.recent_commit_count == 0 {
        return None;
    }
    let p50_ms = timing.p50_total_ms.unwrap_or(0) as f64;
    let p95_ms = timing.p95_total_ms.unwrap_or(0) as f64;
    let latest_ms = timing.latest_total_ms.unwrap_or(0) as f64;
    let max_ms = timing.max_total_ms.unwrap_or(0) as f64;
    let samples_total = timing.recent_commit_count as u64;
    let steady_window_complete = timing.window_capacity
        == EXECUTION_BRIDGE_RUNTIME_PERF_STEADY_WINDOW_SAMPLES
        && timing.recent_commit_count == EXECUTION_BRIDGE_RUNTIME_PERF_STEADY_WINDOW_SAMPLES;
    let (p95_limit_ms, max_over_budget, max_sample_ms) = gate_tier.steady_window_limits();
    let steady_window_qualified = steady_window_complete
        && p95_ms < p95_limit_ms
        && timing.recent_over_budget_count <= max_over_budget
        && max_ms < max_sample_ms;
    // A partial window is always non-qualifying. Severe, sustained evidence
    // retains the higher-severity signal without making that window qualifying.
    let health = if steady_window_qualified {
        RuntimePerfHealth::Healthy
    } else if samples_total >= EXECUTION_BRIDGE_RUNTIME_PERF_CRITICAL_MIN_SAMPLES
        && p95_ms >= EXECUTION_BRIDGE_RUNTIME_PERF_BUDGET_MS * 2.0
        && timing.recent_over_budget_ratio_ppm
            >= EXECUTION_BRIDGE_RUNTIME_PERF_CRITICAL_OVER_BUDGET_RATIO_PPM
    {
        RuntimePerfHealth::Critical
    } else {
        RuntimePerfHealth::Warn
    };
    let action_execution = RuntimePerfSeriesSnapshot {
        samples_total,
        samples_window: timing.recent_commit_count,
        budget_ms: EXECUTION_BRIDGE_RUNTIME_PERF_BUDGET_MS,
        last_ms: latest_ms,
        avg_ms: 0.0,
        min_ms: 0.0,
        max_ms,
        p50_ms,
        p95_ms,
        p99_ms: max_ms,
        over_budget_total: timing.recent_over_budget_count,
        over_budget_ratio_ppm: timing.recent_over_budget_ratio_ppm,
    };

    Some(RuntimePerfSnapshot {
        sample_window: timing.window_capacity,
        action_execution,
        health,
        bottleneck: if health == RuntimePerfHealth::Healthy {
            RuntimePerfBottleneck::None
        } else {
            RuntimePerfBottleneck::ActionExecution
        },
        ..RuntimePerfSnapshot::default()
    })
}

pub(crate) fn push_local_chain_ahead_alert(
    alerts: &mut Vec<ChainNodeObservabilityAlert>,
    snapshot: &NodeSnapshot,
    network_height: Option<u64>,
) {
    let Some(network_height) = network_height else {
        return;
    };
    let local_height = snapshot.consensus.committed_height;
    if local_height <= network_height {
        return;
    }
    push_observability_alert(
        alerts,
        "critical",
        "local_chain_ahead_of_network_head",
        format!(
            "local committed height {local_height} is ahead of network head {network_height} by {}; verify clean-rebuild generation before preserving local state",
            local_height.saturating_sub(network_height)
        ),
    );
}

pub(crate) fn sequencer_head_publication_pending_summary(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    policy: &ChainReadinessPolicyStatus,
    observed_at_unix_ms: i64,
) -> Option<String> {
    let local_height = snapshot.consensus.committed_height;
    let parent_height = local_height.checked_sub(1)?;
    let local_block_hash = nonempty(snapshot.consensus.last_block_hash.as_deref())?;
    let local_execution_block_hash =
        nonempty(snapshot.consensus.last_execution_block_hash.as_deref())?;
    let local_execution_state_root =
        nonempty(snapshot.consensus.last_execution_state_root.as_deref())?;
    let parent_block_hash = nonempty(network_head.block_hash.as_deref())?;
    let parent_execution_block_hash = nonempty(network_head.execution_block_hash.as_deref())?;
    let parent_execution_state_root = nonempty(network_head.execution_state_root.as_deref())?;
    let commit_age_ms = observed_at_unix_ms
        .saturating_sub(snapshot.consensus.last_committed_at_ms?)
        .max(0);

    let exact_parent_quorum =
        has_complete_publication_quorum_at_height(snapshot, network_head, parent_height, false);

    if policy.tier != "public_testnet"
        || policy.role != "sequencer"
        || !exact_parent_quorum
        || commit_age_ms > SEQUENCER_HEAD_PUBLICATION_GRACE_MS
    {
        return None;
    }

    Some(format!(
        "sequencer head publication pending within {SEQUENCER_HEAD_PUBLICATION_GRACE_MS}ms grace: local_height={local_height} local_block_hash={local_block_hash} local_execution_block_hash={local_execution_block_hash} local_execution_state_root={local_execution_state_root} parent_height={parent_height} parent_block_hash={parent_block_hash} parent_execution_block_hash={parent_execution_block_hash} parent_execution_state_root={parent_execution_state_root} commit_age_ms={commit_age_ms}"
    ))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

pub(crate) fn observability_status_for_alerts(alerts: &[ChainNodeObservabilityAlert]) -> String {
    if alerts.iter().any(|alert| alert.severity == "critical") {
        "critical".to_string()
    } else if alerts.iter().any(|alert| alert.severity == "warn") {
        "warn".to_string()
    } else {
        "ok".to_string()
    }
}

pub(crate) fn observability_summary_for_alerts(alerts: &[ChainNodeObservabilityAlert]) -> String {
    match alerts {
        [] => "no active node alerts".to_string(),
        [only] => only.summary.clone(),
        _ => format!("{}; +{} more alerts", alerts[0].summary, alerts.len() - 1),
    }
}

pub(crate) fn classify_transport_stability(
    replication: &super::super::ChainReplicationDebugStatus,
) -> ChainReplicationTransportStability {
    let mut connection_closed_count = 0usize;
    let mut insufficient_peers_count = 0usize;
    let mut timeout_count = 0usize;
    let mut protocol_error_count = 0usize;
    for error in &replication.recent_errors {
        if !replication_error_is_blocking(replication, error) {
            continue;
        }
        let lower = error.to_ascii_lowercase();
        if lower.contains("connectionclosed") || lower.contains("connection closed") {
            connection_closed_count += 1;
        }
        if lower.contains("insufficientpeers") || lower.contains("insufficient peers") {
            insufficient_peers_count += 1;
        }
        if lower.contains("timeout") {
            timeout_count += 1;
        }
        if lower.contains("protocol")
            || lower.contains("unsupported")
            || lower.contains("mismatch")
            || lower.contains("unavailable")
        {
            protocol_error_count += 1;
        }
    }
    let active_peer_ids = replication
        .peer_healths
        .iter()
        .filter(|health| health.status == "active")
        .map(|health| health.peer_id.as_str())
        .collect::<BTreeSet<_>>();
    let protocol_cooldown_peer_ids = replication
        .protocol_retry_cooldown_peers
        .values()
        .flat_map(|peers| peers.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    let transport_cooldown_peer_ids = replication
        .transport_retry_cooldown_peers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let has_stable_active_peer = active_peer_ids.iter().any(|peer_id| {
        !protocol_cooldown_peer_ids.contains(peer_id)
            && !transport_cooldown_peer_ids.contains(peer_id)
    });
    let transport_cooldown_penalty_peers = if has_stable_active_peer {
        0
    } else {
        replication.transport_retry_cooldown_peers.len()
    };
    let protocol_cooldown_penalty_peers = if has_stable_active_peer {
        0
    } else {
        replication
            .protocol_retry_cooldown_peers
            .values()
            .map(Vec::len)
            .sum::<usize>()
    };
    let penalty = connection_closed_count
        .saturating_mul(5)
        .saturating_add(insufficient_peers_count.saturating_mul(10))
        .saturating_add(timeout_count.saturating_mul(10))
        .saturating_add(protocol_error_count.saturating_mul(15))
        .saturating_add(transport_cooldown_penalty_peers.saturating_mul(15))
        .saturating_add(protocol_cooldown_penalty_peers.saturating_mul(20));
    let score = 100u8.saturating_sub(penalty.min(100) as u8);
    ChainReplicationTransportStability {
        stable: score >= TRANSPORT_STABILITY_MIN_SCORE,
        score,
        recent_error_count: replication.recent_errors.len(),
        blocking_error_count: blocking_replication_error_count(replication),
        connection_closed_count,
        insufficient_peers_count,
        timeout_count,
        protocol_error_count,
    }
}

pub(crate) fn reachability_policy_ok(
    snapshot: &NodeSnapshot,
    p2p: &ChainP2pStatus,
    active_peer_count: usize,
    policy: &ChainReadinessPolicyStatus,
) -> bool {
    if !snapshot.replication_enabled {
        return true;
    }
    if snapshot.role.as_str() == "observer" {
        return active_peer_count > 0 || p2p.relay_available;
    }
    let has_public_direct = p2p.detected_reachability.as_deref() == Some("public")
        || p2p.autonat_status == "public"
        || p2p.public_port_reachability == "reachable"
        || !p2p.confirmed_external_direct_addrs.is_empty()
        || p2p.observed_public_addr.is_some();
    if policy.tier == "mainnet" && policy.relay_policy == "public_direct_or_governed_relay" {
        return has_public_direct || (p2p.relay_available && active_peer_count >= 2);
    }
    has_public_direct || p2p.relay_available
}

pub(crate) fn build_liveness_status(snapshot: &NodeSnapshot) -> ChainLivenessStatus {
    let status = if snapshot.running && snapshot.last_error.is_none() {
        "ok"
    } else {
        "critical"
    };
    ChainLivenessStatus {
        status: status.to_string(),
        running: snapshot.running,
        runtime_last_error: snapshot.last_error.clone(),
    }
}

fn blocking_replication_error_count(
    replication: &super::super::ChainReplicationDebugStatus,
) -> usize {
    replication
        .recent_errors
        .iter()
        .filter(|error| replication_error_is_blocking(replication, error))
        .count()
}

fn replication_error_is_blocking(
    replication: &super::super::ChainReplicationDebugStatus,
    error: &str,
) -> bool {
    if replication_error_is_diagnostic(error) {
        return false;
    }
    let lower = error.to_ascii_lowercase();
    let recovered_request_path = replication_has_recovered_request_path(replication);
    if recovered_request_path && replication_error_is_recovered_request_path_failure(&lower) {
        return false;
    }
    if recovered_request_path && replication_error_is_public_inbound_noise(replication, &lower) {
        return false;
    }
    let active_peer_available = replication
        .peer_healths
        .iter()
        .any(|health| health.status == "active")
        || !replication.connected_peers.is_empty();
    if active_peer_available
        && (lower.contains("connection closed")
            || lower.contains("connectionclosed")
            || lower.contains("outgoing connection error")
            || lower.contains("connection refused")
            || lower.contains("redundant connections pruned"))
    {
        return false;
    }
    true
}

fn replication_has_recovered_request_path(
    replication: &super::super::ChainReplicationDebugStatus,
) -> bool {
    let protocol_cooldown_peer_ids = replication
        .protocol_retry_cooldown_peers
        .values()
        .flat_map(|peers| peers.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    let transport_cooldown_peer_ids = replication
        .transport_retry_cooldown_peers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let peer_is_requestable = |peer_id: &str| {
        !protocol_cooldown_peer_ids.contains(peer_id)
            && !transport_cooldown_peer_ids.contains(peer_id)
            && replication
                .request_peer_scores
                .get(peer_id)
                .copied()
                .unwrap_or(100)
                > 0
    };
    replication
        .peer_healths
        .iter()
        .any(|health| health.status == "active" && peer_is_requestable(health.peer_id.as_str()))
        || (replication.peer_healths.is_empty()
            && replication
                .connected_peers
                .iter()
                .any(|peer_id| peer_is_requestable(peer_id.as_str())))
}

fn replication_error_is_recovered_request_path_failure(lower: &str) -> bool {
    (lower.contains("request failed:")
        || lower.contains("outbound request failed")
        || lower.contains("network request failed"))
        && (lower.contains("timeout")
            || lower.contains("unexpectedeof")
            || lower.contains("unexpected eof")
            || lower.contains("eof")
            || lower.contains("connectionclosed")
            || lower.contains("connection closed")
            || lower.contains("connection reset"))
}

fn replication_error_is_public_inbound_noise(
    replication: &super::super::ChainReplicationDebugStatus,
    lower: &str,
) -> bool {
    if lower.contains("request failed")
        || lower.contains("outbound request failed")
        || lower.contains("network request failed")
        || lower.contains("fetch-commit")
        || lower.contains("fetch-blob")
    {
        return false;
    }

    let inbound_noise = (lower.contains("libp2p inbound failure")
        && (lower.contains("timeout")
            || lower.contains("connectionclosed")
            || lower.contains("connection closed")))
        || (lower.contains("libp2p incoming connection error")
            && (lower.contains("invalidmessage")
                || lower.contains("unsupportedprotocol")
                || lower.contains("protocolerror")
                || lower.contains("timeout")));
    if !inbound_noise {
        return false;
    }

    match extract_peer_id_from_error_lower(lower) {
        Some(peer_id) => replication_peer_is_non_core_public_noise(replication, peer_id),
        None => true,
    }
}

fn extract_peer_id_from_error_lower(error: &str) -> Option<&str> {
    if let Some(start) = error.find("peerid(\"") {
        let rest = &error[start + "peerid(\"".len()..];
        return rest.split('"').next().filter(|peer_id| !peer_id.is_empty());
    }
    if let Some(start) = error.find("peer=") {
        let rest = &error[start + "peer=".len()..];
        let end = rest
            .find(|ch: char| ch.is_whitespace() || ch == ',' || ch == ')' || ch == ']')
            .unwrap_or(rest.len());
        return (!rest[..end].is_empty()).then_some(&rest[..end]);
    }
    None
}

fn replication_peer_is_non_core_public_noise(
    replication: &super::super::ChainReplicationDebugStatus,
    peer_id: &str,
) -> bool {
    let is_non_core_public_peer = replication.peer_healths.iter().any(|health| {
        health.peer_id.eq_ignore_ascii_case(peer_id)
            && health.status == "active"
            && (health
                .source_operator
                .as_deref()
                .map(|operator| {
                    let operator = operator.to_ascii_lowercase();
                    operator.contains("observer")
                        || operator.contains("client")
                        || operator.contains("public")
                        || operator.contains("external")
                })
                .unwrap_or(false)
                || health.discovery_sources.iter().any(|source| {
                    matches!(
                        source.as_str(),
                        "dht" | "mdns" | "discovered" | "peer_exchange"
                    )
                }))
    });

    if !is_non_core_public_peer {
        return false;
    }

    replication
        .request_peer_scores
        .iter()
        .any(|(scored_peer, score)| scored_peer.eq_ignore_ascii_case(peer_id) && *score == 0)
        || is_non_core_public_peer
}

fn replication_error_is_diagnostic(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("libp2p autonat")
        || lower.contains("autonat")
        || lower.contains("libp2p connection established")
        || lower.contains("libp2p routing updated")
        || lower.contains("libp2p transport active")
        || lower.contains("libp2p redundant connections pruned")
        || lower.contains("peer record request failed")
        || lower.contains("missingpeerrecord")
        || lower.contains("missing_peer_record")
        || lower.contains("insufficientactivediscoverysources")
        || lower.contains("insufficient_active_discovery_sources")
        || lower.contains("single_source_discovery")
        || lower.contains("quarantine suppresses")
        || lower.contains("dial condition")
        || lower.contains("localpeerid")
}
