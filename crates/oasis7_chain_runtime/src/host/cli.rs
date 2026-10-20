#[path = "cli_contract.rs"]
pub(super) mod cli_contract;
use cli_contract::parse_required_value;
#[path = "cli_provider_validation.rs"]
mod provider_validation;
use provider_validation::validate_local_test_provider_options;

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, BufReader, Read};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use oasis7::chain_pos_defaults;
use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7_node::{
    NodeAutoNatStatus, NodeHolePunchViability, NodePublicPortReachability,
    NodeReachabilityAutoDetection, NodeRole, NodeUserMode, PosValidator,
};
use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole, PeerReachabilityClass};
use oasis7_proto::storage_profile::StorageProfile;
use oasis7_runtime::RewardAssetConfig;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::distfs_probe_runtime::{DistfsProbeRuntimeConfig, parse_distfs_probe_runtime_option};

#[path = "cli_help.rs"]
pub(super) mod cli_help;
pub(super) use cli_help::print_help;
#[path = "cli_default.rs"]
mod cli_default;

pub(super) const DEFAULT_NODE_ID: &str = "viewer-live-node";
pub(super) const DEFAULT_WORLD_ID: &str = "oasis7-unified-world-v1";
pub(super) const DEFAULT_STATUS_BIND: &str = "127.0.0.1:5121";
pub(super) const DEFAULT_CONFIG_FILE: &str = "config.toml";
pub(super) const DEFAULT_REPLICATION_NETWORK_LISTEN: &str = "/ip4/127.0.0.1/tcp/0";
pub(super) const DEFAULT_NODE_TICK_MS: u64 = 200;
pub(super) const DEFAULT_REWARD_RUNTIME_STATE_FILE: &str = "reward-runtime-state.json";
pub(super) const DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_STATE_FILE: &str =
    "reward-runtime-distfs-probe-state.json";
pub(super) const DEFAULT_REWARD_RUNTIME_REPORT_DIR: &str = "reward-runtime-report";
pub(super) const DEFAULT_REWARD_RUNTIME_STORAGE_METRICS_FILE: &str =
    "reward-runtime-storage-metrics.json";
pub(super) const DEFAULT_REWARD_RUNTIME_RESERVE_UNITS: i64 = 100_000;
pub(super) const DEFAULT_LOCAL_TEST_PROVIDER_AGENT_ID: &str = "starter-agent-0";
pub(super) const DEFAULT_LOCAL_TEST_PROVIDER_OWNER_BINDING: &str = "local-test-owner-0";
pub(super) const DEFAULT_LOCAL_TEST_PROVIDER_SESSION_MODE: &str = "hosted_public_join";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TrafficProfile {
    Default,
    TriadLowTraffic,
}

impl TrafficProfile {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::TriadLowTraffic => "triad_low_traffic",
        }
    }
}

impl fmt::Display for TrafficProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TrafficProfile {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "default" => Ok(Self::Default),
            "triad_low_traffic" => Ok(Self::TriadLowTraffic),
            _ => Err("traffic profile must be one of: default, triad_low_traffic".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CliOptions {
    pub node_id: String,
    pub world_id: String,
    pub status_bind: String,
    pub guarded_initial_config: Option<PathBuf>,
    pub formal_live_config: Option<PathBuf>,
    pub development_runtime: bool,
    pub storage_profile: StorageProfile,
    pub traffic_profile: TrafficProfile,
    pub node_role: NodeRole,
    pub p2p_user_mode: NodeUserMode,
    pub p2p_accept_public_entry: bool,
    pub p2p_detected_reachability: Option<PeerReachabilityClass>,
    pub p2p_detected_hole_punch_viability: NodeHolePunchViability,
    pub p2p_detected_relay_available: bool,
    pub p2p_detected_probe_stable: bool,
    pub p2p_deployment_mode: PeerDeploymentMode,
    pub p2p_node_role: PeerNodeRole,
    pub p2p_source_operator: Option<String>,
    pub p2p_source_asn: Option<String>,
    pub p2p_max_ipv4_subnet_active_peers: Option<usize>,
    pub node_tick_ms: u64,
    pub pos_slot_duration_ms: u64,
    pub pos_ticks_per_slot: u64,
    pub pos_proposal_tick_phase: u64,
    pub pos_adaptive_tick_scheduler_enabled: bool,
    pub pos_slot_clock_genesis_unix_ms: Option<i64>,
    pub pos_max_past_slot_lag: u64,
    pub node_auto_attest_all_validators: bool,
    pub node_validators: Vec<PosValidator>,
    pub node_validator_signer_public_keys: BTreeMap<String, String>,
    pub node_gossip_bind: Option<SocketAddr>,
    pub node_gossip_peers: Vec<SocketAddr>,
    pub replication_network_listen_addrs: Vec<String>,
    pub replication_network_bootstrap_peers: Vec<String>,
    pub replication_fetch_requester_public_keys: Vec<String>,
    pub network_tier_manifest_path: Option<PathBuf>,
    pub loaded_network_tier_manifest: Option<LoadedNetworkTierManifest>,
    pub genesis_validator_registry_path: Option<PathBuf>,
    pub deployment_inventory_path: Option<PathBuf>,
    pub config_path: String,
    pub runtime_root: Option<PathBuf>,
    pub execution_bridge_state_path: Option<PathBuf>,
    pub execution_world_dir: Option<PathBuf>,
    pub execution_records_dir: Option<PathBuf>,
    pub capture_schedule_recipe_history: bool,
    pub provider_backed_bootstrap_authority_paths: Vec<PathBuf>,
    pub local_test_provider_authority_path: Option<PathBuf>,
    pub local_test_provider_wasm_path: Option<PathBuf>,
    pub local_test_provider_metadata_path: Option<PathBuf>,
    pub local_test_provider_agent_id: String,
    pub local_test_provider_owner_binding: String,
    pub local_test_provider_finality_block_hash: Option<String>,
    pub local_test_provider_session_mode: String,
    pub chain_local_standalone_test: bool,
    pub agent_decision_source: Option<String>,
    pub agent_provider_backend: Option<String>,
    pub agent_provider_contract: Option<String>,
    pub agent_provider_transport: Option<String>,
    pub agent_execution_lane: Option<String>,
    pub storage_root: Option<PathBuf>,
    pub replication_root: Option<PathBuf>,
    pub reward_runtime_enabled: bool,
    pub reward_runtime_signer_node_id: Option<String>,
    pub reward_runtime_epoch_duration_secs: Option<u64>,
    pub reward_points_per_credit: u64,
    pub reward_runtime_auto_redeem: bool,
    pub reward_initial_reserve_power_units: i64,
    pub reward_distfs_probe_config: DistfsProbeRuntimeConfig,
    pub p2p_detected_reachability_explicit: bool,
    pub p2p_detected_hole_punch_viability_explicit: bool,
    pub p2p_detected_relay_available_explicit: bool,
    pub p2p_detected_probe_stable_explicit: bool,
    pub p2p_deployment_mode_explicit: bool,
    pub p2p_node_role_explicit: bool,
}

pub(super) fn parse_options<'a>(args: impl Iterator<Item = &'a str>) -> Result<CliOptions, String> {
    let mut options = CliOptions::default();
    let mut iter = args.peekable();
    let mut seen = std::collections::BTreeSet::new();
    let mut explicit = std::collections::BTreeSet::new();

    while let Some(arg) = iter.next() {
        let spec = cli_contract::option(arg)?;
        explicit.insert(spec.name);
        cli_contract::check_option_occurrence(spec, &mut seen)?;
        cli_contract::check_required_value(spec, iter.peek().copied())?;
        let arg = spec.name;
        match arg {
            "--node-id" => options.node_id = parse_required_value(&mut iter, "--node-id")?,
            "--world-id" => options.world_id = parse_required_value(&mut iter, "--world-id")?,
            "--guarded-initial-config" => {
                options.guarded_initial_config = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--guarded-initial-config",
                )?));
            }
            "--formal-live-config" => {
                options.formal_live_config = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--formal-live-config",
                )?));
            }
            "--status-bind" => {
                options.status_bind = parse_required_value(&mut iter, "--status-bind")?;
            }
            "--storage-profile" => {
                options.storage_profile = parse_required_value(&mut iter, "--storage-profile")?
                    .parse::<StorageProfile>()?;
            }
            "--traffic-profile" => {
                options.traffic_profile = parse_required_value(&mut iter, "--traffic-profile")?
                    .parse::<TrafficProfile>()?;
            }
            "--node-role" => {
                let raw = parse_required_value(&mut iter, "--node-role")?;
                options.node_role = raw.parse::<NodeRole>().map_err(|_| {
                    "--node-role must be one of: sequencer, storage, observer".to_string()
                })?;
                if !options.p2p_node_role_explicit {
                    options.p2p_node_role = default_p2p_node_role(options.node_role);
                }
            }
            "--p2p-user-mode" => {
                let raw = parse_required_value(&mut iter, "--p2p-user-mode")?;
                options.p2p_user_mode = raw.parse::<NodeUserMode>()?;
            }
            "--p2p-accept-public-entry" => {
                options.p2p_accept_public_entry = true;
            }
            "--p2p-reject-public-entry" => {
                options.p2p_accept_public_entry = false;
            }
            "--p2p-detected-reachability" => {
                let raw = parse_required_value(&mut iter, "--p2p-detected-reachability")?;
                options.p2p_detected_reachability =
                    Some(parse_peer_reachability_class(raw.as_str())?);
                options.p2p_detected_reachability_explicit = true;
            }
            "--p2p-clear-detected-reachability" => {
                options.p2p_detected_reachability = None;
                options.p2p_detected_reachability_explicit = false;
            }
            "--p2p-detected-hole-punch" => {
                let raw = parse_required_value(&mut iter, "--p2p-detected-hole-punch")?;
                options.p2p_detected_hole_punch_viability =
                    raw.parse::<NodeHolePunchViability>()?;
                options.p2p_detected_hole_punch_viability_explicit = true;
            }
            "--p2p-detected-relay-available" => {
                options.p2p_detected_relay_available = true;
                options.p2p_detected_relay_available_explicit = true;
            }
            "--p2p-detected-relay-unavailable" => {
                options.p2p_detected_relay_available = false;
                options.p2p_detected_relay_available_explicit = true;
            }
            "--p2p-detected-probe-stable" => {
                options.p2p_detected_probe_stable = true;
                options.p2p_detected_probe_stable_explicit = true;
            }
            "--p2p-detected-probe-unstable" => {
                options.p2p_detected_probe_stable = false;
                options.p2p_detected_probe_stable_explicit = true;
            }
            "--p2p-deployment-mode" => {
                let raw = parse_required_value(&mut iter, "--p2p-deployment-mode")?;
                options.p2p_deployment_mode = raw.parse::<PeerDeploymentMode>()?;
                options.p2p_deployment_mode_explicit = true;
            }
            "--p2p-node-role" => {
                let raw = parse_required_value(&mut iter, "--p2p-node-role")?;
                options.p2p_node_role = raw.parse::<PeerNodeRole>()?;
                options.p2p_node_role_explicit = true;
            }
            "--p2p-source-operator" => {
                let raw = parse_required_value(&mut iter, "--p2p-source-operator")?;
                options.p2p_source_operator = Some(normalize_p2p_metadata_label(
                    raw.as_str(),
                    "--p2p-source-operator",
                )?);
            }
            "--p2p-source-asn" => {
                let raw = parse_required_value(&mut iter, "--p2p-source-asn")?;
                options.p2p_source_asn = Some(normalize_p2p_metadata_label(
                    raw.as_str(),
                    "--p2p-source-asn",
                )?);
            }
            "--p2p-max-ipv4-subnet-active-peers" => {
                let raw = parse_required_value(&mut iter, "--p2p-max-ipv4-subnet-active-peers")?;
                options.p2p_max_ipv4_subnet_active_peers = Some(
                    raw.parse::<usize>()
                        .ok()
                        .filter(|value| *value > 0)
                        .ok_or_else(|| {
                            "--p2p-max-ipv4-subnet-active-peers requires a positive integer"
                                .to_string()
                        })?,
                );
            }
            "--node-tick-ms" => {
                let raw = parse_required_value(&mut iter, "--node-tick-ms")?;
                options.node_tick_ms = raw
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| "--node-tick-ms requires a positive integer".to_string())?;
            }
            "--pos-slot-duration-ms" => {
                let raw = parse_required_value(&mut iter, "--pos-slot-duration-ms")?;
                options.pos_slot_duration_ms = raw
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                    "--pos-slot-duration-ms requires a positive integer".to_string()
                })?;
            }
            "--pos-ticks-per-slot" => {
                let raw = parse_required_value(&mut iter, "--pos-ticks-per-slot")?;
                options.pos_ticks_per_slot = raw
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        "--pos-ticks-per-slot requires a positive integer".to_string()
                    })?;
            }
            "--pos-proposal-tick-phase" => {
                let raw = parse_required_value(&mut iter, "--pos-proposal-tick-phase")?;
                options.pos_proposal_tick_phase = raw.parse::<u64>().map_err(|_| {
                    "--pos-proposal-tick-phase requires a non-negative integer".to_string()
                })?;
            }
            "--pos-adaptive-tick-scheduler" => options.pos_adaptive_tick_scheduler_enabled = true,
            "--pos-no-adaptive-tick-scheduler" => {
                options.pos_adaptive_tick_scheduler_enabled = false;
            }
            "--pos-slot-clock-genesis-unix-ms" => {
                let raw = parse_required_value(&mut iter, "--pos-slot-clock-genesis-unix-ms")?;
                options.pos_slot_clock_genesis_unix_ms =
                    Some(raw.parse::<i64>().map_err(|_| {
                        "--pos-slot-clock-genesis-unix-ms requires an integer".to_string()
                    })?);
            }
            "--pos-max-past-slot-lag" => {
                let raw = parse_required_value(&mut iter, "--pos-max-past-slot-lag")?;
                options.pos_max_past_slot_lag = raw.parse::<u64>().map_err(|_| {
                    "--pos-max-past-slot-lag requires a non-negative integer".to_string()
                })?;
            }
            "--node-validator" => {
                let raw = parse_required_value(&mut iter, "--node-validator")?;
                let validator = parse_validator_spec(raw.as_str())?;
                if options
                    .node_validators
                    .iter()
                    .any(|existing| existing.validator_id == validator.validator_id)
                {
                    return Err(format!(
                        "duplicate validator identity: {}",
                        validator.validator_id
                    ));
                }
                options.node_validators.push(validator);
            }
            "--node-validator-signer-public-key" => {
                let raw = parse_required_value(&mut iter, "--node-validator-signer-public-key")?;
                let (validator_id, public_key_hex) =
                    parse_validator_signer_public_key_spec(raw.as_str())?;
                if options
                    .node_validator_signer_public_keys
                    .contains_key(&validator_id)
                {
                    return Err(format!(
                        "duplicate validator signer binding: {validator_id}"
                    ));
                }
                options
                    .node_validator_signer_public_keys
                    .insert(validator_id, public_key_hex);
            }
            "--node-auto-attest-all" => options.node_auto_attest_all_validators = true,
            "--node-no-auto-attest-all" => options.node_auto_attest_all_validators = false,
            "--node-gossip-bind" => {
                let raw = parse_required_value(&mut iter, "--node-gossip-bind")?;
                options.node_gossip_bind =
                    Some(parse_socket_addr(raw.as_str(), "--node-gossip-bind")?);
            }
            "--node-gossip-peer" => {
                let raw = parse_required_value(&mut iter, "--node-gossip-peer")?;
                options
                    .node_gossip_peers
                    .push(parse_socket_addr(raw.as_str(), "--node-gossip-peer")?);
            }
            "--replication-network-listen" => {
                let raw = parse_required_value(&mut iter, "--replication-network-listen")?;
                options.replication_network_listen_addrs.push(raw);
            }
            "--replication-network-peer" => {
                let raw = parse_required_value(&mut iter, "--replication-network-peer")?;
                options.replication_network_bootstrap_peers.push(raw);
            }
            "--replication-fetch-requester-public-key" => {
                let raw =
                    parse_required_value(&mut iter, "--replication-fetch-requester-public-key")?;
                options.replication_fetch_requester_public_keys.push(raw);
            }
            "--network-tier-manifest" => {
                let raw = parse_required_value(&mut iter, "--network-tier-manifest")?;
                options.network_tier_manifest_path = Some(PathBuf::from(raw));
            }
            "--genesis-validator-registry" => {
                let raw = parse_required_value(&mut iter, "--genesis-validator-registry")?;
                options.genesis_validator_registry_path = Some(PathBuf::from(raw));
            }
            "--deployment-inventory" => {
                let raw = parse_required_value(&mut iter, "--deployment-inventory")?;
                options.deployment_inventory_path = Some(PathBuf::from(raw));
            }
            "--config" => options.config_path = parse_required_value(&mut iter, "--config")?,
            "--runtime-root" => {
                let raw = parse_required_value(&mut iter, "--runtime-root")?;
                options.runtime_root = Some(PathBuf::from(raw));
            }
            "--execution-bridge-state" => {
                let raw = parse_required_value(&mut iter, "--execution-bridge-state")?;
                options.execution_bridge_state_path = Some(PathBuf::from(raw));
            }
            "--execution-world-dir" => {
                let raw = parse_required_value(&mut iter, "--execution-world-dir")?;
                options.execution_world_dir = Some(PathBuf::from(raw));
            }
            "--capture-schedule-recipe-history" => {
                if !cfg!(unix) {
                    return Err("durable history capture is unsupported on this platform".into());
                }
                if !cfg!(feature = "wasmtime") {
                    return Err("history capture requires wasmtime feature".into());
                }
                options.capture_schedule_recipe_history = true;
            }
            "--execution-records-dir" => {
                let raw = parse_required_value(&mut iter, "--execution-records-dir")?;
                options.execution_records_dir = Some(PathBuf::from(raw));
            }
            "--provider-bootstrap-authority" => {
                let raw = parse_required_value(&mut iter, "--provider-bootstrap-authority")?;
                options
                    .provider_backed_bootstrap_authority_paths
                    .push(PathBuf::from(raw));
            }
            "--local-test-provider-authority" => {
                options.local_test_provider_authority_path = Some(PathBuf::from(
                    parse_required_value(&mut iter, "--local-test-provider-authority")?,
                ));
            }
            "--local-test-provider-wasm" => {
                options.local_test_provider_wasm_path = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--local-test-provider-wasm",
                )?));
            }
            "--local-test-provider-metadata" => {
                options.local_test_provider_metadata_path = Some(PathBuf::from(
                    parse_required_value(&mut iter, "--local-test-provider-metadata")?,
                ));
            }
            "--local-test-provider-agent-id" => {
                options.local_test_provider_agent_id =
                    parse_required_value(&mut iter, "--local-test-provider-agent-id")?;
            }
            "--local-test-provider-owner-binding" => {
                options.local_test_provider_owner_binding =
                    parse_required_value(&mut iter, "--local-test-provider-owner-binding")?;
            }
            "--local-test-provider-finality-block-hash" => {
                options.local_test_provider_finality_block_hash = Some(parse_required_value(
                    &mut iter,
                    "--local-test-provider-finality-block-hash",
                )?);
            }
            "--local-test-provider-session-mode" => {
                options.local_test_provider_session_mode =
                    parse_required_value(&mut iter, "--local-test-provider-session-mode")?;
            }
            "--chain-local-standalone-test" => options.chain_local_standalone_test = true,
            "--agent-decision-source" => {
                options.agent_decision_source =
                    Some(parse_required_value(&mut iter, "--agent-decision-source")?);
            }
            "--agent-provider-backend" => {
                options.agent_provider_backend =
                    Some(parse_required_value(&mut iter, "--agent-provider-backend")?);
            }
            "--agent-provider-contract" => {
                options.agent_provider_contract = Some(parse_required_value(
                    &mut iter,
                    "--agent-provider-contract",
                )?);
            }
            "--agent-provider-transport" => {
                options.agent_provider_transport = Some(parse_required_value(
                    &mut iter,
                    "--agent-provider-transport",
                )?);
            }
            "--agent-execution-lane" => {
                options.agent_execution_lane =
                    Some(parse_required_value(&mut iter, "--agent-execution-lane")?);
            }
            "--storage-root" => {
                let raw = parse_required_value(&mut iter, "--storage-root")?;
                options.storage_root = Some(PathBuf::from(raw));
            }
            "--replication-root" => {
                let raw = parse_required_value(&mut iter, "--replication-root")?;
                options.replication_root = Some(PathBuf::from(raw));
            }
            "--reward-runtime-enable" => options.reward_runtime_enabled = true,
            "--reward-runtime-disable" => options.reward_runtime_enabled = false,
            "--reward-runtime-signer-node-id" => {
                options.reward_runtime_signer_node_id = Some(parse_required_value(
                    &mut iter,
                    "--reward-runtime-signer-node-id",
                )?);
            }
            "--reward-runtime-epoch-duration-secs" => {
                let raw = parse_required_value(&mut iter, "--reward-runtime-epoch-duration-secs")?;
                let value = raw.parse::<u64>().ok().filter(|v| *v > 0).ok_or_else(|| {
                    "--reward-runtime-epoch-duration-secs requires a positive integer".to_string()
                })?;
                options.reward_runtime_epoch_duration_secs = Some(value);
            }
            "--reward-points-per-credit" => {
                let raw = parse_required_value(&mut iter, "--reward-points-per-credit")?;
                options.reward_points_per_credit =
                    raw.parse::<u64>().ok().filter(|v| *v > 0).ok_or_else(|| {
                        "--reward-points-per-credit requires a positive integer".to_string()
                    })?;
            }
            "--reward-runtime-auto-redeem" => options.reward_runtime_auto_redeem = true,
            "--reward-runtime-no-auto-redeem" => options.reward_runtime_auto_redeem = false,
            "--reward-initial-reserve-power-units" => {
                let raw = parse_required_value(&mut iter, "--reward-initial-reserve-power-units")?;
                options.reward_initial_reserve_power_units = raw
                    .parse::<i64>()
                    .ok()
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| {
                        "--reward-initial-reserve-power-units requires a non-negative integer"
                            .to_string()
                    })?;
            }
            _ => {
                if parse_distfs_probe_runtime_option(
                    arg,
                    &mut iter,
                    &mut options.reward_distfs_probe_config,
                )? {
                    continue;
                }
                return Err(format!("unknown option: {arg}"));
            }
        }
    }

    if let Some(option) = options.formal_live_config.as_ref().and_then(|_| {
        seen.iter()
            .find(|option| option.as_str() != "--formal-live-config")
    }) {
        return Err(format!(
            "{option} cannot be combined with --formal-live-config; runtime settings come from the formal profile"
        ));
    }

    if options.guarded_initial_config.is_some()
        && let Some(flag) = explicit
            .iter()
            .find(|flag| !["--guarded-initial-config", "--reward-runtime-disable"].contains(flag))
    {
        return Err(format!(
            "{flag} is inapplicable to guarded engineering; use dev guarded-world --config PATH"
        ));
    }

    let provider_explicit = explicit
        .iter()
        .any(|flag| flag.starts_with("--local-test-provider-") || flag.starts_with("--agent-"));
    if provider_explicit && options.local_test_provider_authority_path.is_none() {
        return Err("explicit local-provider/agent engineering options require complete provider setup; use dev local-provider --config PATH".into());
    }
    validate_effective_options(&options)?;
    if options.network_tier_manifest_path.is_some() && options.node_auto_attest_all_validators {
        return Err("formal network-tier startup rejects synthetic auto-attestation; use dev node --config for engineering fixtures".into());
    }
    if let Some(manifest_path) = options.network_tier_manifest_path.as_ref() {
        let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path())?;
        loaded.validate_runtime_support()?;
        validate_current_runtime_hash_against_network_tier_bundle(
            manifest_path.as_path(),
            &loaded,
        )?;
        options.loaded_network_tier_manifest = Some(loaded);
    }

    if options.local_test_provider_authority_path.is_some()
        || options.node_auto_attest_all_validators
        || options.capture_schedule_recipe_history
        || options.chain_local_standalone_test
        || (options.network_tier_manifest_path.is_none()
            && options.formal_live_config.is_none()
            && options.guarded_initial_config.is_none()
            && options.genesis_validator_registry_path.is_none()
            && options.deployment_inventory_path.is_none())
        || (options.deployment_inventory_path.is_some()
            && options.network_tier_manifest_path.is_none())
    {
        let typed = super::development_runtime::from_options(&options)?;
        super::development_runtime::into_options(typed, false)?;
        options.development_runtime = true;
        return Ok(options);
    }
    Ok(options)
}

pub(super) fn validate_effective_options(options: &CliOptions) -> Result<(), String> {
    parse_host_port(options.status_bind.as_str(), "--status-bind")?;
    if options.node_id.trim().is_empty() {
        return Err("--node-id requires a non-empty value".to_string());
    }
    if options.world_id.trim().is_empty() {
        return Err("--world-id requires a non-empty value".to_string());
    }
    if options.config_path.trim().is_empty() {
        return Err("--config requires a non-empty value".to_string());
    }
    if options.reward_points_per_credit == 0 {
        return Err("--reward-points-per-credit requires a positive integer".to_string());
    }
    if options.reward_initial_reserve_power_units < 0 {
        return Err("--reward-initial-reserve-power-units requires a non-negative integer".into());
    }
    if options.pos_proposal_tick_phase >= options.pos_ticks_per_slot {
        return Err(format!(
            "--pos-proposal-tick-phase={} must be less than --pos-ticks-per-slot={}",
            options.pos_proposal_tick_phase, options.pos_ticks_per_slot
        ));
    }
    if !options.node_gossip_peers.is_empty() && options.node_gossip_bind.is_none() {
        return Err("--node-gossip-peer requires --node-gossip-bind".to_string());
    }
    if options.capture_schedule_recipe_history
        && !super::execution_role::node_role_materializes_execution_state(options.node_role)
    {
        return Err("history capture requires an execution-materializing node role".into());
    }
    validate_local_test_provider_options(options)?;
    if options.node_tick_ms == 0
        || options.pos_slot_duration_ms == 0
        || options.pos_ticks_per_slot == 0
        || options.reward_runtime_epoch_duration_secs == Some(0)
        || options.p2p_max_ipv4_subnet_active_peers == Some(0)
    {
        return Err("runtime timing/budget fields must be positive".into());
    }
    Ok(())
}

fn valid_blake3_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Deserialize)]
struct NetworkTierReleaseCandidateBundle {
    runtime_build: NetworkTierRuntimeBuildRef,
    generated_world_sidecar: Option<NetworkTierArtifactRef>,
    world_generation_provenance: Option<NetworkTierArtifactRef>,
}

#[derive(Debug, Deserialize)]
struct NetworkTierRuntimeBuildRef {
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct NetworkTierArtifactRef {
    #[serde(default)]
    resolved_path: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    sha256_tree: Option<String>,
    #[serde(default)]
    file_count: Option<u64>,
    #[serde(default)]
    total_bytes: Option<u64>,
}

/// Inspect declared attachment integrity for preserving backups. This does not
/// establish chain authority or change the runtime bundle's interpretation.
pub(crate) fn inspect_network_tier_bundle_attachments(
    bundle_path: &Path,
) -> Result<Vec<(String, PathBuf)>, String> {
    let bundle: NetworkTierReleaseCandidateBundle =
        serde_json::from_slice(&fs::read(bundle_path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("invalid release bundle: {error}"))?;
    let mut attachments = Vec::new();
    for (label, artifact, kind) in [
        (
            "generated_world_sidecar",
            bundle.generated_world_sidecar.as_ref(),
            "directory",
        ),
        (
            "world_generation_provenance",
            bundle.world_generation_provenance.as_ref(),
            "file",
        ),
    ] {
        if let Some(artifact) = artifact {
            let path = Path::new(&artifact.resolved_path);
            if !path.is_absolute() {
                return Err(format!(
                    "release bundle {label}.resolved_path must be absolute"
                ));
            }
            validate_network_tier_bundle_artifact(bundle_path, label, artifact, kind)?;
            attachments.push((label.to_owned(), path.to_path_buf()));
        }
    }
    Ok(attachments)
}

fn validate_current_runtime_hash_against_network_tier_bundle(
    manifest_path: &Path,
    loaded: &LoadedNetworkTierManifest,
) -> Result<(), String> {
    let (bundle_path, expected_sha256) =
        approved_runtime_sha256_for_network_tier_bundle(manifest_path, loaded)?;
    let current_exe = std::env::current_exe()
        .map_err(|err| format!("resolve current runtime executable path failed: {err}"))?;
    let actual_sha256 = sha256_file_hex(current_exe.as_path()).map_err(|err| {
        format!(
            "hash current runtime executable {} failed: {err}",
            current_exe.display()
        )
    })?;
    if actual_sha256 != expected_sha256 {
        return Err(format!(
            "network tier runtime bundle hash mismatch: executable={} actual_sha256={} bundle_path={} expected_sha256={}",
            current_exe.display(),
            actual_sha256,
            bundle_path.display(),
            expected_sha256
        ));
    }
    Ok(())
}

/// Return the executable digest from the exact manifest-bound runtime bundle
/// after applying the same attachment integrity checks used by normal startup.
/// Package admission uses this to reject a binary replacement before it stops
/// the currently valid runtime; callers must still preserve the original chain
/// manifest pin when no candidate-authority transaction is present.
pub(super) fn approved_runtime_sha256_for_network_tier_bundle(
    manifest_path: &Path,
    loaded: &LoadedNetworkTierManifest,
) -> Result<(PathBuf, String), String> {
    let bundle_path = resolve_network_tier_runtime_ref_path(
        manifest_path,
        loaded
            .manifest
            .runtime_refs
            .release_candidate_bundle_ref
            .as_str(),
    );
    let bundle_bytes = fs::read(bundle_path.as_path()).map_err(|err| {
        format!(
            "read network tier release_candidate_bundle_ref {} failed: {err}",
            bundle_path.display()
        )
    })?;
    let bundle: NetworkTierReleaseCandidateBundle = serde_json::from_slice(bundle_bytes.as_slice())
        .map_err(|err| {
            format!(
                "parse network tier release_candidate_bundle_ref {} failed: {err}",
                bundle_path.display()
            )
        })?;
    if loaded.manifest.tier == "public_testnet" {
        validate_network_tier_bundle_artifact(
            bundle_path.as_path(),
            "generated_world_sidecar",
            bundle.generated_world_sidecar.as_ref().ok_or_else(|| {
                format!(
                    "network tier release_candidate_bundle_ref {} missing generated_world_sidecar",
                    bundle_path.display()
                )
            })?,
            "directory",
        )?;
        validate_network_tier_bundle_artifact(
            bundle_path.as_path(),
            "world_generation_provenance",
            bundle.world_generation_provenance.as_ref().ok_or_else(|| {
                format!(
                    "network tier release_candidate_bundle_ref {} missing world_generation_provenance",
                    bundle_path.display()
                )
            })?,
            "file",
        )?;
    }
    let expected_sha256 = bundle.runtime_build.sha256.trim().to_ascii_lowercase();
    if expected_sha256.is_empty() {
        return Err(format!(
            "network tier release_candidate_bundle_ref {} missing runtime_build.sha256",
            bundle_path.display()
        ));
    }
    if !is_sha256_hex(expected_sha256.as_str()) {
        return Err(format!(
            "network tier release_candidate_bundle_ref {} has invalid runtime_build.sha256: expected 64 lowercase hex characters",
            bundle_path.display()
        ));
    }

    Ok((bundle_path, expected_sha256))
}

fn validate_network_tier_bundle_artifact(
    bundle_path: &Path,
    label: &str,
    artifact: &NetworkTierArtifactRef,
    expected_kind: &str,
) -> Result<(), String> {
    if artifact.resolved_path.trim().is_empty() {
        return Err(format!(
            "network tier release_candidate_bundle_ref {} missing {label}.resolved_path",
            bundle_path.display()
        ));
    }
    if artifact.kind.trim() != expected_kind {
        return Err(format!(
            "network tier release_candidate_bundle_ref {} has invalid {label}.kind: expected {expected_kind}, got {}",
            bundle_path.display(),
            artifact.kind.trim()
        ));
    }
    let artifact_path = Path::new(artifact.resolved_path.as_str());
    if !artifact_path.exists() {
        return Err(format!(
            "network tier release_candidate_bundle_ref {} {label} path missing: {}",
            bundle_path.display(),
            artifact_path.display()
        ));
    }
    if expected_kind == "directory" {
        for required in ["snapshot.json", "journal.json"] {
            if !artifact_path.join(required).is_file() {
                return Err(format!(
                    "network tier release_candidate_bundle_ref {} {label} missing {required}: {}",
                    bundle_path.display(),
                    artifact_path.display()
                ));
            }
        }
    }
    match expected_kind {
        "file" => {
            let expected_sha256 = artifact.sha256.as_deref().ok_or_else(|| {
                format!(
                    "network tier release_candidate_bundle_ref {} missing {label}.sha256",
                    bundle_path.display()
                )
            })?;
            if !is_sha256_hex(expected_sha256) {
                return Err(format!(
                    "network tier release_candidate_bundle_ref {} has invalid {label}.sha256",
                    bundle_path.display()
                ));
            }
            let actual_sha256 = sha256_file_hex(artifact_path).map_err(|err| {
                format!(
                    "hash network tier release_candidate_bundle_ref {} {label} {} failed: {err}",
                    bundle_path.display(),
                    artifact_path.display()
                )
            })?;
            if actual_sha256 != expected_sha256 {
                return Err(format!(
                    "network tier release_candidate_bundle_ref {} {label} drift detected: bundle={} current={}",
                    bundle_path.display(),
                    expected_sha256,
                    actual_sha256
                ));
            }
        }
        "directory" => {
            let expected_sha256_tree = artifact.sha256_tree.as_deref().ok_or_else(|| {
                format!(
                    "network tier release_candidate_bundle_ref {} missing {label}.sha256_tree",
                    bundle_path.display()
                )
            })?;
            if !is_sha256_hex(expected_sha256_tree) {
                return Err(format!(
                    "network tier release_candidate_bundle_ref {} has invalid {label}.sha256_tree",
                    bundle_path.display()
                ));
            }
            let actual_tree = sha256_dir_tree_hex(artifact_path).map_err(|err| {
                format!(
                    "hash network tier release_candidate_bundle_ref {} {label} {} failed: {err}",
                    bundle_path.display(),
                    artifact_path.display()
                )
            })?;
            if actual_tree.sha256_tree != expected_sha256_tree {
                return Err(format!(
                    "network tier release_candidate_bundle_ref {} {label} drift detected: bundle={} current={}",
                    bundle_path.display(),
                    expected_sha256_tree,
                    actual_tree.sha256_tree
                ));
            }
            if let Some(expected_file_count) = artifact.file_count
                && expected_file_count != actual_tree.file_count
            {
                return Err(format!(
                    "network tier release_candidate_bundle_ref {} {label} file_count drift: bundle={} current={}",
                    bundle_path.display(),
                    expected_file_count,
                    actual_tree.file_count
                ));
            }
            if let Some(expected_total_bytes) = artifact.total_bytes
                && expected_total_bytes != actual_tree.total_bytes
            {
                return Err(format!(
                    "network tier release_candidate_bundle_ref {} {label} total_bytes drift: bundle={} current={}",
                    bundle_path.display(),
                    expected_total_bytes,
                    actual_tree.total_bytes
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn resolve_network_tier_runtime_ref_path(manifest_path: &Path, raw_ref: &str) -> PathBuf {
    let candidate = PathBuf::from(raw_ref);
    if candidate.is_absolute() {
        return candidate;
    }
    manifest_path
        .parent()
        .map(|parent| parent.join(candidate.clone()))
        .unwrap_or(candidate)
}

fn is_sha256_hex(raw: &str) -> bool {
    raw.len() == 64 && raw.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_file_hex(path: &Path) -> io::Result<String> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Sha256DirTree {
    sha256_tree: String,
    file_count: u64,
    total_bytes: u64,
}

fn sha256_dir_tree_hex(path: &Path) -> io::Result<Sha256DirTree> {
    let mut files = Vec::new();
    collect_files(path, &mut files)?;
    files.sort();

    let mut combined = Sha256::new();
    let mut total_bytes = 0_u64;
    for file in files.iter() {
        let rel = file.strip_prefix(path).map_err(io::Error::other)?;
        let rel = rel.to_string_lossy().replace('\\', "/");
        let digest = sha256_file_hex(file.as_path())?;
        let size = fs::metadata(file.as_path())?.len();
        combined.update(rel.as_bytes());
        combined.update(b"\0");
        combined.update(digest.as_bytes());
        combined.update(b"\0");
        combined.update(size.to_string().as_bytes());
        combined.update(b"\n");
        total_bytes += size;
    }
    Ok(Sha256DirTree {
        sha256_tree: hex::encode(combined.finalize()),
        file_count: files.len() as u64,
        total_bytes,
    })
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            collect_files(child.as_path(), files)?;
        } else if child.is_file() {
            files.push(child);
        }
    }
    Ok(())
}

pub(super) fn p2p_auto_detection_from_options(
    options: &CliOptions,
) -> NodeReachabilityAutoDetection {
    NodeReachabilityAutoDetection {
        observed_reachability: options.p2p_detected_reachability,
        hole_punch_viability: options.p2p_detected_hole_punch_viability,
        autonat_status: NodeAutoNatStatus::Unknown,
        public_port_reachability: NodePublicPortReachability::Unknown,
        relay_available: options.p2p_detected_relay_available,
        probe_stable: options.p2p_detected_probe_stable,
    }
}

fn parse_socket_addr(raw: &str, label: &str) -> Result<SocketAddr, String> {
    raw.parse::<SocketAddr>()
        .map_err(|_| format!("{label} requires <addr:port>"))
}

pub(super) fn parse_host_port(raw: &str, label: &str) -> Result<(String, u16), String> {
    let trimmed = raw.trim();
    let (host, port_text) = trimmed
        .rsplit_once(':')
        .ok_or_else(|| format!("{label} must be in <host:port> format"))?;
    if host.trim().is_empty() {
        return Err(format!("{label} host cannot be empty"));
    }
    let port = port_text
        .parse::<u16>()
        .map_err(|_| format!("{label} port must be an integer in 1..=65535"))?;
    if port == 0 {
        return Err(format!("{label} port must be in 1..=65535"));
    }
    Ok((host.trim().to_string(), port))
}

fn parse_peer_reachability_class(raw: &str) -> Result<PeerReachabilityClass, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "public" => Ok(PeerReachabilityClass::Public),
        "hybrid" => Ok(PeerReachabilityClass::Hybrid),
        "private" => Ok(PeerReachabilityClass::Private),
        "relay_only" => Ok(PeerReachabilityClass::RelayOnly),
        "validator_hidden" => Ok(PeerReachabilityClass::ValidatorHidden),
        _ => Err(
            "detected reachability must be one of: public, hybrid, private, relay_only, validator_hidden"
                .to_string(),
        ),
    }
}

pub(super) fn normalize_p2p_metadata_label(raw: &str, flag: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(format!("{flag} requires a non-empty value"));
    }
    Ok(normalized)
}

pub(super) fn parse_validator_spec(raw: &str) -> Result<PosValidator, String> {
    let (validator_id, stake_text) = raw
        .rsplit_once(':')
        .ok_or_else(|| "--node-validator requires <validator_id:stake>".to_string())?;
    let validator_id = validator_id.trim();
    if validator_id.is_empty() {
        return Err("--node-validator validator_id cannot be empty".to_string());
    }
    let stake = stake_text
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| "--node-validator stake must be a positive integer".to_string())?;
    Ok(PosValidator {
        validator_id: validator_id.to_string(),
        stake,
    })
}

pub(super) fn parse_validator_signer_public_key_spec(
    raw: &str,
) -> Result<(String, String), String> {
    let (validator_id, public_key_hex) = raw.rsplit_once(':').ok_or_else(|| {
        "--node-validator-signer-public-key requires <validator_id:public_key_hex>".to_string()
    })?;
    let validator_id = validator_id.trim();
    if validator_id.is_empty() {
        return Err("--node-validator-signer-public-key validator_id cannot be empty".to_string());
    }
    let public_key_hex = public_key_hex.trim();
    if public_key_hex.is_empty() {
        return Err(
            "--node-validator-signer-public-key public_key_hex cannot be empty".to_string(),
        );
    }
    Ok((validator_id.to_string(), public_key_hex.to_string()))
}

pub(super) fn default_p2p_node_role(node_role: NodeRole) -> PeerNodeRole {
    match node_role {
        NodeRole::Sequencer => PeerNodeRole::ValidatorCore,
        NodeRole::Storage => PeerNodeRole::FullStorage,
        NodeRole::Observer => PeerNodeRole::ObserverLight,
    }
}
