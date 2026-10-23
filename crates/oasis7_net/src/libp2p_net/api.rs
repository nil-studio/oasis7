use super::peer_manager::peer_health_issue_is_record_exchange_pending;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use libp2p::{Multiaddr, PeerId};
use oasis7_proto::distributed_dht::DistributedDht as ProtoDistributedDht;
use oasis7_proto::distributed_net::DistributedNetwork as ProtoDistributedNetwork;

use crate::error::WorldError;
use crate::util::to_canonical_cbor;
use oasis7_proto::distributed::{
    WorldHeadAnnounce, dht_membership_key, dht_peer_record_key, dht_provider_key,
    dht_world_head_key,
};
use oasis7_proto::distributed_dht::{
    MembershipDirectorySnapshot, ProviderRecord, SignedPeerRecord,
};
use oasis7_proto::distributed_net::{NetworkMessage, NetworkSubscription};

use super::{
    Command, Libp2pReachabilitySnapshot, Libp2pTrafficMetricsSnapshot, PeerManagerBlockArtifact,
    PeerManagerHealthIssue, PeerManagerHealthStatus, PeerManagerPeerHealth, snapshot_clone,
    snapshot_traffic_metrics,
};

use super::traffic_metrics::SharedLibp2pTrafficMetrics;
use super::wire_bytes::SharedLibp2pWireByteCounters;

#[derive(Clone)]
pub struct Libp2pNetwork {
    pub(super) peer_id: PeerId,
    pub(super) keypair: libp2p::identity::Keypair,
    pub(super) command_tx: futures::channel::mpsc::Sender<Command>,
    pub(super) inbox: Arc<Mutex<HashMap<String, Vec<Vec<u8>>>>>,
    pub(super) published: Arc<Mutex<Vec<NetworkMessage>>>,
    pub(super) listening_addrs: Arc<Mutex<Vec<Multiaddr>>>,
    pub(super) connected_peers: Arc<Mutex<HashSet<PeerId>>>,
    pub(super) errors: Arc<Mutex<Vec<String>>>,
    pub(super) peer_healths: Arc<Mutex<HashMap<String, PeerManagerPeerHealth>>>,
    pub(super) peer_block_artifacts: Arc<Mutex<HashMap<String, PeerManagerBlockArtifact>>>,
    pub(super) reachability: Arc<Mutex<Libp2pReachabilitySnapshot>>,
    pub(super) traffic_metrics: SharedLibp2pTrafficMetrics,
    pub(super) wire_byte_counters: SharedLibp2pWireByteCounters,
    pub(super) _shutdown_guard: Arc<super::drop_support::ShutdownGuard>,
}
const LIBP2P_COMMAND_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

impl Libp2pNetwork {
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn keypair(&self) -> &libp2p::identity::Keypair {
        &self.keypair
    }

    pub fn published(&self) -> Vec<NetworkMessage> {
        self.published.lock().expect("lock published").clone()
    }

    pub fn dial(&self, addr: Multiaddr) -> Result<(), WorldError> {
        self.enqueue_command(Command::Dial(addr))
    }

    pub fn listening_addrs(&self) -> Vec<Multiaddr> {
        self.listening_addrs
            .lock()
            .expect("lock listening addrs")
            .clone()
    }

    pub fn connected_peers(&self) -> Vec<PeerId> {
        self.connected_peers
            .lock()
            .expect("lock connected peers")
            .iter()
            .cloned()
            .collect()
    }

    pub fn admissible_request_peers(&self) -> Vec<PeerId> {
        let connected_peers: Vec<PeerId> = self
            .connected_peers
            .lock()
            .expect("lock connected peers")
            .iter()
            .copied()
            .collect();
        let peer_healths = self.peer_healths.lock().expect("lock peer healths");
        connected_or_active_transport_peers_from_healths(connected_peers, &peer_healths)
    }

    pub fn debug_errors(&self) -> Vec<String> {
        self.errors.lock().expect("lock errors").clone()
    }

    pub fn request_to_peer(
        &self,
        protocol: &str,
        payload: &[u8],
        peer: PeerId,
    ) -> Result<Vec<u8>, WorldError> {
        self.request_to_peer_with_timeout(protocol, payload, peer, LIBP2P_COMMAND_RESPONSE_TIMEOUT)
    }

    pub fn request_to_peer_with_timeout(
        &self,
        protocol: &str,
        payload: &[u8],
        peer: PeerId,
        timeout: Duration,
    ) -> Result<Vec<u8>, WorldError> {
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::RequestToPeer {
            protocol: protocol.to_string(),
            payload: payload.to_vec(),
            peer,
            response: sender,
        })?;
        let operation = format!("request_to_peer protocol={protocol} peer={peer}");
        block_on_command_response_with_timeout(receiver, operation.as_str(), timeout)
    }

    pub fn debug_peer_healths(&self) -> Vec<PeerManagerPeerHealth> {
        self.peer_healths
            .lock()
            .expect("lock peer healths")
            .values()
            .cloned()
            .collect()
    }

    pub fn debug_peer_block_artifacts(&self) -> Vec<PeerManagerBlockArtifact> {
        self.peer_block_artifacts
            .lock()
            .expect("lock peer block artifacts")
            .values()
            .cloned()
            .collect()
    }

    pub fn reachability_snapshot(&self) -> Libp2pReachabilitySnapshot {
        snapshot_clone(&self.reachability)
    }

    pub fn traffic_metrics_snapshot(&self) -> Libp2pTrafficMetricsSnapshot {
        snapshot_traffic_metrics(&self.traffic_metrics, &self.wire_byte_counters)
    }

    pub(super) fn enqueue_command(&self, command: Command) -> Result<(), WorldError> {
        super::try_send_command(&self.command_tx, command)
    }
}

fn peer_health_is_hard_request_blocked(health: &PeerManagerPeerHealth) -> bool {
    matches!(health.status, PeerManagerHealthStatus::Blocked)
        && !health.issues.is_empty()
        && !health
            .issues
            .iter()
            .all(peer_health_issue_is_record_exchange_pending)
}

fn peer_health_is_soft_deprioritized(health: &PeerManagerPeerHealth) -> bool {
    matches!(health.status, PeerManagerHealthStatus::Blocked)
        && !health.issues.is_empty()
        && health
            .issues
            .iter()
            .all(peer_health_issue_is_record_exchange_pending)
        && health
            .issues
            .iter()
            .any(|issue| matches!(issue, PeerManagerHealthIssue::MissingPeerRecord))
}

fn blocked_and_soft_deprioritized_peers(
    peer_healths: &HashMap<String, PeerManagerPeerHealth>,
) -> (HashSet<PeerId>, HashSet<PeerId>) {
    let mut hard_blocked_peers = HashSet::new();
    let mut soft_deprioritized_peers = HashSet::new();
    for health in peer_healths.values() {
        let Ok(peer_id) = health.peer_id.parse::<PeerId>() else {
            continue;
        };
        if peer_health_is_hard_request_blocked(health) {
            hard_blocked_peers.insert(peer_id);
        }
        if peer_health_is_soft_deprioritized(health) {
            soft_deprioritized_peers.insert(peer_id);
        }
    }
    (hard_blocked_peers, soft_deprioritized_peers)
}

pub(super) fn request_candidate_peers_from_healths(
    peers: Vec<PeerId>,
    hard_blocked_peers: &HashSet<PeerId>,
    soft_deprioritized_peers: &HashSet<PeerId>,
) -> Vec<PeerId> {
    let mut preferred = Vec::new();
    let mut fallback = Vec::new();
    for peer_id in peers {
        if hard_blocked_peers.contains(&peer_id) {
            continue;
        }
        if soft_deprioritized_peers.contains(&peer_id) {
            fallback.push(peer_id);
        } else {
            preferred.push(peer_id);
        }
    }
    if !preferred.is_empty() {
        return preferred;
    }
    fallback
}

fn active_transport_peers_from_healths(
    peer_healths: &HashMap<String, PeerManagerPeerHealth>,
    hard_blocked_peers: &HashSet<PeerId>,
    soft_deprioritized_peers: &HashSet<PeerId>,
) -> Vec<PeerId> {
    let peers = peer_healths
        .values()
        .filter(|health| health.active_path_kind.is_some())
        .filter_map(|health| health.peer_id.parse::<PeerId>().ok())
        .collect();
    let peers = dedup_sorted_peers(peers);
    request_candidate_peers_from_healths(peers, hard_blocked_peers, soft_deprioritized_peers)
}

fn connected_or_active_transport_peers_from_healths(
    connected_peers: Vec<PeerId>,
    peer_healths: &HashMap<String, PeerManagerPeerHealth>,
) -> Vec<PeerId> {
    let connected_peers = dedup_sorted_peers(connected_peers);
    let (hard_blocked_peers, soft_deprioritized_peers) =
        blocked_and_soft_deprioritized_peers(peer_healths);
    let admissible_connected_peers = request_candidate_peers_from_healths(
        connected_peers.clone(),
        &hard_blocked_peers,
        &soft_deprioritized_peers,
    );
    if !admissible_connected_peers.is_empty() {
        return admissible_connected_peers;
    }
    if !connected_peers.is_empty() {
        return Vec::new();
    }
    active_transport_peers_from_healths(
        peer_healths,
        &hard_blocked_peers,
        &soft_deprioritized_peers,
    )
}

pub(super) fn dedup_sorted_peers(mut peers: Vec<PeerId>) -> Vec<PeerId> {
    let mut peers = peers
        .drain(..)
        .map(|peer| (peer.to_bytes(), peer))
        .collect::<Vec<_>>();
    peers.sort_unstable_by(|(left_key, _), (right_key, _)| left_key.cmp(right_key));
    peers.dedup_by(|(left_key, _), (right_key, _)| left_key == right_key);
    peers.into_iter().map(|(_, peer)| peer).collect()
}

impl ProtoDistributedNetwork<WorldError> for Libp2pNetwork {
    fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), WorldError> {
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::Publish {
            topic: topic.to_string(),
            payload: payload.to_vec(),
            response: Some(sender),
        })?;
        block_on_command_response(receiver, "publish")
    }

    fn publish_best_effort(&self, topic: &str, payload: &[u8]) -> Result<(), WorldError> {
        self.enqueue_command(Command::Publish {
            topic: topic.to_string(),
            payload: payload.to_vec(),
            response: None,
        })
    }

    fn subscribe(&self, topic: &str) -> Result<NetworkSubscription, WorldError> {
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::Subscribe {
            topic: topic.to_string(),
            response: sender,
        })?;
        block_on_command_response(receiver, "subscribe")?;
        Ok(NetworkSubscription::new(
            topic.to_string(),
            Arc::clone(&self.inbox),
        ))
    }

    fn request(&self, protocol: &str, payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        self.request_with_providers(protocol, payload, &[])
    }

    fn request_with_providers(
        &self,
        protocol: &str,
        payload: &[u8],
        providers: &[String],
    ) -> Result<Vec<u8>, WorldError> {
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::Request {
            protocol: protocol.to_string(),
            payload: payload.to_vec(),
            providers: providers.to_vec(),
            response: sender,
        })?;
        let operation = if providers.is_empty() {
            format!("request protocol={protocol}")
        } else {
            format!(
                "request protocol={protocol} providers={}",
                providers.join(",")
            )
        };
        block_on_command_response(receiver, operation.as_str())
    }

    fn register_handler(
        &self,
        protocol: &str,
        handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::RegisterHandler {
            protocol: protocol.to_string(),
            handler: Arc::new(move |_context, payload| handler(payload)),
            admission: None,
            response: sender,
        })?;
        block_on_command_response(receiver, "register_handler")
    }

    fn register_handler_with_admission(
        &self,
        protocol: &str,
        admission: oasis7_proto::distributed_net::NetworkAdmission<WorldError>,
        handler: oasis7_proto::distributed_net::NetworkHandler<WorldError>,
    ) -> Result<(), WorldError> {
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::RegisterHandler {
            protocol: protocol.to_string(),
            handler: Arc::new(move |_context, payload| handler(payload)),
            admission: Some(Arc::from(admission)),
            response: sender,
        })?;
        block_on_command_response(receiver, "register_handler_with_admission")
    }

    fn register_context_handler_with_admission(
        &self,
        protocol: &str,
        admission: oasis7_proto::distributed_net::NetworkAdmission<WorldError>,
        handler: oasis7_proto::distributed_net::ContextNetworkHandler<WorldError>,
    ) -> Result<(), WorldError> {
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::RegisterHandler {
            protocol: protocol.to_string(),
            handler: Arc::from(handler),
            admission: Some(Arc::from(admission)),
            response: sender,
        })?;
        block_on_command_response(receiver, "register_context_handler_with_admission")
    }
}

impl ProtoDistributedDht<WorldError> for Libp2pNetwork {
    fn publish_provider(
        &self,
        world_id: &str,
        content_hash: &str,
        _provider_id: &str,
    ) -> Result<(), WorldError> {
        let key = dht_provider_key(world_id, content_hash);
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::PublishProvider(key, Some(sender)))?;
        block_on_command_response(receiver, "publish_provider")
    }

    fn publish_provider_best_effort(
        &self,
        world_id: &str,
        content_hash: &str,
        _provider_id: &str,
    ) -> Result<(), WorldError> {
        let key = dht_provider_key(world_id, content_hash);
        self.enqueue_command(Command::PublishProvider(key, None))
    }

    fn get_providers(
        &self,
        world_id: &str,
        content_hash: &str,
    ) -> Result<Vec<ProviderRecord>, WorldError> {
        let key = dht_provider_key(world_id, content_hash);
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::GetProviders(key, sender))?;
        block_on_command_response(receiver, "get_providers")
    }

    fn put_world_head(&self, world_id: &str, head: &WorldHeadAnnounce) -> Result<(), WorldError> {
        let key = dht_world_head_key(world_id);
        let payload = to_canonical_cbor(head)?;
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::PutWorldHead {
            key,
            payload,
            response: sender,
        })?;
        block_on_command_response(receiver, "put_world_head")
    }

    fn get_world_head(&self, world_id: &str) -> Result<Option<WorldHeadAnnounce>, WorldError> {
        let key = dht_world_head_key(world_id);
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::GetWorldHead(key, sender))?;
        block_on_command_response(receiver, "get_world_head")
    }

    fn put_membership_directory(
        &self,
        world_id: &str,
        snapshot: &MembershipDirectorySnapshot,
    ) -> Result<(), WorldError> {
        let key = dht_membership_key(world_id);
        let payload = to_canonical_cbor(snapshot)?;
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::PutMembershipDirectory {
            key,
            payload,
            response: sender,
        })?;
        block_on_command_response(receiver, "put_membership_directory")
    }

    fn get_membership_directory(
        &self,
        world_id: &str,
    ) -> Result<Option<MembershipDirectorySnapshot>, WorldError> {
        let key = dht_membership_key(world_id);
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::GetMembershipDirectory {
            key,
            response: sender,
        })?;
        block_on_command_response(receiver, "get_membership_directory")
    }

    fn put_peer_record(&self, world_id: &str, record: &SignedPeerRecord) -> Result<(), WorldError> {
        let key = dht_peer_record_key(world_id, record.record.peer_id.as_str());
        let payload = to_canonical_cbor(record)?;
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::PutPeerRecord {
            key,
            payload,
            response: sender,
        })?;
        block_on_command_response(receiver, "put_peer_record")
    }

    fn get_peer_record(
        &self,
        world_id: &str,
        peer_id: &str,
    ) -> Result<Option<SignedPeerRecord>, WorldError> {
        let requested_peer_id =
            peer_id
                .parse::<PeerId>()
                .map_err(|_| WorldError::NetworkProtocolUnavailable {
                    protocol: "get peer record peer_id must be valid".to_string(),
                })?;
        let key = dht_peer_record_key(world_id, peer_id);
        let (sender, receiver) = mpsc::channel();
        self.enqueue_command(Command::GetPeerRecord {
            key,
            peer_id: requested_peer_id,
            response: sender,
        })?;
        block_on_command_response(receiver, "get_peer_record")
    }
}

fn block_on_command_response<T>(
    receiver: mpsc::Receiver<Result<T, WorldError>>,
    operation: &str,
) -> Result<T, WorldError> {
    block_on_command_response_with_timeout(receiver, operation, LIBP2P_COMMAND_RESPONSE_TIMEOUT)
}

fn block_on_command_response_with_timeout<T>(
    receiver: mpsc::Receiver<Result<T, WorldError>>,
    operation: &str,
    timeout: Duration,
) -> Result<T, WorldError> {
    match receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(WorldError::NetworkProtocolUnavailable {
            protocol: format!("libp2p command {operation} response channel closed"),
        }),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(WorldError::NetworkProtocolUnavailable {
            protocol: format!(
                "libp2p command {operation} timed out after {}ms",
                timeout.as_millis()
            ),
        }),
    }
}

#[cfg(test)]
mod command_response_tests {
    use super::*;

    #[test]
    fn command_response_wait_times_out_when_runtime_does_not_reply() {
        let (_sender, receiver) = mpsc::channel::<Result<(), WorldError>>();

        let err = block_on_command_response_with_timeout(
            receiver,
            "request_to_peer protocol=/aw/node/replication/fetch-commit/1.0.0 peer=12D3KooWTest",
            Duration::from_millis(1),
        )
        .expect_err("missing runtime response should time out");

        match err {
            WorldError::NetworkProtocolUnavailable { protocol } => {
                assert!(protocol.contains("libp2p command request_to_peer"));
                assert!(protocol.contains("/aw/node/replication/fetch-commit/1.0.0"));
                assert!(protocol.contains("peer=12D3KooWTest"));
                assert!(protocol.contains("timed out"));
            }
            other => panic!("expected NetworkProtocolUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn command_response_wait_reports_closed_channel() {
        let (sender, receiver) = mpsc::channel::<Result<(), WorldError>>();
        drop(sender);

        let err =
            block_on_command_response_with_timeout(receiver, "request", Duration::from_millis(100))
                .expect_err("closed runtime response should fail");

        match err {
            WorldError::NetworkProtocolUnavailable { protocol } => {
                assert!(protocol.contains("libp2p command request response channel closed"));
            }
            other => panic!("expected NetworkProtocolUnavailable, got {other:?}"),
        }
    }
}
