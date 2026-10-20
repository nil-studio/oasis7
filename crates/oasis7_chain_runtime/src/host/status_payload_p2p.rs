use super::*;

#[derive(Debug, Serialize)]
pub(crate) struct ChainP2pStatus {
    pub(crate) requested_user_mode: String,
    pub(crate) recommended_user_mode: String,
    pub(crate) effective_user_mode: String,
    pub(crate) applied_effective_user_mode: Option<String>,
    pub(crate) requires_explicit_public_entry_confirmation: bool,
    pub(crate) detected_reachability: Option<String>,
    pub(crate) hole_punch_viability: String,
    pub(crate) autonat_status: String,
    pub(crate) public_port_reachability: String,
    pub(crate) observed_public_addr: Option<String>,
    pub(crate) confirmed_external_direct_addrs: Vec<String>,
    pub(crate) active_transport_kind: Option<String>,
    pub(crate) active_transport_kind_since_unix_ms: Option<i64>,
    pub(crate) active_direct_path_count: usize,
    pub(crate) active_hole_punch_path_count: usize,
    pub(crate) active_relay_path_count: usize,
    pub(crate) transport_transition_count: u64,
    pub(crate) transport_transitions: ChainP2pTransportTransitionCounters,
    pub(crate) last_transport_transition: Option<ChainP2pTransportTransition>,
    pub(crate) relay_available: bool,
    pub(crate) probe_stable: bool,
    pub(crate) deployment_mode: String,
    pub(crate) node_role_claim: String,
    pub(crate) rationale: Vec<String>,
}

pub(crate) fn build_chain_p2p_status(
    live: &NodeUserModeRecommendation,
    applied: Option<String>,
    policy: NodeNetworkPolicy,
    snapshot: &Libp2pReachabilitySnapshot,
    detection: NodeReachabilityAutoDetection,
) -> ChainP2pStatus {
    ChainP2pStatus {
        requested_user_mode: live.requested_user_mode.as_str().to_string(),
        recommended_user_mode: live.recommended_user_mode.as_str().to_string(),
        effective_user_mode: live.effective_user_mode.as_str().to_string(),
        applied_effective_user_mode: applied,
        requires_explicit_public_entry_confirmation: live
            .requires_explicit_public_entry_confirmation,
        detected_reachability: detection
            .observed_reachability
            .map(peer_reachability_as_str)
            .map(str::to_string),
        hole_punch_viability: detection.hole_punch_viability.to_string(),
        autonat_status: detection.autonat_status.to_string(),
        public_port_reachability: detection.public_port_reachability.to_string(),
        observed_public_addr: snapshot.observed_public_addr.clone(),
        confirmed_external_direct_addrs: snapshot.confirmed_external_direct_addrs.clone(),
        active_transport_kind: snapshot
            .active_transport_kind
            .map(|kind| kind.as_str().to_string()),
        active_transport_kind_since_unix_ms: snapshot.active_transport_kind_since_unix_ms,
        active_direct_path_count: snapshot.active_direct_path_count,
        active_hole_punch_path_count: snapshot.active_hole_punch_path_count,
        active_relay_path_count: snapshot.active_relay_path_count,
        transport_transition_count: snapshot
            .transport_transition_counters
            .selected_kind_change_count,
        transport_transitions: ChainP2pTransportTransitionCounters {
            direct_to_hole_punched: snapshot
                .transport_transition_counters
                .direct_to_hole_punched,
            direct_to_relay_reserved: snapshot
                .transport_transition_counters
                .direct_to_relay_reserved,
            hole_punched_to_direct: snapshot
                .transport_transition_counters
                .hole_punched_to_direct,
            hole_punched_to_relay_reserved: snapshot
                .transport_transition_counters
                .hole_punched_to_relay_reserved,
            relay_reserved_to_direct: snapshot
                .transport_transition_counters
                .relay_reserved_to_direct,
            relay_reserved_to_hole_punched: snapshot
                .transport_transition_counters
                .relay_reserved_to_hole_punched,
        },
        last_transport_transition: snapshot
            .last_transport_transition
            .as_ref()
            .map(|transition| ChainP2pTransportTransition {
                from_kind: transition.from_kind.map(|kind| kind.as_str().to_string()),
                to_kind: transition.to_kind.map(|kind| kind.as_str().to_string()),
                at_unix_ms: Some(transition.at_unix_ms),
            }),
        relay_available: detection.relay_available,
        probe_stable: detection.probe_stable,
        deployment_mode: policy.deployment_mode.as_str().to_string(),
        node_role_claim: policy.node_role_claim.as_str().to_string(),
        rationale: live.rationale.clone(),
    }
}

pub(crate) fn build_readiness_status(
    observability: &ChainNodeObservabilityStatus,
    policy: ChainReadinessPolicyStatus,
) -> ChainReadinessStatus {
    let failed_gates = if !observability.ready {
        {
            observability
                .alerts
                .iter()
                .filter(|alert| {
                    alert.severity == "critical"
                        || matches!(
                            alert.code.as_str(),
                            "consensus_network_lag"
                                | "consensus_peer_head_unavailable"
                                | "consensus_peer_head_quorum_missing"
                                | "storage_degraded"
                                | "storage_challenge_network_degraded"
                                | "replication_no_connected_peers"
                                | "replication_transport_unstable"
                                | "p2p_reachability_degraded"
                        )
                })
                .map(|alert| alert.code.clone())
                .collect()
        }
    } else {
        Default::default()
    };
    ChainReadinessStatus {
        status: if observability.ready {
            "ready"
        } else {
            "not_ready"
        }
        .to_string(),
        ready: observability.ready,
        failed_gates,
        policy,
    }
}

pub(crate) fn build_sync_status(
    network_head: &ChainConsensusNetworkHeadStatus,
    network_height_lag: u64,
    policy: &ChainReadinessPolicyStatus,
    snapshot: &NodeSnapshot,
    observed_at_unix_ms: i64,
) -> ChainSyncStatus {
    let stalled = network_height_lag > 0
        && snapshot
            .consensus
            .last_committed_at_ms
            .map(|last_ms| {
                observed_at_unix_ms.saturating_sub(last_ms).max(0) > policy.sync_stalled_after_ms
            })
            .unwrap_or(false);
    let status = if network_head.conflicting_peer_count > 0 {
        "conflicting"
    } else if network_head.fresh_peer_count == 0 && network_head.required_peer_count > 0 {
        "unknown"
    } else if stalled {
        "stalled"
    } else if network_height_lag > 0 {
        "catching_up"
    } else if network_head.stale_peer_count > 0 {
        "stale_peer_view"
    } else {
        "synced"
    };
    ChainSyncStatus {
        status: status.to_string(),
        network_head_source: network_head.source.clone(),
        network_height_lag,
        fresh_peer_count: network_head.fresh_peer_count,
        stale_peer_count: network_head.stale_peer_count,
        conflicting_peer_count: network_head.conflicting_peer_count,
    }
}
