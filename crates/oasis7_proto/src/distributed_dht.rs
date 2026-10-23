//! Distributed DHT adapter abstractions (provider/head indexing).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::distributed::WorldHeadAnnounce;
use crate::distributed_net::NetworkLane;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRecord {
    pub provider_id: String,
    pub last_seen_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_available_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_ratio_per_mille: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge_pass_ratio_per_mille: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_ratio_per_mille: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p50_read_latency_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipDirectorySnapshot {
    pub world_id: String,
    pub requester_id: String,
    pub requested_at_ms: i64,
    pub reason: Option<String>,
    pub validators: Vec<String>,
    pub quorum_threshold: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerReachabilityClass {
    Public,
    Hybrid,
    Private,
    RelayOnly,
    ValidatorHidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerDeploymentMode {
    Public,
    Hybrid,
    Private,
    RelayOnly,
    ValidatorHidden,
}

impl PeerDeploymentMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Hybrid => "hybrid",
            Self::Private => "private",
            Self::RelayOnly => "relay_only",
            Self::ValidatorHidden => "validator_hidden",
        }
    }

    pub fn initial_reachability_class(self) -> PeerReachabilityClass {
        match self {
            Self::Public => PeerReachabilityClass::Public,
            Self::Hybrid => PeerReachabilityClass::Hybrid,
            Self::Private => PeerReachabilityClass::Private,
            Self::RelayOnly => PeerReachabilityClass::RelayOnly,
            Self::ValidatorHidden => PeerReachabilityClass::ValidatorHidden,
        }
    }
}

impl fmt::Display for PeerDeploymentMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PeerDeploymentMode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "public" => Ok(Self::Public),
            "hybrid" => Ok(Self::Hybrid),
            "private" => Ok(Self::Private),
            "relay_only" => Ok(Self::RelayOnly),
            "validator_hidden" => Ok(Self::ValidatorHidden),
            _ => Err(
                "deployment_mode must be one of: public, hybrid, private, relay_only, validator_hidden"
                    .to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerNodeRole {
    ValidatorCore,
    Sentry,
    Relay,
    FullStorage,
    ObserverLight,
}

impl PeerNodeRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ValidatorCore => "validator_core",
            Self::Sentry => "sentry",
            Self::Relay => "relay",
            Self::FullStorage => "full_storage",
            Self::ObserverLight => "observer_light",
        }
    }

    pub fn parse_wire(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "validator_core" | "sequencer" => Ok(Self::ValidatorCore),
            "sentry" => Ok(Self::Sentry),
            "relay" => Ok(Self::Relay),
            "full_storage" | "storage" => Ok(Self::FullStorage),
            "observer_light" | "observer" => Ok(Self::ObserverLight),
            _ => Err(
                "node_role must be one of: validator_core, sentry, relay, full_storage, observer_light"
                    .to_string(),
            ),
        }
    }

    pub fn validate_deployment_mode(
        self,
        deployment_mode: PeerDeploymentMode,
    ) -> Result<(), String> {
        match self {
            Self::ValidatorCore => {
                if matches!(
                    deployment_mode,
                    PeerDeploymentMode::Public | PeerDeploymentMode::RelayOnly
                ) {
                    return Err(format!(
                        "node_role={} cannot use deployment_mode={deployment_mode}",
                        self.as_str()
                    ));
                }
            }
            Self::Sentry => {
                if !matches!(
                    deployment_mode,
                    PeerDeploymentMode::Public | PeerDeploymentMode::Hybrid
                ) {
                    return Err(format!(
                        "node_role={} requires deployment_mode public or hybrid, got {deployment_mode}",
                        self.as_str()
                    ));
                }
            }
            Self::Relay => {
                if !matches!(deployment_mode, PeerDeploymentMode::Public) {
                    return Err(format!(
                        "node_role={} requires deployment_mode public, got {deployment_mode}",
                        self.as_str()
                    ));
                }
            }
            Self::FullStorage | Self::ObserverLight => {
                if matches!(deployment_mode, PeerDeploymentMode::ValidatorHidden) {
                    return Err(format!(
                        "node_role={} cannot use deployment_mode={deployment_mode}",
                        self.as_str()
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn default_capability_lanes(self) -> Vec<NetworkLane> {
        match self {
            Self::ValidatorCore | Self::Sentry => vec![
                NetworkLane::ConsensusGossip,
                NetworkLane::Sync,
                NetworkLane::BlobState,
                NetworkLane::Control,
            ],
            Self::Relay => vec![NetworkLane::Control],
            Self::FullStorage => vec![
                NetworkLane::ConsensusGossip,
                NetworkLane::Sync,
                NetworkLane::BlobState,
                NetworkLane::Control,
            ],
            Self::ObserverLight => vec![NetworkLane::ConsensusGossip, NetworkLane::Control],
        }
    }
}

impl fmt::Display for PeerNodeRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PeerNodeRole {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse_wire(raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerDiscoverySource {
    StaticBootstrap,
    Dht,
    Rendezvous,
    PeerExchange,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRecord {
    pub peer_id: String,
    pub node_id: String,
    pub world_id: String,
    pub network_id: String,
    pub node_role: String,
    #[serde(default = "default_peer_deployment_mode")]
    pub deployment_mode: PeerDeploymentMode,
    pub reachability_class: PeerReachabilityClass,
    #[serde(default)]
    pub direct_addrs: Vec<String>,
    #[serde(default)]
    pub hole_punch_addrs: Vec<String>,
    #[serde(default)]
    pub relay_addrs: Vec<String>,
    #[serde(default)]
    pub discovery_sources: Vec<PeerDiscoverySource>,
    #[serde(default)]
    pub capability_lanes: Vec<NetworkLane>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_operator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_asn: Option<String>,
    pub published_at_ms: i64,
    pub ttl_ms: i64,
}

fn default_peer_deployment_mode() -> PeerDeploymentMode {
    PeerDeploymentMode::Private
}

impl PeerRecord {
    pub fn parsed_node_role(&self) -> Result<PeerNodeRole, String> {
        PeerNodeRole::parse_wire(self.node_role.as_str())
    }

    /// Checked absolute expiry under the current v1 wire time policy.
    pub fn expires_at_ms(&self) -> Result<i64, String> {
        if self.published_at_ms < 0 {
            return Err("peer record published_at_ms must be nonnegative".to_string());
        }
        if !(1..=3_600_000).contains(&self.ttl_ms) {
            return Err("peer record ttl_ms must be in 1..=3600000".to_string());
        }
        self.published_at_ms
            .checked_add(self.ttl_ms)
            .ok_or_else(|| "peer record expiry overflows i64".to_string())
    }

    pub fn validate_policy(&self) -> Result<(), String> {
        self.expires_at_ms()?;
        let node_role = self.parsed_node_role()?;
        node_role.validate_deployment_mode(self.deployment_mode)?;
        if matches!(
            self.deployment_mode,
            PeerDeploymentMode::Private
                | PeerDeploymentMode::RelayOnly
                | PeerDeploymentMode::ValidatorHidden
        ) && !self.direct_addrs.is_empty()
        {
            return Err(format!(
                "deployment_mode={} cannot advertise direct_addrs",
                self.deployment_mode
            ));
        }
        if matches!(node_role, PeerNodeRole::ValidatorCore) && !self.direct_addrs.is_empty() {
            return Err("node_role=validator_core cannot advertise direct_addrs".to_string());
        }
        if !self
            .effective_capability_lanes()
            .contains(&NetworkLane::Control)
        {
            return Err("peer record capability_lanes must include control".to_string());
        }
        if matches!(node_role, PeerNodeRole::Relay)
            && self
                .effective_capability_lanes()
                .iter()
                .any(|lane| !matches!(lane, NetworkLane::Control))
        {
            return Err("node_role=relay can only advertise control capability".to_string());
        }
        if self.effective_capability_lanes().iter().any(|lane| {
            matches!(lane, NetworkLane::Sync | NetworkLane::BlobState)
                && !lane.allows_role(
                    node_role,
                    crate::distributed_net::NetworkLaneOperation::Serve,
                )
        }) {
            return Err(format!(
                "node_role={} cannot advertise sync/blob_state service capability",
                node_role.as_str()
            ));
        }
        Ok(())
    }

    pub fn effective_capability_lanes(&self) -> Vec<NetworkLane> {
        let node_role = match self.parsed_node_role() {
            Ok(role) => role,
            Err(_) => return Vec::new(),
        };
        if self.capability_lanes.is_empty() {
            return node_role.default_capability_lanes();
        }
        let mut lanes = self.capability_lanes.clone();
        lanes.sort_by_key(|lane| lane.as_str());
        lanes.dedup();
        lanes
    }

    pub fn supports_lane(&self, lane: NetworkLane) -> bool {
        self.effective_capability_lanes().contains(&lane)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedPeerRecord {
    pub record: PeerRecord,
    pub identity_public_key_protobuf_hex: String,
    pub signature_hex: String,
}

pub trait DistributedDht<E> {
    fn publish_provider(
        &self,
        world_id: &str,
        content_hash: &str,
        provider_id: &str,
    ) -> Result<(), E>;

    fn publish_provider_best_effort(
        &self,
        world_id: &str,
        content_hash: &str,
        provider_id: &str,
    ) -> Result<(), E> {
        self.publish_provider(world_id, content_hash, provider_id)
    }

    fn get_providers(&self, world_id: &str, content_hash: &str) -> Result<Vec<ProviderRecord>, E>;

    fn put_world_head(&self, world_id: &str, head: &WorldHeadAnnounce) -> Result<(), E>;

    fn get_world_head(&self, world_id: &str) -> Result<Option<WorldHeadAnnounce>, E>;

    fn put_membership_directory(
        &self,
        world_id: &str,
        snapshot: &MembershipDirectorySnapshot,
    ) -> Result<(), E>;

    fn get_membership_directory(
        &self,
        world_id: &str,
    ) -> Result<Option<MembershipDirectorySnapshot>, E>;

    fn put_peer_record(&self, world_id: &str, record: &SignedPeerRecord) -> Result<(), E>;

    fn get_peer_record(&self, world_id: &str, peer_id: &str)
    -> Result<Option<SignedPeerRecord>, E>;
}

#[cfg(test)]
mod tests {
    use super::{
        PeerDeploymentMode, PeerDiscoverySource, PeerNodeRole, PeerReachabilityClass, PeerRecord,
        ProviderRecord, SignedPeerRecord,
    };
    use crate::distributed_net::NetworkLane;

    #[test]
    fn provider_record_deserializes_legacy_json_without_capability_fields() {
        let legacy_json = r#"{"provider_id":"peer-1","last_seen_ms":42}"#;
        let record: ProviderRecord = serde_json::from_str(legacy_json).expect("deserialize");
        assert_eq!(record.provider_id, "peer-1");
        assert_eq!(record.last_seen_ms, 42);
        assert_eq!(record.storage_total_bytes, None);
        assert_eq!(record.storage_available_bytes, None);
        assert_eq!(record.uptime_ratio_per_mille, None);
        assert_eq!(record.challenge_pass_ratio_per_mille, None);
        assert_eq!(record.load_ratio_per_mille, None);
        assert_eq!(record.p50_read_latency_ms, None);
    }

    #[test]
    fn provider_record_omits_empty_capability_fields_in_json() {
        let record = ProviderRecord {
            provider_id: "peer-1".to_string(),
            last_seen_ms: 42,
            storage_total_bytes: None,
            storage_available_bytes: None,
            uptime_ratio_per_mille: None,
            challenge_pass_ratio_per_mille: None,
            load_ratio_per_mille: None,
            p50_read_latency_ms: None,
        };
        let encoded = serde_json::to_value(&record).expect("serialize");
        let object = encoded.as_object().expect("json object");
        assert_eq!(
            object.get("provider_id").and_then(|v| v.as_str()),
            Some("peer-1")
        );
        assert_eq!(
            object.get("last_seen_ms").and_then(|v| v.as_i64()),
            Some(42)
        );
        assert!(object.get("storage_total_bytes").is_none());
        assert!(object.get("storage_available_bytes").is_none());
        assert!(object.get("uptime_ratio_per_mille").is_none());
        assert!(object.get("challenge_pass_ratio_per_mille").is_none());
        assert!(object.get("load_ratio_per_mille").is_none());
        assert!(object.get("p50_read_latency_ms").is_none());
    }

    #[test]
    fn signed_peer_record_round_trips_with_discovery_sources() {
        let record = SignedPeerRecord {
            record: PeerRecord {
                peer_id: "12D3KooWExample".to_string(),
                node_id: "node-a".to_string(),
                world_id: "world-a".to_string(),
                network_id: "network-a".to_string(),
                node_role: PeerNodeRole::ValidatorCore.as_str().to_string(),
                deployment_mode: PeerDeploymentMode::ValidatorHidden,
                reachability_class: PeerReachabilityClass::Private,
                direct_addrs: vec!["/ip4/127.0.0.1/tcp/4101".to_string()],
                hole_punch_addrs: Vec::new(),
                relay_addrs: vec!["/dns4/relay.example/tcp/443".to_string()],
                discovery_sources: vec![
                    PeerDiscoverySource::StaticBootstrap,
                    PeerDiscoverySource::Dht,
                ],
                capability_lanes: vec![NetworkLane::ConsensusGossip, NetworkLane::Control],
                source_operator: None,
                source_asn: None,
                published_at_ms: 42,
                ttl_ms: 60_000,
            },
            identity_public_key_protobuf_hex: "abcd".to_string(),
            signature_hex: "deadbeef".to_string(),
        };

        let encoded = serde_json::to_string(&record).expect("serialize");
        let decoded: SignedPeerRecord =
            serde_json::from_str(encoded.as_str()).expect("deserialize");
        assert_eq!(decoded, record);
    }

    #[test]
    fn peer_record_policy_accepts_legacy_role_labels() {
        let record = PeerRecord {
            peer_id: "12D3KooWExample".to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: "storage".to_string(),
            deployment_mode: PeerDeploymentMode::Private,
            reachability_class: PeerReachabilityClass::Private,
            direct_addrs: Vec::new(),
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![PeerDiscoverySource::Dht],
            capability_lanes: Vec::new(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 1,
            ttl_ms: 1_000,
        };
        assert_eq!(record.parsed_node_role(), Ok(PeerNodeRole::FullStorage));
        assert!(record.validate_policy().is_ok());
    }

    #[test]
    fn peer_record_policy_rejects_public_validator_core_direct_surface() {
        let record = PeerRecord {
            peer_id: "12D3KooWExample".to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::ValidatorCore.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Public,
            reachability_class: PeerReachabilityClass::Public,
            direct_addrs: vec!["/ip4/127.0.0.1/tcp/4101".to_string()],
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![PeerDiscoverySource::Dht],
            capability_lanes: Vec::new(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 1,
            ttl_ms: 1_000,
        };
        assert!(record.validate_policy().is_err());
    }

    #[test]
    fn peer_record_defaults_capability_lanes_from_role() {
        let record = PeerRecord {
            peer_id: "12D3KooWExample".to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Private,
            reachability_class: PeerReachabilityClass::Private,
            direct_addrs: Vec::new(),
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![PeerDiscoverySource::Dht],
            capability_lanes: Vec::new(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 1,
            ttl_ms: 1_000,
        };
        assert!(record.supports_lane(NetworkLane::Sync));
        assert!(record.supports_lane(NetworkLane::BlobState));
        assert!(record.supports_lane(NetworkLane::Control));
    }

    #[test]
    fn observer_light_defaults_to_non_serving_capabilities() {
        let record = PeerRecord {
            peer_id: "12D3KooWExample".to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::ObserverLight.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Private,
            reachability_class: PeerReachabilityClass::Private,
            direct_addrs: Vec::new(),
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![PeerDiscoverySource::Dht],
            capability_lanes: Vec::new(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 1,
            ttl_ms: 1_000,
        };
        assert!(record.supports_lane(NetworkLane::ConsensusGossip));
        assert!(record.supports_lane(NetworkLane::Control));
        assert!(!record.supports_lane(NetworkLane::Sync));
        assert!(!record.supports_lane(NetworkLane::BlobState));
    }

    #[test]
    fn peer_record_policy_rejects_non_control_capability_for_relay() {
        let record = PeerRecord {
            peer_id: "12D3KooWExample".to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::Relay.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Public,
            reachability_class: PeerReachabilityClass::Public,
            direct_addrs: Vec::new(),
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![PeerDiscoverySource::Dht],
            capability_lanes: vec![NetworkLane::Control, NetworkLane::Sync],
            source_operator: None,
            source_asn: None,
            published_at_ms: 1,
            ttl_ms: 1_000,
        };
        assert!(record.validate_policy().is_err());
    }

    #[test]
    fn peer_record_policy_rejects_observer_data_service_capabilities() {
        let record = PeerRecord {
            peer_id: "12D3KooWExample".to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::ObserverLight.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Private,
            reachability_class: PeerReachabilityClass::Private,
            direct_addrs: Vec::new(),
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![PeerDiscoverySource::Dht],
            capability_lanes: vec![NetworkLane::Control, NetworkLane::Sync],
            source_operator: None,
            source_asn: None,
            published_at_ms: 1,
            ttl_ms: 1_000,
        };
        assert!(record.validate_policy().is_err());
    }
    #[test]
    fn peer_record_signed_expiry_is_checked_and_bounded() {
        let mut record = {
            PeerRecord {
                peer_id: "peer".into(),
                node_id: "node".into(),
                world_id: "world".into(),
                network_id: "network".into(),
                node_role: "observer_light".into(),
                deployment_mode: PeerDeploymentMode::Private,
                reachability_class: PeerReachabilityClass::Private,
                direct_addrs: vec![],
                hole_punch_addrs: vec![],
                relay_addrs: vec![],
                discovery_sources: vec![],
                capability_lanes: vec![],
                source_operator: None,
                source_asn: None,
                published_at_ms: 1_000,
                ttl_ms: 1,
            }
        };
        assert_eq!(record.expires_at_ms().unwrap(), 1_001);
        for (published, ttl) in [(-1, 1), (0, 0), (0, -1), (0, 3_600_001), (i64::MAX, 1)] {
            record.published_at_ms = published;
            record.ttl_ms = ttl;
            assert!(record.expires_at_ms().is_err());
            assert!(record.validate_policy().is_err());
        }
        record.published_at_ms = 0;
        record.ttl_ms = 3_600_000;
        assert_eq!(record.expires_at_ms().unwrap(), 3_600_000);
        assert!(record.validate_policy().is_ok());
    }
}
