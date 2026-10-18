use serde::{Deserialize, Serialize};

/// The canonical network tier used when a client has no explicit selection.
pub const DEFAULT_CHAIN_NETWORK_TIER: &str = "local_devnet";

/// A blank world id leaves world selection to the bootstrap/runtime layer.
pub const DEFAULT_CHAIN_WORLD_ID: &str = "";

/// Official bootstrap addresses currently used by the launcher defaults.
pub const DEFAULT_CHAIN_REPLICATION_BOOTSTRAP_PEERS: &[&str] = &[
    "/dns4/bootstrap1.oasis7.tech/tcp/5612",
    "/dns4/bootstrap2.oasis7.tech/tcp/5611",
];

/// Client-side bootstrap data. Peer strings remain opaque here; the runtime
/// owns multiaddr parsing and connection policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapConfig {
    #[serde(default = "default_network_tier")]
    pub network_tier: String,
    #[serde(default)]
    pub network_tier_manifest: String,
    #[serde(default = "default_world_id")]
    pub world_id: String,
    #[serde(default = "default_bootstrap_peers")]
    pub replication_bootstrap_peers: Vec<String>,
}

/// Alias retained as an explicit client-facing name for downstream callers.
pub type ChainBootstrapConfig = BootstrapConfig;
pub type ClientBootstrapConfig = BootstrapConfig;

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            network_tier: default_network_tier(),
            network_tier_manifest: String::new(),
            world_id: default_world_id(),
            replication_bootstrap_peers: default_bootstrap_peers(),
        }
    }
}

impl BootstrapConfig {
    /// Return a copy with surrounding whitespace removed from opaque fields.
    /// Empty entries are discarded, but values are not otherwise rewritten.
    pub fn normalized(&self) -> Self {
        Self {
            network_tier: self.network_tier.trim().to_string(),
            network_tier_manifest: self.network_tier_manifest.trim().to_string(),
            world_id: self.world_id.trim().to_string(),
            replication_bootstrap_peers: self
                .replication_bootstrap_peers
                .iter()
                .map(|peer| peer.trim())
                .filter(|peer| !peer.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        }
    }

    pub fn default_replication_bootstrap_peers() -> Vec<String> {
        default_bootstrap_peers()
    }
}

pub fn default_chain_replication_bootstrap_peers_csv() -> String {
    DEFAULT_CHAIN_REPLICATION_BOOTSTRAP_PEERS.join(",")
}

pub fn default_chain_replication_bootstrap_peers_vec() -> Vec<String> {
    default_bootstrap_peers()
}

fn default_network_tier() -> String {
    DEFAULT_CHAIN_NETWORK_TIER.to_string()
}

fn default_world_id() -> String {
    DEFAULT_CHAIN_WORLD_ID.to_string()
}

fn default_bootstrap_peers() -> Vec<String> {
    DEFAULT_CHAIN_REPLICATION_BOOTSTRAP_PEERS
        .iter()
        .map(|peer| (*peer).to_string())
        .collect()
}

/// Default identity of the built-in formal release world.
pub const FORMAL_RELEASE_DEFAULT_WORLD_ID: &str = "live-formal-release-default";
