use std::collections::{HashMap, HashSet};

use libp2p::multiaddr::Protocol;
use libp2p::swarm::{ConnectionId, Swarm};
use libp2p::{Multiaddr, PeerId};

use crate::error::WorldError;
use oasis7_proto::distributed_dht::{
    PeerDeploymentMode, PeerNodeRole, PeerReachabilityClass, SignedPeerRecord,
};
use oasis7_proto::distributed_net::NetworkLane;

use super::swarm_behaviour::{
    Behaviour, dial_addr_with_optional_peer_id, ensure_peer_id, split_peer_id,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TransportPathKind {
    Direct,
    HolePunched,
    RelayReserved,
}

impl TransportPathKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::HolePunched => "hole_punched",
            Self::RelayReserved => "relay_reserved",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TransportSessionFlavor {
    Quic,
    TcpNoiseYamux,
    RelayTunnel,
}

impl TransportSessionFlavor {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Quic => "quic",
            Self::TcpNoiseYamux => "tcp+noise+yamux",
            Self::RelayTunnel => "relay+tunnel+noise+yamux",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransportSecurity {
    QuicTls,
    Noise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransportMuxer {
    Quic,
    Yamux,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TransportPath {
    pub(super) record_lease: Option<super::peer_record_time::RecordLease>,
    pub(super) peer_id: PeerId,
    pub(super) addr: Multiaddr,
    pub(super) kind: TransportPathKind,
    pub(super) flavor: TransportSessionFlavor,
    pub(super) security: TransportSecurity,
    pub(super) muxer: TransportMuxer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PeerReachabilityContract {
    pub(super) peer_id: PeerId,
    pub(super) node_role: PeerNodeRole,
    pub(super) deployment_mode: PeerDeploymentMode,
    pub(super) reachability_class: PeerReachabilityClass,
    pub(super) capability_lanes: Vec<NetworkLane>,
    pub(super) ranked_paths: Vec<TransportPath>,
    pub(super) publish_source: PeerReachabilityPublishSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PeerReachabilityPublishSource {
    pub(super) direct_path_count: usize,
    pub(super) hole_punch_path_count: usize,
    pub(super) relay_path_count: usize,
}

impl PeerReachabilityContract {
    pub(super) fn from_signed_peer_record(record: &SignedPeerRecord) -> Result<Self, WorldError> {
        let peer_id = parse_record_peer_id(record)?;
        let node_role = record.record.parsed_node_role().map_err(|err| {
            WorldError::NetworkProtocolUnavailable {
                protocol: format!("peer record node_role must be valid: {err}"),
            }
        })?;
        let mut ranked_paths = Vec::new();
        let mut seen = HashSet::new();
        extend_paths(
            &mut ranked_paths,
            &mut seen,
            peer_id,
            record.record.direct_addrs.iter(),
            TransportPathKind::Direct,
        );
        extend_paths(
            &mut ranked_paths,
            &mut seen,
            peer_id,
            record.record.hole_punch_addrs.iter(),
            TransportPathKind::HolePunched,
        );
        extend_paths(
            &mut ranked_paths,
            &mut seen,
            peer_id,
            record.record.relay_addrs.iter(),
            TransportPathKind::RelayReserved,
        );
        ranked_paths.sort_unstable_by_key(TransportPath::preference_rank);

        Ok(Self {
            peer_id,
            node_role,
            deployment_mode: record.record.deployment_mode,
            reachability_class: record.record.reachability_class,
            capability_lanes: record.record.effective_capability_lanes(),
            publish_source: PeerReachabilityPublishSource {
                direct_path_count: record.record.direct_addrs.len(),
                hole_punch_path_count: record.record.hole_punch_addrs.len(),
                relay_path_count: record.record.relay_addrs.len(),
            },
            ranked_paths,
        })
    }
}

impl TransportPath {
    pub(super) fn label(&self) -> String {
        self.addr.to_string()
    }

    pub(super) fn kind_label(&self) -> &'static str {
        self.kind.label()
    }

    pub(super) fn flavor_label(&self) -> &'static str {
        self.flavor.label()
    }

    pub(super) fn preference_rank(&self) -> (TransportPathKind, TransportSessionFlavor) {
        (self.kind, self.flavor)
    }
}

pub(super) fn peer_record_transport_paths(
    record: &SignedPeerRecord,
) -> Result<Vec<TransportPath>, WorldError> {
    Ok(PeerReachabilityContract::from_signed_peer_record(record)?.ranked_paths)
}

fn parse_record_peer_id(record: &SignedPeerRecord) -> Result<PeerId, WorldError> {
    record
        .record
        .peer_id
        .parse::<PeerId>()
        .map_err(|_| WorldError::NetworkProtocolUnavailable {
            protocol: "peer record peer_id must be valid".to_string(),
        })
}

pub(super) fn sync_known_transport_paths(
    known_transport_paths: &mut HashMap<PeerId, Vec<TransportPath>>,
    failed_transport_path_labels: &mut HashSet<String>,
    peer_id: PeerId,
    paths: Vec<TransportPath>,
) {
    let fresh_labels: HashSet<String> = paths.iter().map(TransportPath::label).collect();
    if let Some(previous) = known_transport_paths.get(&peer_id) {
        for stale in previous
            .iter()
            .map(TransportPath::label)
            .filter(|label| !fresh_labels.contains(label))
        {
            failed_transport_path_labels.remove(&stale);
        }
    }
    known_transport_paths.insert(peer_id, paths);
}

pub(super) fn sync_static_bootstrap_transport_paths(
    known_transport_paths: &mut HashMap<PeerId, Vec<TransportPath>>,
    failed_transport_path_labels: &mut HashSet<String>,
    bootstrap_peers: &[Multiaddr],
) {
    for addr in bootstrap_peers {
        let Some(peer_id) = split_peer_id(addr.clone()).0 else {
            continue;
        };
        // Configuration authorizes this exact address, never an endpoint-family
        // match to a signed record at another port.
        let path = active_transport_path_from_endpoint(&HashMap::new(), peer_id, addr);
        let label = path.label();
        let paths = known_transport_paths.entry(peer_id).or_default();
        if let Some(known) = paths.iter_mut().find(|known| known.label() == label) {
            // Only this exact locally configured address has a non-record lifetime.
            known.record_lease = None;
            continue;
        }
        failed_transport_path_labels.remove(&label);
        paths.push(path);
        paths.sort_unstable_by_key(TransportPath::preference_rank);
    }
}

pub(super) fn bootstrap_transport_path_state(
    bootstrap_peers: &[Multiaddr],
) -> (
    HashMap<PeerId, Vec<TransportPath>>,
    HashSet<String>,
    HashSet<PeerId>,
) {
    let mut known_transport_paths = HashMap::new();
    let mut failed_transport_path_labels = HashSet::new();
    sync_static_bootstrap_transport_paths(
        &mut known_transport_paths,
        &mut failed_transport_path_labels,
        bootstrap_peers,
    );
    let static_bootstrap_peer_ids = bootstrap_peers
        .iter()
        .filter_map(|addr| split_peer_id(addr.clone()).0)
        .collect();
    (
        known_transport_paths,
        failed_transport_path_labels,
        static_bootstrap_peer_ids,
    )
}

pub(super) fn select_preferred_transport_path(
    paths: &[TransportPath],
    failed_transport_path_labels: &HashSet<String>,
) -> Option<TransportPath> {
    paths
        .iter()
        .find(|path| !failed_transport_path_labels.contains(&path.label()))
        .cloned()
        .or_else(|| paths.first().cloned())
}

pub(super) fn active_transport_path_from_endpoint(
    known_transport_paths: &HashMap<PeerId, Vec<TransportPath>>,
    peer_id: PeerId,
    endpoint_addr: &Multiaddr,
) -> TransportPath {
    let normalized = ensure_peer_id(endpoint_addr.clone(), peer_id);
    let normalized_without_peer_id = split_peer_id(normalized.clone()).1;

    if let Some(found) = known_transport_paths
        .get(&peer_id)
        .and_then(|paths| transport_path_for_endpoint(paths.as_slice(), &normalized))
        .cloned()
    {
        return found;
    }

    TransportPath {
        record_lease: None,
        peer_id,
        addr: normalized,
        kind: infer_transport_path_kind(&normalized_without_peer_id),
        flavor: infer_transport_session_flavor(
            infer_transport_path_kind(&normalized_without_peer_id),
            &normalized_without_peer_id,
        ),
        security: infer_transport_security(
            infer_transport_path_kind(&normalized_without_peer_id),
            &normalized_without_peer_id,
        ),
        muxer: infer_transport_muxer(
            infer_transport_path_kind(&normalized_without_peer_id),
            &normalized_without_peer_id,
        ),
    }
}

fn transport_path_for_endpoint<'a>(
    paths: &'a [TransportPath],
    endpoint_addr: &Multiaddr,
) -> Option<&'a TransportPath> {
    let endpoint_without_peer_id = split_peer_id(endpoint_addr.clone()).1;
    paths
        .iter()
        .find(|path| {
            path.addr == *endpoint_addr
                || split_peer_id(path.addr.clone()).1 == endpoint_without_peer_id
        })
        .or_else(|| {
            let endpoint_family = transport_endpoint_family(endpoint_without_peer_id.clone());
            paths.iter().find(|path| {
                transport_endpoint_family(split_peer_id(path.addr.clone()).1) == endpoint_family
            })
        })
}

fn transport_endpoint_family(addr: Multiaddr) -> Vec<String> {
    addr.iter()
        .map(|protocol| match protocol {
            Protocol::Ip4(value) => format!("ip4:{value}"),
            Protocol::Ip6(value) => format!("ip6:{value}"),
            Protocol::Dns(value) => format!("dns:{value}"),
            Protocol::Dns4(value) => format!("dns4:{value}"),
            Protocol::Dns6(value) => format!("dns6:{value}"),
            Protocol::Dnsaddr(value) => format!("dnsaddr:{value}"),
            Protocol::Tcp(_) => "tcp:*".to_string(),
            Protocol::Udp(_) => "udp:*".to_string(),
            Protocol::Quic => "quic".to_string(),
            Protocol::QuicV1 => "quic-v1".to_string(),
            Protocol::P2pCircuit => "p2p-circuit".to_string(),
            other => format!("{other}"),
        })
        .collect()
}

pub(super) fn note_established_transport_path(
    known_transport_paths: &HashMap<PeerId, Vec<TransportPath>>,
    active_transport_paths: &mut HashMap<PeerId, TransportPath>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    failed_transport_path_labels: &mut HashSet<String>,
    peer_id: PeerId,
    endpoint_addr: &Multiaddr,
) -> TransportPath {
    let active_path =
        active_transport_path_from_endpoint(known_transport_paths, peer_id, endpoint_addr);
    failed_transport_path_labels.remove(&active_path.label());
    last_dialed_transport_paths.remove(&peer_id);
    active_transport_paths.insert(peer_id, active_path.clone());
    active_path
}

pub(super) fn recompute_active_transport_path_for_peer(
    active_transport_paths: &mut HashMap<PeerId, TransportPath>,
    established_transport_paths: &HashMap<ConnectionId, TransportPath>,
    established_connections_by_peer: &HashMap<PeerId, HashSet<ConnectionId>>,
    peer_id: PeerId,
) -> Option<TransportPath> {
    let replacement = established_connections_by_peer
        .get(&peer_id)
        .into_iter()
        .flat_map(|connection_ids| connection_ids.iter())
        .filter_map(|connection_id| established_transport_paths.get(connection_id))
        .min_by_key(|path| path.preference_rank())
        .cloned();
    match replacement {
        Some(path) => {
            active_transport_paths.insert(peer_id, path.clone());
            Some(path)
        }
        None => {
            active_transport_paths.remove(&peer_id);
            None
        }
    }
}

pub(super) fn dial_transport_path(
    swarm: &mut Swarm<Behaviour>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    path: TransportPath,
) -> Result<(), WorldError> {
    if let Some(lease) = &path.record_lease {
        lease.validate()?;
    }
    dial_addr_with_optional_peer_id(swarm, path.addr.clone()).map_err(|err| {
        WorldError::NetworkProtocolUnavailable {
            protocol: format!("transport dial failed: {err}"),
        }
    })?;
    last_dialed_transport_paths.insert(path.peer_id, path);
    Ok(())
}

pub(super) fn failover_transport_path_after_close(
    swarm: &mut Swarm<Behaviour>,
    known_transport_paths: &HashMap<PeerId, Vec<TransportPath>>,
    active_transport_paths: &mut HashMap<PeerId, TransportPath>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    failed_transport_path_labels: &mut HashSet<String>,
    peer_id: PeerId,
    disconnected_path: Option<TransportPath>,
) -> Result<Option<(Option<TransportPath>, TransportPath)>, WorldError> {
    let Some((previous_path, next_path)) = select_reconnect_transport_path_after_close(
        known_transport_paths,
        active_transport_paths,
        failed_transport_path_labels,
        peer_id,
        disconnected_path,
    ) else {
        return Ok(None);
    };
    dial_transport_path(swarm, last_dialed_transport_paths, next_path.clone())?;
    Ok(Some((previous_path, next_path)))
}

pub(super) fn select_reconnect_transport_path_after_close(
    known_transport_paths: &HashMap<PeerId, Vec<TransportPath>>,
    active_transport_paths: &mut HashMap<PeerId, TransportPath>,
    failed_transport_path_labels: &mut HashSet<String>,
    peer_id: PeerId,
    disconnected_path: Option<TransportPath>,
) -> Option<(Option<TransportPath>, TransportPath)> {
    let previous_path = active_transport_paths
        .remove(&peer_id)
        .or(disconnected_path);
    if let Some(path) = &previous_path {
        failed_transport_path_labels.insert(path.label());
    }
    let next_path = known_transport_paths.get(&peer_id).and_then(|paths| {
        select_preferred_transport_path(paths.as_slice(), failed_transport_path_labels)
    })?;
    Some((previous_path, next_path))
}

pub(super) fn retry_transport_path_after_error(
    swarm: &mut Swarm<Behaviour>,
    known_transport_paths: &HashMap<PeerId, Vec<TransportPath>>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    failed_transport_path_labels: &mut HashSet<String>,
    peer_id: PeerId,
) -> Result<Option<(TransportPath, TransportPath)>, WorldError> {
    let Some(last_path) = last_dialed_transport_paths.remove(&peer_id) else {
        return Ok(None);
    };
    failed_transport_path_labels.insert(last_path.label());
    let Some(next_path) = known_transport_paths.get(&peer_id).and_then(|paths| {
        select_preferred_transport_path(paths.as_slice(), failed_transport_path_labels)
    }) else {
        return Ok(None);
    };
    if next_path.label() == last_path.label() {
        return Ok(None);
    }
    dial_transport_path(swarm, last_dialed_transport_paths, next_path.clone())?;
    Ok(Some((last_path, next_path)))
}

fn extend_paths<'a>(
    paths: &mut Vec<TransportPath>,
    seen: &mut HashSet<String>,
    peer_id: PeerId,
    raw_addrs: impl Iterator<Item = &'a String>,
    kind: TransportPathKind,
) {
    for addr in raw_addrs.filter_map(|raw| raw.parse::<Multiaddr>().ok()) {
        let addr = ensure_peer_id(addr, peer_id);
        let label = addr.to_string();
        if !seen.insert(label) {
            continue;
        }
        let addr_without_peer_id = split_peer_id(addr.clone()).1;
        paths.push(TransportPath {
            record_lease: None,
            peer_id,
            addr,
            kind,
            flavor: infer_transport_session_flavor(kind, &addr_without_peer_id),
            security: infer_transport_security(kind, &addr_without_peer_id),
            muxer: infer_transport_muxer(kind, &addr_without_peer_id),
        });
    }
}

fn infer_transport_path_kind(addr: &Multiaddr) -> TransportPathKind {
    if addr
        .iter()
        .any(|protocol| matches!(protocol, Protocol::P2pCircuit))
    {
        TransportPathKind::RelayReserved
    } else {
        TransportPathKind::Direct
    }
}

fn infer_transport_session_flavor(
    kind: TransportPathKind,
    addr: &Multiaddr,
) -> TransportSessionFlavor {
    if matches!(kind, TransportPathKind::RelayReserved) {
        return TransportSessionFlavor::RelayTunnel;
    }
    if addr
        .iter()
        .any(|protocol| matches!(protocol, Protocol::QuicV1))
    {
        TransportSessionFlavor::Quic
    } else {
        TransportSessionFlavor::TcpNoiseYamux
    }
}

fn infer_transport_security(kind: TransportPathKind, addr: &Multiaddr) -> TransportSecurity {
    match infer_transport_session_flavor(kind, addr) {
        TransportSessionFlavor::Quic => TransportSecurity::QuicTls,
        TransportSessionFlavor::TcpNoiseYamux | TransportSessionFlavor::RelayTunnel => {
            TransportSecurity::Noise
        }
    }
}

fn infer_transport_muxer(kind: TransportPathKind, addr: &Multiaddr) -> TransportMuxer {
    match infer_transport_session_flavor(kind, addr) {
        TransportSessionFlavor::Quic => TransportMuxer::Quic,
        TransportSessionFlavor::TcpNoiseYamux | TransportSessionFlavor::RelayTunnel => {
            TransportMuxer::Yamux
        }
    }
}

pub(super) fn prune_expired_record_paths(
    known: &mut HashMap<PeerId, Vec<TransportPath>>,
    failed_labels: &mut HashSet<String>,
    bootstrap: &[Multiaddr],
) {
    for paths in known.values_mut() {
        paths.retain(|path| {
            let live = path
                .record_lease
                .as_ref()
                .is_none_or(|lease| lease.is_valid());
            if !live {
                failed_labels.remove(&path.label());
            }
            live
        });
    }
    known.retain(|_, paths| !paths.is_empty());
    sync_static_bootstrap_transport_paths(known, failed_labels, bootstrap);
}
