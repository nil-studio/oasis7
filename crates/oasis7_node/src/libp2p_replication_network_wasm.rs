use std::time::Duration;

use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use oasis7_net::PeerManagerPolicy;
use oasis7_proto::distributed_dht::PeerRecord;
use oasis7_proto::distributed_net::{DistributedNetwork, NetworkSubscription};
use oasis7_proto::world_error::WorldError;

const PROTOCOL_RETRY_COOLDOWN_AFTER_MS: u64 = 5_000;
const REQUEST_TO_PEER_TIMEOUT_MS: u64 = 12_000;
const REQUEST_RETRY_BUDGET_MS: u64 = 20_000;
pub(crate) const FETCH_COMMIT_REQUEST_TO_PEER_TIMEOUT_MS: u64 = 30_000;
pub(crate) const FETCH_COMMIT_REQUEST_RETRY_BUDGET_MS: u64 = 120_000;
const FETCH_BLOB_REQUEST_TO_PEER_TIMEOUT_MS: u64 = 30_000;
const FETCH_BLOB_REQUEST_RETRY_BUDGET_MS: u64 = 180_000;

// wasm32 target intentionally does not ship a full-node networking stack.
// This stub exists only to keep API shape stable for compile-time compatibility.
#[derive(Debug, Clone)]
pub struct Libp2pReplicationNetworkConfig {
    pub keypair: Option<Keypair>,
    pub peer_record: Option<PeerRecord>,
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
        Self {
            keypair: None,
            peer_record: None,
            listen_addrs: Vec::new(),
            bootstrap_peers: Vec::new(),
            bootstrap_redial_interval_ms: 1_000,
            republish_interval_ms: 5 * 60 * 1_000,
            discovery_query_interval_ms: 15_000,
            discovery_query_cooldown_ms: 60_000,
            enable_autonat: true,
            peer_manager_policy: PeerManagerPolicy::default(),
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
    peer_id: PeerId,
}

impl Libp2pReplicationNetwork {
    pub fn new(config: Libp2pReplicationNetworkConfig) -> Self {
        let keypair = config.keypair.unwrap_or_else(Keypair::generate_ed25519);
        let peer_id = PeerId::from(keypair.public());
        Self { peer_id }
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }
}

pub fn derive_libp2p_identity_keypair(private_key_hex: &str) -> Result<Keypair, WorldError> {
    let private_key_bytes =
        hex::decode(private_key_hex).map_err(|_| WorldError::SignatureKeyInvalid)?;
    Keypair::ed25519_from_bytes(private_key_bytes).map_err(|_| WorldError::SignatureKeyInvalid)
}

fn unsupported(protocol: &str) -> WorldError {
    WorldError::NetworkProtocolUnavailable {
        protocol: format!("{protocol} (wasm32 unsupported)"),
    }
}

impl DistributedNetwork<WorldError> for Libp2pReplicationNetwork {
    fn publish(&self, _topic: &str, _payload: &[u8]) -> Result<(), WorldError> {
        Err(unsupported("libp2p-replication publish"))
    }

    fn subscribe(&self, _topic: &str) -> Result<NetworkSubscription, WorldError> {
        Err(unsupported("libp2p-replication subscribe"))
    }

    fn request(&self, _protocol: &str, _payload: &[u8]) -> Result<Vec<u8>, WorldError> {
        Err(unsupported("libp2p-replication request"))
    }

    fn register_handler(
        &self,
        _protocol: &str,
        _handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>,
    ) -> Result<(), WorldError> {
        Err(unsupported("libp2p-replication register_handler"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_stub_generates_peer_id() {
        let network = Libp2pReplicationNetwork::new(Libp2pReplicationNetworkConfig::default());
        assert!(!network.peer_id().to_string().is_empty());
    }
}
