use std::collections::BTreeMap;
use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use oasis7_distfs::FeedbackStoreConfig;
use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole, PeerReachabilityClass};
use oasis7_proto::distributed_net::{NetworkLane, NetworkLaneOperation};

use crate::pos_validation::validate_pos_config;
use crate::types_consensus::{
    NodeConsensusMisbehaviorEvidenceSnapshot, NodeConsensusSlashingIntentSnapshot,
    NodeConsensusSlashingReceiptSnapshot, NodeValidatorStakeProofSnapshot,
};
use crate::{
    NodeConsensusAction, NodeConsensusProgressObserverError, NodeError, NodeReplicationConfig,
};

mod main_token_controller_binding;
mod node_config_flags;

pub use main_token_controller_binding::{
    NodeMainTokenControllerBindingConfig, NodeMainTokenControllerSignerPolicy,
};

const DEFAULT_MAX_PENDING_CONSENSUS_ACTIONS: usize = 4096;
const DEFAULT_MAX_ENGINE_PENDING_CONSENSUS_ACTIONS: usize = 4096;
const DEFAULT_MAX_CONSENSUS_ACTION_PAYLOAD_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_COMMITTED_ACTION_BATCHES: usize = 4096;
// Keep raw action retention below 96 MiB on the constrained public-testnet
// hosts while leaving headroom for execution snapshots and allocator peaks.
const DEFAULT_MAX_PENDING_CONSENSUS_ACTION_QUEUE_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_MAX_COMMITTED_ACTION_BATCH_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_MAX_DYNAMIC_GOSSIP_PEERS: usize = 1024;
const DEFAULT_DYNAMIC_GOSSIP_PEER_TTL_MS: i64 = 10 * 60 * 1000;
const DEFAULT_REPLICA_MAINTENANCE_MAX_CONTENT_HASH_SAMPLES: usize = 128;
const DEFAULT_REPLICA_MAINTENANCE_POLL_INTERVAL_MS: i64 = 60_000;
const DEFAULT_FEEDBACK_P2P_MAX_INCOMING_ANNOUNCES_PER_TICK: usize = 128;
const DEFAULT_FEEDBACK_P2P_MAX_OUTGOING_ANNOUNCES_PER_TICK: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    Sequencer,
    Storage,
    Observer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeNetworkPolicy {
    pub deployment_mode: PeerDeploymentMode,
    pub node_role_claim: PeerNodeRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeUserMode {
    AutoJoin,
    PrivateSafe,
    PublicEntry,
}

impl NodeUserMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AutoJoin => "auto_join",
            Self::PrivateSafe => "private_safe",
            Self::PublicEntry => "public_entry",
        }
    }
}

impl fmt::Display for NodeUserMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for NodeUserMode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto_join" => Ok(Self::AutoJoin),
            "private_safe" => Ok(Self::PrivateSafe),
            "public_entry" => Ok(Self::PublicEntry),
            _ => Err(
                "p2p user mode must be one of: auto_join, private_safe, public_entry".to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeHolePunchViability {
    Unknown,
    Viable,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeAutoNatStatus {
    Unknown,
    Public,
    Private,
}

impl NodeAutoNatStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Public => "public",
            Self::Private => "private",
        }
    }
}

impl fmt::Display for NodeAutoNatStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for NodeAutoNatStatus {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "unknown" => Ok(Self::Unknown),
            "public" => Ok(Self::Public),
            "private" => Ok(Self::Private),
            _ => Err("autonat status must be one of: unknown, public, private".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodePublicPortReachability {
    Unknown,
    Reachable,
    Unreachable,
}

impl NodePublicPortReachability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Reachable => "reachable",
            Self::Unreachable => "unreachable",
        }
    }
}

impl fmt::Display for NodePublicPortReachability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for NodePublicPortReachability {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "unknown" => Ok(Self::Unknown),
            "reachable" => Ok(Self::Reachable),
            "unreachable" => Ok(Self::Unreachable),
            _ => Err(
                "public port reachability must be one of: unknown, reachable, unreachable"
                    .to_string(),
            ),
        }
    }
}

impl NodeHolePunchViability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Viable => "viable",
            Self::Blocked => "blocked",
        }
    }
}

impl fmt::Display for NodeHolePunchViability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for NodeHolePunchViability {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "unknown" => Ok(Self::Unknown),
            "viable" => Ok(Self::Viable),
            "blocked" => Ok(Self::Blocked),
            _ => Err("hole punch viability must be one of: unknown, viable, blocked".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeReachabilityAutoDetection {
    pub observed_reachability: Option<PeerReachabilityClass>,
    pub hole_punch_viability: NodeHolePunchViability,
    pub autonat_status: NodeAutoNatStatus,
    pub public_port_reachability: NodePublicPortReachability,
    pub relay_available: bool,
    pub probe_stable: bool,
}

impl Default for NodeReachabilityAutoDetection {
    fn default() -> Self {
        Self {
            observed_reachability: None,
            hole_punch_viability: NodeHolePunchViability::Unknown,
            autonat_status: NodeAutoNatStatus::Unknown,
            public_port_reachability: NodePublicPortReachability::Unknown,
            relay_available: false,
            probe_stable: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeUserModeRecommendation {
    pub requested_user_mode: NodeUserMode,
    pub recommended_user_mode: NodeUserMode,
    pub effective_user_mode: NodeUserMode,
    pub effective_policy: NodeNetworkPolicy,
    pub requires_explicit_public_entry_confirmation: bool,
    pub rationale: Vec<String>,
}

impl NodeNetworkPolicy {
    pub fn for_runtime_role(role: NodeRole) -> Self {
        let node_role_claim = match role {
            NodeRole::Sequencer => PeerNodeRole::ValidatorCore,
            NodeRole::Storage => PeerNodeRole::FullStorage,
            NodeRole::Observer => PeerNodeRole::ObserverLight,
        };
        Self {
            deployment_mode: PeerDeploymentMode::Private,
            node_role_claim,
        }
    }

    pub fn validate_for_runtime_role(&self, runtime_role: NodeRole) -> Result<(), NodeError> {
        self.node_role_claim
            .validate_deployment_mode(self.deployment_mode)
            .map_err(|reason| NodeError::InvalidConfig { reason })?;
        match runtime_role {
            NodeRole::Sequencer => {
                if self.node_role_claim != PeerNodeRole::ValidatorCore {
                    return Err(NodeError::InvalidConfig {
                        reason: format!(
                            "node role {} requires network node_role_claim=validator_core, got {}",
                            runtime_role, self.node_role_claim
                        ),
                    });
                }
            }
            NodeRole::Storage => {
                if !matches!(
                    self.node_role_claim,
                    PeerNodeRole::FullStorage | PeerNodeRole::ValidatorCore
                ) {
                    return Err(NodeError::InvalidConfig {
                        reason: format!(
                            "node role {} requires network node_role_claim=full_storage or validator_core, got {}",
                            runtime_role, self.node_role_claim
                        ),
                    });
                }
            }
            NodeRole::Observer => {
                if matches!(
                    self.node_role_claim,
                    PeerNodeRole::ValidatorCore | PeerNodeRole::FullStorage
                ) {
                    return Err(NodeError::InvalidConfig {
                        reason: format!(
                            "node role {} cannot use network node_role_claim={}",
                            runtime_role, self.node_role_claim
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn advertised_reachability_class(&self) -> PeerReachabilityClass {
        self.deployment_mode.initial_reachability_class()
    }

    pub fn minimum_independent_ingress_paths(&self) -> usize {
        if matches!(self.deployment_mode, PeerDeploymentMode::ValidatorHidden) {
            2
        } else {
            0
        }
    }

    pub fn public_direct_surface_allowed(&self) -> bool {
        !matches!(
            self.deployment_mode,
            PeerDeploymentMode::Private
                | PeerDeploymentMode::RelayOnly
                | PeerDeploymentMode::ValidatorHidden
        )
    }

    pub fn allows_lane_operation(
        &self,
        lane: NetworkLane,
        operation: NetworkLaneOperation,
    ) -> bool {
        lane.allows_role(self.node_role_claim, operation)
    }

    pub fn recommend_for_user_mode(
        runtime_role: NodeRole,
        requested_user_mode: NodeUserMode,
        detection: NodeReachabilityAutoDetection,
        accept_public_entry: bool,
    ) -> Result<NodeUserModeRecommendation, NodeError> {
        let recommended_user_mode = recommend_user_mode(runtime_role, detection);
        let requires_explicit_public_entry_confirmation = requested_user_mode
            == NodeUserMode::AutoJoin
            && recommended_user_mode == NodeUserMode::PublicEntry
            && !accept_public_entry;
        let effective_user_mode = match requested_user_mode {
            NodeUserMode::AutoJoin => {
                if requires_explicit_public_entry_confirmation {
                    NodeUserMode::PrivateSafe
                } else {
                    recommended_user_mode
                }
            }
            explicit => explicit,
        };

        let effective_policy = policy_for_user_mode(runtime_role, effective_user_mode);
        effective_policy.validate_for_runtime_role(runtime_role)?;

        let mut rationale = Vec::new();
        if let Some(reachability) = detection.observed_reachability {
            rationale.push(format!(
                "observed_reachability={}",
                peer_reachability_as_str(reachability)
            ));
        } else {
            rationale.push("observed_reachability=unknown".to_string());
        }
        rationale.push(format!(
            "hole_punch_viability={}",
            detection.hole_punch_viability
        ));
        rationale.push(format!("autonat_status={}", detection.autonat_status));
        rationale.push(format!(
            "public_port_reachability={}",
            detection.public_port_reachability
        ));
        rationale.push(format!("relay_available={}", detection.relay_available));
        rationale.push(format!("probe_stable={}", detection.probe_stable));
        if requires_explicit_public_entry_confirmation {
            rationale.push("public_entry_confirmation=pending".to_string());
        }

        Ok(NodeUserModeRecommendation {
            requested_user_mode,
            recommended_user_mode,
            effective_user_mode,
            effective_policy,
            requires_explicit_public_entry_confirmation,
            rationale,
        })
    }
}

fn recommend_user_mode(
    _runtime_role: NodeRole,
    detection: NodeReachabilityAutoDetection,
) -> NodeUserMode {
    if !detection.probe_stable {
        return NodeUserMode::PrivateSafe;
    }

    if matches!(
        detection.public_port_reachability,
        NodePublicPortReachability::Reachable
    ) || matches!(detection.autonat_status, NodeAutoNatStatus::Public)
    {
        return NodeUserMode::PublicEntry;
    }

    if matches!(
        detection.observed_reachability,
        Some(PeerReachabilityClass::Public | PeerReachabilityClass::Hybrid)
    ) {
        return NodeUserMode::PublicEntry;
    }

    if matches!(
        detection.hole_punch_viability,
        NodeHolePunchViability::Viable
    ) {
        return NodeUserMode::AutoJoin;
    }

    NodeUserMode::PrivateSafe
}

fn policy_for_user_mode(runtime_role: NodeRole, user_mode: NodeUserMode) -> NodeNetworkPolicy {
    let node_role_claim = match runtime_role {
        NodeRole::Sequencer => PeerNodeRole::ValidatorCore,
        NodeRole::Storage => PeerNodeRole::FullStorage,
        NodeRole::Observer => PeerNodeRole::ObserverLight,
    };
    let deployment_mode = match user_mode {
        NodeUserMode::AutoJoin | NodeUserMode::PrivateSafe => PeerDeploymentMode::Private,
        NodeUserMode::PublicEntry => match runtime_role {
            NodeRole::Sequencer => PeerDeploymentMode::Hybrid,
            NodeRole::Storage | NodeRole::Observer => PeerDeploymentMode::Public,
        },
    };
    NodeNetworkPolicy {
        deployment_mode,
        node_role_claim,
    }
}

fn peer_reachability_as_str(reachability: PeerReachabilityClass) -> &'static str {
    match reachability {
        PeerReachabilityClass::Public => "public",
        PeerReachabilityClass::Hybrid => "hybrid",
        PeerReachabilityClass::Private => "private",
        PeerReachabilityClass::RelayOnly => "relay_only",
        PeerReachabilityClass::ValidatorHidden => "validator_hidden",
    }
}

impl NodeRole {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeRole::Sequencer => "sequencer",
            NodeRole::Storage => "storage",
            NodeRole::Observer => "observer",
        }
    }
}

impl fmt::Display for NodeRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for NodeRole {
    type Err = NodeError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "sequencer" => Ok(NodeRole::Sequencer),
            "storage" => Ok(NodeRole::Storage),
            "observer" => Ok(NodeRole::Observer),
            _ => Err(NodeError::InvalidRole {
                role: raw.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeConsensusMode {
    Pos,
    TendermintV1,
    TendermintV2,
    TendermintV3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum PosConsensusStatus {
    Pending,
    Committed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosValidator {
    pub validator_id: String,
    pub stake: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePosConfig {
    pub validators: Vec<PosValidator>,
    pub validator_player_ids: BTreeMap<String, String>,
    pub validator_signer_public_keys: BTreeMap<String, String>,
    pub supermajority_numerator: u64,
    pub supermajority_denominator: u64,
    pub epoch_length_slots: u64,
    pub slot_duration_ms: u64,
    pub ticks_per_slot: u64,
    pub proposal_tick_phase: u64,
    pub adaptive_tick_scheduler_enabled: bool,
    pub slot_clock_genesis_unix_ms: Option<i64>,
    pub max_past_slot_lag: u64,
}

impl NodePosConfig {
    pub fn ethereum_like(validators: Vec<PosValidator>) -> Self {
        let mut validator_player_ids = BTreeMap::new();
        for validator in &validators {
            validator_player_ids.insert(
                validator.validator_id.clone(),
                validator.validator_id.clone(),
            );
        }
        Self {
            validators,
            validator_player_ids,
            validator_signer_public_keys: BTreeMap::new(),
            supermajority_numerator: 2,
            supermajority_denominator: 3,
            epoch_length_slots: 32,
            slot_duration_ms: 1,
            ticks_per_slot: 1,
            proposal_tick_phase: 0,
            adaptive_tick_scheduler_enabled: false,
            slot_clock_genesis_unix_ms: None,
            max_past_slot_lag: 256,
        }
    }

    pub fn with_validator_player_ids(
        mut self,
        validator_player_ids: BTreeMap<String, String>,
    ) -> Result<Self, NodeError> {
        self.validator_player_ids = validator_player_ids;
        validate_pos_config(&self)?;
        Ok(self)
    }

    pub fn with_validator_signer_public_keys(
        mut self,
        validator_signer_public_keys: BTreeMap<String, String>,
    ) -> Result<Self, NodeError> {
        self.validator_signer_public_keys = validator_signer_public_keys;
        validate_pos_config(&self)?;
        Ok(self)
    }

    pub fn with_ticks_per_slot(mut self, ticks_per_slot: u64) -> Result<Self, NodeError> {
        self.ticks_per_slot = ticks_per_slot;
        if ticks_per_slot > 0 && self.proposal_tick_phase >= ticks_per_slot {
            self.proposal_tick_phase = ticks_per_slot - 1;
        }
        validate_pos_config(&self)?;
        Ok(self)
    }

    pub fn with_proposal_tick_phase(mut self, proposal_tick_phase: u64) -> Result<Self, NodeError> {
        self.proposal_tick_phase = proposal_tick_phase;
        validate_pos_config(&self)?;
        Ok(self)
    }

    pub fn with_adaptive_tick_scheduler_enabled(mut self, enabled: bool) -> Self {
        self.adaptive_tick_scheduler_enabled = enabled;
        self
    }

    pub fn with_max_past_slot_lag(mut self, max_past_slot_lag: u64) -> Result<Self, NodeError> {
        self.max_past_slot_lag = max_past_slot_lag;
        validate_pos_config(&self)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeConfig {
    pub node_id: String,
    pub player_id: String,
    pub world_id: String,
    pub tick_interval: Duration,
    pub role: NodeRole,
    pub network_policy: NodeNetworkPolicy,
    pub pos_config: NodePosConfig,
    pub allow_local_proposals: bool,
    pub auto_attest_all_validators: bool,
    pub require_execution_on_commit: bool,
    pub require_peer_execution_hashes: bool,
    pub max_pending_consensus_actions: usize,
    pub max_engine_pending_consensus_actions: usize,
    pub max_consensus_action_payload_bytes: usize,
    pub max_committed_action_batches: usize,
    pub max_pending_consensus_action_queue_bytes: usize,
    pub max_committed_action_batch_bytes: usize,
    pub max_dynamic_gossip_peers: usize,
    pub dynamic_gossip_peer_ttl_ms: i64,
    pub main_token_controller_binding: NodeMainTokenControllerBindingConfig,
    pub replica_maintenance: Option<NodeReplicaMaintenanceConfig>,
    pub gossip: Option<NodeGossipConfig>,
    pub replication: Option<NodeReplicationConfig>,
    pub feedback_p2p: Option<NodeFeedbackP2pConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeReplicaMaintenanceConfig {
    pub enabled: bool,
    pub max_content_hash_samples_per_round: usize,
    pub target_replicas_per_blob: usize,
    pub max_repairs_per_round: usize,
    pub max_rebalances_per_round: usize,
    pub rebalance_source_load_min_per_mille: u16,
    pub rebalance_target_load_max_per_mille: u16,
    pub poll_interval_ms: i64,
}

impl Default for NodeReplicaMaintenanceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_content_hash_samples_per_round:
                DEFAULT_REPLICA_MAINTENANCE_MAX_CONTENT_HASH_SAMPLES,
            target_replicas_per_blob: 3,
            max_repairs_per_round: 32,
            max_rebalances_per_round: 32,
            rebalance_source_load_min_per_mille: 850,
            rebalance_target_load_max_per_mille: 450,
            poll_interval_ms: DEFAULT_REPLICA_MAINTENANCE_POLL_INTERVAL_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeGossipConfig {
    pub bind_addr: SocketAddr,
    pub peers: Vec<SocketAddr>,
    pub max_dynamic_peers: usize,
    pub dynamic_peer_ttl_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeFeedbackP2pConfig {
    pub store: FeedbackStoreConfig,
    pub max_incoming_announces_per_tick: usize,
    pub max_outgoing_announces_per_tick: usize,
}

impl Default for NodeFeedbackP2pConfig {
    fn default() -> Self {
        Self {
            store: FeedbackStoreConfig::default(),
            max_incoming_announces_per_tick: DEFAULT_FEEDBACK_P2P_MAX_INCOMING_ANNOUNCES_PER_TICK,
            max_outgoing_announces_per_tick: DEFAULT_FEEDBACK_P2P_MAX_OUTGOING_ANNOUNCES_PER_TICK,
        }
    }
}

impl NodeFeedbackP2pConfig {
    pub fn with_store(mut self, store: FeedbackStoreConfig) -> Self {
        self.store = store;
        self
    }

    pub fn with_max_incoming_announces_per_tick(
        mut self,
        max_incoming_announces_per_tick: usize,
    ) -> Result<Self, NodeError> {
        if max_incoming_announces_per_tick == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "feedback_p2p.max_incoming_announces_per_tick must be positive".to_string(),
            });
        }
        self.max_incoming_announces_per_tick = max_incoming_announces_per_tick;
        Ok(self)
    }

    pub fn with_max_outgoing_announces_per_tick(
        mut self,
        max_outgoing_announces_per_tick: usize,
    ) -> Result<Self, NodeError> {
        if max_outgoing_announces_per_tick == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "feedback_p2p.max_outgoing_announces_per_tick must be positive".to_string(),
            });
        }
        self.max_outgoing_announces_per_tick = max_outgoing_announces_per_tick;
        Ok(self)
    }
}

impl NodeConfig {
    pub fn new(
        node_id: impl Into<String>,
        world_id: impl Into<String>,
        role: NodeRole,
    ) -> Result<Self, NodeError> {
        let node_id = node_id.into();
        let world_id = world_id.into();
        if node_id.trim().is_empty() {
            return Err(NodeError::InvalidConfig {
                reason: "node_id cannot be empty".to_string(),
            });
        }
        if world_id.trim().is_empty() {
            return Err(NodeError::InvalidConfig {
                reason: "world_id cannot be empty".to_string(),
            });
        }

        let pos_config = NodePosConfig::ethereum_like(vec![PosValidator {
            validator_id: node_id.clone(),
            stake: 100,
        }]);
        validate_pos_config(&pos_config)?;

        Ok(Self {
            player_id: node_id.clone(),
            node_id,
            world_id,
            tick_interval: Duration::from_millis(200),
            role,
            network_policy: NodeNetworkPolicy::for_runtime_role(role),
            pos_config,
            allow_local_proposals: true,
            auto_attest_all_validators: false,
            require_execution_on_commit: matches!(role, NodeRole::Sequencer),
            require_peer_execution_hashes: false,
            max_pending_consensus_actions: DEFAULT_MAX_PENDING_CONSENSUS_ACTIONS,
            max_engine_pending_consensus_actions: DEFAULT_MAX_ENGINE_PENDING_CONSENSUS_ACTIONS,
            max_consensus_action_payload_bytes: DEFAULT_MAX_CONSENSUS_ACTION_PAYLOAD_BYTES,
            max_committed_action_batches: DEFAULT_MAX_COMMITTED_ACTION_BATCHES,
            max_pending_consensus_action_queue_bytes:
                DEFAULT_MAX_PENDING_CONSENSUS_ACTION_QUEUE_BYTES,
            max_committed_action_batch_bytes: DEFAULT_MAX_COMMITTED_ACTION_BATCH_BYTES,
            max_dynamic_gossip_peers: DEFAULT_MAX_DYNAMIC_GOSSIP_PEERS,
            dynamic_gossip_peer_ttl_ms: DEFAULT_DYNAMIC_GOSSIP_PEER_TTL_MS,
            main_token_controller_binding: NodeMainTokenControllerBindingConfig::default(),
            replica_maintenance: None,
            gossip: None,
            replication: None,
            feedback_p2p: None,
        })
    }

    pub fn with_player_id(mut self, player_id: impl Into<String>) -> Result<Self, NodeError> {
        let player_id = player_id.into();
        if player_id.trim().is_empty() {
            return Err(NodeError::InvalidConfig {
                reason: "player_id cannot be empty".to_string(),
            });
        }
        self.player_id = player_id.trim().to_string();
        Ok(self)
    }

    pub fn with_tick_interval(mut self, interval: Duration) -> Result<Self, NodeError> {
        if interval.is_zero() {
            return Err(NodeError::InvalidConfig {
                reason: "tick_interval must be positive".to_string(),
            });
        }
        self.tick_interval = interval;
        Ok(self)
    }

    pub fn with_pos_config(mut self, pos_config: NodePosConfig) -> Result<Self, NodeError> {
        validate_pos_config(&pos_config)?;
        self.pos_config = pos_config;
        Ok(self)
    }

    pub fn with_pos_validators(self, validators: Vec<PosValidator>) -> Result<Self, NodeError> {
        self.with_pos_config(NodePosConfig::ethereum_like(validators))
    }

    pub fn with_network_policy(
        mut self,
        network_policy: NodeNetworkPolicy,
    ) -> Result<Self, NodeError> {
        network_policy.validate_for_runtime_role(self.role)?;
        self.network_policy = network_policy;
        Ok(self)
    }

    pub fn with_max_pending_consensus_actions(
        mut self,
        max_pending_consensus_actions: usize,
    ) -> Result<Self, NodeError> {
        if max_pending_consensus_actions == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "max_pending_consensus_actions must be positive".to_string(),
            });
        }
        self.max_pending_consensus_actions = max_pending_consensus_actions;
        Ok(self)
    }

    pub fn with_max_consensus_action_payload_bytes(
        mut self,
        max_consensus_action_payload_bytes: usize,
    ) -> Result<Self, NodeError> {
        if max_consensus_action_payload_bytes == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "max_consensus_action_payload_bytes must be positive".to_string(),
            });
        }
        self.max_consensus_action_payload_bytes = max_consensus_action_payload_bytes;
        Ok(self)
    }

    pub fn with_max_engine_pending_consensus_actions(
        mut self,
        max_engine_pending_consensus_actions: usize,
    ) -> Result<Self, NodeError> {
        if max_engine_pending_consensus_actions == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "max_engine_pending_consensus_actions must be positive".to_string(),
            });
        }
        self.max_engine_pending_consensus_actions = max_engine_pending_consensus_actions;
        Ok(self)
    }

    pub fn with_max_committed_action_batches(
        mut self,
        max_committed_action_batches: usize,
    ) -> Result<Self, NodeError> {
        if max_committed_action_batches == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "max_committed_action_batches must be positive".to_string(),
            });
        }
        self.max_committed_action_batches = max_committed_action_batches;
        Ok(self)
    }

    pub fn with_max_dynamic_gossip_peers(
        mut self,
        max_dynamic_gossip_peers: usize,
    ) -> Result<Self, NodeError> {
        if max_dynamic_gossip_peers == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "max_dynamic_gossip_peers must be positive".to_string(),
            });
        }
        self.max_dynamic_gossip_peers = max_dynamic_gossip_peers;
        self.refresh_gossip_limits();
        Ok(self)
    }

    pub fn with_dynamic_gossip_peer_ttl_ms(
        mut self,
        dynamic_gossip_peer_ttl_ms: i64,
    ) -> Result<Self, NodeError> {
        if dynamic_gossip_peer_ttl_ms <= 0 {
            return Err(NodeError::InvalidConfig {
                reason: "dynamic_gossip_peer_ttl_ms must be positive".to_string(),
            });
        }
        self.dynamic_gossip_peer_ttl_ms = dynamic_gossip_peer_ttl_ms;
        self.refresh_gossip_limits();
        Ok(self)
    }

    pub fn with_gossip(
        mut self,
        bind_addr: SocketAddr,
        peers: Vec<SocketAddr>,
    ) -> Result<Self, NodeError> {
        if peers.is_empty() {
            return Err(NodeError::InvalidConfig {
                reason: "gossip peers cannot be empty".to_string(),
            });
        }
        let mut dedup = BTreeMap::new();
        for peer in peers {
            dedup.insert(peer, ());
        }
        self.gossip = Some(NodeGossipConfig {
            bind_addr,
            peers: dedup.keys().copied().collect(),
            max_dynamic_peers: self.max_dynamic_gossip_peers,
            dynamic_peer_ttl_ms: self.dynamic_gossip_peer_ttl_ms,
        });
        Ok(self)
    }

    pub fn with_gossip_optional(mut self, bind_addr: SocketAddr, peers: Vec<SocketAddr>) -> Self {
        let mut dedup = BTreeMap::new();
        for peer in peers {
            dedup.insert(peer, ());
        }
        self.gossip = Some(NodeGossipConfig {
            bind_addr,
            peers: dedup.keys().copied().collect(),
            max_dynamic_peers: self.max_dynamic_gossip_peers,
            dynamic_peer_ttl_ms: self.dynamic_gossip_peer_ttl_ms,
        });
        self
    }

    pub fn with_main_token_controller_binding(
        mut self,
        main_token_controller_binding: NodeMainTokenControllerBindingConfig,
    ) -> Result<Self, NodeError> {
        main_token_controller_binding.validate()?;
        self.main_token_controller_binding = main_token_controller_binding;
        Ok(self)
    }

    pub fn with_replication_root(
        mut self,
        root_dir: impl Into<PathBuf>,
    ) -> Result<Self, NodeError> {
        self.replication = Some(NodeReplicationConfig::new(root_dir)?);
        Ok(self)
    }

    pub fn with_replication(mut self, replication: NodeReplicationConfig) -> Self {
        self.replication = Some(replication);
        self
    }

    pub fn with_feedback_p2p(
        mut self,
        feedback_p2p: NodeFeedbackP2pConfig,
    ) -> Result<Self, NodeError> {
        if feedback_p2p.max_incoming_announces_per_tick == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "feedback_p2p.max_incoming_announces_per_tick must be positive".to_string(),
            });
        }
        if feedback_p2p.max_outgoing_announces_per_tick == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "feedback_p2p.max_outgoing_announces_per_tick must be positive".to_string(),
            });
        }
        self.feedback_p2p = Some(feedback_p2p);
        Ok(self)
    }

    pub fn with_replica_maintenance(
        mut self,
        replica_maintenance: NodeReplicaMaintenanceConfig,
    ) -> Result<Self, NodeError> {
        if replica_maintenance.max_content_hash_samples_per_round == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "replica_maintenance.max_content_hash_samples_per_round must be positive"
                    .to_string(),
            });
        }
        if replica_maintenance.target_replicas_per_blob == 0 {
            return Err(NodeError::InvalidConfig {
                reason: "replica_maintenance.target_replicas_per_blob must be positive".to_string(),
            });
        }
        if replica_maintenance.poll_interval_ms <= 0 {
            return Err(NodeError::InvalidConfig {
                reason: "replica_maintenance.poll_interval_ms must be positive".to_string(),
            });
        }
        self.replica_maintenance = Some(replica_maintenance);
        Ok(self)
    }

    fn refresh_gossip_limits(&mut self) {
        if let Some(gossip) = self.gossip.as_mut() {
            gossip.max_dynamic_peers = self.max_dynamic_gossip_peers;
            gossip.dynamic_peer_ttl_ms = self.dynamic_gossip_peer_ttl_ms;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeCommittedActionBatch {
    pub height: u64,
    pub slot: u64,
    pub epoch: u64,
    pub block_hash: String,
    pub action_root: String,
    pub committed_at_unix_ms: i64,
    pub actions: Vec<NodeConsensusAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePendingProposalSnapshot {
    pub height: u64,
    pub slot: u64,
    pub epoch: u64,
    pub proposer_id: String,
    pub opened_at_ms: i64,
    pub action_count: usize,
    pub action_payload_bytes: usize,
    pub attestation_count: usize,
    pub approved_stake: u64,
    pub rejected_stake: u64,
    pub required_stake: u64,
    pub total_stake: u64,
    pub approval_progress_bps: u16,
    pub rejection_progress_bps: u16,
    pub remaining_approval_stake: u64,
    pub status: PosConsensusStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodePendingConsensusActionsSnapshot {
    pub queued_action_count: usize,
    pub queued_payload_bytes: usize,
    pub reserved_requeue_action_count: usize,
    pub reserved_requeue_payload_bytes: usize,
    pub available_capacity: usize,
    pub max_capacity: usize,
    pub submit_buffer_action_count: usize,
    pub submit_buffer_payload_bytes: usize,
    pub submit_buffer_max_capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeFinalityLatencySnapshot {
    pub sample_count: usize,
    pub avg_latency_ms: Option<i64>,
    pub max_latency_ms: Option<i64>,
    pub p50_latency_ms: Option<i64>,
    pub p95_latency_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct NodeReplicationGapSyncRouteSnapshot {
    pub elapsed_ms: u64,
    pub route_attempt_count: u64,
    pub synced_route_count: u64,
    pub not_found_route_count: u64,
    pub error_route_count: u64,
    pub generic_route_count: u64,
    pub provider_route_count: u64,
    pub generic_retry_route_count: u64,
    pub budget_exhausted_count: u64,
    pub last_slow_route_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeConsensusSnapshot {
    pub mode: NodeConsensusMode,
    pub slot: u64,
    pub epoch: u64,
    pub ticks_per_slot: u64,
    pub tick_phase: u64,
    pub proposal_tick_phase: u64,
    pub last_observed_slot: u64,
    pub missed_slot_count: u64,
    pub last_observed_tick: u64,
    pub missed_tick_count: u64,
    pub adaptive_tick_scheduler_enabled: bool,
    pub latest_height: u64,
    pub committed_height: u64,
    pub last_committed_at_ms: Option<i64>,
    pub network_committed_height: u64,
    pub replication_persisted_height: u64,
    pub replication_gap_sync_blocked_height: Option<u64>,
    pub replication_gap_sync_blocked_reason: Option<String>,
    pub replication_gap_sync_repair_attempt_height: Option<u64>,
    pub replication_gap_sync_repair_attempt_summary: Option<String>,
    pub replication_gap_sync_repair_attempt_route_snapshot:
        Option<NodeReplicationGapSyncRouteSnapshot>,
    pub storage_challenge_network_degraded_height: Option<u64>,
    pub storage_challenge_network_degraded_reason: Option<String>,
    pub storage_challenge_network_last_probe_at_ms: Option<i64>,
    pub storage_challenge_network_next_probe_after_ms: Option<i64>,
    pub known_peer_heads: usize,
    pub peer_heads: Vec<NodePeerCommittedHead>,
    pub inbound_rejected_proposal_future_slot: u64,
    pub inbound_rejected_proposal_stale_slot: u64,
    pub inbound_rejected_attestation_future_slot: u64,
    pub inbound_rejected_attestation_stale_slot: u64,
    pub inbound_rejected_attestation_epoch_mismatch: u64,
    pub last_inbound_timing_reject_reason: Option<String>,
    pub pending_proposal: Option<NodePendingProposalSnapshot>,
    pub pending_consensus_actions: NodePendingConsensusActionsSnapshot,
    pub recent_finality_latency: NodeFinalityLatencySnapshot,
    pub last_status: Option<PosConsensusStatus>,
    pub last_block_hash: Option<String>,
    pub last_execution_height: u64,
    pub last_execution_block_hash: Option<String>,
    pub last_execution_state_root: Option<String>,
    pub validator_stakes: BTreeMap<String, u64>,
    pub required_stake: u64,
    pub total_stake: u64,
    pub validator_set_hash: String,
    pub validator_stake_root: String,
    pub validator_stake_proofs: Vec<NodeValidatorStakeProofSnapshot>,
    pub misbehavior_evidence: Vec<NodeConsensusMisbehaviorEvidenceSnapshot>,
    pub slashing_intents: Vec<NodeConsensusSlashingIntentSnapshot>,
    pub slashing_receipts: Vec<NodeConsensusSlashingReceiptSnapshot>,
    pub quarantined_validators: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NodePeerCommittedHead {
    pub node_id: String,
    pub validator_id: Option<String>,
    pub height: u64,
    pub block_hash: String,
    pub committed_at_ms: i64,
    pub observed_at_ms: i64,
    pub execution_block_hash: Option<String>,
    pub execution_state_root: Option<String>,
}

impl Default for NodeConsensusSnapshot {
    fn default() -> Self {
        Self {
            mode: NodeConsensusMode::Pos,
            slot: 0,
            epoch: 0,
            ticks_per_slot: 1,
            tick_phase: 0,
            proposal_tick_phase: 0,
            last_observed_slot: 0,
            missed_slot_count: 0,
            last_observed_tick: 0,
            missed_tick_count: 0,
            adaptive_tick_scheduler_enabled: false,
            latest_height: 0,
            committed_height: 0,
            last_committed_at_ms: None,
            network_committed_height: 0,
            replication_persisted_height: 0,
            replication_gap_sync_blocked_height: None,
            replication_gap_sync_blocked_reason: None,
            replication_gap_sync_repair_attempt_height: None,
            replication_gap_sync_repair_attempt_summary: None,
            replication_gap_sync_repair_attempt_route_snapshot: None,
            storage_challenge_network_degraded_height: None,
            storage_challenge_network_degraded_reason: None,
            storage_challenge_network_last_probe_at_ms: None,
            storage_challenge_network_next_probe_after_ms: None,
            known_peer_heads: 0,
            peer_heads: Vec::new(),
            inbound_rejected_proposal_future_slot: 0,
            inbound_rejected_proposal_stale_slot: 0,
            inbound_rejected_attestation_future_slot: 0,
            inbound_rejected_attestation_stale_slot: 0,
            inbound_rejected_attestation_epoch_mismatch: 0,
            last_inbound_timing_reject_reason: None,
            pending_proposal: None,
            pending_consensus_actions: NodePendingConsensusActionsSnapshot::default(),
            recent_finality_latency: NodeFinalityLatencySnapshot::default(),
            last_status: None,
            last_block_hash: None,
            last_execution_height: 0,
            last_execution_block_hash: None,
            last_execution_state_root: None,
            validator_stakes: BTreeMap::new(),
            required_stake: 0,
            total_stake: 0,
            validator_set_hash: String::new(),
            validator_stake_root: String::new(),
            validator_stake_proofs: Vec::new(),
            misbehavior_evidence: Vec::new(),
            slashing_intents: Vec::new(),
            slashing_receipts: Vec::new(),
            quarantined_validators: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSnapshot {
    pub node_id: String,
    pub player_id: String,
    pub world_id: String,
    pub role: NodeRole,
    pub replication_enabled: bool,
    pub running: bool,
    pub tick_count: u64,
    pub last_tick_unix_ms: Option<i64>,
    pub consensus: NodeConsensusSnapshot,
    pub consensus_progress_observer_error: Option<NodeConsensusProgressObserverError>,
    pub last_error: Option<String>,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
