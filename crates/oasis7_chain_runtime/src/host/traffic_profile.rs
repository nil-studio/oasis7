use oasis7_node::{Libp2pReplicationNetworkConfig, NodeConfig, NodeFeedbackP2pConfig, NodeRole};

use super::cli::{CliOptions, TrafficProfile};

const TRIAD_LOW_TRAFFIC_BOOTSTRAP_REDIAL_INTERVAL_MS: i64 = 10_000;
const TRIAD_LOW_TRAFFIC_DISCOVERY_QUERY_INTERVAL_MS: i64 = 180_000;
const TRIAD_LOW_TRAFFIC_REPUBLISH_INTERVAL_MS: i64 = 30 * 60 * 1000;
const TRIAD_LOW_TRAFFIC_MAX_DYNAMIC_GOSSIP_PEERS: usize = 8;
const TRIAD_LOW_TRAFFIC_DYNAMIC_GOSSIP_PEER_TTL_MS: i64 = 60 * 60 * 1000;
const TRIAD_LOW_TRAFFIC_FEEDBACK_P2P_MAX_INCOMING_ANNOUNCES_PER_TICK: usize = 8;
const TRIAD_LOW_TRAFFIC_FEEDBACK_P2P_MAX_OUTGOING_ANNOUNCES_PER_TICK: usize = 8;

pub(super) fn apply_traffic_profile_to_node_config(
    mut config: NodeConfig,
    options: &CliOptions,
) -> Result<NodeConfig, String> {
    match options.traffic_profile {
        TrafficProfile::Default => {}
        TrafficProfile::TriadLowTraffic => {
            config = config
                .with_max_dynamic_gossip_peers(TRIAD_LOW_TRAFFIC_MAX_DYNAMIC_GOSSIP_PEERS)
                .and_then(|cfg| {
                    cfg.with_dynamic_gossip_peer_ttl_ms(
                        TRIAD_LOW_TRAFFIC_DYNAMIC_GOSSIP_PEER_TTL_MS,
                    )
                })
                .map_err(|err| {
                    format!("failed to apply low-traffic gossip profile to node config: {err:?}")
                })?;
        }
    }

    if let Some(feedback_p2p_config) =
        feedback_p2p_config_for_role(options.node_role, options.traffic_profile)
    {
        config = config
            .with_feedback_p2p(feedback_p2p_config)
            .map_err(|err| format!("failed to enable node feedback p2p: {err:?}"))?;
    }

    Ok(config)
}

pub(super) fn feedback_p2p_config_for_role(
    node_role: NodeRole,
    traffic_profile: TrafficProfile,
) -> Option<NodeFeedbackP2pConfig> {
    if matches!(node_role, NodeRole::Observer) {
        return None;
    }

    let mut config = NodeFeedbackP2pConfig::default();
    if matches!(traffic_profile, TrafficProfile::TriadLowTraffic) {
        config.max_incoming_announces_per_tick =
            TRIAD_LOW_TRAFFIC_FEEDBACK_P2P_MAX_INCOMING_ANNOUNCES_PER_TICK;
        config.max_outgoing_announces_per_tick =
            TRIAD_LOW_TRAFFIC_FEEDBACK_P2P_MAX_OUTGOING_ANNOUNCES_PER_TICK;
    }
    Some(config)
}

pub(super) fn apply_traffic_profile_to_replication_network_config(
    config: &mut Libp2pReplicationNetworkConfig,
    traffic_profile: TrafficProfile,
) {
    if matches!(traffic_profile, TrafficProfile::TriadLowTraffic) {
        config.bootstrap_redial_interval_ms = TRIAD_LOW_TRAFFIC_BOOTSTRAP_REDIAL_INTERVAL_MS;
        config.discovery_query_interval_ms = TRIAD_LOW_TRAFFIC_DISCOVERY_QUERY_INTERVAL_MS;
        config.republish_interval_ms = TRIAD_LOW_TRAFFIC_REPUBLISH_INTERVAL_MS;
        config.enable_autonat = false;
    }
}
