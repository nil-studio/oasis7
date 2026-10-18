use oasis7::chain_pos_defaults;
use oasis7::observability::{
    TRACE_SESSION_ID_ENV, emit_stderr_or_event, init_tracing, resolve_trace_session_id,
};
use oasis7::simulator::ProviderExecutionMode;
use oasis7_launcher_config::default_chain_replication_bootstrap_peers_vec;
use oasis7_proto::storage_profile::StorageProfile;
use oasis7_runtime::MajorWorldEventVisibilityPermission;
use oasis7_simulator::WorldScenario;
use std::collections::BTreeSet;
use std::env;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{self, Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{Level, error, info};
#[path = "oasis7_game_launcher/chain_command.rs"]
mod chain_command;
#[path = "oasis7_game_launcher/cli.rs"]
mod cli;
#[path = "oasis7_game_launcher/director_capability.rs"]
mod director_capability;
use oasis7_hosted_access as hosted_access;
#[path = "oasis7_game_launcher/hosted_account_identity.rs"]
mod hosted_account_identity;
#[path = "oasis7_game_launcher/hosted_account_store_backend.rs"]
mod hosted_account_store_backend;
#[path = "oasis7_game_launcher/hosted_identity/mod.rs"]
mod hosted_identity;
#[path = "oasis7_game_launcher/hosted_model_client/mod.rs"]
mod hosted_model_client;
#[path = "oasis7_game_launcher/hosted_player_session.rs"]
mod hosted_player_session;
#[path = "oasis7_game_launcher/hosted_strong_auth.rs"]
mod hosted_strong_auth;
#[path = "oasis7_game_launcher/runtime_paths.rs"]
mod runtime_paths;
#[path = "oasis7_game_launcher/runtime_presence.rs"]
mod runtime_presence;
#[path = "oasis7_game_launcher/static_http.rs"]
mod static_http;
#[path = "oasis7_game_launcher/url_encoding.rs"]
mod url_encoding;
#[path = "oasis7_game_launcher/viewer_live_command.rs"]
mod viewer_live_command;
#[path = "oasis7_game_launcher/viewer_runtime_config.rs"]
mod viewer_runtime_config;
#[cfg(test)]
use chain_command::build_oasis7_chain_runtime_args;
use chain_command::{
    missing_execution_world_persistence_files, resolved_chain_execution_world_dir,
};
use cli::{
    major_world_event_visibility_as_str, parse_host_port, parse_options, print_help,
    uses_provider_http_transport, viewer_deployment_mode_from_options,
};
use hosted_access::{DEFAULT_DEPLOYMENT_MODE, DeploymentMode};
use hosted_account_identity::HostedAccountIdentityBroker;
use hosted_player_session::HostedPlayerSessionIssuer;
use oasis7::viewer::{ChainLinkPolicy, VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID};
use runtime_paths::{
    resolve_oasis7_chain_runtime_binary, resolve_oasis7_viewer_live_binary,
    resolve_viewer_static_dir,
};
#[cfg(test)]
use runtime_paths::{resolve_viewer_static_dir_with_override, viewer_dev_dist_candidates};
#[cfg(test)]
use runtime_presence::query_runtime_bound_players;
use runtime_presence::run_runtime_presence_monitor;
use static_http::handle_http_connection;
#[cfg(test)]
use static_http::{
    build_viewer_auth_bootstrap_script, content_type_for_path, resolve_static_asset_path,
    resolve_viewer_auth_bootstrap_for_embedded_server, resolve_viewer_auth_bootstrap_from_path,
    sanitize_index_html_for_embedded_server, sanitize_relative_request_path,
};
use url_encoding::encoded_query_pair;
#[cfg(test)]
use viewer_live_command::apply_viewer_live_env_overrides;
use viewer_live_command::build_oasis7_viewer_live_command;
const DEFAULT_SCENARIO: &str = "";
const DEFAULT_LIVE_BIND: &str = "127.0.0.1:5023";
const DEFAULT_WEB_BIND: &str = "127.0.0.1:5011";
const DEFAULT_VIEWER_HOST: &str = "127.0.0.1";
const DEFAULT_VIEWER_PORT: u16 = 4173;
const DEFAULT_VIEWER_STATIC_DIR: &str = "web";
const GAME_STATIC_DIR_ENV: &str = "OASIS7_GAME_STATIC_DIR";
const OASIS7_VIEWER_LIVE_BIN_ENV: &str = "OASIS7_VIEWER_LIVE_BIN";
const OASIS7_CHAIN_RUNTIME_BIN_ENV: &str = "OASIS7_CHAIN_RUNTIME_BIN";
const BUILTIN_LLM_DECISION_SOURCE: &str = "builtin_llm";
const PROVIDER_BACKED_DECISION_SOURCE: &str = "provider_backed";
const PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION: &str = "provider_loopback_http";
const LOCAL_BRIDGE_PROVIDER_BACKEND: &str = "provider_local_bridge";
const LOCAL_MOCK_PROVIDER_BACKEND: &str = "provider_local_mock";
const WORLDSIM_PROVIDER_CONTRACT: &str = "worldsim_provider_v1";
const LOOPBACK_HTTP_PROVIDER_TRANSPORT: &str = "loopback_http";
const REMOTE_HTTPS_PROVIDER_TRANSPORT: &str = "remote_https";
const AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS: &str = "agent_direct_connect";
const DEFAULT_AGENT_PROVIDER_URL: &str = "http://127.0.0.1:5841";
const DEFAULT_AGENT_PROVIDER_CONNECT_TIMEOUT_MS: u64 = 15_000;
const DEFAULT_AGENT_PROVIDER_PROFILE: &str = "oasis7_p0_low_freq_npc";
// Keep the launcher default aligned with the viewer live bounded ceiling. The
// underlying LLM config defaults to 180s, while viewer live deliberately
// caps an inherited budget at 30s; injecting a smaller 10s value here made
// launcher-spawned gameplay time out before the viewer policy was applied.
const DEFAULT_INTERACTIVE_LLM_TIMEOUT_MS: u64 = 30_000;
const LLM_TIMEOUT_MS_ENV: &str = "OASIS7_LLM_TIMEOUT_MS";
const VIEWER_AGENT_DECISION_SOURCE_ENV: &str = "OASIS7_AGENT_DECISION_SOURCE";
const VIEWER_AGENT_PROVIDER_BACKEND_ENV: &str = "OASIS7_AGENT_PROVIDER_BACKEND";
const VIEWER_AGENT_PROVIDER_CONTRACT_ENV: &str = "OASIS7_AGENT_PROVIDER_CONTRACT";
const VIEWER_AGENT_PROVIDER_TRANSPORT_ENV: &str = "OASIS7_AGENT_PROVIDER_TRANSPORT";
const VIEWER_AGENT_PROVIDER_URL_ENV: &str = "OASIS7_AGENT_PROVIDER_URL";
const VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV: &str = "OASIS7_AGENT_PROVIDER_AUTH_TOKEN";
const VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV: &str =
    "OASIS7_AGENT_PROVIDER_CONNECT_TIMEOUT_MS";
const VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV: &str =
    "OASIS7_AGENT_PROVIDER_DECISION_TIMEOUT_MS";
const VIEWER_AGENT_PROVIDER_PROFILE_ENV: &str = "OASIS7_AGENT_PROVIDER_PROFILE";
const VIEWER_AGENT_EXECUTION_LANE_ENV: &str = "OASIS7_AGENT_EXECUTION_LANE";
const VIEWER_AGENT_PROVIDER_MODE_ENV: &str = "OASIS7_AGENT_PROVIDER_MODE";
const DEFAULT_VIEWER_PLAYER_ID: &str = "viewer-player";
const DEFAULT_CHAIN_STATUS_BIND: &str = "127.0.0.1:5121";
const DEFAULT_CHAIN_LINK_POLICY: &str = "enforcing";
const DEFAULT_CHAIN_NODE_ID: &str = "viewer-live-node";
const DEFAULT_CHAIN_NODE_ROLE: &str = "sequencer";
const DEFAULT_CHAIN_P2P_USER_MODE: &str = "auto_join";
const DEFAULT_CHAIN_NETWORK_TIER_MANIFEST: &str = "";
const DEFAULT_CHAIN_NODE_TICK_MS: u64 = 200;
const VIEWER_PLAYER_ID_ENV: &str = "OASIS7_VIEWER_PLAYER_ID";
const VIEWER_AUTH_PUBLIC_KEY_ENV: &str = "OASIS7_VIEWER_AUTH_PUBLIC_KEY";
const VIEWER_AUTH_PRIVATE_KEY_ENV: &str = "OASIS7_VIEWER_AUTH_PRIVATE_KEY";
const VIEWER_AUTH_BOOTSTRAP_OBJECT: &str = "__OASIS7_VIEWER_AUTH_ENV";
#[cfg(not(test))]
const HOSTED_PLAYER_SESSION_LEDGER_PATH_ENV: &str = "OASIS7_HOSTED_PLAYER_SESSION_LEDGER_PATH";
const NODE_CONFIG_FILE_NAME: &str = "config.toml";
const NODE_TABLE_KEY: &str = "node";
const NODE_PRIVATE_KEY_FIELD: &str = "private_key";
const NODE_PUBLIC_KEY_FIELD: &str = "public_key";
const TRACE_SESSION_PROCESS_LABEL: &str = "oasis7_game_launcher";
static TERMINATION_REQUESTED: AtomicBool = AtomicBool::new(false);
static SIGNAL_HANDLER_INSTALL: OnceLock<Result<(), String>> = OnceLock::new();

fn default_chain_node_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{DEFAULT_CHAIN_NODE_ID}-fresh-{}-{now}", process::id())
}

fn default_chain_world_id(scenario: &str) -> String {
    let scenario = scenario.trim();
    if scenario.is_empty() {
        VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID.to_string()
    } else {
        format!("live-{scenario}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ViewerAuthBootstrap {
    player_id: String,
    public_key: String,
    private_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    deployment_mode: String,
    allow_trusted_local_playtest: bool,
    scenario: String,
    live_bind: String,
    web_bind: String,
    viewer_host: String,
    viewer_port: u16,
    viewer_static_dir: String,
    with_llm: bool,
    auto_play: bool,
    allow_debug_scenario: bool,
    generated_world_dir: String,
    provider_lineage_store: String,
    agent_decision_source: String,
    agent_provider_backend: String,
    agent_provider_contract: String,
    agent_provider_transport: String,
    agent_provider_url: String,
    agent_provider_auth_token: String,
    agent_provider_connect_timeout_ms: u64,
    agent_provider_profile: String,
    agent_execution_lane: ProviderExecutionMode,
    open_browser: bool,
    chain_enabled: bool,
    chain_status_bind: String,
    chain_execution_world_dir: Option<PathBuf>,
    chain_status_bind_explicit: bool,
    chain_link_policy: String,
    major_world_event_visibility: MajorWorldEventVisibilityPermission,
    chain_node_id: String,
    chain_network_tier_manifest: String,
    chain_storage_profile: StorageProfile,
    chain_world_id: Option<String>,
    chain_node_role: String,
    chain_p2p_user_mode: String,
    chain_p2p_accept_public_entry: bool,
    chain_replication_bootstrap_peers: Vec<String>,
    provider_bootstrap_authority_paths: Vec<String>,
    local_test_provider_authority_path: Option<String>,
    local_test_provider_wasm_path: Option<String>,
    local_test_provider_metadata_path: Option<String>,
    local_test_provider_agent_id: String,
    local_test_provider_owner_binding: String,
    local_test_provider_finality_block_hash: Option<String>,
    local_test_provider_session_mode: String,
    chain_local_standalone_test: bool,
    chain_node_tick_ms: u64,
    chain_pos_slot_duration_ms: u64,
    chain_pos_ticks_per_slot: u64,
    chain_pos_proposal_tick_phase: u64,
    chain_pos_adaptive_tick_scheduler_enabled: bool,
    chain_pos_slot_clock_genesis_unix_ms: Option<i64>,
    chain_pos_max_past_slot_lag: u64,
    chain_node_auto_attest_all_validators: bool,
    chain_node_validators: Vec<String>,
}

impl Default for CliOptions {
    fn default() -> Self {
        let pos_defaults = chain_pos_defaults::defaults();
        Self {
            deployment_mode: DEFAULT_DEPLOYMENT_MODE.to_string(),
            allow_trusted_local_playtest: false,
            scenario: DEFAULT_SCENARIO.to_string(),
            live_bind: DEFAULT_LIVE_BIND.to_string(),
            web_bind: DEFAULT_WEB_BIND.to_string(),
            viewer_host: DEFAULT_VIEWER_HOST.to_string(),
            viewer_port: DEFAULT_VIEWER_PORT,
            viewer_static_dir: DEFAULT_VIEWER_STATIC_DIR.to_string(),
            with_llm: true,
            auto_play: true,
            allow_debug_scenario: false,
            generated_world_dir: String::new(),
            provider_lineage_store: String::new(),
            agent_decision_source: PROVIDER_BACKED_DECISION_SOURCE.to_string(),
            agent_provider_backend: LOCAL_BRIDGE_PROVIDER_BACKEND.to_string(),
            agent_provider_contract: WORLDSIM_PROVIDER_CONTRACT.to_string(),
            agent_provider_transport: LOOPBACK_HTTP_PROVIDER_TRANSPORT.to_string(),
            agent_provider_url: DEFAULT_AGENT_PROVIDER_URL.to_string(),
            agent_provider_auth_token: String::new(),
            agent_provider_connect_timeout_ms: DEFAULT_AGENT_PROVIDER_CONNECT_TIMEOUT_MS,
            agent_provider_profile: DEFAULT_AGENT_PROVIDER_PROFILE.to_string(),
            agent_execution_lane: ProviderExecutionMode::HeadlessAgent,
            open_browser: true,
            chain_enabled: true,
            chain_status_bind: DEFAULT_CHAIN_STATUS_BIND.to_string(),
            chain_execution_world_dir: None,
            chain_status_bind_explicit: false,
            chain_link_policy: DEFAULT_CHAIN_LINK_POLICY.to_string(),
            major_world_event_visibility: MajorWorldEventVisibilityPermission::Unknown,
            chain_node_id: default_chain_node_id(),
            chain_network_tier_manifest: DEFAULT_CHAIN_NETWORK_TIER_MANIFEST.to_string(),
            chain_storage_profile: StorageProfile::DevLocal,
            chain_world_id: None,
            chain_node_role: DEFAULT_CHAIN_NODE_ROLE.to_string(),
            chain_p2p_user_mode: DEFAULT_CHAIN_P2P_USER_MODE.to_string(),
            chain_p2p_accept_public_entry: false,
            chain_replication_bootstrap_peers: default_chain_replication_bootstrap_peers_vec(),
            provider_bootstrap_authority_paths: Vec::new(),
            local_test_provider_authority_path: None,
            local_test_provider_wasm_path: None,
            local_test_provider_metadata_path: None,
            local_test_provider_agent_id: "starter-agent-0".to_string(),
            local_test_provider_owner_binding: "local-test-owner-0".to_string(),
            local_test_provider_finality_block_hash: None,
            local_test_provider_session_mode: "hosted_public_join".to_string(),
            chain_local_standalone_test: false,
            chain_node_tick_ms: DEFAULT_CHAIN_NODE_TICK_MS,
            chain_pos_slot_duration_ms: pos_defaults.slot_duration_ms,
            chain_pos_ticks_per_slot: pos_defaults.ticks_per_slot,
            chain_pos_proposal_tick_phase: pos_defaults.proposal_tick_phase,
            chain_pos_adaptive_tick_scheduler_enabled: false,
            chain_pos_slot_clock_genesis_unix_ms: None,
            chain_pos_max_past_slot_lag: pos_defaults.max_past_slot_lag,
            chain_node_auto_attest_all_validators: false,
            chain_node_validators: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct StaticHttpServer {
    stop_requested: Arc<AtomicBool>,
    stop_tx: Sender<()>,
    error_rx: Receiver<String>,
    join_handle: Option<thread::JoinHandle<()>>,
    runtime_presence_join_handle: Option<thread::JoinHandle<()>>,
    identity_maintenance_join_handle: Option<thread::JoinHandle<()>>,
}

fn main() {
    init_tracing(TRACE_SESSION_PROCESS_LABEL);
    let trace_session_id = resolve_trace_session_id(TRACE_SESSION_PROCESS_LABEL);
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }

    let options = match parse_options(raw_args.iter().map(|arg| arg.as_str())) {
        Ok(options) => options,
        Err(err) => {
            error!(trace_session_id = %trace_session_id, error = %err, "failed to parse game launcher options");
            print_help();
            process::exit(1);
        }
    };

    if let Err(err) = run_launcher(&options, trace_session_id.as_str()) {
        error!(trace_session_id = %trace_session_id, error = %err, "oasis7_game_launcher failed");
        process::exit(1);
    }
}

fn run_launcher(options: &CliOptions, trace_session_id: &str) -> Result<(), String> {
    install_signal_handler()?;
    TERMINATION_REQUESTED.store(false, Ordering::SeqCst);
    info!(
        trace_session_id = %trace_session_id,
        scenario = %options.scenario,
        live_bind = %options.live_bind,
        web_bind = %options.web_bind,
        deployment_mode = %options.deployment_mode,
        chain_enabled = options.chain_enabled,
        "starting game launcher"
    );
    if !options.with_llm {
        return Err(
            "oasis7 gameplay requires --with-llm; no-LLM launch is no longer a playable entry path"
                .to_string(),
        );
    }
    if viewer_deployment_mode_from_options(options) == DeploymentMode::HostedPublicJoin {
        let issuer_private_key =
            env::var(oasis7::viewer::HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY_ENV)
                .map_err(|_| "hosted registration issuer private key is required".to_string())?;
        oasis7::viewer::derive_hosted_registration_issuer_public_key(issuer_private_key.as_str())?;
        oasis7::viewer::preflight_hosted_registration_replay_ledger()
            .map_err(|err| format!("hosted registration replay ledger preflight failed: {err}"))?;
    }

    let oasis7_viewer_live_bin = resolve_oasis7_viewer_live_binary()?;
    let oasis7_chain_runtime_bin = if options.chain_enabled {
        Some(resolve_oasis7_chain_runtime_binary()?)
    } else {
        None
    };
    let viewer_static_dir = resolve_viewer_static_dir(options.viewer_static_dir.as_str())?;

    let mut chain_child = if let Some(chain_bin) = oasis7_chain_runtime_bin.as_ref() {
        Some(spawn_oasis7_chain_runtime(
            chain_bin.as_path(),
            options,
            trace_session_id,
        )?)
    } else {
        None
    };
    let mut world_child =
        match spawn_oasis7_viewer_live(&oasis7_viewer_live_bin, options, trace_session_id) {
            Ok(child) => child,
            Err(err) => {
                if let Some(child) = chain_child.as_mut() {
                    terminate_child(child);
                }
                return Err(err);
            }
        };
    let mut server = match start_static_http_server(
        viewer_deployment_mode_from_options(options),
        options.live_bind.as_str(),
        options.web_bind.as_str(),
        options.viewer_host.as_str(),
        options.viewer_port,
        viewer_static_dir.as_path(),
        if options.chain_enabled {
            Some(options.chain_node_id.as_str())
        } else {
            None
        },
    ) {
        Ok(server) => server,
        Err(err) => {
            terminate_child(&mut world_child);
            if let Some(child) = chain_child.as_mut() {
                terminate_child(child);
            }
            return Err(err);
        }
    };

    let ready_result = wait_until_ready(options, &mut world_child, chain_child.as_mut());
    if let Err(err) = ready_result {
        stop_static_http_server(&mut server);
        terminate_child(&mut world_child);
        if let Some(child) = chain_child.as_mut() {
            terminate_child(child);
        }
        return Err(err);
    }

    let game_url = build_game_url(options);
    info!(
        trace_session_id = %trace_session_id,
        game_url = %game_url,
        viewer_live_pid = world_child.id(),
        chain_runtime_pid = chain_child.as_ref().map(Child::id),
        chain_status_bind = %options.chain_status_bind,
        chain_link_policy = %options.chain_link_policy,
        viewer_static_dir = %viewer_static_dir.display(),
        "launcher stack is ready"
    );
    println!("Launcher stack is ready.");
    println!("- URL: {game_url}");
    println!("- oasis7_viewer_live pid: {}", world_child.id());
    if let Some(chain_child) = chain_child.as_ref() {
        println!("- oasis7_chain_runtime pid: {}", chain_child.id());
        println!(
            "- chain status: http://{}/v1/chain/status",
            options.chain_status_bind
        );
    } else {
        println!("- oasis7_chain_runtime: disabled");
    }
    println!("- web static root: {}", viewer_static_dir.display());
    println!("Press Ctrl+C to stop.");

    if options.open_browser
        && let Err(err) = open_browser(&game_url)
    {
        let stderr_message = format!("warning: failed to open browser automatically: {err}");
        emit_stderr_or_event(
            Level::WARN,
            stderr_message.as_str(),
            "game launcher failed to open browser automatically",
        );
        let browser_fallback_message = format!("open this URL manually: {game_url}");
        emit_stderr_or_event(
            Level::INFO,
            browser_fallback_message.as_str(),
            "game launcher browser fallback url",
        );
    }

    let monitor_result =
        monitor_world_chain_and_server(&mut world_child, chain_child.as_mut(), &mut server);
    stop_static_http_server(&mut server);
    terminate_child(&mut world_child);
    if let Some(child) = chain_child.as_mut() {
        terminate_child(child);
    }
    monitor_result
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

fn spawn_oasis7_viewer_live(
    path: &Path,
    options: &CliOptions,
    trace_session_id: &str,
) -> Result<Child, String> {
    let parent_has_llm_timeout_ms = env::var_os(LLM_TIMEOUT_MS_ENV).is_some();
    let repo_has_node_config_file = Path::new(NODE_CONFIG_FILE_NAME).is_file();
    let mut command = build_oasis7_viewer_live_command(
        path,
        options,
        parent_has_llm_timeout_ms,
        repo_has_node_config_file,
    );
    command.env(TRACE_SESSION_ID_ENV, trace_session_id);
    command.spawn().map_err(|err| {
        format!(
            "failed to start oasis7_viewer_live from `{}`: {err}",
            path.display()
        )
    })
}

fn spawn_oasis7_chain_runtime(
    path: &Path,
    options: &CliOptions,
    trace_session_id: &str,
) -> Result<Child, String> {
    let mut command = Command::new(path);
    command.args(chain_command::prepare_oasis7_chain_runtime_args(options)?);
    command.env(TRACE_SESSION_ID_ENV, trace_session_id);
    command.spawn().map_err(|err| {
        format!(
            "failed to start oasis7_chain_runtime from `{}`: {err}",
            path.display()
        )
    })
}

fn start_static_http_server(
    deployment_mode: DeploymentMode,
    live_bind: &str,
    web_bind: &str,
    host: &str,
    port: u16,
    root_dir: &Path,
    default_viewer_player_id: Option<&str>,
) -> Result<StaticHttpServer, String> {
    let runtime_config = Arc::new(static_http::StaticHttpRuntimeConfig::new(
        deployment_mode,
        live_bind,
        web_bind,
    )?);
    let listener = TcpListener::bind((host, port))
        .map_err(|err| format!("failed to bind static HTTP server at {host}:{port}: {err}"))?;
    let allow_hosted_test_login = static_http::hosted_test_login_allowed_on_host(host);
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("failed to set static HTTP listener nonblocking: {err}"))?;

    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (error_tx, error_rx) = mpsc::channel::<String>();
    let stop_requested = Arc::new(AtomicBool::new(false));
    let root_dir = Arc::new(root_dir.to_path_buf());
    let live_bind = Arc::new(live_bind.to_string());
    let default_viewer_player_id = Arc::new(default_viewer_player_id.map(ToOwned::to_owned));
    #[cfg(test)]
    let hosted_session_ledger_path = root_dir.join(".hosted-player-sessions-test.json");
    #[cfg(not(test))]
    let hosted_session_ledger_path = env::var_os(HOSTED_PLAYER_SESSION_LEDGER_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".oasis7-hosted-player-sessions.json"));
    let hosted_session_issuer = Arc::new(Mutex::new(
        HostedPlayerSessionIssuer::with_ledger_path(hosted_session_ledger_path).map_err(|err| {
            format!("initialize durable hosted player-session ledger failed: {err}")
        })?,
    ));
    let hosted_account_broker = Arc::new(Mutex::new(
        if deployment_mode == DeploymentMode::HostedPublicJoin {
            #[cfg(test)]
            {
                HostedAccountIdentityBroker::with_store_path(
                    root_dir.join(".hosted-account-store-test.json"),
                )?
            }
            #[cfg(not(test))]
            {
                if runtime_config
                    .identity
                    .lock()
                    .map_err(|_| "identity lock poisoned")?
                    .is_some()
                {
                    HostedAccountIdentityBroker::disabled()
                } else {
                    HostedAccountIdentityBroker::from_env()?
                }
            }
        } else {
            HostedAccountIdentityBroker::disabled()
        },
    ));
    let runtime_presence_join_handle = if deployment_mode == DeploymentMode::HostedPublicJoin {
        let stop_requested = Arc::clone(&stop_requested);
        let live_bind = Arc::clone(&live_bind);
        let hosted_session_issuer = Arc::clone(&hosted_session_issuer);
        Some(thread::spawn(move || {
            run_runtime_presence_monitor(stop_requested, live_bind, hosted_session_issuer)
        }))
    } else {
        None
    };
    let identity_maintenance_join_handle = {
        let stop = Arc::clone(&stop_requested);
        let runtime = Arc::clone(&runtime_config);
        Some(thread::spawn(move || {
            hosted_identity::run_maintenance(stop, runtime)
        }))
    };
    let join_handle = thread::spawn(move || {
        if let Err(err) = run_static_http_loop(
            listener,
            deployment_mode,
            root_dir,
            runtime_config,
            default_viewer_player_id,
            hosted_session_issuer,
            hosted_account_broker,
            allow_hosted_test_login,
            stop_rx,
        ) {
            let _ = error_tx.send(err);
        }
    });

    Ok(StaticHttpServer {
        stop_requested,
        stop_tx,
        error_rx,
        join_handle: Some(join_handle),
        runtime_presence_join_handle,
        identity_maintenance_join_handle,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
)]
fn run_static_http_loop(
    listener: TcpListener,
    deployment_mode: DeploymentMode,
    root_dir: Arc<PathBuf>,
    runtime_config: Arc<static_http::StaticHttpRuntimeConfig>,
    default_viewer_player_id: Arc<Option<String>>,
    hosted_session_issuer: Arc<Mutex<HostedPlayerSessionIssuer>>,
    hosted_account_broker: Arc<Mutex<HostedAccountIdentityBroker>>,
    allow_hosted_test_login: bool,
    stop_rx: Receiver<()>,
) -> Result<(), String> {
    let connection_budget = static_http::ConnectionBudget::new();
    loop {
        match stop_rx.try_recv() {
            Ok(_) | Err(TryRecvError::Disconnected) => return Ok(()),
            Err(TryRecvError::Empty) => {}
        }

        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let Some(permit) = connection_budget.try_acquire() else {
                    let _=stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\nRetry-After: 1\r\n\r\n");
                    continue;
                };
                stream
                    .set_nonblocking(false)
                    .map_err(|err| format!("failed to set static HTTP stream blocking: {err}"))?;
                let root_dir = Arc::clone(&root_dir);
                let runtime_config = Arc::clone(&runtime_config);
                let default_viewer_player_id = Arc::clone(&default_viewer_player_id);
                let hosted_session_issuer = Arc::clone(&hosted_session_issuer);
                let hosted_account_broker = Arc::clone(&hosted_account_broker);
                thread::spawn(move || {
                    let _permit = permit;
                    if let Err(err) = handle_http_connection(
                        stream,
                        root_dir.as_path(),
                        &runtime_config,
                        allow_hosted_test_login,
                        default_viewer_player_id.as_deref(),
                        deployment_mode,
                        &hosted_session_issuer,
                        &hosted_account_broker,
                    ) {
                        let stderr_message =
                            format!("warning: static HTTP connection failed: {err}");
                        emit_stderr_or_event(
                            Level::WARN,
                            stderr_message.as_str(),
                            "game launcher static http connection failed",
                        );
                    }
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) => {
                return Err(format!("static HTTP accept failed: {err}"));
            }
        }
    }
}

fn resolve_viewer_player_id_override(
    value: Option<String>,
    fallback_player_id: Option<&str>,
) -> String {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            fallback_player_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| DEFAULT_VIEWER_PLAYER_ID.to_string())
}

fn resolve_required_toml_string(
    table: &toml::value::Table,
    key: &str,
    label: &str,
) -> Result<String, String> {
    let value = table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label} is missing or empty"))?;
    Ok(value.to_string())
}

fn wait_until_ready(
    options: &CliOptions,
    world_child: &mut Child,
    mut chain_child: Option<&mut Child>,
) -> Result<(), String> {
    let (viewer_host, viewer_port) = normalize_http_target(
        options.viewer_host.as_str(),
        options.viewer_port,
        "viewer host/port",
    )?;
    poll_startup_health(world_child, chain_child.as_deref_mut())?;
    wait_for_http_ready(
        viewer_host.as_str(),
        viewer_port,
        Duration::from_secs(30),
        world_child,
        chain_child.as_deref_mut(),
    )
    .map_err(|err| {
        format!("viewer HTTP did not become ready at {viewer_host}:{viewer_port}: {err}")
    })?;
    poll_startup_health(world_child, chain_child.as_deref_mut())?;

    let (bridge_host, bridge_port) = parse_host_port(options.web_bind.as_str(), "--web-bind")?;
    wait_for_tcp_ready(
        bridge_host.as_str(),
        bridge_port,
        Duration::from_secs(60),
        world_child,
        chain_child.as_deref_mut(),
    )
    .map_err(|err| {
        format!("web bridge did not become ready at {bridge_host}:{bridge_port}: {err}")
    })?;
    poll_startup_health(world_child, chain_child.as_deref_mut())?;

    if options.chain_enabled {
        let (chain_status_host, chain_status_port) =
            parse_host_port(options.chain_status_bind.as_str(), "--chain-status-bind")?;
        let chain_status_host = normalize_bind_host_for_local_access(chain_status_host.as_str());
        wait_for_http_ready(
            chain_status_host.as_str(),
            chain_status_port,
            Duration::from_secs(30),
            world_child,
            chain_child.as_deref_mut(),
        )
        .map_err(|err| {
            format!(
                "chain status HTTP did not become ready at {}:{}: {}",
                chain_status_host, chain_status_port, err
            )
        })?;
        poll_startup_health(world_child, chain_child.as_deref_mut())?;
        let execution_world_dir = resolved_chain_execution_world_dir(options);
        wait_for_chain_execution_world_ready(
            Path::new(execution_world_dir.as_str()),
            Duration::from_secs(30),
            world_child,
            chain_child,
        )?;
    }
    Ok(())
}

fn wait_for_chain_execution_world_ready(
    execution_world_dir: &Path,
    timeout: Duration,
    world_child: &mut Child,
    mut chain_child: Option<&mut Child>,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        poll_startup_health(world_child, chain_child.as_deref_mut())?;
        let missing = missing_execution_world_persistence_files(execution_world_dir);
        if missing.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let missing_list = missing
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "execution world persistence did not become ready under {} before timeout; missing: {}",
                execution_world_dir.display(),
                missing_list
            ));
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn monitor_world_chain_and_server(
    world_child: &mut Child,
    mut chain_child: Option<&mut Child>,
    server: &mut StaticHttpServer,
) -> Result<(), String> {
    loop {
        if TERMINATION_REQUESTED.load(Ordering::SeqCst) {
            return Ok(());
        }
        if let Some(status) = world_child
            .try_wait()
            .map_err(|err| format!("failed to query oasis7_viewer_live status: {err}"))?
        {
            return Err(format!("oasis7_viewer_live exited unexpectedly: {status}"));
        }
        if let Some(chain_child) = chain_child.as_deref_mut()
            && let Some(status) = chain_child
                .try_wait()
                .map_err(|err| format!("failed to query oasis7_chain_runtime status: {err}"))?
        {
            return Err(format!(
                "oasis7_chain_runtime exited unexpectedly: {status}"
            ));
        }

        match server.error_rx.try_recv() {
            Ok(err) => return Err(format!("static HTTP server failed: {err}")),
            Err(TryRecvError::Disconnected) => {
                return Err("static HTTP server channel disconnected unexpectedly".to_string());
            }
            Err(TryRecvError::Empty) => {}
        }

        if let Some(handle) = server.join_handle.as_ref()
            && handle.is_finished()
        {
            return Err("static HTTP server exited unexpectedly".to_string());
        }
        if let Some(handle) = server.runtime_presence_join_handle.as_ref()
            && handle.is_finished()
        {
            return Err("runtime presence monitor exited unexpectedly".to_string());
        }

        thread::sleep(Duration::from_millis(400));
    }
}

fn stop_static_http_server(server: &mut StaticHttpServer) {
    server.stop_requested.store(true, Ordering::SeqCst);
    let _ = server.stop_tx.send(());
    if let Some(handle) = server.join_handle.take() {
        let _ = handle.join();
    }
    if let Some(handle) = server.identity_maintenance_join_handle.take() {
        let _ = handle.join();
    }
    if let Some(handle) = server.runtime_presence_join_handle.take() {
        let _ = handle.join();
    }
}

fn terminate_child(child: &mut Child) {
    if let Ok(None) = child.try_wait() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn wait_for_tcp_ready(
    host: &str,
    port: u16,
    timeout: Duration,
    world_child: &mut Child,
    mut chain_child: Option<&mut Child>,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        poll_startup_health(world_child, chain_child.as_deref_mut())?;
        match TcpStream::connect((host, port)) {
            Ok(_) => {
                poll_startup_health(world_child, chain_child.as_deref_mut())?;
                return Ok(());
            }
            Err(_) => thread::sleep(Duration::from_millis(200)),
        }
    }
    poll_startup_health(world_child, chain_child)?;
    Err(format!("timeout after {}s", timeout.as_secs()))
}

fn wait_for_http_ready(
    host: &str,
    port: u16,
    timeout: Duration,
    world_child: &mut Child,
    mut chain_child: Option<&mut Child>,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let request = format!("GET / HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");

    while Instant::now() < deadline {
        poll_startup_health(world_child, chain_child.as_deref_mut())?;
        if let Ok(mut stream) = TcpStream::connect((host, port)) {
            let _ = stream.write_all(request.as_bytes());
            let mut buf = [0u8; 256];
            match stream.read(&mut buf) {
                Ok(0) => {}
                Ok(bytes) => {
                    let response = String::from_utf8_lossy(&buf[..bytes]);
                    if response.starts_with("HTTP/") {
                        poll_startup_health(world_child, chain_child.as_deref_mut())?;
                        return Ok(());
                    }
                }
                Err(_) => {}
            }
        }
        thread::sleep(Duration::from_millis(200));
    }

    poll_startup_health(world_child, chain_child)?;
    Err(format!("timeout after {}s", timeout.as_secs()))
}

fn poll_startup_health(
    world_child: &mut Child,
    chain_child: Option<&mut Child>,
) -> Result<(), String> {
    if TERMINATION_REQUESTED.load(Ordering::SeqCst) {
        return Err("termination requested".to_string());
    }
    if let Some(status) = world_child
        .try_wait()
        .map_err(|err| format!("failed to query oasis7_viewer_live status during startup: {err}"))?
    {
        return Err(format!(
            "oasis7_viewer_live exited during startup: {status}"
        ));
    }
    if let Some(chain_child) = chain_child
        && let Some(status) = chain_child.try_wait().map_err(|err| {
            format!("failed to query oasis7_chain_runtime status during startup: {err}")
        })?
    {
        return Err(format!(
            "oasis7_chain_runtime exited during startup: {status}"
        ));
    }
    Ok(())
}

fn build_game_url(options: &CliOptions) -> String {
    let viewer_host = normalize_bind_host_for_local_access(options.viewer_host.as_str());
    let viewer_host = host_for_url(viewer_host.as_str());
    let (bridge_host, bridge_port) = parse_host_port(options.web_bind.as_str(), "--web-bind")
        .unwrap_or_else(|_| ("127.0.0.1".to_string(), 5011));
    let bridge_host = normalize_bind_host_for_local_access(bridge_host.as_str());
    let bridge_host = host_for_url(bridge_host.as_str());
    let ws_url = format!("ws://{bridge_host}:{bridge_port}");
    let hosted_access_hint = serde_json::to_string(&hosted_access::hosted_viewer_access_hint(
        viewer_deployment_mode_from_options(options),
    ))
    .unwrap_or_else(|_| "{}".to_string());
    format!(
        "http://{viewer_host}:{}/?{}&{}&{}",
        options.viewer_port,
        encoded_query_pair("render_mode", "viewer"),
        encoded_query_pair("ws", ws_url.as_str()),
        encoded_query_pair("hosted_access", hosted_access_hint.as_str()),
    )
}

fn normalize_http_target(host: &str, port: u16, label: &str) -> Result<(String, u16), String> {
    let normalized = normalize_bind_host_for_local_access(host);
    if normalized.trim().is_empty() {
        return Err(format!("{label} host cannot be empty"));
    }
    Ok((normalized, port))
}

fn normalize_bind_host_for_local_access(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed == "0.0.0.0" || trimmed == "::" || trimmed == "[::]" {
        "127.0.0.1".to_string()
    } else {
        trimmed.to_string()
    }
}

fn host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') && !host.ends_with(']') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn validate_agent_decision_source(raw: &str) -> Result<(), String> {
    match raw.trim() {
        BUILTIN_LLM_DECISION_SOURCE
        | PROVIDER_BACKED_DECISION_SOURCE
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => Ok(()),
        _ => Err("--agent-decision-source must be builtin_llm or provider_backed".to_string()),
    }
}

fn canonical_agent_decision_source(raw: &str) -> &'static str {
    match raw.trim() {
        BUILTIN_LLM_DECISION_SOURCE => BUILTIN_LLM_DECISION_SOURCE,
        PROVIDER_BACKED_DECISION_SOURCE
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => PROVIDER_BACKED_DECISION_SOURCE,
        _ => BUILTIN_LLM_DECISION_SOURCE,
    }
}

fn validate_agent_provider_backend(raw: &str) -> Result<(), String> {
    match raw.trim() {
        LOCAL_BRIDGE_PROVIDER_BACKEND
        | LOCAL_MOCK_PROVIDER_BACKEND
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => Ok(()),
        _ => Err(
            "--agent-provider-backend must be provider_local_bridge or provider_local_mock"
                .to_string(),
        ),
    }
}

fn canonical_agent_provider_backend(raw: &str) -> &'static str {
    match raw.trim() {
        LOCAL_MOCK_PROVIDER_BACKEND => LOCAL_MOCK_PROVIDER_BACKEND,
        LOCAL_BRIDGE_PROVIDER_BACKEND
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => LOCAL_BRIDGE_PROVIDER_BACKEND,
        _ => LOCAL_BRIDGE_PROVIDER_BACKEND,
    }
}

fn validate_agent_provider_contract(raw: &str) -> Result<(), String> {
    match raw.trim() {
        WORLDSIM_PROVIDER_CONTRACT
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => Ok(()),
        _ => Err("--agent-provider-contract must be worldsim_provider_v1".to_string()),
    }
}

fn canonical_agent_provider_contract(raw: &str) -> &'static str {
    match raw.trim() {
        WORLDSIM_PROVIDER_CONTRACT
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => WORLDSIM_PROVIDER_CONTRACT,
        _ => WORLDSIM_PROVIDER_CONTRACT,
    }
}

fn validate_agent_provider_transport(raw: &str) -> Result<(), String> {
    match raw.trim() {
        LOOPBACK_HTTP_PROVIDER_TRANSPORT
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS
        | REMOTE_HTTPS_PROVIDER_TRANSPORT => Ok(()),
        _ => Err("--agent-provider-transport must be loopback_http or remote_https".to_string()),
    }
}

fn canonical_agent_provider_transport(raw: &str) -> &'static str {
    match raw.trim() {
        LOOPBACK_HTTP_PROVIDER_TRANSPORT
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => LOOPBACK_HTTP_PROVIDER_TRANSPORT,
        REMOTE_HTTPS_PROVIDER_TRANSPORT => REMOTE_HTTPS_PROVIDER_TRANSPORT,
        _ => LOOPBACK_HTTP_PROVIDER_TRANSPORT,
    }
}

fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .arg(url)
            .status()
            .map_err(|err| format!("failed to execute `open`: {err}"))?;
        if status.success() {
            return Ok(());
        }
        Err(format!("`open` exited with status {status}"))
    }

    #[cfg(target_os = "windows")]
    {
        let status = Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(url)
            .status()
            .map_err(|err| format!("failed to execute `cmd /C start`: {err}"))?;
        if status.success() {
            return Ok(());
        }
        return Err(format!("`cmd /C start` exited with status {status}"));
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let status = Command::new("xdg-open")
            .arg(url)
            .status()
            .map_err(|err| format!("failed to execute `xdg-open`: {err}"))?;
        if status.success() {
            return Ok(());
        }
        Err(format!("`xdg-open` exited with status {status}"))
    }
}

#[cfg(test)]
use hosted_access::hosted_test_env;
#[cfg(test)]
#[path = "oasis7_game_launcher/launcher_visibility_policy_tests.rs"]
mod launcher_visibility_policy_tests;
#[cfg(test)]
#[path = "oasis7_game_launcher/oasis7_game_launcher_tests.rs"]
mod oasis7_game_launcher_tests;

#[cfg(test)]
#[path = "oasis7_game_launcher/environment_contract_tests.rs"]
mod environment_contract_tests;
