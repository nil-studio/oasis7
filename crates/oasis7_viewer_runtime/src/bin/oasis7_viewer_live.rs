use std::env;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::process;
use std::thread;

use oasis7::observability::init_tracing;
use oasis7::runtime::MajorWorldEventVisibilityPermission;
use oasis7::viewer::{
    ChainLinkPolicy, ProviderBackedBootstrapAuthorityV1, ViewerLiveDecisionMode,
    ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig,
};
use oasis7_simulator::WorldScenario;
use oasis7_viewer_runtime::{ViewerWebBridge, ViewerWebBridgeConfig};
use tracing::{error, info, warn};
#[path = "oasis7_viewer_live/world_service_bootstrap.rs"]
mod world_service_bootstrap;

const DEFAULT_SCENARIO_LABEL: &str = "formal_release_default";
const DEFAULT_BIND: &str = "127.0.0.1:5023";
const DEFAULT_WEB_BIND: &str = "127.0.0.1:5011";
const DEFAULT_DEPLOYMENT_MODE: &str = "trusted_local_only";
const REMOVAL_HINT: &str = "embedded node flags were removed from oasis7_viewer_live; use oasis7_chain_runtime (normally launched by oasis7_game_launcher)";
const RUNTIME_ALIAS_REMOVAL_HINT: &str = "`--runtime-world` was removed; oasis7_viewer_live is runtime/world only, start without this flag";

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    scenario: Option<WorldScenario>,
    debug_scenario: Option<DebugScenario>,
    bind_addr: String,
    web_bind_addr: Option<String>,
    game_api_read: bool,
    llm_mode: bool,
    deployment_mode: String,
    chain_status_bind: Option<String>,
    chain_execution_world_dir: Option<PathBuf>,
    chain_submit_bind: Option<String>,
    chain_link_policy: ChainLinkPolicy,
    auto_play: bool,
    allow_debug_scenario: bool,
    agent_chat_echo: bool,
    major_world_event_visibility: MajorWorldEventVisibilityPermission,
    generated_world_dir: Option<PathBuf>,
    provider_lineage_store: Option<PathBuf>,
    provider_backed_bootstrap_authority_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DebugScenario {
    SmelterAffordability,
    GovernanceVoteQuote,
}

impl DebugScenario {
    fn as_str(self) -> &'static str {
        match self {
            Self::SmelterAffordability => "smelter_affordability",
            Self::GovernanceVoteQuote => "governance_vote_quote",
        }
    }
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            scenario: None,
            debug_scenario: None,
            bind_addr: DEFAULT_BIND.to_string(),
            web_bind_addr: Some(DEFAULT_WEB_BIND.to_string()),
            game_api_read: false,
            llm_mode: true,
            deployment_mode: DEFAULT_DEPLOYMENT_MODE.to_string(),
            chain_status_bind: None,
            chain_execution_world_dir: None,
            chain_submit_bind: None,
            chain_link_policy: ChainLinkPolicy::Enforcing,
            auto_play: true,
            allow_debug_scenario: false,
            agent_chat_echo: oasis7::viewer::runtime_agent_chat_echo_enabled_from_env(),
            major_world_event_visibility: MajorWorldEventVisibilityPermission::Unknown,
            generated_world_dir: None,
            provider_lineage_store: None,
            provider_backed_bootstrap_authority_paths: Vec::new(),
        }
    }
}

fn main() {
    init_tracing("oasis7_viewer_live");
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }

    let options = match parse_options(raw_args.iter().map(|arg| arg.as_str())) {
        Ok(options) => options,
        Err(err) => {
            error!(error = %err, "failed to parse viewer live options");
            print_help();
            process::exit(1);
        }
    };

    if let Err(err) = run_viewer(options) {
        error!(error = %err, "oasis7_viewer_live failed");
        process::exit(1);
    }
}

fn run_viewer(options: CliOptions) -> Result<(), String> {
    let trace_session_id = oasis7::observability::resolve_trace_session_id("oasis7_viewer_live");
    info!(
        trace_session_id = %trace_session_id,
        bind_addr = %options.bind_addr,
        web_bind_addr = ?options.web_bind_addr,
        llm_mode = options.llm_mode,
        deployment_mode = %options.deployment_mode,
        chain_status_bind = ?options.chain_status_bind,
        chain_submit_bind = ?options.chain_submit_bind,
        chain_link_policy = %options.chain_link_policy.as_str(),
        auto_play = options.auto_play,
        allow_debug_scenario = options.allow_debug_scenario,
        agent_chat_echo = options.agent_chat_echo,
        major_world_event_visibility = ?options.major_world_event_visibility,
        generated_world_dir = ?options.generated_world_dir,
        provider_lineage_store = ?options.provider_lineage_store,
        provider_backed_bootstrap_authority_count = options.provider_backed_bootstrap_authority_paths.len(),
        scenario = %options
            .scenario
            .map(|value| value.as_str().to_string())
            .unwrap_or_else(|| DEFAULT_SCENARIO_LABEL.to_string()),
        debug_scenario = ?options.debug_scenario,
        "starting viewer live runtime"
    );
    let server = initialize_viewer_server(&options)?;
    let game_read_client = if options.game_api_read {
        let config = oasis7::world_service::client::WorldServiceClientConfig::from_env()?
            .ok_or("--game-api-read requires authenticated World Service configuration")?;
        Some(
            oasis7::world_service::client::RemoteWorldServiceClient::new(config)
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    if let Some(web_bind_addr) = options.web_bind_addr.clone() {
        let upstream_addr = options.bind_addr.clone();
        let bridge = ViewerWebBridge::new(ViewerWebBridgeConfig::new(
            web_bind_addr.clone(),
            upstream_addr,
        ));
        let bridge = if let Some(client) = game_read_client {
            bridge.with_game_read_api(client)?
        } else {
            bridge
        };
        thread::spawn(move || {
            if let Err(err) = bridge.run() {
                warn!(bind_addr = %web_bind_addr, error = ?err, "viewer web bridge exited with error");
            }
        });
    }

    server
        .run()
        .map_err(|err| format!("runtime viewer server exited with error: {err:?}"))
}

fn initialize_viewer_server(options: &CliOptions) -> Result<ViewerRuntimeLiveServer, String> {
    let base_config = match options.debug_scenario {
        Some(DebugScenario::SmelterAffordability) => {
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_world_id("live-runtime-smelter-affordability")
        }
        Some(DebugScenario::GovernanceVoteQuote) => {
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_world_id("live-runtime-governance-vote-quote")
        }
        None => match options.scenario {
            Some(scenario) => ViewerRuntimeLiveServerConfig::new(scenario),
            None => ViewerRuntimeLiveServerConfig::formal_release_default(),
        },
    };
    let config = base_config
        .with_bind_addr(options.bind_addr.clone())
        .with_hosted_public_join_mode(options.deployment_mode == "hosted_public_join")
        .with_auto_play_on_connect(options.auto_play)
        .with_agent_chat_echo_enabled(options.agent_chat_echo)
        .with_major_world_event_visibility(options.major_world_event_visibility)
        .with_chain_link_policy(options.chain_link_policy)
        .with_decision_mode(if options.llm_mode {
            ViewerLiveDecisionMode::Llm
        } else {
            ViewerLiveDecisionMode::Script
        });
    let config = if let Some(generated_world_dir) = options.generated_world_dir.as_ref() {
        config.with_generated_world_dir(generated_world_dir.clone())
    } else {
        config
    };
    let config = if let Some(provider_lineage_store) = options.provider_lineage_store.as_ref() {
        config.with_provider_lineage_store(provider_lineage_store.clone())
    } else {
        config
    };
    let config = if let Some(path) = options.chain_execution_world_dir.as_ref() {
        config.with_chain_execution_world_dir(path.clone())
    } else {
        config
    };
    let config = if let Some(chain_status_bind) = options.chain_status_bind.as_ref() {
        config.with_chain_status_bind(chain_status_bind.clone())
    } else {
        config
    };
    let config = if let Some(chain_submit_bind) = options.chain_submit_bind.as_ref() {
        config.with_chain_submit_bind(chain_submit_bind.clone())
    } else {
        config
    };
    let mut config = world_service_bootstrap::configure(
        config,
        options.allow_debug_scenario && options.chain_status_bind.is_none(),
        oasis7::world_service::client::WorldServiceClientConfig::from_env()?,
        oasis7::world_service::client::WorldServiceAgentSignerConfig::from_env()?,
    )?;
    validate_execution_world_boundary(options, config.world_service.is_some())?;
    if config.world_service.is_some() && options.debug_scenario.is_some() {
        return Err("seeded debug scenarios require isolated offline mode".into());
    }
    for path in &options.provider_backed_bootstrap_authority_paths {
        let bytes = std::fs::read(path).map_err(|error| {
            format!(
                "failed to read ProviderBacked authority bundle {}: {error}",
                path.display()
            )
        })?;
        let authority = serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(&bytes)
            .map_err(|error| {
                format!(
                    "failed to decode ProviderBacked authority bundle {}: {error}",
                    path.display()
                )
            })?;
        config = config.with_provider_backed_bootstrap_authority(authority);
    }
    let mut server = ViewerRuntimeLiveServer::new(config)
        .map_err(|err| format!("failed to create runtime viewer server: {err:?}"))?;
    match options.debug_scenario {
        Some(DebugScenario::SmelterAffordability) => server
            .seed_smelter_affordability_debug_scenario()
            .map_err(|err| format!("failed to seed smelter affordability debug scenario: {err}"))?,
        Some(DebugScenario::GovernanceVoteQuote) => {
            server
                .seed_governance_vote_quote_debug_scenario()
                .map_err(|err| {
                    format!("failed to seed governance vote quote debug scenario: {err}")
                })?;
        }
        None => {}
    }
    Ok(server)
}

// Service connections never acquire authority from an operator's node directory.
// The explicit checkpoint requirement belongs only to the legacy local observer lane.
fn validate_execution_world_boundary(
    options: &CliOptions,
    service_connected: bool,
) -> Result<(), String> {
    if service_connected && options.chain_execution_world_dir.is_some() {
        return Err(
            "world service cannot be combined with a local execution world directory".into(),
        );
    }
    if !service_connected
        && options.chain_status_bind.is_some()
        && options.chain_execution_world_dir.is_none()
    {
        return Err(
            "--chain-status-bind requires operator-configured --chain-execution-world-dir".into(),
        );
    }
    Ok(())
}

fn parse_options<'a>(args: impl Iterator<Item = &'a str>) -> Result<CliOptions, String> {
    let mut options = CliOptions::default();
    let mut iter = args.peekable();
    let mut scenario_set = false;

    while let Some(arg) = iter.next() {
        if !arg.starts_with('-') {
            if scenario_set {
                return Err(format!("unexpected positional argument `{arg}`"));
            }
            if arg == "smelter_affordability" {
                options.scenario = None;
                options.debug_scenario = Some(DebugScenario::SmelterAffordability);
            } else if arg == "governance_vote_quote" {
                options.scenario = None;
                options.debug_scenario = Some(DebugScenario::GovernanceVoteQuote);
            } else {
                options.scenario = Some(parse_world_scenario(arg)?);
            }
            scenario_set = true;
            continue;
        }

        match arg {
            "--bind" => {
                options.bind_addr = parse_required_value(&mut iter, "--bind")?;
            }
            "--web-bind" => {
                options.web_bind_addr = Some(parse_required_value(&mut iter, "--web-bind")?);
            }
            "--no-web-bind" => {
                options.web_bind_addr = None;
            }
            "--game-api-read" => {
                options.game_api_read = true;
            }
            "--llm" => {
                options.llm_mode = true;
            }
            "--no-llm" => {
                options.llm_mode = false;
            }
            "--deployment-mode" => {
                let raw = parse_required_value(&mut iter, "--deployment-mode")?;
                options.deployment_mode = parse_deployment_mode(raw.as_str())?.to_string();
            }
            "--chain-execution-world-dir" => {
                options.chain_execution_world_dir = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--chain-execution-world-dir",
                )?));
            }
            "--chain-status-bind" => {
                options.chain_status_bind =
                    Some(parse_required_value(&mut iter, "--chain-status-bind")?);
            }
            "--chain-submit-bind" => {
                options.chain_submit_bind =
                    Some(parse_required_value(&mut iter, "--chain-submit-bind")?);
            }
            "--chain-link-policy" => {
                let raw = parse_required_value(&mut iter, "--chain-link-policy")?;
                options.chain_link_policy = parse_chain_link_policy(raw.as_str())?;
            }
            "--auto-play" => {
                options.auto_play = true;
            }
            "--no-auto-play" => {
                options.auto_play = false;
            }
            "--allow-debug-scenario" => {
                options.allow_debug_scenario = true;
            }
            "--agent-chat-echo" => {
                options.agent_chat_echo = true;
            }
            "--major-world-event-visibility" => {
                let raw = parse_required_value(&mut iter, "--major-world-event-visibility")?;
                options.major_world_event_visibility =
                    parse_major_world_event_visibility(raw.as_str())?;
            }
            "--generated-world-dir" => {
                options.generated_world_dir = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--generated-world-dir",
                )?));
            }
            "--provider-lineage-store" => {
                options.provider_lineage_store = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--provider-lineage-store",
                )?));
            }
            "--provider-bootstrap-authority" => {
                options
                    .provider_backed_bootstrap_authority_paths
                    .push(PathBuf::from(parse_required_value(
                        &mut iter,
                        "--provider-bootstrap-authority",
                    )?));
            }
            "--runtime-world" => {
                return Err(RUNTIME_ALIAS_REMOVAL_HINT.to_string());
            }
            "--no-runtime-world" => {
                return Err(
                    "`--no-runtime-world` is no longer supported: oasis7_viewer_live is runtime-only"
                        .to_string(),
                );
            }
            "--release-config" => {
                return Err(format!("`{arg}` is no longer supported: {REMOVAL_HINT}"));
            }
            "--topology" | "--no-node" | "--viewer-no-consensus-gate" => {
                return Err(format!("`{arg}` is no longer supported: {REMOVAL_HINT}"));
            }
            _ if arg.starts_with("--node-")
                || arg.starts_with("--triad-")
                || arg.starts_with("--reward-runtime-") =>
            {
                return Err(format!("`{arg}` is no longer supported: {REMOVAL_HINT}"));
            }
            _ => {
                return Err(format!("unknown option: {arg}"));
            }
        }
    }

    parse_socket_addr(options.bind_addr.as_str(), "--bind")?;
    if options.game_api_read && options.web_bind_addr.is_none() {
        return Err("--game-api-read requires --web-bind".into());
    }
    if let Some(web_bind_addr) = options.web_bind_addr.as_deref() {
        parse_socket_addr(web_bind_addr, "--web-bind")?;
    }
    if let Some(chain_status_bind) = options.chain_status_bind.as_deref() {
        parse_resolvable_socket_addr(chain_status_bind, "--chain-status-bind")?;
    }
    if let Some(chain_submit_bind) = options.chain_submit_bind.as_deref() {
        parse_resolvable_socket_addr(chain_submit_bind, "--chain-submit-bind")?;
        if options.chain_status_bind.is_none() {
            return Err("--chain-submit-bind requires --chain-status-bind; submit-only mode would not enable chain-linked gameplay".to_string());
        }
    }
    let _ = parse_deployment_mode(options.deployment_mode.as_str())?;
    validate_generated_world_options(&options)?;
    validate_debug_scenario_guardrail(&options)?;

    Ok(options)
}

fn validate_generated_world_options(options: &CliOptions) -> Result<(), String> {
    let Some(generated_world_dir) = options.generated_world_dir.as_ref() else {
        return Ok(());
    };
    if options.scenario.is_some() {
        return Err(
            "`--generated-world-dir` cannot be combined with a positional scenario; generated map sidecar is the viewer world source"
                .to_string(),
        );
    }
    let sidecar_snapshot = generated_world_dir
        .join("generated-scenario-world")
        .join("snapshot.json");
    let sidecar_journal = generated_world_dir
        .join("generated-scenario-world")
        .join("journal.json");
    let provenance = generated_world_dir.join("world-generation-provenance.json");
    for required in [&sidecar_snapshot, &sidecar_journal, &provenance] {
        if !required.is_file() {
            return Err(format!(
                "`--generated-world-dir` is missing required file {}",
                required.display()
            ));
        }
    }
    Ok(())
}

fn validate_debug_scenario_guardrail(options: &CliOptions) -> Result<(), String> {
    if (matches!(options.scenario, Some(WorldScenario::LlmBootstrap))
        || options.debug_scenario.is_some())
        && !options.allow_debug_scenario
    {
        let scenario = options
            .debug_scenario
            .map(DebugScenario::as_str)
            .unwrap_or("llm_bootstrap");
        return Err(format!(
            "`{scenario}` is a seeded debug scenario, not a normal playtest or testnet entry; \
rerun with `--allow-debug-scenario` only for targeted diagnostics, or omit the scenario for the formal release default world"
        ));
    }
    Ok(())
}

fn parse_chain_link_policy(raw: &str) -> Result<ChainLinkPolicy, String> {
    ChainLinkPolicy::parse(raw).ok_or_else(|| {
        format!(
            "--chain-link-policy must be one of enforcing|shadow, got `{}`",
            raw.trim()
        )
    })
}

fn parse_major_world_event_visibility(
    raw: &str,
) -> Result<MajorWorldEventVisibilityPermission, String> {
    match raw.trim() {
        "unknown" => Ok(MajorWorldEventVisibilityPermission::Unknown),
        "public" => Ok(MajorWorldEventVisibilityPermission::Public),
        "restricted" => Ok(MajorWorldEventVisibilityPermission::Restricted),
        "denied" => Ok(MajorWorldEventVisibilityPermission::Denied),
        value => Err(format!(
            "--major-world-event-visibility must be one of unknown|public|restricted|denied, got `{value}`"
        )),
    }
}

fn parse_deployment_mode(raw: &str) -> Result<&'static str, String> {
    match raw.trim() {
        "trusted_local_only" => Ok("trusted_local_only"),
        "hosted_public_join" => Ok("hosted_public_join"),
        _ => Err(format!(
            "--deployment-mode must be one of trusted_local_only|hosted_public_join, got `{}`",
            raw.trim()
        )),
    }
}

fn parse_required_value<'a, I>(
    iter: &mut std::iter::Peekable<I>,
    flag: &str,
) -> Result<String, String>
where
    I: Iterator<Item = &'a str>,
{
    let Some(value) = iter.next() else {
        return Err(format!("{flag} requires a value"));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{flag} requires a non-empty value"));
    }
    Ok(value.to_string())
}

fn parse_socket_addr(raw: &str, label: &str) -> Result<std::net::SocketAddr, String> {
    raw.parse::<std::net::SocketAddr>()
        .map_err(|_| format!("{label} must be in <host:port> format"))
}

fn parse_resolvable_socket_addr(raw: &str, label: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} must be in <host:port> format"));
    }
    trimmed
        .to_socket_addrs()
        .map_err(|_| format!("{label} must be in resolvable <host:port> format"))?
        .next()
        .ok_or_else(|| format!("{label} resolved to no addresses"))?;
    Ok(())
}

fn parse_world_scenario(raw: &str) -> Result<WorldScenario, String> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err("scenario cannot be empty".to_string());
    }
    WorldScenario::parse(normalized).ok_or_else(|| {
        format!(
            "unknown scenario `{normalized}`; supported: {}",
            WorldScenario::variants().join(", ")
        )
    })
}

fn print_help() {
    println!(
        "Usage: oasis7_viewer_live [scenario] [options]\n\n\
Starts pure viewer live server (no embedded chain/node runtime).\n\n\
Options:\n\
  [scenario]                world scenario (optional; default: {DEFAULT_SCENARIO_LABEL})\n\
  --bind <host:port>        viewer live server bind (default: {DEFAULT_BIND})\n\
  --web-bind <host:port>    websocket bridge bind (default: {DEFAULT_WEB_BIND})\n\
  --no-web-bind             disable websocket bridge\n\
  --game-api-read           enable loopback signed Game API reads (requires World Service)\n\
  --llm                     enable llm mode (default; required for gameplay)\n\
  --no-llm                  disable llm mode (observer/debug only; gameplay blocked)\n\
  --chain-execution-world-dir <path> operator-owned local observer checkpoint root; unavailable with world service\n\
  --chain-status-bind <addr> legacy operator association; authenticated world service is required (no node-directory fallback)\n\
  --chain-submit-bind <addr> legacy endpoint setting; formal mutations use the authenticated world service\n\
  --chain-link-policy <mode> chain sync policy: enforcing|shadow (default: enforcing)\n\
  --deployment-mode <mode>  trusted_local_only|hosted_public_join (default: {DEFAULT_DEPLOYMENT_MODE})\n\
  --auto-play               display synchronized service progress automatically (offline debug may advance locally)\n\
  --no-auto-play            pause automatic playback (does not pause canonical service execution)\n\
  --allow-debug-scenario    allow seeded debug scenarios such as llm_bootstrap, smelter_affordability, governance_vote_quote\n\
  --agent-chat-echo         accept provider-backed local QA chat with an echo event\n\
  --major-world-event-visibility <policy> explicit audience policy: unknown|public|restricted|denied (default: unknown)\n\
  --generated-world-dir <dir> initialize viewer from generated-world/generated-scenario-world and provenance\n\
  --provider-lineage-store <path> explicit durable provider lineage checkpoint (for formal/synthetic restart recovery)\n\
  --provider-bootstrap-authority <path> explicit JSON Runtime authority bundle; repeat per ProviderBacked agent\n\
  -h, --help                show help\n\n\
World service environment (required for formal/chain-linked mode):\n\
  OASIS7_WORLD_SERVICE_ENDPOINT, OASIS7_WORLD_SERVICE_PUBLIC_KEY,\n\
  OASIS7_WORLD_SERVICE_WORLD_ID, OASIS7_WORLD_SERVICE_GENESIS_DIGEST,\n\
  OASIS7_WORLD_SERVICE_SCOPE, OASIS7_WORLD_SERVICE_READ_PRIVATE_KEY\n\
Agent writes additionally require both OASIS7_WORLD_SERVICE_AGENT_PRIVATE_KEY\n\
  and OASIS7_WORLD_SERVICE_AGENT_DELEGATION_GENERATION; read identity never grants Agent write authority.\n\
Isolated offline diagnostics require --allow-debug-scenario without --chain-status-bind.\n\n\
Removed:\n\
  --release-config, --runtime-world, all --node-*, --topology, --triad-*, --reward-runtime-*, --no-node, --viewer-no-consensus-gate\n\
  -> use oasis7_chain_runtime (usually managed by oasis7_game_launcher)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oasis7-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn parse_options_defaults() {
        let options = parse_options(std::iter::empty()).expect("defaults");
        assert_eq!(options.scenario, None);
        assert_eq!(options.bind_addr, DEFAULT_BIND);
        assert_eq!(options.web_bind_addr.as_deref(), Some(DEFAULT_WEB_BIND));
        assert!(options.llm_mode);
        assert_eq!(options.deployment_mode, DEFAULT_DEPLOYMENT_MODE);
        assert_eq!(options.chain_status_bind, None);
        assert_eq!(options.chain_submit_bind, None);
        assert_eq!(options.chain_link_policy, ChainLinkPolicy::Enforcing);
        assert_eq!(
            options.major_world_event_visibility,
            MajorWorldEventVisibilityPermission::Unknown
        );
        assert!(options.auto_play);
        assert!(!options.allow_debug_scenario);
        assert_eq!(options.generated_world_dir, None);
        assert_eq!(options.provider_lineage_store, None);
        assert!(options.provider_backed_bootstrap_authority_paths.is_empty());
    }

    #[test]
    fn parse_options_reads_custom_values() {
        let options = parse_options(
            [
                "asteroid_fragment",
                "--bind",
                "127.0.0.1:6200",
                "--web-bind",
                "127.0.0.1:6300",
                "--llm",
                "--chain-execution-world-dir",
                "output/test-observer-root",
                "--chain-status-bind",
                "127.0.0.1:7123",
                "--chain-submit-bind",
                "127.0.0.1:7124",
                "--chain-link-policy",
                "shadow",
                "--auto-play",
                "--allow-debug-scenario",
                "--agent-chat-echo",
                "--deployment-mode",
                "hosted_public_join",
                "--major-world-event-visibility",
                "restricted",
                "--provider-bootstrap-authority",
                "/var/lib/oasis7/provider-authority-agent-a.json",
            ]
            .into_iter(),
        )
        .expect("custom values");
        assert_eq!(
            options.scenario,
            Some(WorldScenario::AsteroidFragmentBootstrap)
        );
        assert_eq!(options.bind_addr, "127.0.0.1:6200");
        assert_eq!(options.web_bind_addr.as_deref(), Some("127.0.0.1:6300"));
        assert!(options.llm_mode);
        assert_eq!(options.deployment_mode, "hosted_public_join");
        assert_eq!(options.chain_status_bind.as_deref(), Some("127.0.0.1:7123"));
        assert_eq!(options.chain_submit_bind.as_deref(), Some("127.0.0.1:7124"));
        assert_eq!(options.chain_link_policy, ChainLinkPolicy::Shadow);
        assert!(options.auto_play);
        assert!(options.allow_debug_scenario);
        assert!(options.agent_chat_echo);
        assert_eq!(
            options.major_world_event_visibility,
            MajorWorldEventVisibilityPermission::Restricted
        );
        assert_eq!(options.generated_world_dir, None);
        assert_eq!(options.provider_lineage_store, None);
        assert_eq!(
            options.provider_backed_bootstrap_authority_paths,
            vec![PathBuf::from(
                "/var/lib/oasis7/provider-authority-agent-a.json"
            )]
        );
    }

    #[test]
    fn parse_options_accepts_explicit_provider_lineage_store() {
        let path = PathBuf::from("/var/lib/oasis7/provider-lineage.json");
        let options = parse_options(
            [
                "--provider-lineage-store",
                path.to_str().expect("utf8 provider lineage path"),
            ]
            .into_iter(),
        )
        .expect("provider lineage store");
        assert_eq!(options.provider_lineage_store, Some(path));
    }

    #[test]
    fn parse_options_rejects_unknown_major_world_event_visibility() {
        let error = parse_options(["--major-world-event-visibility", "authenticated"].into_iter())
            .expect_err("implicit audience policy must fail");
        assert!(error.contains("unknown|public|restricted|denied"));
    }

    #[test]
    fn parse_options_supports_no_web_bind() {
        let options = parse_options(["--no-web-bind"].into_iter()).expect("no web bind");
        assert_eq!(options.web_bind_addr, None);
    }

    #[test]
    fn parse_options_requires_web_bridge_for_opt_in_game_reads() {
        assert!(!CliOptions::default().game_api_read);
        assert!(
            parse_options(["--game-api-read"].into_iter())
                .unwrap()
                .game_api_read
        );
        assert!(parse_options(["--game-api-read", "--no-web-bind"].into_iter()).is_err());
    }

    #[test]
    fn parse_options_supports_no_auto_play() {
        let options = parse_options(["--no-auto-play"].into_iter()).expect("no auto play");
        assert!(!options.auto_play);
    }

    #[test]
    fn parse_options_accepts_generated_world_dir_with_required_files() {
        let root = unique_test_dir("viewer-live-generated-world");
        std::fs::create_dir_all(root.join("generated-scenario-world")).expect("create sidecar dir");
        std::fs::write(
            root.join("generated-scenario-world").join("snapshot.json"),
            "{}",
        )
        .expect("write snapshot");
        std::fs::write(
            root.join("generated-scenario-world").join("journal.json"),
            "{}",
        )
        .expect("write journal");
        std::fs::write(root.join("world-generation-provenance.json"), "{}")
            .expect("write provenance");

        let options = parse_options(
            [
                "--generated-world-dir",
                root.to_str().expect("utf8 temp path"),
                "--no-web-bind",
            ]
            .into_iter(),
        )
        .expect("generated world dir");
        assert_eq!(options.generated_world_dir.as_deref(), Some(root.as_path()));
        assert_eq!(options.scenario, None);

        std::fs::remove_dir_all(root).expect("cleanup generated world fixture");
    }

    #[test]
    fn parse_options_rejects_generated_world_dir_with_scenario() {
        let root = unique_test_dir("viewer-live-generated-world-scenario");
        std::fs::create_dir_all(root.join("generated-scenario-world")).expect("create sidecar dir");
        std::fs::write(
            root.join("generated-scenario-world").join("snapshot.json"),
            "{}",
        )
        .expect("write snapshot");
        std::fs::write(
            root.join("generated-scenario-world").join("journal.json"),
            "{}",
        )
        .expect("write journal");
        std::fs::write(root.join("world-generation-provenance.json"), "{}")
            .expect("write provenance");

        let err = parse_options(
            [
                "minimal",
                "--generated-world-dir",
                root.to_str().expect("utf8 temp path"),
            ]
            .into_iter(),
        )
        .expect_err("generated world dir conflicts with scenario");
        assert!(err.contains("cannot be combined"));

        std::fs::remove_dir_all(root).expect("cleanup generated world fixture");
    }

    #[test]
    fn parse_options_rejects_generated_world_dir_missing_sidecar_files() {
        let root = unique_test_dir("viewer-live-generated-world-missing");
        std::fs::create_dir_all(&root).expect("create generated world dir");

        let err = parse_options(
            [
                "--generated-world-dir",
                root.to_str().expect("utf8 temp path"),
            ]
            .into_iter(),
        )
        .expect_err("missing sidecar files");
        assert!(err.contains("generated-scenario-world"));
        assert!(err.contains("snapshot.json"));

        std::fs::remove_dir_all(root).expect("cleanup generated world fixture");
    }

    #[test]
    fn parse_options_rejects_invalid_bind() {
        let err = parse_options(["--bind", "bad-bind"].into_iter()).expect_err("invalid bind");
        assert!(err.contains("--bind"));
    }

    #[test]
    fn parse_options_rejects_invalid_chain_submit_bind() {
        let err = parse_options(["--chain-submit-bind", "bad-bind"].into_iter())
            .expect_err("invalid submit bind");
        assert!(err.contains("--chain-submit-bind"));
    }

    #[test]
    fn parse_options_rejects_submit_bind_without_status_bind() {
        let err = parse_options(["--chain-submit-bind", "127.0.0.1:7124"].into_iter())
            .expect_err("submit bind requires status bind");
        assert!(err.contains("--chain-status-bind"));
    }

    #[test]
    fn parse_options_accepts_resolvable_submit_hostname() {
        let options = parse_options(
            [
                "--chain-execution-world-dir",
                "output/test-observer-root",
                "--chain-status-bind",
                "127.0.0.1:7123",
                "--chain-submit-bind",
                "localhost:7124",
            ]
            .into_iter(),
        )
        .expect("resolvable hostname submit bind");
        assert_eq!(options.chain_submit_bind.as_deref(), Some("localhost:7124"));
    }

    #[test]
    fn service_connection_requires_no_local_execution_directory() {
        let mut options =
            parse_options(["--chain-status-bind", "localhost:7123"].into_iter()).unwrap();
        assert!(validate_execution_world_boundary(&options, true).is_ok());
        assert!(validate_execution_world_boundary(&options, false).is_err());
        options.chain_execution_world_dir = Some("output/test-observer-root".into());
        assert!(validate_execution_world_boundary(&options, false).is_ok());
        assert!(validate_execution_world_boundary(&options, true).is_err());
    }

    #[test]
    fn parse_options_accepts_resolvable_status_hostname() {
        let options = parse_options(
            [
                "--chain-execution-world-dir",
                "output/test-observer-root",
                "--chain-status-bind",
                "localhost:7123",
            ]
            .into_iter(),
        )
        .expect("resolvable hostname status bind");
        assert_eq!(options.chain_status_bind.as_deref(), Some("localhost:7123"));
    }

    #[test]
    fn parse_options_rejects_invalid_deployment_mode() {
        let err = parse_options(["--deployment-mode", "invalid"].into_iter())
            .expect_err("invalid deployment mode");
        assert!(err.contains("--deployment-mode"));
    }

    #[test]
    fn parse_options_rejects_legacy_node_flags() {
        let err = parse_options(["--no-node"].into_iter()).expect_err("legacy flag should fail");
        assert!(err.contains("no longer supported"));
        assert!(err.contains("oasis7_chain_runtime"));
    }

    #[test]
    fn parse_options_rejects_legacy_node_prefix_flags() {
        let err = parse_options(["--node-id", "n1"].into_iter()).expect_err("node-id should fail");
        assert!(err.contains("no longer supported"));
        assert!(err.contains("oasis7_chain_runtime"));
    }

    #[test]
    fn parse_options_rejects_unknown_option() {
        let err = parse_options(["--wat"].into_iter()).expect_err("unknown option");
        assert!(err.contains("unknown option"));
    }

    #[test]
    fn parse_options_rejects_unknown_scenario() {
        let err = parse_options(["wat"].into_iter()).expect_err("unknown scenario");
        assert!(err.contains("unknown scenario"));
    }

    #[test]
    fn parse_options_rejects_llm_bootstrap_without_debug_opt_in() {
        let err = parse_options(["llm_bootstrap"].into_iter()).expect_err("debug scenario");
        assert!(err.contains("seeded debug scenario"));
        assert!(err.contains("--allow-debug-scenario"));
    }

    #[test]
    fn parse_options_rejects_smelter_affordability_without_debug_opt_in() {
        let err = parse_options(["smelter_affordability"].into_iter())
            .expect_err("debug scenario requires an explicit opt-in");
        assert!(err.contains("smelter_affordability"));
        assert!(err.contains("--allow-debug-scenario"));
    }

    #[test]
    fn parse_options_accepts_smelter_affordability_with_debug_opt_in() {
        let options =
            parse_options(["smelter_affordability", "--allow-debug-scenario"].into_iter())
                .expect("debug scenario opt-in");
        assert_eq!(
            options.debug_scenario,
            Some(DebugScenario::SmelterAffordability)
        );
        assert!(options.allow_debug_scenario);
    }

    #[test]
    fn governance_vote_quote_debug_scenario_requires_explicit_opt_in() {
        let err = parse_options(["governance_vote_quote"].into_iter())
            .expect_err("debug scenario requires an explicit opt-in");
        assert!(err.contains("governance_vote_quote"));
        assert!(err.contains("--allow-debug-scenario"));
    }

    #[test]
    fn governance_vote_quote_debug_scenario_opens_one_deterministic_unvoted_proposal() {
        let options = parse_options(
            [
                "governance_vote_quote",
                "--allow-debug-scenario",
                "--no-web-bind",
                "--no-auto-play",
            ]
            .into_iter(),
        )
        .expect("debug scenario opt-in");
        assert_eq!(
            options.debug_scenario,
            Some(DebugScenario::GovernanceVoteQuote)
        );

        initialize_viewer_server(&options).expect("initialize debug viewer");
    }

    #[test]
    fn parse_options_accepts_llm_bootstrap_with_debug_opt_in() {
        let options = parse_options(["llm_bootstrap", "--allow-debug-scenario"].into_iter())
            .expect("debug scenario opt-in");
        assert_eq!(options.scenario, Some(WorldScenario::LlmBootstrap));
        assert!(options.allow_debug_scenario);
    }

    #[test]
    fn parse_options_rejects_runtime_world_alias() {
        let err =
            parse_options(["--runtime-world"].into_iter()).expect_err("runtime alias should fail");
        assert!(err.contains("removed"));
        assert!(err.contains("runtime/world"));
    }

    #[test]
    fn parse_options_rejects_release_config_flag() {
        let err = parse_options(["--release-config", "legacy.toml"].into_iter()).expect_err("flag");
        assert!(err.contains("no longer supported"));
        assert!(err.contains("oasis7_chain_runtime"));
    }

    #[test]
    fn parse_options_rejects_no_runtime_world() {
        let err = parse_options(["--no-runtime-world"].into_iter()).expect_err("flag should fail");
        assert!(err.contains("no longer supported"));
        assert!(err.contains("runtime-only"));
    }
}
