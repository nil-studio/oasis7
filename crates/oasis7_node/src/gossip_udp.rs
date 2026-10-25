use std::collections::{BTreeMap, BTreeSet};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) use oasis7_consensus::node_consensus_message::{
    NodeGossipAttestationMessage as GossipAttestationMessage,
    NodeGossipCommitMessage as GossipCommitMessage,
    NodeGossipProposalMessage as GossipProposalMessage,
};
use oasis7_proto::distributed_checkpoint_lineage::{
    CheckpointLineageCheckpointV1, CheckpointLineageHeadV1, CheckpointLineageVoteV1,
};
use serde::{Deserialize, Serialize};

use crate::replication::GossipReplicationMessage;
use crate::{NodeError, NodeGossipConfig};
use oasis7_proto::distributed_bft::BftMessageV1;

const MAX_GOSSIP_DATAGRAM_PAYLOAD_BYTES: usize = 65_507;
const MAX_GOSSIP_RECV_BUFFER_BYTES: usize = 65_535;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum GossipMessage {
    Hello(GossipHelloMessage),
    Commit(GossipCommitMessage),
    Proposal(GossipProposalMessage),
    Attestation(GossipAttestationMessage),
    CheckpointLineageVote(GossipCheckpointLineageVoteMessage),
    Replication(GossipReplicationMessage),
    Bft { message: BftMessageV1 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GossipCheckpointLineageVoteMessage {
    pub version: u8,
    pub world_id: String,
    pub round_id: String,
    pub checkpoint: CheckpointLineageCheckpointV1,
    pub head: CheckpointLineageHeadV1,
    pub validator_set_hash: String,
    pub total_stake: u64,
    pub required_stake: u64,
    pub vote: CheckpointLineageVoteV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GossipHelloMessage {
    pub version: u8,
    pub world_id: String,
    pub node_id: String,
    pub sent_at_ms: i64,
}

#[derive(Debug)]
pub(crate) struct GossipEndpoint {
    socket: UdpSocket,
    bind_addr: SocketAddr,
    peers: Mutex<GossipPeerBook>,
    traffic_metrics: Mutex<GossipTrafficMetricsSnapshot>,
    max_dynamic_peers: usize,
    dynamic_peer_ttl_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReceivedGossipMessage {
    pub from: SocketAddr,
    pub message: GossipMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GossipTrafficDirectionMetricsSnapshot {
    pub datagrams: u64,
    pub payload_bytes: u64,
    pub attempted_datagrams: u64,
    pub succeeded_datagrams: u64,
    pub failed_datagrams: u64,
    pub failure_ratio_ppm: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GossipTrafficLaneMetricsSnapshot {
    pub inbound: GossipTrafficDirectionMetricsSnapshot,
    pub outbound: GossipTrafficDirectionMetricsSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GossipTrafficMetricsSnapshot {
    pub observed_since_unix_ms: i64,
    pub scope: String,
    pub excludes_transport_headers: bool,
    pub totals: GossipTrafficLaneMetricsSnapshot,
    pub by_kind: BTreeMap<String, GossipTrafficLaneMetricsSnapshot>,
    pub by_error_kind: BTreeMap<String, u64>,
}

impl Default for GossipTrafficMetricsSnapshot {
    fn default() -> Self {
        Self {
            observed_since_unix_ms: now_unix_ms(),
            scope: "udp_payload_bytes".to_string(),
            excludes_transport_headers: true,
            totals: GossipTrafficLaneMetricsSnapshot::default(),
            by_kind: BTreeMap::new(),
            by_error_kind: BTreeMap::new(),
        }
    }
}

impl GossipEndpoint {
    pub(crate) fn broadcast_hello(&self, message: &GossipHelloMessage) -> Result<(), NodeError> {
        self.broadcast_message(GossipMessage::Hello(message.clone()))
    }

    pub(crate) fn bind(config: &NodeGossipConfig) -> Result<Self, NodeError> {
        let socket = UdpSocket::bind(config.bind_addr).map_err(|err| NodeError::Gossip {
            reason: format!("bind {} failed: {}", config.bind_addr, err),
        })?;
        socket
            .set_nonblocking(true)
            .map_err(|err| NodeError::Gossip {
                reason: format!("set_nonblocking failed: {}", err),
            })?;
        let peers = config
            .peers
            .iter()
            .copied()
            .filter(|peer| *peer != config.bind_addr)
            .collect::<BTreeSet<_>>();
        Ok(Self {
            socket,
            bind_addr: config.bind_addr,
            peers: Mutex::new(GossipPeerBook::new(peers)),
            traffic_metrics: Mutex::new(GossipTrafficMetricsSnapshot::default()),
            max_dynamic_peers: config.max_dynamic_peers.max(1),
            dynamic_peer_ttl_ms: config.dynamic_peer_ttl_ms.max(1),
        })
    }

    pub(crate) fn broadcast_commit(&self, message: &GossipCommitMessage) -> Result<(), NodeError> {
        self.broadcast_message(GossipMessage::Commit(message.clone()))
    }

    pub(crate) fn broadcast_proposal(
        &self,
        message: &GossipProposalMessage,
    ) -> Result<(), NodeError> {
        self.broadcast_message(GossipMessage::Proposal(message.clone()))
    }

    pub(crate) fn broadcast_attestation(
        &self,
        message: &GossipAttestationMessage,
    ) -> Result<(), NodeError> {
        self.broadcast_message(GossipMessage::Attestation(message.clone()))
    }

    pub(crate) fn broadcast_checkpoint_lineage_vote(
        &self,
        message: &GossipCheckpointLineageVoteMessage,
    ) -> Result<(), NodeError> {
        self.broadcast_message(GossipMessage::CheckpointLineageVote(message.clone()))
    }

    pub(crate) fn broadcast_replication(
        &self,
        message: &GossipReplicationMessage,
    ) -> Result<(), NodeError> {
        self.broadcast_message(GossipMessage::Replication(message.clone()))
    }

    pub(crate) fn broadcast_bft(&self, message: &BftMessageV1) -> Result<(), NodeError> {
        self.broadcast_message(GossipMessage::Bft {
            message: message.clone(),
        })
    }

    fn broadcast_message(&self, envelope: GossipMessage) -> Result<(), NodeError> {
        let kind = gossip_message_kind_label(&envelope);
        let bytes = serde_json::to_vec(&envelope).map_err(|err| NodeError::Gossip {
            reason: format!("serialize gossip message failed: {}", err),
        })?;
        if bytes.len() > MAX_GOSSIP_DATAGRAM_PAYLOAD_BYTES {
            return Err(NodeError::Gossip {
                reason: format!(
                    "gossip datagram too large bytes={} max={MAX_GOSSIP_DATAGRAM_PAYLOAD_BYTES}",
                    bytes.len()
                ),
            });
        }
        self.broadcast_bytes(kind, &bytes)
    }

    fn broadcast_bytes(&self, kind: &str, bytes: &[u8]) -> Result<(), NodeError> {
        let peers = self.snapshot_peers()?;
        let mut sent_datagrams = 0u64;
        let mut failure_count = 0usize;
        let mut first_error = None;
        let mut failure_by_error_kind = BTreeMap::new();
        for peer in &peers {
            match self.socket.send_to(bytes, peer) {
                Ok(_) => sent_datagrams = sent_datagrams.saturating_add(1),
                Err(err) => {
                    failure_count = failure_count.saturating_add(1);
                    *failure_by_error_kind
                        .entry(io_error_kind_label(err.kind()).to_string())
                        .or_insert(0u64) += 1;
                    if first_error.is_none() {
                        first_error = Some(format!("send_to {} failed: {}", peer, err));
                    }
                }
            }
        }
        self.record_outbound(
            kind,
            bytes.len(),
            peers.len() as u64,
            sent_datagrams,
            &failure_by_error_kind,
        );
        if failure_count == 0 {
            Ok(())
        } else {
            Err(NodeError::Gossip {
                reason: format!(
                    "gossip broadcast partial failure sent={} failed={} first_error={}",
                    sent_datagrams,
                    failure_count,
                    first_error.as_deref().unwrap_or("unknown")
                ),
            })
        }
    }

    pub(crate) fn remember_peer(&self, peer: SocketAddr) -> Result<(), NodeError> {
        if peer == self.bind_addr || peer.port() == 0 {
            return Ok(());
        }
        let mut peers = self.peers.lock().map_err(|_| NodeError::Gossip {
            reason: "peers mutex poisoned".to_string(),
        })?;
        peers.remember_dynamic_peer(
            peer,
            now_unix_ms(),
            self.max_dynamic_peers,
            self.dynamic_peer_ttl_ms,
        );
        Ok(())
    }

    pub(crate) fn drain_messages(&self) -> Result<Vec<ReceivedGossipMessage>, NodeError> {
        self.drain_messages_limited(usize::MAX)
    }

    pub(crate) fn drain_messages_limited(
        &self,
        max_messages: usize,
    ) -> Result<Vec<ReceivedGossipMessage>, NodeError> {
        let mut buf = [0u8; MAX_GOSSIP_RECV_BUFFER_BYTES];
        let mut messages = Vec::new();
        let mut datagrams = 0usize;
        while datagrams < max_messages {
            match self.socket.recv_from(&mut buf) {
                Ok((size, from)) => {
                    datagrams = datagrams.saturating_add(1);
                    let payload = &buf[..size];
                    if let Ok(message) = serde_json::from_slice::<GossipMessage>(payload) {
                        self.record_inbound(gossip_message_kind_label(&message), size);
                        messages.push(ReceivedGossipMessage { from, message });
                        continue;
                    }
                    if let Ok(commit) = serde_json::from_slice::<GossipCommitMessage>(payload) {
                        self.record_inbound("commit", size);
                        messages.push(ReceivedGossipMessage {
                            from,
                            message: GossipMessage::Commit(commit),
                        });
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(err) => {
                    return Err(NodeError::Gossip {
                        reason: format!("recv_from failed: {}", err),
                    });
                }
            }
        }
        Ok(messages)
    }

    fn snapshot_peers(&self) -> Result<Vec<SocketAddr>, NodeError> {
        let mut peers = self.peers.lock().map_err(|_| NodeError::Gossip {
            reason: "peers mutex poisoned".to_string(),
        })?;
        Ok(peers.snapshot(now_unix_ms(), self.dynamic_peer_ttl_ms))
    }

    pub(crate) fn traffic_metrics_snapshot(&self) -> GossipTrafficMetricsSnapshot {
        self.traffic_metrics
            .lock()
            .map(|locked| locked.clone())
            .unwrap_or_else(|_| GossipTrafficMetricsSnapshot {
                scope: "udp_payload_bytes_degraded".to_string(),
                ..GossipTrafficMetricsSnapshot::default()
            })
    }

    fn record_outbound(
        &self,
        kind: &str,
        payload_bytes: usize,
        attempted_datagrams: u64,
        succeeded_datagrams: u64,
        failure_by_error_kind: &BTreeMap<String, u64>,
    ) {
        let Ok(mut snapshot) = self.traffic_metrics.lock() else {
            return;
        };
        let payload_bytes = (payload_bytes as u64).saturating_mul(succeeded_datagrams);
        bump_gossip_lane(
            &mut snapshot.totals.outbound,
            payload_bytes,
            succeeded_datagrams,
        );
        bump_gossip_outbound_attempts(
            &mut snapshot.totals.outbound,
            attempted_datagrams,
            succeeded_datagrams,
        );
        for (error_kind, count) in failure_by_error_kind {
            *snapshot
                .by_error_kind
                .entry(error_kind.clone())
                .or_default() += count;
        }
        bump_gossip_lane(
            &mut snapshot
                .by_kind
                .entry(kind.to_string())
                .or_default()
                .outbound,
            payload_bytes,
            succeeded_datagrams,
        );
        bump_gossip_outbound_attempts(
            &mut snapshot
                .by_kind
                .entry(kind.to_string())
                .or_default()
                .outbound,
            attempted_datagrams,
            succeeded_datagrams,
        );
    }

    fn record_inbound(&self, kind: &str, payload_bytes: usize) {
        let Ok(mut snapshot) = self.traffic_metrics.lock() else {
            return;
        };
        let payload_bytes = payload_bytes as u64;
        bump_gossip_lane(&mut snapshot.totals.inbound, payload_bytes, 1);
        bump_gossip_lane(
            &mut snapshot
                .by_kind
                .entry(kind.to_string())
                .or_default()
                .inbound,
            payload_bytes,
            1,
        );
    }
}

fn bump_gossip_lane(
    lane: &mut GossipTrafficDirectionMetricsSnapshot,
    payload_bytes: u64,
    datagrams: u64,
) {
    lane.datagrams = lane.datagrams.saturating_add(datagrams);
    lane.payload_bytes = lane.payload_bytes.saturating_add(payload_bytes);
}

fn bump_gossip_outbound_attempts(
    lane: &mut GossipTrafficDirectionMetricsSnapshot,
    attempted_datagrams: u64,
    succeeded_datagrams: u64,
) {
    lane.attempted_datagrams = lane.attempted_datagrams.saturating_add(attempted_datagrams);
    lane.succeeded_datagrams = lane.succeeded_datagrams.saturating_add(succeeded_datagrams);
    lane.failed_datagrams = lane
        .attempted_datagrams
        .saturating_sub(lane.succeeded_datagrams);
    lane.failure_ratio_ppm = if lane.attempted_datagrams == 0 {
        0
    } else {
        lane.failed_datagrams
            .saturating_mul(1_000_000)
            .checked_div(lane.attempted_datagrams)
            .unwrap_or(0)
    };
}

fn io_error_kind_label(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::TimedOut => "timed_out",
        std::io::ErrorKind::ConnectionRefused => "connection_refused",
        std::io::ErrorKind::ConnectionReset => "connection_reset",
        std::io::ErrorKind::ConnectionAborted => "connection_aborted",
        std::io::ErrorKind::NotConnected => "not_connected",
        std::io::ErrorKind::AddrInUse => "addr_in_use",
        std::io::ErrorKind::AddrNotAvailable => "addr_not_available",
        std::io::ErrorKind::NetworkDown => "network_down",
        std::io::ErrorKind::NetworkUnreachable => "network_unreachable",
        std::io::ErrorKind::HostUnreachable => "host_unreachable",
        _ => "other",
    }
}

fn gossip_message_kind_label(message: &GossipMessage) -> &'static str {
    match message {
        GossipMessage::Hello(_) => "hello",
        GossipMessage::Commit(_) => "commit",
        GossipMessage::Proposal(_) => "proposal",
        GossipMessage::Attestation(_) => "attestation",
        GossipMessage::CheckpointLineageVote(_) => "checkpoint_lineage_vote",
        GossipMessage::Replication(_) => "replication",
        GossipMessage::Bft { .. } => "bft",
    }
}

#[derive(Debug, Clone)]
struct GossipPeerBook {
    static_peers: BTreeSet<SocketAddr>,
    dynamic_peers: BTreeMap<SocketAddr, i64>,
}

impl GossipPeerBook {
    fn new(static_peers: BTreeSet<SocketAddr>) -> Self {
        Self {
            static_peers,
            dynamic_peers: BTreeMap::new(),
        }
    }

    fn remember_dynamic_peer(
        &mut self,
        peer: SocketAddr,
        now_ms: i64,
        max_dynamic_peers: usize,
        dynamic_peer_ttl_ms: i64,
    ) {
        self.prune_expired_dynamic_peers(now_ms, dynamic_peer_ttl_ms);
        if self.static_peers.contains(&peer) {
            return;
        }
        if let Some(last_seen) = self.dynamic_peers.get_mut(&peer) {
            *last_seen = now_ms;
            return;
        }
        while self.dynamic_peers.len() >= max_dynamic_peers {
            let Some(evicted_peer) = self.oldest_dynamic_peer() else {
                break;
            };
            self.dynamic_peers.remove(&evicted_peer);
        }
        self.dynamic_peers.insert(peer, now_ms);
    }

    fn snapshot(&mut self, now_ms: i64, dynamic_peer_ttl_ms: i64) -> Vec<SocketAddr> {
        self.prune_expired_dynamic_peers(now_ms, dynamic_peer_ttl_ms);
        let mut peers = self.static_peers.iter().copied().collect::<Vec<_>>();
        peers.extend(
            self.dynamic_peers
                .keys()
                .filter(|peer| !self.static_peers.contains(peer))
                .copied(),
        );
        peers
    }

    fn oldest_dynamic_peer(&self) -> Option<SocketAddr> {
        self.dynamic_peers
            .iter()
            .min_by(|(left_addr, left_seen), (right_addr, right_seen)| {
                left_seen
                    .cmp(right_seen)
                    .then_with(|| left_addr.cmp(right_addr))
            })
            .map(|(peer, _)| *peer)
    }

    fn prune_expired_dynamic_peers(&mut self, now_ms: i64, dynamic_peer_ttl_ms: i64) {
        self.dynamic_peers
            .retain(|_, last_seen_ms| now_ms.saturating_sub(*last_seen_ms) <= dynamic_peer_ttl_ms);
    }
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::net::UdpSocket;
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::NodeGossipConfig;

    #[test]
    fn bft_envelope_roundtrips_without_colliding_with_legacy_kind_tag() {
        let message = GossipMessage::Bft {
            message: BftMessageV1::HistoryRequest(
                oasis7_proto::distributed_bft::BftHistoryRequestV1 {
                    scope: oasis7_proto::distributed_bft::BftScopeV1 {
                        world_id: "w".into(),
                        chain_id: "c".into(),
                        genesis_hash: "11".repeat(32),
                    },
                    validator_set_hash: "22".repeat(32),
                    after_height: 0,
                    after_block_id: "11".repeat(32),
                },
            ),
        };
        let bytes = serde_json::to_vec(&message).expect("encode BFT envelope");
        let decoded: GossipMessage = serde_json::from_slice(&bytes).expect("decode same envelope");
        assert_eq!(decoded, message);
    }

    fn gossip_config(bind_addr: SocketAddr, peers: Vec<SocketAddr>) -> NodeGossipConfig {
        NodeGossipConfig {
            bind_addr,
            peers,
            max_dynamic_peers: 8,
            dynamic_peer_ttl_ms: 60_000,
        }
    }

    #[test]
    fn gossip_endpoint_tracks_inbound_and_outbound_payload_bytes() {
        let socket_a = UdpSocket::bind("127.0.0.1:0").expect("bind a");
        let socket_b = UdpSocket::bind("127.0.0.1:0").expect("bind b");
        let addr_a = socket_a.local_addr().expect("addr a");
        let addr_b = socket_b.local_addr().expect("addr b");
        drop(socket_a);
        drop(socket_b);

        let endpoint_a = GossipEndpoint::bind(&gossip_config(addr_a, vec![addr_b])).expect("a");
        let endpoint_b = GossipEndpoint::bind(&gossip_config(addr_b, vec![addr_a])).expect("b");
        endpoint_a
            .broadcast_commit(&GossipCommitMessage {
                version: 1,
                world_id: "w".to_string(),
                node_id: "node-a".to_string(),
                player_id: "player-a".to_string(),
                height: 1,
                slot: 1,
                epoch: 0,
                block_hash: "block-1".to_string(),
                action_root: "root".to_string(),
                actions: Vec::new(),
                committed_at_ms: 1_000,
                execution_block_hash: None,
                execution_state_root: None,
                public_key_hex: None,
                signature_hex: None,
            })
            .expect("broadcast");
        thread::sleep(Duration::from_millis(20));

        let received = endpoint_b.drain_messages().expect("drain");
        assert_eq!(received.len(), 1);

        let outbound = endpoint_a.traffic_metrics_snapshot();
        assert_eq!(outbound.totals.outbound.datagrams, 1);
        assert!(outbound.totals.outbound.payload_bytes > 0);
        assert_eq!(
            outbound
                .by_kind
                .get("commit")
                .map(|lane| lane.outbound.datagrams),
            Some(1)
        );

        let inbound = endpoint_b.traffic_metrics_snapshot();
        assert_eq!(inbound.totals.inbound.datagrams, 1);
        assert!(inbound.totals.inbound.payload_bytes > 0);
        assert_eq!(
            inbound
                .by_kind
                .get("commit")
                .map(|lane| lane.inbound.datagrams),
            Some(1)
        );
    }

    #[test]
    fn limited_drain_counts_malformed_datagrams_toward_its_budget() {
        let endpoint = GossipEndpoint::bind(&gossip_config(
            "127.0.0.1:0".parse().expect("loopback address"),
            Vec::new(),
        ))
        .expect("endpoint");
        let destination = endpoint.socket.local_addr().expect("endpoint address");
        let sender = UdpSocket::bind("127.0.0.1:0").expect("sender socket");
        for _ in 0..3 {
            sender
                .send_to(b"not a gossip message", destination)
                .expect("send malformed datagram");
        }
        let valid = GossipMessage::Hello(GossipHelloMessage {
            version: 1,
            world_id: "world".into(),
            node_id: "node".into(),
            sent_at_ms: 1,
        });
        sender
            .send_to(
                serde_json::to_vec(&valid)
                    .expect("encode valid gossip")
                    .as_slice(),
                destination,
            )
            .expect("send valid datagram");
        thread::sleep(Duration::from_millis(20));

        assert!(
            endpoint
                .drain_messages_limited(2)
                .expect("bounded malformed drain")
                .is_empty()
        );
        let remaining = endpoint.drain_messages().expect("drain remaining packets");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].message, valid);
    }

    #[test]
    fn gossip_endpoint_continues_broadcast_after_intermediate_send_error() {
        let socket_a = UdpSocket::bind("127.0.0.1:0").expect("bind a");
        let socket_b = UdpSocket::bind("127.0.0.1:0").expect("bind b");
        let socket_c = UdpSocket::bind("127.0.0.1:0").expect("bind c");
        let addr_a = socket_a.local_addr().expect("addr a");
        let addr_b = socket_b.local_addr().expect("addr b");
        let addr_c = socket_c.local_addr().expect("addr c");
        drop(socket_a);
        drop(socket_b);
        drop(socket_c);

        let invalid_peer: SocketAddr = "255.255.255.255:1".parse().expect("invalid peer");
        let endpoint_a =
            GossipEndpoint::bind(&gossip_config(addr_a, vec![addr_b, invalid_peer, addr_c]))
                .expect("a");
        let endpoint_b = GossipEndpoint::bind(&gossip_config(addr_b, vec![addr_a])).expect("b");
        let endpoint_c = GossipEndpoint::bind(&gossip_config(addr_c, vec![addr_a])).expect("c");

        let err = endpoint_a
            .broadcast_commit(&GossipCommitMessage {
                version: 1,
                world_id: "w".to_string(),
                node_id: "node-a".to_string(),
                player_id: "player-a".to_string(),
                height: 1,
                slot: 1,
                epoch: 0,
                block_hash: "block-1".to_string(),
                action_root: "root".to_string(),
                actions: Vec::new(),
                committed_at_ms: 1_000,
                execution_block_hash: None,
                execution_state_root: None,
                public_key_hex: None,
                signature_hex: None,
            })
            .expect_err("partial send should surface send_to error");
        assert!(matches!(
            err,
            NodeError::Gossip { ref reason }
                if reason.contains("partial failure")
                    && reason.contains("sent=2")
                    && reason.contains("failed=1")
        ));

        thread::sleep(Duration::from_millis(20));

        let received_b = endpoint_b.drain_messages().expect("drain b");
        let received_c = endpoint_c.drain_messages().expect("drain c");
        assert_eq!(received_b.len(), 1);
        assert_eq!(received_c.len(), 1);

        let outbound = endpoint_a.traffic_metrics_snapshot();
        assert_eq!(outbound.totals.outbound.attempted_datagrams, 3);
        assert_eq!(outbound.totals.outbound.succeeded_datagrams, 2);
        assert_eq!(outbound.totals.outbound.failed_datagrams, 1);
        assert_eq!(outbound.totals.outbound.failure_ratio_ppm, 333_333);
        assert_eq!(
            outbound.by_error_kind.get("permission_denied").copied(),
            Some(1)
        );
        assert_eq!(outbound.totals.outbound.datagrams, 2);
        assert!(outbound.totals.outbound.payload_bytes > 0);
        assert_eq!(
            outbound
                .by_kind
                .get("commit")
                .map(|lane| lane.outbound.datagrams),
            Some(2)
        );
    }

    #[test]
    fn gossip_endpoint_rejects_oversized_replication_datagram_before_send() {
        let socket_a = UdpSocket::bind("127.0.0.1:0").expect("bind a");
        let socket_b = UdpSocket::bind("127.0.0.1:0").expect("bind b");
        let addr_a = socket_a.local_addr().expect("addr a");
        let addr_b = socket_b.local_addr().expect("addr b");
        drop(socket_a);
        drop(socket_b);

        let endpoint_a = GossipEndpoint::bind(&gossip_config(addr_a, vec![addr_b])).expect("a");
        let err = endpoint_a
            .broadcast_replication(&GossipReplicationMessage {
                version: 1,
                world_id: "w".to_string(),
                node_id: "node-a".to_string(),
                record: oasis7_distfs::FileReplicationRecord {
                    world_id: "w".to_string(),
                    writer_id: "writer-a".to_string(),
                    writer_epoch: 1,
                    sequence: 1,
                    path: "consensus/commits/0001.json".to_string(),
                    content_hash: "hash-1".to_string(),
                    size_bytes: (MAX_GOSSIP_DATAGRAM_PAYLOAD_BYTES + 1) as u64,
                    updated_at_ms: 1,
                },
                payload: vec![0_u8; MAX_GOSSIP_DATAGRAM_PAYLOAD_BYTES + 1],
                public_key_hex: None,
                signature_hex: None,
            })
            .expect_err("oversized replication datagram must be rejected");
        assert!(matches!(
            err,
            NodeError::Gossip { ref reason }
                if reason.contains("gossip datagram too large")
                    && reason.contains(&format!("max={MAX_GOSSIP_DATAGRAM_PAYLOAD_BYTES}"))
        ));

        let outbound = endpoint_a.traffic_metrics_snapshot();
        assert_eq!(outbound.totals.outbound.datagrams, 0);
        assert_eq!(outbound.totals.outbound.payload_bytes, 0);
    }
}
