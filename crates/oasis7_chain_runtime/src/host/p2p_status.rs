use super::cli::{CliOptions, p2p_auto_detection_from_options};
use oasis7_node::{
    Libp2pReachabilitySnapshot, LiveAutoNatStatus, LiveHolePunchState, LivePublicPortReachability,
    NodeAutoNatStatus, NodeNetworkPolicy, NodePublicPortReachability,
    NodeReachabilityAutoDetection, NodeUserModeRecommendation,
};
use oasis7_proto::distributed_dht::PeerReachabilityClass;

#[cfg(test)]
#[path = "p2p_status_tests.rs"]
mod tests;

pub(super) fn build_node_network_policy(options: &CliOptions) -> NodeNetworkPolicy {
    if options.p2p_deployment_mode_explicit || options.p2p_node_role_explicit {
        return NodeNetworkPolicy {
            deployment_mode: options.p2p_deployment_mode,
            node_role_claim: options.p2p_node_role,
        };
    }
    build_node_network_policy_recommendation(options)
        .expect("user-mode recommendation should satisfy runtime policy constraints")
        .effective_policy
}

pub(super) fn build_node_network_policy_recommendation(
    options: &CliOptions,
) -> Result<NodeUserModeRecommendation, String> {
    NodeNetworkPolicy::recommend_for_user_mode(
        options.node_role,
        options.p2p_user_mode,
        p2p_auto_detection_from_options(options),
        options.p2p_accept_public_entry,
    )
    .map_err(|err| format!("invalid p2p user-mode recommendation: {err}"))
}

pub(super) fn build_live_node_network_policy_recommendation(
    options: &CliOptions,
    live_snapshot: Option<&Libp2pReachabilitySnapshot>,
) -> Result<(NodeUserModeRecommendation, NodeReachabilityAutoDetection), String> {
    let detection = merged_p2p_auto_detection(options, live_snapshot);
    let recommendation = NodeNetworkPolicy::recommend_for_user_mode(
        options.node_role,
        options.p2p_user_mode,
        detection,
        options.p2p_accept_public_entry,
    )
    .map_err(|err| format!("invalid p2p user-mode recommendation: {err}"))?;
    Ok((recommendation, detection))
}

pub(super) fn applied_runtime_user_mode_label(options: &CliOptions) -> Option<&'static str> {
    if options.p2p_deployment_mode_explicit || options.p2p_node_role_explicit {
        return None;
    }
    build_node_network_policy_recommendation(options)
        .ok()
        .map(|recommendation| recommendation.effective_user_mode.as_str())
}

fn merged_p2p_auto_detection(
    options: &CliOptions,
    live_snapshot: Option<&Libp2pReachabilitySnapshot>,
) -> NodeReachabilityAutoDetection {
    let mut detection = p2p_auto_detection_from_options(options);
    let Some(live_snapshot) = live_snapshot else {
        return detection;
    };

    if !options.p2p_detected_reachability_explicit {
        detection.observed_reachability = live_reachability_hint(live_snapshot);
    }
    if !options.p2p_detected_hole_punch_viability_explicit {
        detection.hole_punch_viability = match live_snapshot.hole_punch_state {
            LiveHolePunchState::Unknown => detection.hole_punch_viability,
            LiveHolePunchState::Viable => oasis7_node::NodeHolePunchViability::Viable,
            LiveHolePunchState::Blocked => oasis7_node::NodeHolePunchViability::Blocked,
        };
    }
    detection.autonat_status = match live_snapshot.autonat_status {
        LiveAutoNatStatus::Unknown => NodeAutoNatStatus::Unknown,
        LiveAutoNatStatus::Public => NodeAutoNatStatus::Public,
        LiveAutoNatStatus::Private => NodeAutoNatStatus::Private,
    };
    detection.public_port_reachability = match live_snapshot.public_port_reachability {
        LivePublicPortReachability::Unknown => NodePublicPortReachability::Unknown,
        LivePublicPortReachability::Reachable => NodePublicPortReachability::Reachable,
        LivePublicPortReachability::Unreachable => NodePublicPortReachability::Unreachable,
    };
    if !options.p2p_detected_relay_available_explicit {
        detection.relay_available =
            live_snapshot.relay_reservation_active || live_snapshot.active_relay_path_count > 0;
    }
    if !options.p2p_detected_probe_stable_explicit {
        detection.probe_stable = live_snapshot.has_stable_signal();
    }

    detection
}

fn live_reachability_hint(
    live_snapshot: &Libp2pReachabilitySnapshot,
) -> Option<PeerReachabilityClass> {
    if matches!(
        live_snapshot.public_port_reachability,
        LivePublicPortReachability::Reachable
    ) || matches!(live_snapshot.autonat_status, LiveAutoNatStatus::Public)
    {
        return Some(PeerReachabilityClass::Public);
    }
    if live_snapshot.active_hole_punch_path_count > 0
        || matches!(live_snapshot.hole_punch_state, LiveHolePunchState::Viable)
    {
        return Some(PeerReachabilityClass::Hybrid);
    }
    if live_snapshot.active_relay_path_count > 0 {
        return Some(PeerReachabilityClass::RelayOnly);
    }
    if matches!(
        live_snapshot.public_port_reachability,
        LivePublicPortReachability::Unreachable
    ) || matches!(live_snapshot.autonat_status, LiveAutoNatStatus::Private)
    {
        return Some(PeerReachabilityClass::Private);
    }
    None
}

pub(super) fn peer_reachability_as_str(reachability: PeerReachabilityClass) -> &'static str {
    match reachability {
        PeerReachabilityClass::Public => "public",
        PeerReachabilityClass::Hybrid => "hybrid",
        PeerReachabilityClass::Private => "private",
        PeerReachabilityClass::RelayOnly => "relay_only",
        PeerReachabilityClass::ValidatorHidden => "validator_hidden",
    }
}
