use libp2p::Multiaddr;
use libp2p::identity::Keypair;
use std::time::Duration;

use oasis7_proto::distributed_dht::PeerRecord;

use super::PeerManagerPolicy;

const DEFAULT_COMMAND_BUFFER_CAPACITY: usize = 2048;
const DEFAULT_MAX_PUBLISHED_MESSAGES: usize = 4096;
const DEFAULT_MAX_ERROR_MESSAGES: usize = 4096;
const DEFAULT_MAX_LISTENING_ADDRS: usize = 128;
const DEFAULT_BOOTSTRAP_REDIAL_INTERVAL_MS: i64 = 1_000;
const DEFAULT_DISCOVERY_QUERY_INTERVAL_MS: i64 = 15_000;
const DEFAULT_DISCOVERY_QUERY_COOLDOWN_MS: i64 = 60_000;
const DEFAULT_REQUEST_RESPONSE_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone)]
pub struct Libp2pNetworkConfig {
    pub keypair: Option<Keypair>,
    pub peer_record: Option<PeerRecord>,
    pub enable_rendezvous: bool,
    pub enable_autonat: bool,
    pub allow_loopback_external_addrs_for_testing: bool,
    pub listen_addrs: Vec<Multiaddr>,
    pub bootstrap_peers: Vec<Multiaddr>,
    pub bootstrap_redial_interval_ms: i64,
    pub republish_interval_ms: i64,
    pub discovery_query_interval_ms: i64,
    pub discovery_query_cooldown_ms: i64,
    pub command_buffer_capacity: usize,
    pub max_published_messages: usize,
    pub max_error_messages: usize,
    pub max_listening_addrs: usize,
    pub peer_manager_policy: PeerManagerPolicy,
    pub request_response_timeout: Duration,
}

impl Libp2pNetworkConfig {
    pub(super) fn rendezvous_enabled(&self) -> bool {
        self.enable_rendezvous
            || self
                .peer_record
                .as_ref()
                .is_some_and(super::discovery::peer_record_enables_rendezvous)
    }
}

impl Default for Libp2pNetworkConfig {
    fn default() -> Self {
        Self {
            keypair: None,
            peer_record: None,
            enable_rendezvous: false,
            enable_autonat: true,
            allow_loopback_external_addrs_for_testing: false,
            listen_addrs: Vec::new(),
            bootstrap_peers: Vec::new(),
            bootstrap_redial_interval_ms: DEFAULT_BOOTSTRAP_REDIAL_INTERVAL_MS,
            republish_interval_ms: 5 * 60 * 1000,
            discovery_query_interval_ms: DEFAULT_DISCOVERY_QUERY_INTERVAL_MS,
            discovery_query_cooldown_ms: DEFAULT_DISCOVERY_QUERY_COOLDOWN_MS,
            command_buffer_capacity: DEFAULT_COMMAND_BUFFER_CAPACITY,
            max_published_messages: DEFAULT_MAX_PUBLISHED_MESSAGES,
            max_error_messages: DEFAULT_MAX_ERROR_MESSAGES,
            max_listening_addrs: DEFAULT_MAX_LISTENING_ADDRS,
            peer_manager_policy: PeerManagerPolicy::default(),
            request_response_timeout: Duration::from_millis(DEFAULT_REQUEST_RESPONSE_TIMEOUT_MS),
        }
    }
}
