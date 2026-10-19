use oasis7_client_api::chain_pos_defaults;
use oasis7_launcher_config::default_chain_replication_bootstrap_peers_csv;
use oasis7_launcher_ui::launcher_ui_fields_for_web;
use oasis7_observability::init_tracing;
use oasis7_proto::storage_profile::StorageProfile;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::env;
use std::path::PathBuf;
use std::process::{self, Child};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tracing::{error, info};
#[path = "oasis7_web_launcher/control_plane.rs"]
mod control_plane;
#[path = "oasis7_web_launcher/gui_agent_api.rs"]
mod gui_agent_api;
use oasis7_hosted_access as hosted_access;
#[path = "oasis7_web_launcher/http_codec.rs"]
mod http_codec;
#[path = "oasis7_web_launcher/parse_utils.rs"]
mod parse_utils;
#[path = "oasis7_web_launcher/runtime_paths.rs"]
mod runtime_paths;
#[path = "oasis7_web_launcher/server.rs"]
mod server;
#[path = "oasis7_web_launcher/static_files.rs"]
mod static_files;
#[path = "oasis7_web_launcher/transfer_query_proxy.rs"]
mod transfer_query_proxy;
#[path = "oasis7_web_launcher/viewer_auth_bootstrap.rs"]
mod viewer_auth_bootstrap;

use control_plane::*;
use gui_agent_api::{execute_gui_agent_action, gui_agent_capabilities_response};
use hosted_access::{DEFAULT_DEPLOYMENT_MODE, DeploymentMode, hosted_player_access_contract};
use http_codec::{read_http_request, write_http_response, write_json_response};
use parse_utils::{
    next_value, parse_chain_replication_bootstrap_peers, parse_chain_role, parse_chain_validators,
    parse_host_port, parse_non_negative_u64, parse_optional_i64, parse_port, parse_positive_u64,
};
use runtime_paths::{
    normalize_bind_host_for_local_access, now_unix_ms, repo_root_dir,
    resolve_console_static_dir_path, resolve_oasis7_game_launcher_binary, resolve_static_dir_path,
};
#[cfg(test)]
use server::remap_transfer_runtime_target;
use server::run_server;
use static_files::{StaticAsset, load_console_static_asset};
use transfer_query_proxy::query_chain_transfer_json;
use viewer_auth_bootstrap::{
    inject_viewer_auth_bootstrap_if_html, resolve_optional_viewer_auth_bootstrap,
};

const DEFAULT_LISTEN_BIND: &str = "0.0.0.0:5410";
const DEFAULT_SCENARIO: &str = "";
const DEFAULT_LIVE_BIND: &str = "0.0.0.0:5023";
const DEFAULT_WEB_BIND: &str = "0.0.0.0:5011";
const DEFAULT_VIEWER_HOST: &str = "0.0.0.0";
const DEFAULT_VIEWER_PORT: u16 = 4173;
const DEFAULT_VIEWER_STATIC_DIR: &str = "web";
const DEFAULT_CHAIN_STATUS_BIND: &str = "127.0.0.1:5121";
const DEFAULT_CHAIN_NODE_ID: &str = "viewer-live-node";
const DEFAULT_CHAIN_NODE_ROLE: &str = "sequencer";
const DEFAULT_CHAIN_P2P_USER_MODE: &str = "auto_join";
const DEFAULT_CHAIN_NETWORK_TIER: &str = "local_devnet";
const DEFAULT_CHAIN_NETWORK_TIER_MANIFEST: &str = "";
const PUBLIC_TESTNET_NETWORK_TIER_MANIFEST: &str =
    "doc/testing/templates/network-tier-public-testnet.example.json";
const MAINNET_NETWORK_TIER_MANIFEST: &str =
    "doc/testing/templates/network-tier-mainnet.example.json";
const DEFAULT_CHAIN_NODE_TICK_MS: u64 = 200;
#[cfg(test)]
const LAUNCHER_AGENT_PROVIDER_FIELD_IDS: &[&str] = &[
    "agent_decision_source",
    "agent_provider_backend",
    "agent_provider_contract",
    "agent_provider_transport",
    "agent_provider_url",
    "agent_provider_auth_token",
    "provider_auto_discover",
    "agent_provider_connect_timeout_ms",
    "agent_execution_lane",
    "agent_provider_profile",
];
const AGENT_DECISION_SOURCE_BUILTIN_LLM: &str = "builtin_llm";
const AGENT_DECISION_SOURCE_PROVIDER_BACKED: &str = "provider_backed";
const AGENT_PROVIDER_BACKEND_LOCAL_BRIDGE: &str = "provider_local_bridge";
const AGENT_PROVIDER_CONTRACT_WORLDSIM_V1: &str = "worldsim_provider_v1";
const AGENT_PROVIDER_TRANSPORT_LOOPBACK_HTTP: &str = "loopback_http";
const AGENT_PROVIDER_TRANSPORT_REMOTE_HTTPS: &str = "remote_https";
const AGENT_PROVIDER_MODE_PROVIDER_LOOPBACK_HTTP_ALIAS: &str = "provider_loopback_http";
const AGENT_PROVIDER_MODE_DIRECT_CONNECT_ALIAS: &str = "agent_direct_connect";
const DEFAULT_AGENT_PROVIDER_URL: &str = "http://127.0.0.1:5841";
const DEFAULT_AGENT_PROVIDER_CONNECT_TIMEOUT_MS: &str = "15000";
const DEFAULT_AGENT_PROVIDER_PROFILE: &str = "oasis7_p0_low_freq_npc";
const AGENT_EXECUTION_LANE_PLAYER_PARITY: &str = "player_parity";
const AGENT_EXECUTION_LANE_HEADLESS_AGENT: &str = "headless_agent";
const MAX_LOG_LINES: usize = 2000;
const GRACEFUL_STOP_TIMEOUT_MS: u64 = 4000;
const STOP_POLL_INTERVAL_MS: u64 = 80;
const CHAIN_STATUS_PROBE_INTERVAL_MS: u64 = 1000;
const CHAIN_STATUS_PROBE_TIMEOUT_MS: u64 = 300;
const CHAIN_STATUS_STARTING_GRACE_SECS: u64 = 8;

fn default_chain_node_id() -> String {
    format!(
        "{DEFAULT_CHAIN_NODE_ID}-fresh-{}-{}",
        process::id(),
        runtime_paths::now_unix_ms()
    )
}

static TERMINATION_REQUESTED: AtomicBool = AtomicBool::new(false);
static SIGNAL_HANDLER_INSTALL: OnceLock<Result<(), String>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct LauncherConfig {
    deployment_mode: String,
    scenario: String,
    live_bind: String,
    web_bind: String,
    viewer_host: String,
    viewer_port: String,
    viewer_static_dir: String,
    llm_enabled: bool,
    #[serde(alias = "agent_provider_mode")]
    agent_decision_source: String,
    agent_provider_backend: String,
    agent_provider_contract: String,
    agent_provider_transport: String,
    agent_provider_url: String,
    agent_provider_auth_token: String,
    provider_auto_discover: bool,
    agent_provider_connect_timeout_ms: String,
    agent_execution_lane: String,
    agent_provider_profile: String,
    chain_enabled: bool,
    chain_status_bind: String,
    chain_node_id: String,
    chain_network_tier: String,
    chain_network_tier_manifest: String,
    chain_storage_profile: String,
    chain_world_id: String,
    chain_node_role: String,
    chain_p2p_user_mode: String,
    chain_p2p_accept_public_entry: bool,
    chain_replication_bootstrap_peers: String,
    chain_node_tick_ms: String,
    chain_pos_slot_duration_ms: String,
    chain_pos_ticks_per_slot: String,
    chain_pos_proposal_tick_phase: String,
    chain_pos_adaptive_tick_scheduler_enabled: bool,
    chain_pos_slot_clock_genesis_unix_ms: String,
    chain_pos_max_past_slot_lag: String,
    chain_node_validators: String,
    auto_open_browser: bool,
    launcher_bin: String,
    chain_runtime_bin: String,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        let pos_defaults = chain_pos_defaults::defaults();
        Self {
            deployment_mode: DEFAULT_DEPLOYMENT_MODE.to_string(),
            scenario: DEFAULT_SCENARIO.to_string(),
            live_bind: DEFAULT_LIVE_BIND.to_string(),
            web_bind: DEFAULT_WEB_BIND.to_string(),
            viewer_host: DEFAULT_VIEWER_HOST.to_string(),
            viewer_port: DEFAULT_VIEWER_PORT.to_string(),
            viewer_static_dir: resolve_static_dir_path(DEFAULT_VIEWER_STATIC_DIR)
                .to_string_lossy()
                .to_string(),
            llm_enabled: true,
            agent_decision_source: AGENT_DECISION_SOURCE_PROVIDER_BACKED.to_string(),
            agent_provider_backend: AGENT_PROVIDER_BACKEND_LOCAL_BRIDGE.to_string(),
            agent_provider_contract: AGENT_PROVIDER_CONTRACT_WORLDSIM_V1.to_string(),
            agent_provider_transport: AGENT_PROVIDER_TRANSPORT_LOOPBACK_HTTP.to_string(),
            agent_provider_url: DEFAULT_AGENT_PROVIDER_URL.to_string(),
            agent_provider_auth_token: String::new(),
            provider_auto_discover: true,
            agent_provider_connect_timeout_ms: DEFAULT_AGENT_PROVIDER_CONNECT_TIMEOUT_MS
                .to_string(),
            agent_execution_lane: AGENT_EXECUTION_LANE_HEADLESS_AGENT.to_string(),
            agent_provider_profile: DEFAULT_AGENT_PROVIDER_PROFILE.to_string(),
            chain_enabled: true,
            chain_status_bind: DEFAULT_CHAIN_STATUS_BIND.to_string(),
            chain_node_id: default_chain_node_id(),
            chain_network_tier: DEFAULT_CHAIN_NETWORK_TIER.to_string(),
            chain_network_tier_manifest: DEFAULT_CHAIN_NETWORK_TIER_MANIFEST.to_string(),
            chain_storage_profile: StorageProfile::DevLocal.as_str().to_string(),
            chain_world_id: String::new(),
            chain_node_role: DEFAULT_CHAIN_NODE_ROLE.to_string(),
            chain_p2p_user_mode: DEFAULT_CHAIN_P2P_USER_MODE.to_string(),
            chain_p2p_accept_public_entry: false,
            chain_replication_bootstrap_peers: default_chain_replication_bootstrap_peers_csv(),
            chain_node_tick_ms: DEFAULT_CHAIN_NODE_TICK_MS.to_string(),
            chain_pos_slot_duration_ms: pos_defaults.slot_duration_ms.to_string(),
            chain_pos_ticks_per_slot: pos_defaults.ticks_per_slot.to_string(),
            chain_pos_proposal_tick_phase: pos_defaults.proposal_tick_phase.to_string(),
            chain_pos_adaptive_tick_scheduler_enabled: false,
            chain_pos_slot_clock_genesis_unix_ms: String::new(),
            chain_pos_max_past_slot_lag: pos_defaults.max_past_slot_lag.to_string(),
            chain_node_validators: String::new(),
            auto_open_browser: false,
            launcher_bin: String::new(),
            chain_runtime_bin: String::new(),
        }
    }
}

impl LauncherConfig {
    fn redacted_for_state_response(&self) -> Self {
        let mut config = self.clone();
        config.agent_provider_auth_token.clear();
        config
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    listen_bind: String,
    launcher_bin: String,
    chain_runtime_bin: String,
    console_static_dir: PathBuf,
    initial_config: LauncherConfig,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            listen_bind: DEFAULT_LISTEN_BIND.to_string(),
            launcher_bin: resolve_oasis7_game_launcher_binary()
                .to_string_lossy()
                .to_string(),
            chain_runtime_bin: runtime_paths::resolve_oasis7_chain_runtime_binary()
                .to_string_lossy()
                .to_string(),
            console_static_dir: resolve_console_static_dir_path(),
            initial_config: LauncherConfig::default(),
        }
    }
}

#[derive(Debug)]
struct RunningProcess {
    child: Child,
    log_rx: Receiver<String>,
}

#[derive(Debug, Clone)]
enum ProcessState {
    Idle,
    Running { pid: u32 },
    Stopped,
    InvalidConfig(String),
    StartFailed(String),
    StopFailed(String),
    Exited(String),
}

impl ProcessState {
    fn code(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running { .. } => "running",
            Self::Stopped => "stopped",
            Self::InvalidConfig(_) => "invalid_config",
            Self::StartFailed(_) => "start_failed",
            Self::StopFailed(_) => "stop_failed",
            Self::Exited(_) => "exited",
        }
    }

    fn detail(&self) -> Option<String> {
        match self {
            Self::InvalidConfig(detail)
            | Self::StartFailed(detail)
            | Self::StopFailed(detail)
            | Self::Exited(detail) => Some(detail.clone()),
            Self::Idle | Self::Running { .. } | Self::Stopped => None,
        }
    }

    fn pid(&self) -> Option<u32> {
        match self {
            Self::Running { pid } => Some(*pid),
            Self::Idle
            | Self::Stopped
            | Self::InvalidConfig(_)
            | Self::StartFailed(_)
            | Self::StopFailed(_)
            | Self::Exited(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ChainRecoverySnapshot {
    error_code: String,
    reason: String,
    node_id: String,
    execution_world_dir: String,
    recovery_mode: String,
    reset_required: bool,
    fresh_node_id: String,
    fresh_chain_status_bind: String,
    suggested_config: LauncherConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ChainP2pStatusSnapshot {
    requested_user_mode: String,
    recommended_user_mode: String,
    effective_user_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    applied_effective_user_mode: Option<String>,
    requires_explicit_public_entry_confirmation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    detected_reachability: Option<String>,
    hole_punch_viability: String,
    relay_available: bool,
    probe_stable: bool,
    deployment_mode: String,
    node_role_claim: String,
    rationale: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ChainNodeObservabilityAlertSnapshot {
    severity: String,
    code: String,
    summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ChainNodeObservabilitySnapshot {
    status: String,
    summary: String,
    ready: bool,
    connected_peer_count: usize,
    active_peer_count: usize,
    candidate_peer_count: usize,
    suspect_peer_count: usize,
    blocked_peer_count: usize,
    peer_with_issues_count: usize,
    known_peer_heads: usize,
    network_head_available: bool,
    network_height_lag: u64,
    transport_stable: bool,
    transport_stability_score: u8,
    reachability_policy_ok: bool,
    recent_replication_error_count: usize,
    storage_degraded: bool,
    reward_runtime_degraded: bool,
    alerts: Vec<ChainNodeObservabilityAlertSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ChainReplicationPeerHealthSnapshot {
    peer_id: String,
    status: String,
    issues: Vec<String>,
    discovery_sources: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_path_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_asn: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ChainReplicationSnapshot {
    local_peer_id: String,
    connected_peers: Vec<String>,
    peer_healths: Vec<ChainReplicationPeerHealthSnapshot>,
    #[serde(default)]
    request_peer_scores: std::collections::BTreeMap<String, u8>,
}

#[derive(Debug, Clone)]
enum ChainRuntimeStatus {
    Disabled,
    NotStarted,
    Starting,
    Ready,
    StaleExecutionWorld(String),
    Unreachable(String),
    ConfigError(String),
}

impl ChainRuntimeStatus {
    fn code(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NotStarted => "not_started",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::StaleExecutionWorld(_) => "stale_execution_world",
            Self::Unreachable(_) => "unreachable",
            Self::ConfigError(_) => "config_error",
        }
    }

    fn detail(&self) -> Option<String> {
        match self {
            Self::StaleExecutionWorld(detail)
            | Self::Unreachable(detail)
            | Self::ConfigError(detail) => Some(detail.clone()),
            Self::Disabled | Self::NotStarted | Self::Starting | Self::Ready => None,
        }
    }
}

#[derive(Debug)]
struct ServiceState {
    launcher_bin: String,
    chain_runtime_bin: String,
    console_static_dir: PathBuf,
    config: LauncherConfig,
    process_state: ProcessState,
    running: Option<RunningProcess>,
    chain_runtime_status: ChainRuntimeStatus,
    chain_identity: Option<ChainIdentitySnapshot>,
    chain_p2p_status: Option<ChainP2pStatusSnapshot>,
    chain_observability_status: Option<ChainNodeObservabilitySnapshot>,
    chain_replication_status: Option<ChainReplicationSnapshot>,
    chain_recovery: Option<ChainRecoverySnapshot>,
    chain_running: Option<RunningProcess>,
    chain_started_at: Option<Instant>,
    last_chain_probe_at: Option<Instant>,
    logs: VecDeque<String>,
    updated_at_unix_ms: u64,
}

impl ServiceState {
    fn new(
        launcher_bin: String,
        chain_runtime_bin: String,
        console_static_dir: PathBuf,
        config: LauncherConfig,
    ) -> Self {
        let chain_runtime_status = if config.chain_enabled {
            ChainRuntimeStatus::NotStarted
        } else {
            ChainRuntimeStatus::Disabled
        };
        Self {
            launcher_bin,
            chain_runtime_bin,
            console_static_dir,
            config,
            process_state: ProcessState::Idle,
            running: None,
            chain_runtime_status,
            chain_identity: None,
            chain_p2p_status: None,
            chain_observability_status: None,
            chain_replication_status: None,
            chain_recovery: None,
            chain_running: None,
            chain_started_at: None,
            last_chain_probe_at: None,
            logs: VecDeque::new(),
            updated_at_unix_ms: now_unix_ms(),
        }
    }

    fn append_log<S: Into<String>>(&mut self, line: S) {
        self.logs.push_back(line.into());
        while self.logs.len() > MAX_LOG_LINES {
            self.logs.pop_front();
        }
    }

    fn mark_updated(&mut self) {
        self.updated_at_unix_ms = now_unix_ms();
    }
}

#[derive(Debug, Serialize)]
struct StateSnapshot {
    status: String,
    detail: Option<String>,
    pid: Option<u32>,
    running: bool,
    launcher_bin: String,
    chain_status: String,
    chain_detail: Option<String>,
    chain_pid: Option<u32>,
    chain_running: bool,
    chain_runtime_bin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_identity: Option<ChainIdentitySnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_p2p_status: Option<ChainP2pStatusSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_observability_status: Option<ChainNodeObservabilitySnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_replication_status: Option<ChainReplicationSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_recovery: Option<ChainRecoverySnapshot>,
    hosted_access: hosted_access::HostedPlayerAccessContract,
    game_url: String,
    config: LauncherConfig,
    logs: Vec<String>,
    updated_at_unix_ms: u64,
}

#[derive(Debug, Serialize)]
struct PublicStateSnapshot {
    hosted_access: hosted_access::HostedPlayerAccessContract,
    game_url: String,
    status: String,
    chain_status: String,
    updated_at_unix_ms: u64,
}

#[derive(Debug, Serialize)]
struct ApiResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    state: StateSnapshot,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChainTransferSubmitRequest {
    from_account_id: String,
    to_account_id: String,
    amount: u64,
    nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chain_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tx_version: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tx_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    valid_until_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_fee: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fee_asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    application_payload_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_request_id: Option<String>,
    public_key: String,
    signature: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ChainTransferSubmitResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    submitted_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl ChainTransferSubmitResponse {
    fn error(error_code: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            ok: false,
            action_id: None,
            submitted_at_unix_ms: None,
            error_code: Some(error_code.into()),
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChainFeedbackSubmitRequest {
    category: String,
    title: String,
    description: String,
    platform: String,
    game_version: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ChainFeedbackSubmitResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    feedback_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl ChainFeedbackSubmitResponse {
    fn error(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            feedback_id: None,
            event_id: None,
            error: Some(error.into()),
        }
    }
}

fn main() {
    init_tracing("oasis7_web_launcher");
    let trace_session_id = oasis7_observability::resolve_trace_session_id("oasis7_web_launcher");
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }

    let options = match parse_options(raw_args.iter().map(String::as_str)) {
        Ok(options) => options,
        Err(err) => {
            error!(error = %err, "failed to parse web launcher options");
            print_help();
            process::exit(1);
        }
    };

    info!(
        trace_session_id = %trace_session_id,
        listen_bind = %options.listen_bind,
        launcher_bin = %options.launcher_bin,
        chain_runtime_bin = %options.chain_runtime_bin,
        console_static_dir = %options.console_static_dir.display(),
        deployment_mode = %options.initial_config.deployment_mode,
        scenario = %options.initial_config.scenario,
        "starting web launcher"
    );

    if let Err(err) = run_server(options) {
        error!(error = %err, "oasis7_web_launcher failed");
        process::exit(1);
    }
}

fn print_help() {
    println!(
        "Usage: oasis7_web_launcher [options]\n\n\
Options:\n\
  --listen-bind <host:port>       web console listen bind (default: {DEFAULT_LISTEN_BIND})\n\
  --deployment-mode <mode>        hosted_public_join\n\
  --launcher-bin <path>           oasis7_game_launcher binary path\n\
  --chain-runtime-bin <path>      oasis7_chain_runtime binary path\n\
  --console-static-dir <path>     launcher web static directory (default: ../web-launcher)\n\
  --scenario <name>               default scenario for web form\n\
  --live-bind <host:port>         default --live-bind for oasis7_game_launcher\n\
  --web-bind <host:port>          default --web-bind for oasis7_game_launcher\n\
  --viewer-host <host>            default viewer host bind\n\
  --viewer-port <port>            default viewer port\n\
  --viewer-static-dir <path>      default viewer static directory\n\
  --with-llm / --no-llm           default LLM toggle (gameplay requires enabled)\n\
  --chain-enable / --chain-disable\n\
  --chain-status-bind <host:port>\n\
  --chain-node-id <id>\n\
  --chain-network-tier <tier>   technical runtime tier: local_devnet|public_testnet|mainnet\n\
  --chain-network-tier-manifest <path>\n\
  --chain-storage-profile <name>  dev_local|release_default|soak_forensics\n\
  --chain-world-id <id>\n\
  --chain-node-role <role>        sequencer|storage|observer\n\
  --chain-p2p-user-mode <mode>    auto_join|private_safe|public_entry\n\
  --chain-p2p-accept-public-entry / --chain-p2p-reject-public-entry\n\
  --chain-replication-network-peer <multiaddr> (repeatable)\n\
  --chain-node-tick-ms <ms>       worker poll/fallback interval ms\n\
  --chain-pos-slot-duration-ms <n>\n\
  --chain-pos-ticks-per-slot <n>\n\
  --chain-pos-proposal-tick-phase <n>\n\
  --chain-pos-adaptive-tick-scheduler / --chain-pos-no-adaptive-tick-scheduler\n\
  --chain-pos-slot-clock-genesis-unix-ms <n>\n\
  --chain-pos-max-past-slot-lag <n>\n\
  --chain-node-validator <id:stake> (repeatable)\n\
  --open-browser / --no-open-browser\n\
  -h, --help                      show this help\n"
    );
}

fn install_signal_handler() -> Result<(), String> {
    SIGNAL_HANDLER_INSTALL
        .get_or_init(|| {
            ctrlc::set_handler(|| {
                TERMINATION_REQUESTED.store(true, Ordering::SeqCst);
            })
            .map_err(|err| format!("failed to install signal handler: {err}"))
        })
        .clone()
}

fn lock_state(shared: &Arc<Mutex<ServiceState>>) -> std::sync::MutexGuard<'_, ServiceState> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn parse_options<'a, I>(args: I) -> Result<CliOptions, String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut options = CliOptions::default();
    let mut validators: Vec<String> = Vec::new();
    let mut replication_bootstrap_peers: Vec<String> = Vec::new();
    let mut iter = args.into_iter().peekable();

    while let Some(arg) = iter.next() {
        match arg {
            "--deployment-mode" => {
                options.initial_config.deployment_mode =
                    next_value(&mut iter, "--deployment-mode")?;
            }
            "--listen-bind" => {
                options.listen_bind = next_value(&mut iter, "--listen-bind")?;
            }
            "--launcher-bin" => {
                options.launcher_bin = next_value(&mut iter, "--launcher-bin")?;
            }
            "--chain-runtime-bin" => {
                options.chain_runtime_bin = next_value(&mut iter, "--chain-runtime-bin")?;
            }
            "--console-static-dir" => {
                options.console_static_dir =
                    PathBuf::from(next_value(&mut iter, "--console-static-dir")?);
            }
            "--scenario" => {
                options.initial_config.scenario = next_value(&mut iter, "--scenario")?;
            }
            "--live-bind" => {
                options.initial_config.live_bind = next_value(&mut iter, "--live-bind")?;
            }
            "--web-bind" => {
                options.initial_config.web_bind = next_value(&mut iter, "--web-bind")?;
            }
            "--viewer-host" => {
                options.initial_config.viewer_host = next_value(&mut iter, "--viewer-host")?;
            }
            "--viewer-port" => {
                options.initial_config.viewer_port = next_value(&mut iter, "--viewer-port")?;
            }
            "--viewer-static-dir" => {
                options.initial_config.viewer_static_dir =
                    next_value(&mut iter, "--viewer-static-dir")?;
            }
            "--with-llm" => {
                options.initial_config.llm_enabled = true;
            }
            "--no-llm" => {
                options.initial_config.llm_enabled = false;
            }
            "--chain-enable" => {
                options.initial_config.chain_enabled = true;
            }
            "--chain-disable" => {
                options.initial_config.chain_enabled = false;
            }
            "--chain-status-bind" => {
                options.initial_config.chain_status_bind =
                    next_value(&mut iter, "--chain-status-bind")?;
            }
            "--chain-node-id" => {
                options.initial_config.chain_node_id = next_value(&mut iter, "--chain-node-id")?;
            }
            "--chain-network-tier" => {
                options.initial_config.chain_network_tier =
                    next_value(&mut iter, "--chain-network-tier")?;
            }
            "--chain-network-tier-manifest" => {
                options.initial_config.chain_network_tier_manifest =
                    next_value(&mut iter, "--chain-network-tier-manifest")?;
            }
            "--chain-storage-profile" => {
                options.initial_config.chain_storage_profile =
                    next_value(&mut iter, "--chain-storage-profile")?;
            }
            "--chain-world-id" => {
                options.initial_config.chain_world_id = next_value(&mut iter, "--chain-world-id")?;
            }
            "--chain-node-role" => {
                options.initial_config.chain_node_role =
                    next_value(&mut iter, "--chain-node-role")?;
            }
            "--chain-p2p-user-mode" => {
                options.initial_config.chain_p2p_user_mode =
                    next_value(&mut iter, "--chain-p2p-user-mode")?;
            }
            "--chain-p2p-accept-public-entry" => {
                options.initial_config.chain_p2p_accept_public_entry = true;
            }
            "--chain-p2p-reject-public-entry" => {
                options.initial_config.chain_p2p_accept_public_entry = false;
            }
            "--chain-replication-network-peer" => {
                replication_bootstrap_peers
                    .push(next_value(&mut iter, "--chain-replication-network-peer")?);
            }
            "--chain-node-tick-ms" => {
                options.initial_config.chain_node_tick_ms =
                    next_value(&mut iter, "--chain-node-tick-ms")?;
            }
            "--chain-pos-slot-duration-ms" => {
                options.initial_config.chain_pos_slot_duration_ms =
                    next_value(&mut iter, "--chain-pos-slot-duration-ms")?;
            }
            "--chain-pos-ticks-per-slot" => {
                options.initial_config.chain_pos_ticks_per_slot =
                    next_value(&mut iter, "--chain-pos-ticks-per-slot")?;
            }
            "--chain-pos-proposal-tick-phase" => {
                options.initial_config.chain_pos_proposal_tick_phase =
                    next_value(&mut iter, "--chain-pos-proposal-tick-phase")?;
            }
            "--chain-pos-adaptive-tick-scheduler" => {
                options
                    .initial_config
                    .chain_pos_adaptive_tick_scheduler_enabled = true;
            }
            "--chain-pos-no-adaptive-tick-scheduler" => {
                options
                    .initial_config
                    .chain_pos_adaptive_tick_scheduler_enabled = false;
            }
            "--chain-pos-slot-clock-genesis-unix-ms" => {
                options.initial_config.chain_pos_slot_clock_genesis_unix_ms =
                    next_value(&mut iter, "--chain-pos-slot-clock-genesis-unix-ms")?;
            }
            "--chain-pos-max-past-slot-lag" => {
                options.initial_config.chain_pos_max_past_slot_lag =
                    next_value(&mut iter, "--chain-pos-max-past-slot-lag")?;
            }
            "--chain-node-validator" => {
                validators.push(next_value(&mut iter, "--chain-node-validator")?);
            }
            "--open-browser" => {
                options.initial_config.auto_open_browser = true;
            }
            "--no-open-browser" => {
                options.initial_config.auto_open_browser = false;
            }
            unknown => {
                return Err(format!("unknown option: {unknown}"));
            }
        }
    }

    if !validators.is_empty() {
        options.initial_config.chain_node_validators = validators.join(",");
    }
    if !replication_bootstrap_peers.is_empty() {
        options.initial_config.chain_replication_bootstrap_peers =
            replication_bootstrap_peers.join(",");
    }
    options.initial_config.launcher_bin = options.launcher_bin.trim().to_string();
    options.initial_config.chain_runtime_bin = options.chain_runtime_bin.trim().to_string();
    normalize_chain_network_tier_config(&mut options.initial_config)?;

    parse_host_port(options.listen_bind.as_str(), "--listen-bind")?;
    let deployment_mode = DeploymentMode::parse_user_facing(
        options.initial_config.deployment_mode.as_str(),
        "--deployment-mode",
    )?;
    if !deployment_mode.allows_local_chain_runtime() {
        options.initial_config.chain_enabled = false;
    }
    parse_port(options.initial_config.viewer_port.as_str(), "--viewer-port")?;
    parse_host_port(options.initial_config.live_bind.as_str(), "--live-bind")?;
    parse_host_port(options.initial_config.web_bind.as_str(), "--web-bind")?;
    if options.initial_config.chain_enabled {
        parse_host_port(
            options.initial_config.chain_status_bind.as_str(),
            "--chain-status-bind",
        )?;
        if effective_chain_network_tier_manifest(&options.initial_config)
            .trim()
            .is_empty()
        {
            options
                .initial_config
                .chain_storage_profile
                .parse::<StorageProfile>()?;
            parse_chain_role(options.initial_config.chain_node_role.as_str())?;
        }
        parse_chain_p2p_user_mode(options.initial_config.chain_p2p_user_mode.as_str())?;
        parse_positive_u64(
            options.initial_config.chain_node_tick_ms.as_str(),
            "--chain-node-tick-ms",
        )?;
        parse_positive_u64(
            options.initial_config.chain_pos_slot_duration_ms.as_str(),
            "--chain-pos-slot-duration-ms",
        )?;
        let ticks_per_slot = parse_positive_u64(
            options.initial_config.chain_pos_ticks_per_slot.as_str(),
            "--chain-pos-ticks-per-slot",
        )?;
        let proposal_tick_phase = parse_non_negative_u64(
            options
                .initial_config
                .chain_pos_proposal_tick_phase
                .as_str(),
            "--chain-pos-proposal-tick-phase",
        )?;
        if proposal_tick_phase >= ticks_per_slot {
            return Err(format!(
                "--chain-pos-proposal-tick-phase={} must be less than --chain-pos-ticks-per-slot={}",
                proposal_tick_phase, ticks_per_slot
            ));
        }
        parse_optional_i64(
            options
                .initial_config
                .chain_pos_slot_clock_genesis_unix_ms
                .as_str(),
            "--chain-pos-slot-clock-genesis-unix-ms",
        )?;
        parse_non_negative_u64(
            options.initial_config.chain_pos_max_past_slot_lag.as_str(),
            "--chain-pos-max-past-slot-lag",
        )?;
        parse_chain_replication_bootstrap_peers(
            options
                .initial_config
                .chain_replication_bootstrap_peers
                .as_str(),
        )?;
        parse_chain_validators(options.initial_config.chain_node_validators.as_str())?;
    }
    if options.launcher_bin.trim().is_empty() {
        return Err("--launcher-bin must not be empty".to_string());
    }
    if options.chain_runtime_bin.trim().is_empty() {
        return Err("--chain-runtime-bin must not be empty".to_string());
    }
    if options.console_static_dir.as_os_str().is_empty() {
        return Err("--console-static-dir must not be empty".to_string());
    }

    Ok(options)
}

fn canonical_chain_network_tier(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "local" | "local_devnet" | "devnet" => Some("local_devnet"),
        "public_testnet" | "testnet" => Some("public_testnet"),
        "mainnet" => Some("mainnet"),
        _ => None,
    }
}

fn known_network_tier_manifest(tier: &str) -> Option<&'static str> {
    match tier {
        "public_testnet" => Some(PUBLIC_TESTNET_NETWORK_TIER_MANIFEST),
        "mainnet" => Some(MAINNET_NETWORK_TIER_MANIFEST),
        _ => None,
    }
}

fn resolve_known_network_tier_manifest(tier: &str) -> Option<String> {
    known_network_tier_manifest(tier).map(|relative_path| {
        repo_root_dir()
            .join(relative_path)
            .to_string_lossy()
            .to_string()
    })
}

fn normalize_chain_network_tier_config(config: &mut LauncherConfig) -> Result<(), String> {
    let tier =
        canonical_chain_network_tier(config.chain_network_tier.as_str()).ok_or_else(|| {
            "chain runtime technical tier must be one of: local_devnet|public_testnet|mainnet"
                .to_string()
        })?;
    config.chain_network_tier = tier.to_string();
    if config.chain_network_tier_manifest.trim().is_empty()
        && let Some(manifest) = resolve_known_network_tier_manifest(tier)
    {
        config.chain_network_tier_manifest = manifest.to_string();
    }
    Ok(())
}

fn effective_chain_network_tier_manifest(config: &LauncherConfig) -> String {
    let explicit = config.chain_network_tier_manifest.trim();
    if !explicit.is_empty() {
        return explicit.to_string();
    }
    let tier =
        canonical_chain_network_tier(config.chain_network_tier.as_str()).unwrap_or("local_devnet");
    resolve_known_network_tier_manifest(tier)
        .unwrap_or_default()
        .to_string()
}

fn parse_chain_p2p_user_mode(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "auto_join" | "private_safe" | "public_entry" => Ok(normalized),
        _ => Err(
            "--chain-p2p-user-mode must be one of: auto_join, private_safe, public_entry"
                .to_string(),
        ),
    }
}

fn deployment_mode_from_config(config: &LauncherConfig) -> DeploymentMode {
    DeploymentMode::parse(config.deployment_mode.as_str(), "deployment_mode")
        .unwrap_or(DeploymentMode::HostedPublicJoin)
}

fn public_snapshot_from_state(
    state: &ServiceState,
    request_host: Option<&str>,
) -> PublicStateSnapshot {
    PublicStateSnapshot {
        hosted_access: hosted_player_access_contract(deployment_mode_from_config(&state.config)),
        game_url: build_game_url(&state.config, request_host),
        status: state.process_state.code().to_string(),
        chain_status: state.chain_runtime_status.code().to_string(),
        updated_at_unix_ms: state.updated_at_unix_ms,
    }
}

#[cfg(test)]
#[path = "oasis7_web_launcher/oasis7_web_launcher_tests.rs"]
mod oasis7_web_launcher_tests;
#[cfg(test)]
#[path = "oasis7_web_launcher/provider_contract_tests.rs"]
mod provider_contract_tests;
