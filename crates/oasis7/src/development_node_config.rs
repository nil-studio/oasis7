//! Strict engineering configuration shared by the runtime and its callers.
//! This declaration carries no formal authority, deployment evidence or argv.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentNodeConfigV1 {
    pub api_version: String,
    pub working_directory: PathBuf,
    pub runtime: DevelopmentRuntimeConfigV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_provider: Option<DevelopmentLocalProviderV1>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentRuntimeConfigV1 {
    pub node_id: String,
    pub world_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_bind: Option<std::net::SocketAddr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_profile: Option<oasis7_proto::storage_profile::StorageProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traffic_profile: Option<DevelopmentTrafficProfileV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_role: Option<DevelopmentNodeRoleV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_user_mode: Option<DevelopmentUserModeV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_accept_public_entry: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_detected_reachability: Option<oasis7_proto::distributed_dht::PeerReachabilityClass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_detected_hole_punch_viability: Option<DevelopmentHolePunchV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_detected_relay_available: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_detected_probe_stable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_deployment_mode: Option<oasis7_proto::distributed_dht::PeerDeploymentMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_node_role: Option<oasis7_proto::distributed_dht::PeerNodeRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_source_operator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_source_asn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_max_ipv4_subnet_active_peers: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_tick_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos_slot_duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos_ticks_per_slot: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos_proposal_tick_phase: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos_adaptive_tick_scheduler_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos_slot_clock_genesis_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos_max_past_slot_lag: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_auto_attest_all_validators: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_gossip_bind: Option<std::net::SocketAddr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_bridge_state_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_world_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_records_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_schedule_recipe_history: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_local_standalone_test: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replication_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reward_runtime_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reward_runtime_signer_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reward_runtime_epoch_duration_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reward_points_per_credit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reward_runtime_auto_redeem: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reward_initial_reserve_power_units: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reward_distfs_probe: Option<DevelopmentDistfsProbeV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_validators: Vec<DevelopmentValidatorV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_validator_signer_public_keys: Vec<DevelopmentValidatorSignerV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_gossip_peers: Vec<std::net::SocketAddr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replication_network_listen_addrs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replication_network_bootstrap_peers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replication_fetch_requester_public_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_backed_bootstrap_authority_paths: Vec<PathBuf>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentValidatorV1 {
    pub validator_id: String,
    pub stake: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentValidatorSignerV1 {
    pub validator_id: String,
    pub public_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentLocalProviderV1 {
    pub authority_path: PathBuf,
    pub wasm_path: PathBuf,
    pub metadata_path: PathBuf,
    pub agent_id: String,
    pub owner_binding: String,
    pub finality_block_hash: String,
    pub session_mode: DevelopmentProviderSessionV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funding_lane: Option<DevelopmentFundingLaneV1>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevelopmentNodeRoleV1 {
    Sequencer,
    Storage,
    Observer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevelopmentUserModeV1 {
    AutoJoin,
    PrivateSafe,
    PublicEntry,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevelopmentHolePunchV1 {
    Unknown,
    Viable,
    Blocked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevelopmentTrafficProfileV1 {
    Default,
    TriadLowTraffic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevelopmentProviderSessionV1 {
    HostedPublicJoin,
    Loopback,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevelopmentFundingLaneV1 {
    HostedLocalMock,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentDistfsProbeV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_sample_bytes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenges_per_tick: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge_ttl_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_clock_skew_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_checks_per_round: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_backoff_base_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_backoff_max_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_multiplier_hash_mismatch: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_multiplier_missing_sample: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_multiplier_timeout: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_multiplier_read_io_error: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_multiplier_signature_invalid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_multiplier_unknown: Option<u32>,
}
