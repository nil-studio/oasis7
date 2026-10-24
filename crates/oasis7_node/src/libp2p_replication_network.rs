use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use oasis7_net::{
    Libp2pNetwork, Libp2pNetworkConfig, Libp2pReachabilitySnapshot, Libp2pTrafficMetricsSnapshot,
    PeerManagerPolicy, world_error_is_missing_handler, world_error_is_retryable_connection_gap,
};
pub use oasis7_net::{PeerRecordReceiverConfig, PeerRecordReceiverMode};
use oasis7_proto::distributed::WorldHeadAnnounce;
use oasis7_proto::distributed::{DistributedErrorCode, ErrorResponse};
use oasis7_proto::distributed_dht::{
    DistributedDht, MembershipDirectorySnapshot, PeerDiscoverySource, PeerRecord, ProviderRecord,
    SignedPeerRecord,
};
use oasis7_proto::world_error::WorldError;

use crate::NodeError;
use crate::replication::REPLICATION_FETCH_COMMIT_PROTOCOL;

mod net_impl;

#[cfg(test)]
#[path = "libp2p_replication_network/admission_tests.rs"]
mod admission_tests;

#[cfg(test)]
use oasis7_net::{PeerManagerHealthIssue, PeerManagerHealthStatus, PeerManagerPeerHealth};

type Handler = Arc<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>;
type HandlerInput = Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>;
type Admission = Arc<dyn Fn(&[u8]) -> Result<(), WorldError> + Send + Sync>;
type ContextHandler = Arc<
    dyn Fn(
            &oasis7_proto::distributed_net::NetworkRequestContext,
            &[u8],
        ) -> Result<Vec<u8>, WorldError>
        + Send
        + Sync,
>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicationPeerHealthDebug {
    pub peer_id: String,
    pub status: String,
    pub issues: Vec<String>,
    pub discovery_sources: Vec<String>,
    pub active_path_kind: Option<String>,
    pub source_operator: Option<String>,
    pub source_asn: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicationNetworkDebugSnapshot {
    pub local_peer_id: String,
    pub connected_peers: Vec<String>,
    pub peer_healths: Vec<ReplicationPeerHealthDebug>,
    pub registered_protocols: Vec<String>,
    pub protocol_retry_cooldown_peers: HashMap<String, Vec<String>>,
    pub transport_retry_cooldown_peers: Vec<String>,
    pub request_peer_scores: HashMap<String, u8>,
    pub recent_errors: Vec<String>,
}

const REQUEST_CONNECTED_PEER_WAIT_RETRIES: usize = 12;
const REQUEST_PROVIDER_CONNECTED_PEER_WAIT_RETRIES: usize = 80;
const REQUEST_CONNECTED_PEER_WAIT_INTERVAL_MS: u64 = 150;
const REQUEST_CONNECTION_REFRESH_RETRIES: usize = 12;
const PROTOCOL_RETRY_COOLDOWN_AFTER_MS: u64 = 5_000;
const REQUEST_PEER_DEFAULT_SCORE: u8 = 100;
const REQUEST_PEER_TRANSPORT_PENALTY: u8 = 20;
const REQUEST_PEER_PROTOCOL_PENALTY: u8 = 30;
const REQUEST_PEER_GENERIC_PENALTY: u8 = 10;
const REQUEST_PEER_SUCCESS_RECOVERY: u8 = 5;
const REQUEST_PEER_SCORE_RECOVERY_AFTER_MS: u64 = 60_000;
const REQUEST_TO_PEER_TIMEOUT_MS: u64 = 12_000;
const REQUEST_RETRY_BUDGET_MS: u64 = 20_000;
pub(crate) const FETCH_COMMIT_REQUEST_TO_PEER_TIMEOUT_MS: u64 = 30_000;
pub(crate) const FETCH_COMMIT_REQUEST_RETRY_BUDGET_MS: u64 = 120_000;
const FETCH_BLOB_REQUEST_TO_PEER_TIMEOUT_MS: u64 = 30_000;
const FETCH_BLOB_REQUEST_RETRY_BUDGET_MS: u64 = 180_000;
const REPLICATION_FETCH_BLOB_PROTOCOL_MARKER: &str = "/fetch-blob/";

#[derive(Debug, Clone)]
struct RequestPeerScore {
    score: u8,
    updated_at: Instant,
}

#[derive(Debug, Clone)]
pub struct Libp2pReplicationNetworkConfig {
    pub keypair: Option<Keypair>,
    pub peer_record: Option<PeerRecord>,
    /// Enables durable V2 publication for the local signed peer record.
    /// `None` preserves the legacy V1 publication behavior.
    pub peer_record_publication_directory: Option<PathBuf>,
    /// Independent inbound peer-record policy and its fixed durable receiver directory.
    pub peer_record_receiver: PeerRecordReceiverConfig,
    pub listen_addrs: Vec<Multiaddr>,
    pub bootstrap_peers: Vec<Multiaddr>,
    pub bootstrap_redial_interval_ms: i64,
    pub republish_interval_ms: i64,
    pub discovery_query_interval_ms: i64,
    pub discovery_query_cooldown_ms: i64,
    pub enable_autonat: bool,
    pub peer_manager_policy: PeerManagerPolicy,
    pub allow_local_handler_fallback_when_no_peers: bool,
    pub protocol_retry_cooldown_after: Duration,
    pub request_timeout: Duration,
    pub request_retry_budget: Duration,
    pub fetch_blob_request_timeout: Duration,
    pub fetch_blob_request_retry_budget: Duration,
}

impl Default for Libp2pReplicationNetworkConfig {
    fn default() -> Self {
        let inner_defaults = Libp2pNetworkConfig::default();
        Self {
            keypair: None,
            peer_record: None,
            peer_record_publication_directory: None,
            peer_record_receiver: PeerRecordReceiverConfig::default(),
            listen_addrs: Vec::new(),
            bootstrap_peers: Vec::new(),
            bootstrap_redial_interval_ms: inner_defaults.bootstrap_redial_interval_ms,
            republish_interval_ms: inner_defaults.republish_interval_ms,
            discovery_query_interval_ms: inner_defaults.discovery_query_interval_ms,
            discovery_query_cooldown_ms: inner_defaults.discovery_query_cooldown_ms,
            enable_autonat: inner_defaults.enable_autonat,
            peer_manager_policy: inner_defaults.peer_manager_policy,
            allow_local_handler_fallback_when_no_peers: false,
            protocol_retry_cooldown_after: Duration::from_millis(PROTOCOL_RETRY_COOLDOWN_AFTER_MS),
            request_timeout: Duration::from_millis(REQUEST_TO_PEER_TIMEOUT_MS),
            request_retry_budget: Duration::from_millis(REQUEST_RETRY_BUDGET_MS),
            fetch_blob_request_timeout: Duration::from_millis(
                FETCH_BLOB_REQUEST_TO_PEER_TIMEOUT_MS,
            ),
            fetch_blob_request_retry_budget: Duration::from_millis(
                FETCH_BLOB_REQUEST_RETRY_BUDGET_MS,
            ),
        }
    }
}

#[derive(Clone)]
pub struct Libp2pReplicationNetwork {
    inner: Libp2pNetwork,
    allow_local_handler_fallback_when_no_peers: bool,
    bootstrap_addrs_by_peer_id: HashMap<PeerId, Multiaddr>,
    handlers: Arc<Mutex<HashMap<String, Handler>>>,
    request_peer_cursor: Arc<AtomicUsize>,
    protocol_retry_cooldown_after: Duration,
    request_timeout: Duration,
    request_retry_budget: Duration,
    fetch_blob_request_timeout: Duration,
    fetch_blob_request_retry_budget: Duration,
    protocol_retry_cooldown_peers: Arc<Mutex<HashMap<String, HashMap<PeerId, Instant>>>>,
    transport_retry_cooldown_peers: Arc<Mutex<HashMap<PeerId, Instant>>>,
    request_peer_scores: Arc<Mutex<HashMap<PeerId, RequestPeerScore>>>,
}

impl Libp2pReplicationNetwork {
    pub fn new(config: Libp2pReplicationNetworkConfig) -> Self {
        Self::try_new(config).expect("libp2p replication network configuration must be valid")
    }

    /// Constructs a replication network and reports synchronous local setup
    /// failures, including publisher and receiver persistence setup.
    pub fn try_new(config: Libp2pReplicationNetworkConfig) -> Result<Self, WorldError> {
        let bootstrap_addrs_by_peer_id =
            bootstrap_peer_addrs_by_peer_id(config.bootstrap_peers.as_slice());
        let request_response_timeout = request_response_timeout_for_config(&config);
        let inner = Libp2pNetwork::try_new(Libp2pNetworkConfig {
            keypair: config.keypair,
            peer_record: config.peer_record,
            peer_record_publication_directory: config.peer_record_publication_directory,
            peer_record_receiver: config.peer_record_receiver,
            listen_addrs: config.listen_addrs,
            bootstrap_peers: config.bootstrap_peers,
            bootstrap_redial_interval_ms: config.bootstrap_redial_interval_ms,
            republish_interval_ms: config.republish_interval_ms,
            discovery_query_interval_ms: config.discovery_query_interval_ms,
            discovery_query_cooldown_ms: config.discovery_query_cooldown_ms,
            enable_autonat: config.enable_autonat,
            peer_manager_policy: config.peer_manager_policy,
            request_response_timeout,
            ..Libp2pNetworkConfig::default()
        })?;

        Ok(Self {
            inner,
            allow_local_handler_fallback_when_no_peers: config
                .allow_local_handler_fallback_when_no_peers,
            bootstrap_addrs_by_peer_id,
            handlers: Arc::new(Mutex::new(HashMap::new())),
            request_peer_cursor: Arc::new(AtomicUsize::new(0)),
            protocol_retry_cooldown_after: config.protocol_retry_cooldown_after,
            request_timeout: config.request_timeout,
            request_retry_budget: config.request_retry_budget,
            fetch_blob_request_timeout: config.fetch_blob_request_timeout,
            fetch_blob_request_retry_budget: config.fetch_blob_request_retry_budget,
            protocol_retry_cooldown_peers: Arc::new(Mutex::new(HashMap::new())),
            transport_retry_cooldown_peers: Arc::new(Mutex::new(HashMap::new())),
            request_peer_scores: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Initializes the durable V2 publisher state without starting a network.
    /// This is a one-time bootstrap operation; ordinary construction only opens
    /// existing state and never calls this implicitly.
    pub fn initialize_peer_record_publication(
        config: &Libp2pReplicationNetworkConfig,
    ) -> Result<(), WorldError> {
        let publication_directory = config
            .peer_record_publication_directory
            .as_ref()
            .ok_or_else(|| WorldError::NetworkProtocolUnavailable {
                protocol: "durable peer publication initialization requires a configured directory"
                    .into(),
            })?;
        let keypair =
            config
                .keypair
                .as_ref()
                .ok_or_else(|| WorldError::NetworkProtocolUnavailable {
                    protocol: "durable peer publication initialization requires a stable keypair"
                        .into(),
                })?;
        let peer_record =
            config
                .peer_record
                .as_ref()
                .ok_or_else(|| WorldError::NetworkProtocolUnavailable {
                    protocol:
                        "durable peer publication initialization requires a peer record template"
                            .into(),
                })?;

        Libp2pNetwork::initialize_peer_record_publication(&Libp2pNetworkConfig {
            keypair: Some(keypair.clone()),
            peer_record: Some(peer_record.clone()),
            peer_record_publication_directory: Some(publication_directory.clone()),
            ..Libp2pNetworkConfig::default()
        })
    }

    /// Creates the durable receiver floor without starting a network.
    /// Ordinary construction only opens the existing receiver state.
    pub fn initialize_peer_record_receiver(
        config: &Libp2pReplicationNetworkConfig,
    ) -> Result<(), WorldError> {
        Libp2pNetwork::initialize_peer_record_receiver(&Libp2pNetworkConfig {
            keypair: config.keypair.clone(),
            peer_record: config.peer_record.clone(),
            peer_record_receiver: config.peer_record_receiver.clone(),
            ..Libp2pNetworkConfig::default()
        })
    }

    pub fn peer_id(&self) -> PeerId {
        self.inner.peer_id()
    }

    #[cfg(test)]
    pub fn listening_addrs(&self) -> Vec<Multiaddr> {
        self.inner.listening_addrs()
    }

    pub fn connected_peers(&self) -> Vec<PeerId> {
        self.inner.connected_peers()
    }

    pub fn debug_errors(&self) -> Vec<String> {
        self.inner.debug_errors()
    }

    pub fn debug_snapshot(&self) -> ReplicationNetworkDebugSnapshot {
        let local_peer_id = self.peer_id().to_string();
        let mut connected_peers: Vec<String> = self
            .inner
            .connected_peers()
            .into_iter()
            .map(|peer_id| peer_id.to_string())
            .collect();
        connected_peers.sort();
        connected_peers.dedup();

        let mut peer_healths: Vec<ReplicationPeerHealthDebug> = self
            .inner
            .debug_peer_healths()
            .into_iter()
            .map(|health| {
                let mut discovery_sources: Vec<String> = health
                    .discovery_sources
                    .into_iter()
                    .map(replication_discovery_source_label)
                    .collect();
                discovery_sources.sort();
                discovery_sources.dedup();
                ReplicationPeerHealthDebug {
                    peer_id: health.peer_id,
                    status: replication_peer_health_status_label(health.status),
                    issues: health
                        .issues
                        .into_iter()
                        .map(replication_peer_health_issue_label)
                        .collect(),
                    discovery_sources,
                    active_path_kind: health.active_path_kind,
                    source_operator: health.source_operator,
                    source_asn: health.source_asn,
                }
            })
            .collect();
        peer_healths.sort_by(|left, right| left.peer_id.cmp(&right.peer_id));

        let mut protocol_retry_cooldown_peers: HashMap<String, Vec<String>> = self
            .protocol_retry_cooldown_peers
            .lock()
            .expect("lock protocol retry cooldown peers")
            .iter()
            .map(|(protocol, peers)| {
                let mut peer_ids: Vec<String> = peers.keys().map(PeerId::to_string).collect();
                peer_ids.sort();
                peer_ids.dedup();
                (protocol.clone(), peer_ids)
            })
            .collect();
        for peer_ids in protocol_retry_cooldown_peers.values_mut() {
            peer_ids.sort();
            peer_ids.dedup();
        }

        let mut transport_retry_cooldown_peers: Vec<String> = self
            .transport_retry_cooldown_peers
            .lock()
            .expect("lock transport retry cooldown peers")
            .keys()
            .map(PeerId::to_string)
            .collect();
        transport_retry_cooldown_peers.sort();
        transport_retry_cooldown_peers.dedup();

        let mut recent_errors = self.inner.debug_errors();
        recent_errors.sort();
        recent_errors.dedup();
        let mut scores = self
            .request_peer_scores
            .lock()
            .expect("lock request peer scores");
        let now = Instant::now();
        for entry in scores.values_mut() {
            recover_stale_request_peer_score(entry, now);
        }
        let request_peer_scores: HashMap<String, u8> = scores
            .iter()
            .map(|(peer, score)| (peer.to_string(), score.score))
            .collect();

        let mut registered_protocols: Vec<String> = self
            .handlers
            .lock()
            .expect("lock libp2p replication handlers")
            .keys()
            .cloned()
            .collect();
        registered_protocols.sort();
        registered_protocols.dedup();

        ReplicationNetworkDebugSnapshot {
            local_peer_id,
            connected_peers,
            peer_healths,
            registered_protocols,
            protocol_retry_cooldown_peers,
            transport_retry_cooldown_peers,
            request_peer_scores,
            recent_errors,
        }
    }

    pub fn reachability_snapshot(&self) -> Libp2pReachabilitySnapshot {
        self.inner.reachability_snapshot()
    }

    pub fn traffic_metrics_snapshot(&self) -> Libp2pTrafficMetricsSnapshot {
        self.inner.traffic_metrics_snapshot()
    }

    fn local_handler(&self, protocol: &str) -> Option<Handler> {
        self.handlers
            .lock()
            .expect("lock libp2p replication handlers")
            .get(protocol)
            .cloned()
    }

    fn request_via_peer(
        &self,
        protocol: &str,
        payload: &[u8],
        peer: PeerId,
        timeout: Duration,
    ) -> Result<Vec<u8>, WorldError> {
        let response = self
            .inner
            .request_to_peer_with_timeout(protocol, payload, peer, timeout)
            .map_err(|err| WorldError::NetworkProtocolUnavailable {
                protocol: format!("libp2p-replication outbound request failed: {err:?}"),
            })?;

        if let Some(remote_error) = decode_error_response(response.as_slice()) {
            return Err(WorldError::NetworkRequestFailed {
                code: remote_error.code,
                message: remote_error.message,
                retryable: remote_error.retryable,
            });
        }

        Ok(response)
    }

    fn filtered_request_peers(&self, protocol: &str, ordered_peers: Vec<PeerId>) -> Vec<PeerId> {
        let now = Instant::now();
        let mut transport_retry_cooldown_peers = self
            .transport_retry_cooldown_peers
            .lock()
            .expect("lock transport retry cooldown peers");
        transport_retry_cooldown_peers.retain(|_, cooldown_until| *cooldown_until > now);
        let mut protocol_retry_cooldown_peers = self
            .protocol_retry_cooldown_peers
            .lock()
            .expect("lock protocol retry cooldown peers");
        let protocol_cooldown_peers = protocol_retry_cooldown_peers
            .get_mut(protocol)
            .map(|peers| {
                peers.retain(|_, cooldown_until| *cooldown_until > now);
                peers.clone()
            })
            .unwrap_or_default();
        let keep_protocol_entry = !protocol_cooldown_peers.is_empty();
        let mut deferred_protocol_peers = Vec::new();
        let mut deferred_transport_peers = Vec::new();
        let mut scored: Vec<(PeerId, u8, usize)> = ordered_peers
            .into_iter()
            .enumerate()
            .filter_map(|(index, peer)| {
                let score = self.request_peer_score(peer);
                if protocol_cooldown_peers.contains_key(&peer) {
                    deferred_protocol_peers.push((peer, score, index));
                    return None;
                }
                if transport_retry_cooldown_peers.contains_key(&peer) {
                    deferred_transport_peers.push((peer, score, index));
                    return None;
                }
                Some((peer, score, index))
            })
            .collect();
        if scored.is_empty() && !deferred_transport_peers.is_empty() {
            scored = deferred_transport_peers;
        }
        if scored.is_empty() && !deferred_protocol_peers.is_empty() {
            scored = deferred_protocol_peers;
        }
        if scored.iter().any(|(_, score, _)| *score > 0) {
            scored.retain(|(_, score, _)| *score > 0);
        }
        scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.2.cmp(&right.2)));
        let filtered = scored
            .into_iter()
            .map(|(peer, _, _)| peer)
            .collect::<Vec<_>>();
        if !keep_protocol_entry {
            protocol_retry_cooldown_peers.remove(protocol);
        }
        filtered
    }

    fn request_peer_score(&self, peer: PeerId) -> u8 {
        let mut scores = self
            .request_peer_scores
            .lock()
            .expect("lock request peer scores");
        let Some(entry) = scores.get_mut(&peer) else {
            return REQUEST_PEER_DEFAULT_SCORE;
        };
        recover_stale_request_peer_score(entry, Instant::now());
        entry.score
    }

    fn adjust_request_peer_score(&self, peer: PeerId, delta: i16) {
        let mut scores = self
            .request_peer_scores
            .lock()
            .expect("lock request peer scores");
        let entry = scores.entry(peer).or_insert(RequestPeerScore {
            score: REQUEST_PEER_DEFAULT_SCORE,
            updated_at: Instant::now(),
        });
        let updated = (entry.score as i16 + delta).clamp(0, REQUEST_PEER_DEFAULT_SCORE as i16);
        entry.score = updated as u8;
        entry.updated_at = Instant::now();
    }

    fn mark_peer_request_success(&self, peer: PeerId) {
        self.adjust_request_peer_score(peer, REQUEST_PEER_SUCCESS_RECOVERY as i16);
    }

    fn mark_peer_request_failure(&self, protocol: &str, peer: PeerId, err: &WorldError) {
        let penalty = if peer_error_indicates_transport_retry_cooldown(err) {
            REQUEST_PEER_TRANSPORT_PENALTY
        } else if peer_error_indicates_protocol_retry_cooldown(protocol, err) {
            REQUEST_PEER_PROTOCOL_PENALTY
        } else {
            REQUEST_PEER_GENERIC_PENALTY
        };
        self.adjust_request_peer_score(peer, -(penalty as i16));
    }

    fn mark_peer_for_protocol_retry_cooldown(&self, protocol: &str, peer: PeerId) {
        let cooldown_until = Instant::now() + self.protocol_retry_cooldown_after;
        self.protocol_retry_cooldown_peers
            .lock()
            .expect("lock protocol retry cooldown peers")
            .entry(protocol.to_string())
            .or_default()
            .insert(peer, cooldown_until);
    }

    fn mark_peer_for_transport_retry_cooldown(&self, peer: PeerId) {
        let cooldown_until = Instant::now() + self.protocol_retry_cooldown_after;
        self.transport_retry_cooldown_peers
            .lock()
            .expect("lock transport retry cooldown peers")
            .insert(peer, cooldown_until);
    }

    fn call_local_handler(&self, protocol: &str, payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        match self.local_handler(protocol) {
            Some(handler) => handler(payload),
            None => Err(WorldError::NetworkProtocolUnavailable {
                protocol: format!("libp2p-replication handler missing: {protocol}"),
            }),
        }
    }

    fn wait_for_connected_peers(&self) -> Vec<PeerId> {
        let mut peers = self.connected_peers_sorted();
        for _ in 0..REQUEST_CONNECTED_PEER_WAIT_RETRIES {
            if !peers.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(
                REQUEST_CONNECTED_PEER_WAIT_INTERVAL_MS,
            ));
            peers = self.connected_peers_sorted();
        }
        peers
    }

    fn connected_peers_sorted(&self) -> Vec<PeerId> {
        self.inner.admissible_request_peers()
    }

    fn collect_connected_provider_peers(&self, providers: &[String]) -> Vec<PeerId> {
        let connected_peers: HashSet<PeerId> = self.connected_peers_sorted().into_iter().collect();
        let mut ordered_provider_peers = Vec::new();
        let mut seen = HashSet::new();
        for provider in providers {
            let Ok(peer_id) = provider.parse::<PeerId>() else {
                continue;
            };
            if !connected_peers.contains(&peer_id) || !seen.insert(peer_id) {
                continue;
            }
            ordered_provider_peers.push(peer_id);
        }
        ordered_provider_peers
    }

    fn dial_bootstrap_provider_peers(&self, providers: &[String]) {
        let connected_peers: HashSet<PeerId> = self.connected_peers_sorted().into_iter().collect();
        for provider in providers {
            let Ok(peer_id) = provider.parse::<PeerId>() else {
                continue;
            };
            if connected_peers.contains(&peer_id) {
                continue;
            }
            let Some(addr) = self.bootstrap_addrs_by_peer_id.get(&peer_id) else {
                continue;
            };
            let _ = self.inner.dial(addr.clone());
        }
    }

    fn wait_for_connected_provider_peers(&self, providers: &[String]) -> Vec<PeerId> {
        self.wait_for_connected_provider_peers_within(
            providers,
            Duration::from_millis(
                REQUEST_PROVIDER_CONNECTED_PEER_WAIT_RETRIES as u64
                    * REQUEST_CONNECTED_PEER_WAIT_INTERVAL_MS,
            ),
        )
    }

    fn wait_for_connected_provider_peers_within(
        &self,
        providers: &[String],
        max_wait: Duration,
    ) -> Vec<PeerId> {
        let started_at = Instant::now();
        loop {
            self.dial_bootstrap_provider_peers(providers);
            let ordered_provider_peers = self.collect_connected_provider_peers(providers);
            if !ordered_provider_peers.is_empty() || started_at.elapsed() >= max_wait {
                return ordered_provider_peers;
            }
            let remaining_wait = max_wait.saturating_sub(started_at.elapsed());
            if remaining_wait.is_zero() {
                return Vec::new();
            }
            std::thread::sleep(
                Duration::from_millis(REQUEST_CONNECTED_PEER_WAIT_INTERVAL_MS).min(remaining_wait),
            );
        }
    }

    fn request_over_refreshed_peers<F, G>(
        &self,
        protocol: &str,
        payload: &[u8],
        initial_peers: Vec<PeerId>,
        refresh_peers: F,
        no_connected_error: G,
    ) -> Result<Vec<u8>, WorldError>
    where
        F: FnMut() -> Vec<PeerId>,
        G: Fn() -> WorldError,
    {
        self.request_over_refreshed_peers_with_budget(
            protocol,
            payload,
            initial_peers,
            refresh_peers,
            no_connected_error,
            self.request_timeout_for_protocol(protocol),
            self.request_retry_budget_for_protocol(protocol),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable replication request seam keeps protocol, peer refresh, timeout, and retry budget controls explicit"
    )]
    fn request_over_refreshed_peers_with_budget<F, G>(
        &self,
        protocol: &str,
        payload: &[u8],
        initial_peers: Vec<PeerId>,
        mut refresh_peers: F,
        no_connected_error: G,
        request_timeout: Duration,
        request_retry_budget: Duration,
    ) -> Result<Vec<u8>, WorldError>
    where
        F: FnMut() -> Vec<PeerId>,
        G: Fn() -> WorldError,
    {
        let cursor = self.request_peer_cursor.fetch_add(1, Ordering::Relaxed);
        let started_at = Instant::now();
        let mut last_error = None;
        let mut attempted_peers = HashSet::new();

        for attempt in 0..=REQUEST_CONNECTION_REFRESH_RETRIES {
            if started_at.elapsed() >= request_retry_budget {
                return Err(replication_request_budget_exhausted(
                    protocol,
                    request_retry_budget,
                    last_error,
                ));
            }
            let candidate_source = if attempt == 0 {
                initial_peers.clone()
            } else {
                refresh_peers()
            };
            let ordered_peers = self.filtered_request_peers(
                protocol,
                rotated_peers(candidate_source.as_slice(), cursor.saturating_add(attempt)),
            );
            if ordered_peers.is_empty() {
                let should_retry = attempt < REQUEST_CONNECTION_REFRESH_RETRIES
                    && last_error
                        .as_ref()
                        .map(peer_error_indicates_retryable_connection_gap)
                        .unwrap_or(false);
                if should_retry {
                    std::thread::sleep(Duration::from_millis(
                        REQUEST_CONNECTED_PEER_WAIT_INTERVAL_MS,
                    ));
                    continue;
                }
                return Err(last_error.unwrap_or_else(&no_connected_error));
            }

            let mut retryable_connection_gap = false;
            for peer in ordered_peers {
                attempted_peers.insert(peer);
                let elapsed = started_at.elapsed();
                if elapsed >= request_retry_budget {
                    return Err(replication_request_budget_exhausted(
                        protocol,
                        request_retry_budget,
                        last_error,
                    ));
                }
                let remaining_budget = request_retry_budget.saturating_sub(elapsed);
                let peer_request_timeout = request_timeout.min(remaining_budget);
                match self.request_via_peer(protocol, payload, peer, peer_request_timeout) {
                    Ok(reply) => {
                        self.mark_peer_request_success(peer);
                        return Ok(reply);
                    }
                    Err(err) => {
                        self.mark_peer_request_failure(protocol, peer, &err);
                        if peer_error_indicates_transport_retry_cooldown(&err) {
                            self.mark_peer_for_transport_retry_cooldown(peer);
                        }
                        if peer_error_indicates_protocol_retry_cooldown(protocol, &err) {
                            self.mark_peer_for_protocol_retry_cooldown(protocol, peer);
                        }
                        retryable_connection_gap |=
                            peer_error_indicates_retryable_connection_gap(&err);
                        last_error = Some(err);
                        if started_at.elapsed() >= request_retry_budget {
                            return Err(replication_request_budget_exhausted(
                                protocol,
                                request_retry_budget,
                                last_error,
                            ));
                        }
                    }
                }
            }

            let refreshed_has_untried_peer = attempt < REQUEST_CONNECTION_REFRESH_RETRIES
                && self
                    .filtered_request_peers(
                        protocol,
                        rotated_peers(
                            refresh_peers().as_slice(),
                            cursor.saturating_add(attempt + 1),
                        ),
                    )
                    .into_iter()
                    .any(|peer| !attempted_peers.contains(&peer));
            if refreshed_has_untried_peer {
                continue;
            }

            if retryable_connection_gap && attempt < REQUEST_CONNECTION_REFRESH_RETRIES {
                std::thread::sleep(Duration::from_millis(
                    REQUEST_CONNECTED_PEER_WAIT_INTERVAL_MS,
                ));
                continue;
            }

            return Err(last_error.unwrap_or_else(&no_connected_error));
        }

        Err(last_error.unwrap_or_else(no_connected_error))
    }

    fn request_timeout_for_protocol(&self, protocol: &str) -> Duration {
        if protocol == REPLICATION_FETCH_COMMIT_PROTOCOL {
            return Duration::from_millis(FETCH_COMMIT_REQUEST_TO_PEER_TIMEOUT_MS);
        }
        if protocol.contains(REPLICATION_FETCH_BLOB_PROTOCOL_MARKER) {
            return self.fetch_blob_request_timeout;
        }
        self.request_timeout
    }

    fn request_retry_budget_for_protocol(&self, protocol: &str) -> Duration {
        if protocol == REPLICATION_FETCH_COMMIT_PROTOCOL {
            return Duration::from_millis(FETCH_COMMIT_REQUEST_RETRY_BUDGET_MS);
        }
        if protocol.contains(REPLICATION_FETCH_BLOB_PROTOCOL_MARKER) {
            return self.fetch_blob_request_retry_budget;
        }
        self.request_retry_budget
    }
}

fn recover_stale_request_peer_score(entry: &mut RequestPeerScore, now: Instant) {
    if entry.score < REQUEST_PEER_DEFAULT_SCORE
        && now.duration_since(entry.updated_at)
            >= Duration::from_millis(REQUEST_PEER_SCORE_RECOVERY_AFTER_MS)
    {
        entry.score = REQUEST_PEER_DEFAULT_SCORE;
        entry.updated_at = now;
    }
}

fn request_response_timeout_for_config(config: &Libp2pReplicationNetworkConfig) -> Duration {
    config
        .request_retry_budget
        .max(config.fetch_blob_request_retry_budget)
        .max(Duration::from_millis(FETCH_COMMIT_REQUEST_RETRY_BUDGET_MS))
}

fn bootstrap_peer_addrs_by_peer_id(addrs: &[Multiaddr]) -> HashMap<PeerId, Multiaddr> {
    let mut by_peer_id = HashMap::new();
    for addr in addrs {
        let addr_text = addr.to_string();
        let Some((_, peer_id_raw)) = addr_text.rsplit_once("/p2p/") else {
            continue;
        };
        let Ok(peer_id) = peer_id_raw.parse::<PeerId>() else {
            continue;
        };
        by_peer_id.entry(peer_id).or_insert_with(|| addr.clone());
    }
    by_peer_id
}

pub fn derive_libp2p_identity_keypair(private_key_hex: &str) -> Result<Keypair, NodeError> {
    let private_key_bytes = hex::decode(private_key_hex).map_err(|_| NodeError::InvalidConfig {
        reason: "node.private_key must be valid hex for libp2p identity derivation".to_string(),
    })?;
    Keypair::ed25519_from_bytes(private_key_bytes).map_err(|err| NodeError::InvalidConfig {
        reason: format!("failed to derive libp2p identity keypair: {err}"),
    })
}

impl DistributedDht<WorldError> for Libp2pReplicationNetwork {
    fn publish_provider(
        &self,
        world_id: &str,
        content_hash: &str,
        provider_id: &str,
    ) -> Result<(), WorldError> {
        self.inner
            .publish_provider(world_id, content_hash, provider_id)
    }

    fn get_providers(
        &self,
        world_id: &str,
        content_hash: &str,
    ) -> Result<Vec<ProviderRecord>, WorldError> {
        self.inner.get_providers(world_id, content_hash)
    }

    fn put_world_head(&self, world_id: &str, head: &WorldHeadAnnounce) -> Result<(), WorldError> {
        self.inner.put_world_head(world_id, head)
    }

    fn get_world_head(&self, world_id: &str) -> Result<Option<WorldHeadAnnounce>, WorldError> {
        self.inner.get_world_head(world_id)
    }

    fn put_membership_directory(
        &self,
        world_id: &str,
        snapshot: &MembershipDirectorySnapshot,
    ) -> Result<(), WorldError> {
        self.inner.put_membership_directory(world_id, snapshot)
    }

    fn get_membership_directory(
        &self,
        world_id: &str,
    ) -> Result<Option<MembershipDirectorySnapshot>, WorldError> {
        self.inner.get_membership_directory(world_id)
    }

    fn put_peer_record(&self, world_id: &str, record: &SignedPeerRecord) -> Result<(), WorldError> {
        self.inner.put_peer_record(world_id, record)
    }

    fn get_peer_record(
        &self,
        world_id: &str,
        peer_id: &str,
    ) -> Result<Option<SignedPeerRecord>, WorldError> {
        self.inner.get_peer_record(world_id, peer_id)
    }
}

fn decode_error_response(payload: &[u8]) -> Option<ErrorResponse> {
    serde_cbor::from_slice(payload).ok()
}

fn replication_discovery_source_label(source: PeerDiscoverySource) -> String {
    match source {
        PeerDiscoverySource::StaticBootstrap => "static_bootstrap".to_string(),
        PeerDiscoverySource::Dht => "dht".to_string(),
        PeerDiscoverySource::Rendezvous => "rendezvous".to_string(),
        PeerDiscoverySource::PeerExchange => "peer_exchange".to_string(),
        PeerDiscoverySource::Manual => "manual".to_string(),
    }
}

fn replication_peer_health_status_label(status: oasis7_net::PeerManagerHealthStatus) -> String {
    match status {
        oasis7_net::PeerManagerHealthStatus::Active => "active".to_string(),
        oasis7_net::PeerManagerHealthStatus::Candidate => "candidate".to_string(),
        oasis7_net::PeerManagerHealthStatus::Suspect => "suspect".to_string(),
        oasis7_net::PeerManagerHealthStatus::Blocked => "blocked".to_string(),
    }
}

fn replication_peer_health_issue_label(issue: oasis7_net::PeerManagerHealthIssue) -> String {
    match issue {
        oasis7_net::PeerManagerHealthIssue::DiscoveryIndependenceUnverified => {
            "discovery_independence_unverified".to_string()
        }
        oasis7_net::PeerManagerHealthIssue::MissingPeerRecord => "missing_peer_record".to_string(),
        oasis7_net::PeerManagerHealthIssue::SingleSourceDiscovery {
            observed_sources,
            required_sources,
        } => format!(
            "single_source_discovery observed={observed_sources} required={required_sources}"
        ),
        oasis7_net::PeerManagerHealthIssue::InsufficientActiveDiscoverySources {
            observed_sources,
            required_sources,
        } => format!(
            "insufficient_active_discovery_sources observed={observed_sources} required={required_sources}"
        ),
        oasis7_net::PeerManagerHealthIssue::Ipv4SubnetConcentration {
            subnet,
            peers_in_bucket,
            active_peer_count,
            limit_per_mille,
        } => format!(
            "ipv4_subnet_concentration subnet={subnet} peers_in_bucket={peers_in_bucket} active_peer_count={active_peer_count} limit_per_mille={limit_per_mille}"
        ),
        oasis7_net::PeerManagerHealthIssue::Ipv4SubnetActivePeerLimit {
            subnet,
            peers_in_bucket,
            active_peer_count,
            max_active_peers,
        } => format!(
            "ipv4_subnet_active_peer_limit subnet={subnet} peers_in_bucket={peers_in_bucket} active_peer_count={active_peer_count} max_active_peers={max_active_peers}"
        ),
        oasis7_net::PeerManagerHealthIssue::RelayDomainConcentration {
            relay_domain,
            peers_in_bucket,
            active_peer_count,
            limit_per_mille,
        } => format!(
            "relay_domain_concentration relay_domain={relay_domain} peers_in_bucket={peers_in_bucket} active_peer_count={active_peer_count} limit_per_mille={limit_per_mille}"
        ),
        oasis7_net::PeerManagerHealthIssue::OperatorConcentration {
            source_operator,
            peers_in_bucket,
            active_peer_count,
            limit_per_mille,
        } => format!(
            "operator_concentration source_operator={source_operator} peers_in_bucket={peers_in_bucket} active_peer_count={active_peer_count} limit_per_mille={limit_per_mille}"
        ),
        oasis7_net::PeerManagerHealthIssue::AsnConcentration {
            source_asn,
            peers_in_bucket,
            active_peer_count,
            limit_per_mille,
        } => format!(
            "asn_concentration source_asn={source_asn} peers_in_bucket={peers_in_bucket} active_peer_count={active_peer_count} limit_per_mille={limit_per_mille}"
        ),
        oasis7_net::PeerManagerHealthIssue::RelayBudgetExceeded {
            relayed_active_peers,
            active_peer_count,
            limit_per_mille,
        } => format!(
            "relay_budget_exceeded relayed_active_peers={relayed_active_peers} active_peer_count={active_peer_count} limit_per_mille={limit_per_mille}"
        ),
    }
}

fn peer_error_indicates_unsupported_protocol(err: &WorldError) -> bool {
    world_error_is_missing_handler(err) && !world_error_is_retryable_connection_gap(err)
}

fn peer_error_indicates_protocol_retry_cooldown(protocol: &str, err: &WorldError) -> bool {
    peer_error_indicates_unsupported_protocol(err)
        || peer_error_indicates_fetch_commit_not_found_retry_cooldown(protocol, err)
}

fn peer_error_indicates_transport_retry_cooldown(err: &WorldError) -> bool {
    peer_error_indicates_retryable_connection_gap(err)
}

fn peer_error_indicates_fetch_commit_not_found_retry_cooldown(
    protocol: &str,
    err: &WorldError,
) -> bool {
    if protocol != REPLICATION_FETCH_COMMIT_PROTOCOL {
        return false;
    }

    match err {
        WorldError::NetworkRequestFailed { code, .. } => {
            matches!(code, DistributedErrorCode::ErrNotFound)
        }
        _ => false,
    }
}

fn peer_error_indicates_retryable_connection_gap(err: &WorldError) -> bool {
    world_error_is_retryable_connection_gap(err)
}

fn replication_request_budget_exhausted(
    protocol: &str,
    budget: Duration,
    last_error: Option<WorldError>,
) -> WorldError {
    WorldError::NetworkProtocolUnavailable {
        protocol: format!(
            "libp2p-replication request budget exhausted protocol={} budget_ms={} last_error={:?}",
            protocol,
            budget.as_millis(),
            last_error
        ),
    }
}

fn rotated_peers(peers: &[PeerId], cursor: usize) -> Vec<PeerId> {
    if peers.is_empty() {
        return Vec::new();
    }
    let start = cursor % peers.len();
    peers[start..]
        .iter()
        .chain(peers[..start].iter())
        .copied()
        .collect()
}

#[cfg(test)]
fn dedup_sorted_peers(mut peers: Vec<PeerId>) -> Vec<PeerId> {
    peers.sort_by_key(|peer| peer.to_string());
    peers.dedup();
    peers
}

#[cfg(test)]
fn blocked_peers_from_healths(healths: &[PeerManagerPeerHealth]) -> HashSet<PeerId> {
    healths
        .iter()
        .filter(|health| peer_is_request_blocked(health))
        .filter_map(|health| health.peer_id.parse::<PeerId>().ok())
        .collect()
}

#[cfg(test)]
fn soft_deprioritized_peers_from_healths(healths: &[PeerManagerPeerHealth]) -> HashSet<PeerId> {
    healths
        .iter()
        .filter(|health| peer_is_soft_deprioritized_for_requests(health))
        .filter_map(|health| health.peer_id.parse::<PeerId>().ok())
        .collect()
}

#[cfg(test)]
fn peer_is_request_blocked(health: &PeerManagerPeerHealth) -> bool {
    matches!(health.status, PeerManagerHealthStatus::Blocked)
        && !health.issues.is_empty()
        && !health.issues.iter().all(issue_is_soft_bootstrap_constraint)
}

#[cfg(test)]
fn peer_is_soft_deprioritized_for_requests(health: &PeerManagerPeerHealth) -> bool {
    matches!(health.status, PeerManagerHealthStatus::Blocked)
        && !health.issues.is_empty()
        && health.issues.iter().all(issue_is_soft_bootstrap_constraint)
        && health
            .issues
            .iter()
            .any(|issue| matches!(issue, PeerManagerHealthIssue::MissingPeerRecord))
}

#[cfg(test)]
fn issue_is_soft_bootstrap_constraint(issue: &PeerManagerHealthIssue) -> bool {
    matches!(
        issue,
        PeerManagerHealthIssue::MissingPeerRecord
            | PeerManagerHealthIssue::DiscoveryIndependenceUnverified
            | PeerManagerHealthIssue::InsufficientActiveDiscoverySources { .. }
            | PeerManagerHealthIssue::SingleSourceDiscovery { .. }
    )
}

#[cfg(test)]
fn request_candidate_peers(peers: Vec<PeerId>, healths: &[PeerManagerPeerHealth]) -> Vec<PeerId> {
    let blocked_peers = blocked_peers_from_healths(healths);
    let soft_deprioritized_peers = soft_deprioritized_peers_from_healths(healths);
    let preferred = peers
        .iter()
        .copied()
        .filter(|peer_id| {
            !blocked_peers.contains(peer_id) && !soft_deprioritized_peers.contains(peer_id)
        })
        .collect::<Vec<_>>();
    if !preferred.is_empty() {
        return preferred;
    }
    peers
        .into_iter()
        .filter(|peer_id| !blocked_peers.contains(peer_id))
        .collect()
}

#[cfg(test)]
fn active_transport_peers_from_healths(healths: &[PeerManagerPeerHealth]) -> Vec<PeerId> {
    let peers = healths
        .iter()
        .filter(|health| health.active_path_kind.is_some())
        .filter_map(|health| health.peer_id.parse::<PeerId>().ok())
        .collect();
    let peers = dedup_sorted_peers(peers);
    request_candidate_peers(peers, healths)
}

#[cfg(test)]
fn connected_or_active_transport_peers(
    connected_peers: Vec<PeerId>,
    healths: &[PeerManagerPeerHealth],
) -> Vec<PeerId> {
    let connected_peers = dedup_sorted_peers(connected_peers);
    let admissible_connected_peers = request_candidate_peers(connected_peers.clone(), healths);
    if !admissible_connected_peers.is_empty() {
        return admissible_connected_peers;
    }
    if !connected_peers.is_empty() {
        return Vec::new();
    }
    active_transport_peers_from_healths(healths)
}

#[cfg(test)]
mod bootstrap_provider_tests;

#[cfg(test)]
mod peer_selection_tests;

#[cfg(test)]
mod protocol_budget_tests;

#[cfg(test)]
mod tests;
