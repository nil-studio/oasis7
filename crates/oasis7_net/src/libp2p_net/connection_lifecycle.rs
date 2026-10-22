use super::peer_discovery_observations::DiscoveryObservations;
use super::peer_record_cache::PeerRecordCache;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use libp2p::core::connection::ConnectedPoint;
use libp2p::rendezvous;
use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId};

use super::reachability::refresh_active_transport_snapshot;
use super::runtime_loop::{enforce_peer_manager_quarantine, refresh_peer_manager_healths};
use super::swarm_behaviour::Behaviour;
use super::transport_paths::{
    TransportPath, failover_transport_path_after_close, note_established_transport_path,
    recompute_active_transport_path_for_peer,
};
use super::utils::{push_bounded_clone, push_bounded_string_with_keyed_cooldown};
use super::{
    Libp2pReachabilitySnapshot, PeerManagerBlockArtifact, PeerManagerPeerHealth, PeerManagerPolicy,
};

#[expect(
    clippy::too_many_arguments,
    reason = "Connection lifecycle state is passed explicitly to preserve transport ordering and lifetimes"
)]
pub(super) fn record_established_connection(
    known_transport_paths: &HashMap<PeerId, Vec<TransportPath>>,
    active_transport_paths: &mut HashMap<PeerId, TransportPath>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    failed_transport_path_labels: &mut HashSet<String>,
    established_transport_paths_by_connection: &mut HashMap<ConnectionId, TransportPath>,
    established_connections_by_peer: &mut HashMap<PeerId, HashSet<ConnectionId>>,
    peer_id: PeerId,
    connection_id: ConnectionId,
    endpoint: &ConnectedPoint,
) -> (Option<TransportPath>, Option<Multiaddr>) {
    let (established_path, dialed_addr) = match endpoint {
        ConnectedPoint::Dialer { address, .. } => (
            note_established_transport_path(
                known_transport_paths,
                active_transport_paths,
                last_dialed_transport_paths,
                failed_transport_path_labels,
                peer_id,
                address,
            ),
            Some(address.clone()),
        ),
        ConnectedPoint::Listener { send_back_addr, .. } => (
            note_established_transport_path(
                known_transport_paths,
                active_transport_paths,
                last_dialed_transport_paths,
                failed_transport_path_labels,
                peer_id,
                send_back_addr,
            ),
            None,
        ),
    };
    established_transport_paths_by_connection.insert(connection_id, established_path);
    established_connections_by_peer
        .entry(peer_id)
        .or_default()
        .insert(connection_id);
    (
        recompute_active_transport_path_for_peer(
            active_transport_paths,
            established_transport_paths_by_connection,
            established_connections_by_peer,
            peer_id,
        ),
        dialed_addr,
    )
}

pub(super) fn redundant_connection_ids_for_peer(
    established_transport_paths_by_connection: &HashMap<ConnectionId, TransportPath>,
    established_connections_by_peer: &HashMap<PeerId, HashSet<ConnectionId>>,
    peer_id: PeerId,
) -> Vec<ConnectionId> {
    let Some(connection_ids) = established_connections_by_peer.get(&peer_id) else {
        return Vec::new();
    };
    if connection_ids.len() <= 1 {
        return Vec::new();
    }

    let Some(keep_connection_id) = connection_ids
        .iter()
        .copied()
        .filter_map(|connection_id| {
            established_transport_paths_by_connection
                .get(&connection_id)
                .map(|path| (path.preference_rank(), connection_id))
        })
        .min_by_key(|(rank, connection_id)| (*rank, *connection_id))
        .map(|(_, connection_id)| connection_id)
    else {
        return Vec::new();
    };

    let mut redundant: Vec<ConnectionId> = connection_ids
        .iter()
        .copied()
        .filter(|connection_id| *connection_id != keep_connection_id)
        .collect();
    redundant.sort_unstable();
    redundant
}

#[expect(
    clippy::too_many_arguments,
    reason = "Connection lifecycle state is passed explicitly to preserve transport ordering and lifetimes"
)]
pub(super) fn prune_redundant_peer_connections(
    swarm: &mut libp2p::Swarm<Behaviour>,
    established_transport_paths_by_connection: &HashMap<ConnectionId, TransportPath>,
    established_connections_by_peer: &HashMap<PeerId, HashSet<ConnectionId>>,
    peer_id: PeerId,
    event_errors: &Arc<Mutex<Vec<String>>>,
    lifecycle_event_errors_at_ms: &mut HashMap<String, i64>,
    lifecycle_event_errors_last_prune_at_ms: &mut Option<i64>,
    max_error_messages: usize,
    now_ms: i64,
    cooldown_ms: i64,
) {
    let redundant_connection_ids = redundant_connection_ids_for_peer(
        established_transport_paths_by_connection,
        established_connections_by_peer,
        peer_id,
    );
    if redundant_connection_ids.is_empty() {
        return;
    }
    let closed_any = redundant_connection_ids
        .iter()
        .copied()
        .any(|redundant_connection_id| swarm.close_connection(redundant_connection_id));
    if closed_any {
        push_bounded_string_with_keyed_cooldown(
            event_errors,
            lifecycle_event_errors_at_ms,
            lifecycle_event_errors_last_prune_at_ms,
            format!("connection-pruned:{peer_id}"),
            format!(
                "libp2p redundant connections pruned peer={peer_id} count={}",
                redundant_connection_ids.len()
            ),
            max_error_messages,
            "lock errors",
            now_ms,
            cooldown_ms,
        );
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Connection lifecycle state is passed explicitly to preserve transport ordering and lifetimes"
)]
pub(super) fn log_active_transport_path(
    event_errors: &Arc<Mutex<Vec<String>>>,
    lifecycle_event_errors_at_ms: &mut HashMap<String, i64>,
    lifecycle_event_errors_last_prune_at_ms: &mut Option<i64>,
    peer_id: PeerId,
    active_path: Option<&TransportPath>,
    max_error_messages: usize,
    now_ms: i64,
    cooldown_ms: i64,
) {
    let Some(active_path) = active_path else {
        return;
    };
    push_bounded_string_with_keyed_cooldown(
        event_errors,
        lifecycle_event_errors_at_ms,
        lifecycle_event_errors_last_prune_at_ms,
        format!(
            "transport-active:{peer_id}:{}:{}",
            active_path.kind_label(),
            active_path.flavor_label(),
        ),
        format!(
            "libp2p transport active peer={peer_id} kind={} flavor={} addr={}",
            active_path.kind_label(),
            active_path.flavor_label(),
            active_path.addr,
        ),
        max_error_messages,
        "lock errors",
        now_ms,
        cooldown_ms,
    );
}

pub(super) fn refresh_active_path_after_connection_close(
    active_transport_paths: &mut HashMap<PeerId, TransportPath>,
    established_transport_paths_by_connection: &mut HashMap<ConnectionId, TransportPath>,
    established_connections_by_peer: &mut HashMap<PeerId, HashSet<ConnectionId>>,
    peer_id: PeerId,
    connection_id: ConnectionId,
) -> Option<TransportPath> {
    established_transport_paths_by_connection.remove(&connection_id);
    if let Some(connection_ids) = established_connections_by_peer.get_mut(&peer_id) {
        connection_ids.remove(&connection_id);
        if connection_ids.is_empty() {
            established_connections_by_peer.remove(&peer_id);
        }
    }
    recompute_active_transport_path_for_peer(
        active_transport_paths,
        established_transport_paths_by_connection,
        established_connections_by_peer,
        peer_id,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Connection lifecycle state is passed explicitly to preserve transport ordering and lifetimes"
)]
pub(super) fn clear_disconnected_peer_state(
    peers: &mut Vec<PeerId>,
    admitted_active_peers: &mut HashSet<PeerId>,
    quarantined_active_peers: &mut HashSet<PeerId>,
    pending_quarantine_disconnects: &mut HashSet<PeerId>,
    active_transport_paths: &mut HashMap<PeerId, TransportPath>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    connected_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    cached_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    cached_discovery_peer_cooldowns: &mut HashMap<PeerId, i64>,
    pending_rendezvous_registers: &mut HashSet<PeerId>,
    pending_rendezvous_discovers: &mut HashSet<PeerId>,
    registered_rendezvous_nodes: &mut HashSet<PeerId>,
    rendezvous_cookies: &mut HashMap<PeerId, rendezvous::Cookie>,
    event_connected_peers: &Arc<Mutex<HashSet<PeerId>>>,
    peer_id: PeerId,
) -> bool {
    peers.retain(|peer| peer != &peer_id);
    admitted_active_peers.remove(&peer_id);
    let quarantined = quarantined_active_peers.remove(&peer_id)
        || pending_quarantine_disconnects.contains(&peer_id);
    pending_quarantine_disconnects.remove(&peer_id);
    if quarantined {
        active_transport_paths.remove(&peer_id);
        last_dialed_transport_paths.remove(&peer_id);
    }
    connected_peer_record_cooldowns.remove(&peer_id);
    cached_peer_record_cooldowns.remove(&peer_id);
    cached_discovery_peer_cooldowns.remove(&peer_id);
    pending_rendezvous_registers.remove(&peer_id);
    pending_rendezvous_discovers.remove(&peer_id);
    registered_rendezvous_nodes.remove(&peer_id);
    rendezvous_cookies.remove(&peer_id);
    event_connected_peers
        .lock()
        .expect("lock connected peers")
        .remove(&peer_id);
    quarantined
}

#[expect(
    clippy::too_many_arguments,
    reason = "Connection lifecycle state is passed explicitly to preserve transport ordering and lifetimes"
)]
pub(super) fn refresh_peer_manager_views(
    swarm: &mut libp2p::Swarm<Behaviour>,
    discovered_peer_records: &PeerRecordCache,
    discovery_observations: &DiscoveryObservations,
    active_transport_paths: &HashMap<PeerId, TransportPath>,
    admitted_active_peers: &HashSet<PeerId>,
    peer_manager_policy: &PeerManagerPolicy,
    event_peer_healths: &Arc<Mutex<HashMap<String, PeerManagerPeerHealth>>>,
    event_block_artifacts: &Arc<Mutex<HashMap<String, PeerManagerBlockArtifact>>>,
    pending_quarantine_disconnects: &mut HashSet<PeerId>,
    event_errors: &Arc<Mutex<Vec<String>>>,
    max_error_messages: usize,
    event_reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
) -> (
    HashMap<PeerId, PeerManagerPeerHealth>,
    HashSet<PeerId>,
    HashSet<PeerId>,
) {
    let (peer_healths_by_id, quarantined_active_peers, admitted_active_peers) =
        refresh_peer_manager_healths(
            discovered_peer_records,
            discovery_observations,
            active_transport_paths,
            admitted_active_peers,
            peer_manager_policy,
            event_peer_healths,
            event_block_artifacts,
            event_errors,
            max_error_messages,
        );
    enforce_peer_manager_quarantine(
        swarm,
        &quarantined_active_peers,
        pending_quarantine_disconnects,
        event_errors,
        max_error_messages,
    );
    refresh_active_transport_snapshot(event_reachability, active_transport_paths);
    (
        peer_healths_by_id,
        quarantined_active_peers,
        admitted_active_peers,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Connection lifecycle state is passed explicitly to preserve transport ordering and lifetimes"
)]
pub(super) fn failover_after_disconnect(
    swarm: &mut libp2p::Swarm<Behaviour>,
    known_transport_paths: &HashMap<PeerId, Vec<TransportPath>>,
    active_transport_paths: &mut HashMap<PeerId, TransportPath>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    failed_transport_path_labels: &mut HashSet<String>,
    event_errors: &Arc<Mutex<Vec<String>>>,
    max_error_messages: usize,
    peer_id: PeerId,
    disconnected_path: Option<TransportPath>,
) {
    match failover_transport_path_after_close(
        swarm,
        known_transport_paths,
        active_transport_paths,
        last_dialed_transport_paths,
        failed_transport_path_labels,
        peer_id,
        disconnected_path,
    ) {
        Ok(Some((previous_path, next_path))) => {
            let previous_addr = previous_path
                .map(|path| path.addr.to_string())
                .unwrap_or_else(|| "none".to_string());
            push_bounded_clone(
                event_errors,
                format!(
                    "libp2p transport failover peer={peer_id} from={} to={}",
                    previous_addr, next_path.addr,
                ),
                max_error_messages,
                "lock errors",
            );
        }
        Err(err) => {
            push_bounded_clone(
                event_errors,
                format!("libp2p transport failover dial failed peer={peer_id}: {err:?}"),
                max_error_messages,
                "lock errors",
            );
        }
        Ok(None) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libp2p_net::transport_paths::{
        TransportMuxer, TransportPathKind, TransportSecurity, TransportSessionFlavor,
    };

    fn path(
        peer_id: PeerId,
        addr: &str,
        kind: TransportPathKind,
        flavor: TransportSessionFlavor,
    ) -> TransportPath {
        TransportPath {
            record_lease: None,
            peer_id,
            addr: addr.parse().expect("multiaddr"),
            kind,
            flavor,
            security: match flavor {
                TransportSessionFlavor::Quic => TransportSecurity::QuicTls,
                TransportSessionFlavor::TcpNoiseYamux | TransportSessionFlavor::RelayTunnel => {
                    TransportSecurity::Noise
                }
            },
            muxer: match flavor {
                TransportSessionFlavor::Quic => TransportMuxer::Quic,
                TransportSessionFlavor::TcpNoiseYamux | TransportSessionFlavor::RelayTunnel => {
                    TransportMuxer::Yamux
                }
            },
        }
    }

    #[test]
    fn redundant_connection_ids_for_peer_keeps_best_ranked_connection() {
        let peer_id = PeerId::random();
        let direct = ConnectionId::new_unchecked(1);
        let relay = ConnectionId::new_unchecked(2);
        let mut established_transport_paths_by_connection = HashMap::new();
        established_transport_paths_by_connection.insert(
            direct,
            path(
                peer_id,
                &format!("/ip4/127.0.0.1/tcp/4101/p2p/{peer_id}"),
                TransportPathKind::Direct,
                TransportSessionFlavor::TcpNoiseYamux,
            ),
        );
        established_transport_paths_by_connection.insert(
            relay,
            path(
                peer_id,
                &format!("/ip4/127.0.0.1/tcp/4201/p2p-circuit/p2p/{peer_id}"),
                TransportPathKind::RelayReserved,
                TransportSessionFlavor::RelayTunnel,
            ),
        );
        let mut established_connections_by_peer = HashMap::new();
        established_connections_by_peer.insert(peer_id, HashSet::from([direct, relay]));

        assert_eq!(
            redundant_connection_ids_for_peer(
                &established_transport_paths_by_connection,
                &established_connections_by_peer,
                peer_id,
            ),
            vec![relay]
        );
    }

    #[test]
    fn redundant_connection_ids_for_peer_keeps_lowest_connection_id_for_equal_rank() {
        let peer_id = PeerId::random();
        let first = ConnectionId::new_unchecked(3);
        let second = ConnectionId::new_unchecked(9);
        let mut established_transport_paths_by_connection = HashMap::new();
        for connection_id in [first, second] {
            established_transport_paths_by_connection.insert(
                connection_id,
                path(
                    peer_id,
                    &format!("/ip4/127.0.0.1/tcp/4101/p2p/{peer_id}"),
                    TransportPathKind::Direct,
                    TransportSessionFlavor::TcpNoiseYamux,
                ),
            );
        }
        let mut established_connections_by_peer = HashMap::new();
        established_connections_by_peer.insert(peer_id, HashSet::from([first, second]));

        assert_eq!(
            redundant_connection_ids_for_peer(
                &established_transport_paths_by_connection,
                &established_connections_by_peer,
                peer_id,
            ),
            vec![second]
        );
    }
}
