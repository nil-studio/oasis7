use oasis7_node::{
    GossipTrafficMetricsSnapshot, Libp2pReplicationNetwork, Libp2pTrafficMetricsSnapshot,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct ChainTrafficStatus {
    pub(super) udp_gossip: Option<GossipTrafficMetricsSnapshot>,
    pub(super) libp2p_replication: Libp2pTrafficMetricsSnapshot,
}

pub(super) fn build_chain_traffic_status(
    replication_network: &Libp2pReplicationNetwork,
    udp_gossip: Option<GossipTrafficMetricsSnapshot>,
) -> ChainTrafficStatus {
    ChainTrafficStatus {
        udp_gossip,
        libp2p_replication: replication_network.traffic_metrics_snapshot(),
    }
}
