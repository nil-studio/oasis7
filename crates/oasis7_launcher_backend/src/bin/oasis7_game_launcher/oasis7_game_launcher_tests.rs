use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::thread;

use super::{
    BUILTIN_LLM_DECISION_SOURCE, CliOptions, DEFAULT_AGENT_PROVIDER_CONNECT_TIMEOUT_MS,
    DEFAULT_AGENT_PROVIDER_PROFILE, DEFAULT_AGENT_PROVIDER_URL, DEFAULT_CHAIN_LINK_POLICY,
    DEFAULT_CHAIN_NODE_ID, DEFAULT_CHAIN_STATUS_BIND, DEFAULT_DEPLOYMENT_MODE,
    DEFAULT_INTERACTIVE_LLM_TIMEOUT_MS, DEFAULT_LIVE_BIND, DEFAULT_SCENARIO,
    DEFAULT_VIEWER_STATIC_DIR, DeploymentMode, GAME_STATIC_DIR_ENV, LLM_TIMEOUT_MS_ENV,
    LOCAL_BRIDGE_PROVIDER_BACKEND, LOCAL_MOCK_PROVIDER_BACKEND, LOOPBACK_HTTP_PROVIDER_TRANSPORT,
    PROVIDER_BACKED_DECISION_SOURCE, VIEWER_AGENT_DECISION_SOURCE_ENV,
    VIEWER_AGENT_EXECUTION_LANE_ENV, VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
    VIEWER_AGENT_PROVIDER_BACKEND_ENV, VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
    VIEWER_AGENT_PROVIDER_CONTRACT_ENV, VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
    VIEWER_AGENT_PROVIDER_MODE_ENV, VIEWER_AGENT_PROVIDER_PROFILE_ENV,
    VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, VIEWER_AGENT_PROVIDER_URL_ENV, WORLDSIM_PROVIDER_CONTRACT,
    apply_viewer_live_env_overrides, build_game_url, build_oasis7_chain_runtime_args,
    build_oasis7_viewer_live_command, parse_host_port, parse_options, query_runtime_bound_players,
    resolve_viewer_static_dir_with_override, viewer_deployment_mode_from_options,
    viewer_dev_dist_candidates,
};
use oasis7::simulator::ProviderExecutionMode;
use oasis7::simulator::WorldSnapshot;
use oasis7::viewer::{VIEWER_PROTOCOL_VERSION, ViewerRequest, ViewerResponse};
use oasis7_launcher_config::DEFAULT_CHAIN_REPLICATION_BOOTSTRAP_PEERS;
use oasis7_proto::storage_profile::StorageProfile;
use oasis7_simulator::{WorldConfig, WorldModel};

#[path = "provider_lineage_tests.rs"]
mod provider_lineage_tests;

#[path = "viewer_static_dir_tests.rs"]
mod viewer_static_dir_tests;
use viewer_static_dir_tests::make_temp_dir;
#[path = "launcher_endpoint_chain_tests.rs"]
mod launcher_endpoint_chain_tests;
#[path = "launcher_hosted_public_join_tests.rs"]
mod launcher_hosted_public_join_tests;
#[path = "launcher_static_http_tests.rs"]
mod launcher_static_http_tests;
#[path = "launcher_viewer_auth_bootstrap_tests.rs"]
mod launcher_viewer_auth_bootstrap_tests;

fn command_env_value(command: &Command, key: &str) -> Option<Option<String>> {
    command
        .get_envs()
        .find(|(env_key, _)| env_key.to_string_lossy() == key)
        .map(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()))
}

fn make_generated_world_fixture(label: &str) -> std::path::PathBuf {
    let root = make_temp_dir(label);
    fs::create_dir_all(root.join("generated-scenario-world")).expect("create sidecar dir");
    fs::write(
        root.join("generated-scenario-world").join("snapshot.json"),
        "{}",
    )
    .expect("write sidecar snapshot");
    fs::write(
        root.join("generated-scenario-world").join("journal.json"),
        "{}",
    )
    .expect("write sidecar journal");
    fs::write(root.join("world-generation-provenance.json"), "{}").expect("write provenance");
    root
}

#[test]
fn parse_options_defaults() {
    let options = parse_options(std::iter::empty()).expect("parse should succeed");
    assert_eq!(options.scenario, DEFAULT_SCENARIO);
    assert_eq!(options.live_bind, DEFAULT_LIVE_BIND);
    assert_eq!(options.deployment_mode, DEFAULT_DEPLOYMENT_MODE);
    assert!(options.with_llm);
    assert!(options.auto_play);
    assert!(!options.allow_debug_scenario);
    assert_eq!(options.generated_world_dir, "");
    assert_eq!(options.provider_lineage_store, "");
    assert_eq!(
        options.agent_decision_source,
        PROVIDER_BACKED_DECISION_SOURCE
    );
    assert_eq!(
        options.agent_provider_backend,
        LOCAL_BRIDGE_PROVIDER_BACKEND
    );
    assert_eq!(options.agent_provider_contract, WORLDSIM_PROVIDER_CONTRACT);
    assert_eq!(
        options.agent_provider_transport,
        LOOPBACK_HTTP_PROVIDER_TRANSPORT
    );
    assert_eq!(options.agent_provider_url, DEFAULT_AGENT_PROVIDER_URL);
    assert_eq!(options.agent_provider_auth_token, "");
    assert_eq!(
        options.agent_provider_profile,
        DEFAULT_AGENT_PROVIDER_PROFILE
    );
    assert_eq!(
        options.agent_execution_lane,
        ProviderExecutionMode::HeadlessAgent
    );
    assert_eq!(
        options.agent_provider_connect_timeout_ms,
        DEFAULT_AGENT_PROVIDER_CONNECT_TIMEOUT_MS
    );
    assert!(options.open_browser);
    assert_eq!(options.viewer_static_dir, "web");
    assert!(!options.chain_enabled);
    assert_eq!(options.chain_status_bind, DEFAULT_CHAIN_STATUS_BIND);
    assert_eq!(options.chain_link_policy, DEFAULT_CHAIN_LINK_POLICY);
    assert!(options.provider_bootstrap_authority_paths.is_empty());
    assert!(
        options
            .chain_node_id
            .starts_with(&format!("{DEFAULT_CHAIN_NODE_ID}-fresh-"))
    );
    assert_eq!(options.chain_storage_profile, StorageProfile::DevLocal);
    assert_eq!(options.chain_node_role, "sequencer");
    assert_eq!(options.chain_p2p_user_mode, "auto_join");
    assert!(!options.chain_p2p_accept_public_entry);
    assert_eq!(
        options.chain_replication_bootstrap_peers,
        DEFAULT_CHAIN_REPLICATION_BOOTSTRAP_PEERS
            .iter()
            .map(|peer| (*peer).to_string())
            .collect::<Vec<_>>()
    );
    let pos_defaults = oasis7::chain_pos_defaults::defaults();
    assert_eq!(
        options.chain_pos_slot_duration_ms,
        pos_defaults.slot_duration_ms
    );
    assert_eq!(
        options.chain_pos_ticks_per_slot,
        pos_defaults.ticks_per_slot
    );
    assert_eq!(
        options.chain_pos_proposal_tick_phase,
        pos_defaults.proposal_tick_phase
    );
    assert!(!options.chain_pos_adaptive_tick_scheduler_enabled);
    assert_eq!(options.chain_pos_slot_clock_genesis_unix_ms, None);
    assert_eq!(
        options.chain_pos_max_past_slot_lag,
        pos_defaults.max_past_slot_lag
    );
    assert_eq!(options.chain_world_id, None);
    assert!(!options.chain_local_standalone_test);
    assert!(!options.chain_node_auto_attest_all_validators);
}

#[test]
fn parse_options_supports_no_auto_play() {
    let options = parse_options(["--no-auto-play"].into_iter()).expect("parse should succeed");
    assert!(!options.auto_play);
}

#[test]
fn parse_options_accepts_generated_world_dir() {
    let root = make_generated_world_fixture("launcher_generated_world");
    let options = parse_options(
        [
            "--generated-world-dir",
            root.to_str().expect("utf8 temp path"),
            "--no-open-browser",
        ]
        .into_iter(),
    )
    .expect("generated world dir");
    assert_eq!(options.generated_world_dir, root.to_string_lossy());
    assert_eq!(options.scenario, "");
    fs::remove_dir_all(root).expect("cleanup generated world fixture");
}

#[test]
fn parse_options_rejects_generated_world_dir_with_scenario() {
    let root = make_generated_world_fixture("launcher_generated_world_scenario");
    let err = parse_options(
        [
            "--scenario",
            "minimal",
            "--generated-world-dir",
            root.to_str().expect("utf8 temp path"),
        ]
        .into_iter(),
    )
    .expect_err("generated world dir conflicts with scenario");
    assert!(err.contains("cannot be combined"));
    fs::remove_dir_all(root).expect("cleanup generated world fixture");
}

#[test]
fn parse_options_rejects_llm_bootstrap_without_debug_opt_in() {
    let err = parse_options(["--scenario", "llm_bootstrap"].into_iter())
        .expect_err("debug scenario should require opt-in");
    assert!(err.contains("seeded debug/LLM scenario"));
    assert!(err.contains("--allow-debug-scenario"));
}

#[test]
fn parse_options_accepts_llm_bootstrap_with_debug_opt_in() {
    let options =
        parse_options(["--scenario", "llm_bootstrap", "--allow-debug-scenario"].into_iter())
            .expect("debug scenario opt-in");
    assert_eq!(options.scenario, "llm_bootstrap");
    assert!(options.allow_debug_scenario);
}

#[test]
fn parse_options_accepts_overrides() {
    let options = parse_options(
        [
            "--scenario",
            "twin_region_bootstrap",
            "--deployment-mode",
            "hosted_public_join",
            "--live-bind",
            "127.0.0.1:6200",
            "--web-bind",
            "127.0.0.1:6201",
            "--viewer-host",
            "0.0.0.0",
            "--viewer-port",
            "4777",
            "--viewer-static-dir",
            "dist",
            "--chain-status-bind",
            "127.0.0.1:6331",
            "--chain-execution-world-dir",
            "output/trusted-chain-execution",
            "--chain-link-policy",
            "shadow",
            "--chain-node-id",
            "chain-a",
            "--chain-storage-profile",
            "soak_forensics",
            "--chain-world-id",
            "live-chain-a",
            "--chain-node-role",
            "storage",
            "--chain-p2p-user-mode",
            "public_entry",
            "--chain-p2p-accept-public-entry",
            "--chain-replication-network-peer",
            "/ip4/127.0.0.1/tcp/4100",
            "--chain-replication-network-peer",
            "/dns4/bootstrap.example/tcp/4101",
            "--chain-node-tick-ms",
            "350",
            "--chain-pos-slot-duration-ms",
            "8000",
            "--chain-pos-ticks-per-slot",
            "10",
            "--chain-pos-proposal-tick-phase",
            "9",
            "--chain-pos-adaptive-tick-scheduler",
            "--chain-pos-slot-clock-genesis-unix-ms",
            "1700000000000",
            "--chain-pos-max-past-slot-lag",
            "32",
            "--chain-node-validator",
            "chain-a:55",
            "--chain-node-auto-attest-all",
            "--with-llm",
            "--auto-play",
            "--agent-decision-source",
            "provider_backed",
            "--agent-provider-backend",
            "provider_local_bridge",
            "--agent-provider-contract",
            "worldsim_provider_v1",
            "--agent-provider-transport",
            "loopback_http",
            "--agent-provider-url",
            "http://127.0.0.1:5841",
            "--agent-provider-auth-token",
            "secret-token",
            "--agent-provider-connect-timeout-ms",
            "3000",
            "--agent-provider-profile",
            "oasis7_p0_low_freq_npc",
            "--agent-execution-lane",
            "player_parity",
            "--no-open-browser",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    assert_eq!(options.scenario, "twin_region_bootstrap");
    assert_eq!(options.deployment_mode, "hosted_public_join");
    assert_eq!(options.live_bind, "127.0.0.1:6200");
    assert_eq!(options.web_bind, "127.0.0.1:6201");
    assert_eq!(options.viewer_host, "0.0.0.0");
    assert_eq!(options.viewer_port, 4777);
    assert_eq!(options.viewer_static_dir, "dist");
    assert!(options.auto_play);
    assert_eq!(options.chain_status_bind, "127.0.0.1:6331");
    assert_eq!(
        options.chain_execution_world_dir,
        Some(std::path::PathBuf::from("output/trusted-chain-execution"))
    );
    assert_eq!(options.chain_link_policy, "shadow");
    assert_eq!(options.chain_node_id, "chain-a");
    assert_eq!(options.chain_network_tier_manifest, "");
    assert_eq!(options.chain_storage_profile, StorageProfile::SoakForensics);
    assert_eq!(options.chain_world_id, Some("live-chain-a".to_string()));
    assert_eq!(options.chain_node_role, "storage");
    assert_eq!(options.chain_p2p_user_mode, "public_entry");
    assert!(options.chain_p2p_accept_public_entry);
    assert_eq!(
        options.chain_replication_bootstrap_peers,
        vec![
            "/ip4/127.0.0.1/tcp/4100".to_string(),
            "/dns4/bootstrap.example/tcp/4101".to_string(),
        ]
    );
    assert_eq!(options.chain_node_tick_ms, 350);
    assert_eq!(options.chain_pos_slot_duration_ms, 8_000);
    assert_eq!(options.chain_pos_ticks_per_slot, 10);
    assert_eq!(options.chain_pos_proposal_tick_phase, 9);
    assert!(options.chain_pos_adaptive_tick_scheduler_enabled);
    assert_eq!(
        options.chain_pos_slot_clock_genesis_unix_ms,
        Some(1_700_000_000_000)
    );
    assert_eq!(options.chain_pos_max_past_slot_lag, 32);
    assert_eq!(
        options.chain_node_validators,
        vec!["chain-a:55".to_string()]
    );
    assert!(options.chain_node_auto_attest_all_validators);
    assert!(options.with_llm);
    assert_eq!(
        options.agent_decision_source,
        PROVIDER_BACKED_DECISION_SOURCE
    );
    assert_eq!(
        options.agent_provider_backend,
        LOCAL_BRIDGE_PROVIDER_BACKEND
    );
    assert_eq!(options.agent_provider_contract, WORLDSIM_PROVIDER_CONTRACT);
    assert_eq!(
        options.agent_provider_transport,
        LOOPBACK_HTTP_PROVIDER_TRANSPORT
    );
    assert_eq!(options.agent_provider_url, "http://127.0.0.1:5841");
    assert_eq!(options.agent_provider_auth_token, "secret-token");
    assert_eq!(options.agent_provider_connect_timeout_ms, 3000);
    assert_eq!(options.agent_provider_profile, "oasis7_p0_low_freq_npc");
    assert_eq!(
        options.agent_execution_lane,
        ProviderExecutionMode::PlayerParity
    );
    assert!(!options.chain_enabled);
    assert!(!options.open_browser);
}

#[test]
fn parse_options_accepts_remote_https_provider_transport() {
    let options = parse_options(
        [
            "--with-llm",
            "--agent-decision-source",
            "provider_backed",
            "--agent-provider-backend",
            "provider_local_bridge",
            "--agent-provider-contract",
            "worldsim_provider_v1",
            "--agent-provider-transport",
            "remote_https",
            "--agent-provider-url",
            "https://provider.example",
            "--agent-provider-auth-token",
            "secret-token",
            "--agent-provider-connect-timeout-ms",
            "3000",
            "--agent-provider-profile",
            "oasis7_p0_low_freq_npc",
            "--agent-execution-lane",
            "player_parity",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    assert_eq!(options.agent_provider_transport, "remote_https");
    assert_eq!(options.agent_provider_url, "https://provider.example");
}

#[test]
fn parse_options_accepts_local_mock_provider_backend() {
    let options = parse_options(
        [
            "--with-llm",
            "--agent-decision-source",
            "provider_backed",
            "--agent-provider-backend",
            "provider_local_mock",
            "--agent-provider-contract",
            "worldsim_provider_v1",
            "--agent-provider-transport",
            "loopback_http",
            "--agent-provider-url",
            "http://127.0.0.1:5841",
            "--agent-provider-connect-timeout-ms",
            "3000",
            "--agent-provider-profile",
            "oasis7_p0_low_freq_npc",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    assert_eq!(options.agent_provider_backend, LOCAL_MOCK_PROVIDER_BACKEND);
    assert_eq!(
        options.agent_provider_transport,
        LOOPBACK_HTTP_PROVIDER_TRANSPORT
    );
    assert_eq!(options.agent_provider_url, DEFAULT_AGENT_PROVIDER_URL);
}

#[test]
fn parse_options_accepts_chain_disable() {
    let options = parse_options(["--chain-disable"].into_iter()).expect("parse should succeed");
    assert!(!options.chain_enabled);
}

#[test]
fn parse_options_collects_repeat_replication_bootstrap_peers() {
    let options = parse_options(
        [
            "--chain-replication-network-peer",
            "/ip4/127.0.0.1/tcp/4100",
            "--chain-replication-network-peer",
            "/dns4/bootstrap.example/tcp/4101",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    assert_eq!(
        options.chain_replication_bootstrap_peers,
        vec![
            "/ip4/127.0.0.1/tcp/4100".to_string(),
            "/dns4/bootstrap.example/tcp/4101".to_string(),
        ]
    );
}

#[test]
fn parse_options_accepts_agent_direct_connect_alias() {
    let options = parse_options(
        [
            "--with-llm",
            "--agent-provider-mode",
            "agent_direct_connect",
            "--agent-provider-url",
            "http://127.0.0.1:5841",
            "--agent-provider-profile",
            "oasis7_p0_low_freq_npc",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    assert_eq!(
        options.agent_decision_source,
        PROVIDER_BACKED_DECISION_SOURCE
    );
    assert_eq!(
        options.agent_provider_backend,
        LOCAL_BRIDGE_PROVIDER_BACKEND
    );
    assert_eq!(options.agent_provider_contract, WORLDSIM_PROVIDER_CONTRACT);
    assert_eq!(
        options.agent_provider_transport,
        LOOPBACK_HTTP_PROVIDER_TRANSPORT
    );
}

#[test]
fn parse_options_accepts_local_test_authority_for_builtin_llm() {
    if !crate::hosted_test_env::with_issuer(None) {
        return;
    }
    let options = parse_options(
        [
            "--deployment-mode",
            "trusted_local_only",
            "--allow-trusted-local-playtest",
            "--chain-local-standalone-test",
            "--agent-decision-source",
            "builtin_llm",
            "--local-test-provider-authority",
            "/tmp/local-test-provider-authority.json",
            "--local-test-provider-wasm",
            "/tmp/local-test-provider.wasm",
            "--local-test-provider-metadata",
            "/tmp/local-test-provider.metadata.json",
            "--local-test-provider-finality-block-hash",
            "blake3:0000000000000000000000000000000000000000000000000000000000000000",
        ]
        .into_iter(),
    )
    .expect("local authority setup should accept the native Builtin LLM lane");

    assert_eq!(options.agent_decision_source, BUILTIN_LLM_DECISION_SOURCE);
    assert!(options.chain_enabled);
    assert!(options.chain_local_standalone_test);
    assert_eq!(options.local_test_provider_agent_id, "starter-agent-0");
    assert_eq!(
        options.local_test_provider_session_mode,
        "hosted_public_join"
    );
    assert_eq!(
        viewer_deployment_mode_from_options(&options),
        DeploymentMode::HostedPublicJoin
    );
    let viewer_command =
        build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, false);
    let viewer_args = viewer_command
        .get_args()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let deployment_mode_index = viewer_args
        .iter()
        .position(|value| value == "--deployment-mode")
        .expect("viewer deployment mode argument");
    assert_eq!(
        viewer_args
            .get(deployment_mode_index + 1)
            .map(String::as_str),
        Some("hosted_public_join")
    );
}

#[test]
fn parse_options_rejects_local_test_authority_for_provider_backed_lane() {
    let err = parse_options(
        [
            "--deployment-mode",
            "trusted_local_only",
            "--allow-trusted-local-playtest",
            "--chain-local-standalone-test",
            "--local-test-provider-authority",
            "/tmp/local-test-provider-authority.json",
            "--local-test-provider-wasm",
            "/tmp/local-test-provider.wasm",
            "--local-test-provider-metadata",
            "/tmp/local-test-provider.metadata.json",
            "--local-test-provider-finality-block-hash",
            "blake3:0000000000000000000000000000000000000000000000000000000000000000",
        ]
        .into_iter(),
    )
    .expect_err("local authority setup must stay on the native Builtin LLM lane");

    assert!(err.contains("builtin_llm"));
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn builtin_viewer_live_env_applies_default_llm_timeout_when_parent_is_unset() {
    let mut options = CliOptions::default();
    options.agent_decision_source = BUILTIN_LLM_DECISION_SOURCE.to_string();
    let mut command = Command::new("echo");

    apply_viewer_live_env_overrides(&mut command, &options, false, false);

    assert_eq!(
        command_env_value(&command, LLM_TIMEOUT_MS_ENV),
        Some(Some("30000".to_string()))
    );
    assert_eq!(DEFAULT_INTERACTIVE_LLM_TIMEOUT_MS, 30_000);
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_DECISION_SOURCE_ENV),
        Some(None)
    );
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn builtin_viewer_live_env_preserves_explicit_parent_llm_timeout() {
    let mut options = CliOptions::default();
    options.agent_decision_source = BUILTIN_LLM_DECISION_SOURCE.to_string();
    let mut command = Command::new("echo");

    apply_viewer_live_env_overrides(&mut command, &options, true, false);

    assert_eq!(command_env_value(&command, LLM_TIMEOUT_MS_ENV), None);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn builtin_viewer_live_env_skips_default_llm_timeout_when_repo_config_exists() {
    let mut options = CliOptions::default();
    options.agent_decision_source = BUILTIN_LLM_DECISION_SOURCE.to_string();
    let mut command = Command::new("echo");

    apply_viewer_live_env_overrides(&mut command, &options, false, true);

    assert_eq!(command_env_value(&command, LLM_TIMEOUT_MS_ENV), None);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn provider_backed_viewer_live_env_sets_provider_specific_overrides_without_builtin_llm_timeout() {
    let mut options = CliOptions::default();
    options.agent_decision_source = PROVIDER_BACKED_DECISION_SOURCE.to_string();
    options.agent_provider_backend = LOCAL_BRIDGE_PROVIDER_BACKEND.to_string();
    options.agent_provider_contract = WORLDSIM_PROVIDER_CONTRACT.to_string();
    options.agent_provider_transport = LOOPBACK_HTTP_PROVIDER_TRANSPORT.to_string();
    options.agent_provider_url = "http://127.0.0.1:5841".to_string();
    options.agent_provider_auth_token = "secret-token".to_string();
    options.agent_provider_connect_timeout_ms = 3000;
    options.agent_provider_profile = "oasis7_p0_low_freq_npc".to_string();
    options.agent_execution_lane = ProviderExecutionMode::PlayerParity;
    let mut command = Command::new("echo");

    apply_viewer_live_env_overrides(&mut command, &options, false, false);

    assert_eq!(command_env_value(&command, LLM_TIMEOUT_MS_ENV), None);
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_DECISION_SOURCE_ENV),
        Some(Some(PROVIDER_BACKED_DECISION_SOURCE.to_string()))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_PROVIDER_BACKEND_ENV),
        Some(Some(LOCAL_BRIDGE_PROVIDER_BACKEND.to_string()))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_PROVIDER_CONTRACT_ENV),
        Some(Some(WORLDSIM_PROVIDER_CONTRACT.to_string()))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_PROVIDER_TRANSPORT_ENV),
        Some(Some(LOOPBACK_HTTP_PROVIDER_TRANSPORT.to_string()))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_PROVIDER_URL_ENV),
        Some(Some("http://127.0.0.1:5841".to_string()))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV),
        Some(Some("secret-token".to_string()))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV),
        Some(Some("3000".to_string()))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV),
        Some(Some("3000".to_string()))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_PROVIDER_PROFILE_ENV),
        Some(Some("oasis7_p0_low_freq_npc".to_string()))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_EXECUTION_LANE_ENV),
        Some(Some(
            ProviderExecutionMode::PlayerParity.as_str().to_string()
        ))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_PROVIDER_MODE_ENV),
        Some(None)
    );
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn provider_backed_viewer_live_env_preserves_local_mock_backend() {
    let mut options = CliOptions::default();
    options.agent_decision_source = PROVIDER_BACKED_DECISION_SOURCE.to_string();
    options.agent_provider_backend = LOCAL_MOCK_PROVIDER_BACKEND.to_string();
    options.agent_provider_contract = WORLDSIM_PROVIDER_CONTRACT.to_string();
    options.agent_provider_transport = LOOPBACK_HTTP_PROVIDER_TRANSPORT.to_string();
    options.agent_provider_url = "http://127.0.0.1:5841".to_string();
    let mut command = Command::new("echo");

    apply_viewer_live_env_overrides(&mut command, &options, false, false);

    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_DECISION_SOURCE_ENV),
        Some(Some(PROVIDER_BACKED_DECISION_SOURCE.to_string()))
    );
    assert_eq!(
        command_env_value(&command, VIEWER_AGENT_PROVIDER_BACKEND_ENV),
        Some(Some(LOCAL_MOCK_PROVIDER_BACKEND.to_string()))
    );
}

fn hosted_local_mock_provider_args() -> Vec<String> {
    vec![
        "--deployment-mode".to_string(),
        "hosted_public_join".to_string(),
        "--chain-enable".to_string(),
        "--chain-local-standalone-test".to_string(),
        "--local-test-provider-authority".to_string(),
        "authority.json".to_string(),
        "--local-test-provider-wasm".to_string(),
        "provider.wasm".to_string(),
        "--local-test-provider-metadata".to_string(),
        "provider.metadata.json".to_string(),
        "--local-test-provider-finality-block-hash".to_string(),
        format!("blake3:{}", "0".repeat(64)),
        "--local-test-provider-session-mode".to_string(),
        "hosted_public_join".to_string(),
        "--agent-decision-source".to_string(),
        "provider_backed".to_string(),
        "--agent-provider-backend".to_string(),
        "provider_local_mock".to_string(),
        "--agent-provider-contract".to_string(),
        "worldsim_provider_v1".to_string(),
        "--agent-provider-transport".to_string(),
        "loopback_http".to_string(),
        "--agent-provider-url".to_string(),
        "http://127.0.0.1:5841".to_string(),
        "--agent-execution-lane".to_string(),
        "player_parity".to_string(),
        "--with-llm".to_string(),
        "--no-open-browser".to_string(),
    ]
}

#[cfg(feature = "test_tier_required")]
#[test]
fn parse_options_accepts_hosted_local_mock_funding_admission() {
    let args = hosted_local_mock_provider_args();
    let options = parse_options(args.iter().map(String::as_str))
        .expect("test-tier Hosted local-mock fixture should be admitted");
    assert!(options.chain_enabled);
    assert!(options.chain_local_standalone_test);
    assert_eq!(options.deployment_mode, "hosted_public_join");
    assert_eq!(
        options.agent_decision_source,
        PROVIDER_BACKED_DECISION_SOURCE
    );
    assert_eq!(options.agent_provider_backend, LOCAL_MOCK_PROVIDER_BACKEND);
    assert_eq!(options.agent_provider_contract, WORLDSIM_PROVIDER_CONTRACT);
    assert_eq!(
        options.agent_provider_transport,
        LOOPBACK_HTTP_PROVIDER_TRANSPORT
    );
    assert_eq!(
        options.agent_execution_lane,
        ProviderExecutionMode::PlayerParity
    );
}

#[cfg(not(feature = "test_tier_required"))]
#[test]
fn parse_options_rejects_hosted_local_mock_funding_without_test_tier() {
    let args = hosted_local_mock_provider_args();
    let error = parse_options(args.iter().map(String::as_str))
        .expect_err("production binary must reject Hosted local-mock funding");
    assert!(error.contains("test_tier_required"), "{error}");
}

#[cfg(feature = "test_tier_required")]
#[test]
fn parse_options_rejects_hosted_local_mock_funding_with_non_loopback_transport() {
    let mut args = hosted_local_mock_provider_args();
    let index = args
        .iter()
        .position(|arg| arg == "loopback_http")
        .expect("loopback transport argument");
    args[index] = "remote_https".to_string();
    let error = parse_options(args.iter().map(String::as_str))
        .expect_err("Hosted local-mock funding must remain loopback-only");
    assert!(error.contains("loopback_http"), "{error}");
}

#[cfg(feature = "test_tier_required")]
#[test]
fn parse_options_rejects_hosted_local_mock_url_userinfo_host_spoof() {
    let mut args = hosted_local_mock_provider_args();
    let index = args
        .iter()
        .position(|arg| arg == "http://127.0.0.1:5841")
        .expect("provider URL argument");
    args[index] = "http://127.0.0.1:5841@evil.example/".to_string();
    let error = parse_options(args.iter().map(String::as_str))
        .expect_err("userinfo must not hide a non-loopback provider host");
    assert!(error.contains("loopback_http"), "{error}");
}

#[cfg(feature = "test_tier_required")]
#[test]
fn parse_options_rejects_hosted_local_mock_url_malformed_port() {
    let mut args = hosted_local_mock_provider_args();
    let index = args
        .iter()
        .position(|arg| arg == "http://127.0.0.1:5841")
        .expect("provider URL argument");
    args[index] = "http://127.0.0.1:not-a-port/".to_string();
    let error = parse_options(args.iter().map(String::as_str))
        .expect_err("provider URL must contain a numeric port");
    assert!(error.contains("loopback_http"), "{error}");
}

#[cfg(feature = "test_tier_required")]
#[test]
fn parse_options_rejects_hosted_local_mock_url_malformed_authority_path() {
    let mut args = hosted_local_mock_provider_args();
    let index = args
        .iter()
        .position(|arg| arg == "http://127.0.0.1:5841")
        .expect("provider URL argument");
    args[index] = "http://127.0.0.1:5841:9999/provider".to_string();
    let error = parse_options(args.iter().map(String::as_str))
        .expect_err("provider URL must not contain a second authority port");
    assert!(error.contains("loopback_http"), "{error}");
}

#[cfg(feature = "test_tier_required")]
#[test]
fn parse_options_accepts_hosted_local_mock_url_with_valid_loopback_path() {
    let mut args = hosted_local_mock_provider_args();
    let index = args
        .iter()
        .position(|arg| arg == "http://127.0.0.1:5841")
        .expect("provider URL argument");
    args[index] = "http://127.0.0.1:5841/provider".to_string();
    parse_options(args.iter().map(String::as_str))
        .expect("a valid loopback host, port, and path should be admitted");
}

#[cfg(feature = "test_tier_required")]
#[test]
fn parse_options_rejects_hosted_local_mock_funding_without_standalone_chain() {
    let mut args = hosted_local_mock_provider_args();
    args.retain(|arg| arg != "--chain-local-standalone-test");
    let error = parse_options(args.iter().map(String::as_str))
        .expect_err("Hosted local-mock funding must require standalone chain");
    assert!(error.contains("--chain-local-standalone-test"), "{error}");
}

#[test]
fn parse_options_preserves_builtin_llm_local_funding_admission() {
    let mut args = hosted_local_mock_provider_args();
    let replacements = [
        ("hosted_public_join", "trusted_local_only"),
        ("provider_backed", "builtin_llm"),
        ("provider_local_mock", "provider_local_bridge"),
    ];
    for (from, to) in replacements {
        let index = args
            .iter()
            .position(|arg| arg == from)
            .expect("legacy fixture argument");
        args[index] = to.to_string();
    }
    args.insert(2, "--allow-trusted-local-playtest".to_string());
    let options = parse_options(args.iter().map(String::as_str))
        .expect("legacy builtin_llm local funding must remain admitted");
    assert!(options.chain_enabled);
    assert_eq!(options.agent_decision_source, BUILTIN_LLM_DECISION_SOURCE);
}

#[test]
fn parse_options_rejects_hosted_builtin_llm_local_funding() {
    let mut args = hosted_local_mock_provider_args();
    let index = args
        .iter()
        .position(|arg| arg == "provider_backed")
        .expect("provider decision source argument");
    args[index] = BUILTIN_LLM_DECISION_SOURCE.to_string();
    let error = parse_options(args.iter().map(String::as_str))
        .expect_err("Hosted local funding must not widen the builtin_llm lane");
    assert!(error.contains("trusted_local_only"), "{error}");
}

#[test]
fn build_viewer_live_command_wires_agent_chat_echo_flag_from_env() {
    if !crate::hosted_test_env::run(&[(
        "OASIS7_RUNTIME_AGENT_CHAT_ECHO",
        std::ffi::OsStr::new("1"),
    )]) {
        return;
    }
    let command = build_oasis7_viewer_live_command(
        Path::new("/bin/echo"),
        &CliOptions::default(),
        false,
        false,
    );
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(args.iter().any(|arg| arg == "--agent-chat-echo"));
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_viewer_live_command_wires_auto_play_flags() {
    if !crate::hosted_test_env::with_issuer(None) {
        return;
    }
    let options = CliOptions::default();
    let command = build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, false);
    assert!(command.get_args().any(|arg| arg == "--auto-play"));

    let mut options = CliOptions::default();
    options.auto_play = false;
    let command = build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, false);
    assert!(command.get_args().any(|arg| arg == "--no-auto-play"));
    assert!(!command.get_args().any(|arg| arg == "--auto-play"));
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_viewer_live_command_wires_debug_scenario_opt_in() {
    if !crate::hosted_test_env::with_issuer(None) {
        return;
    }
    let mut options = CliOptions::default();
    options.scenario = "llm_bootstrap".to_string();
    options.allow_debug_scenario = true;
    let command = build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, false);
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(args.iter().any(|arg| arg == "llm_bootstrap"));
    assert!(args.iter().any(|arg| arg == "--allow-debug-scenario"));
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_viewer_live_command_wires_generated_world_dir() {
    if !crate::hosted_test_env::with_issuer(None) {
        return;
    }
    let mut options = CliOptions::default();
    options.generated_world_dir = "output/public-testnet/generated-world".to_string();
    let command = build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, false);
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(args.iter().any(|arg| arg == "--generated-world-dir"));
    assert!(
        args.iter()
            .any(|arg| arg == "output/public-testnet/generated-world")
    );
    assert!(!args.iter().any(|arg| arg == DEFAULT_SCENARIO));
}

#[test]
fn build_viewer_live_command_skips_default_llm_timeout_when_repo_config_exists() {
    if !crate::hosted_test_env::with_issuer(None) {
        return;
    }
    let options = CliOptions::default();
    let command = build_oasis7_viewer_live_command(Path::new("/bin/echo"), &options, false, true);
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    assert!(args.contains(&"--llm".to_string()));
    assert_eq!(command_env_value(&command, LLM_TIMEOUT_MS_ENV), None);
}

#[test]
fn parse_options_rejects_unknown_deployment_mode() {
    let err = parse_options(["--deployment-mode", "invalid"].into_iter())
        .expect_err("invalid deployment mode should fail");
    assert!(err.contains("hosted_public_join"));
}

#[test]
fn parse_options_rejects_trusted_local_playtest_without_explicit_allow() {
    let err = parse_options(["--deployment-mode", "trusted_local_only"].into_iter())
        .expect_err("trusted local playtest should require an explicit local flag");
    assert!(err.contains("trusted_local_only"));
    assert!(err.contains("hosted_public_join"));
}

#[test]
fn parse_options_accepts_trusted_local_playtest_with_explicit_allow() {
    let options = parse_options(
        [
            "--deployment-mode",
            "trusted_local_only",
            "--allow-trusted-local-playtest",
        ]
        .into_iter(),
    )
    .expect("explicit local playtest flag should allow trusted local mode");
    assert_eq!(options.deployment_mode, "trusted_local_only");
    assert!(options.allow_trusted_local_playtest);
    assert!(options.chain_enabled);
}

#[test]
fn parse_options_rejects_invalid_chain_role() {
    let err = parse_options(["--chain-node-role", "invalid"].into_iter()).expect_err("should fail");
    assert!(err.contains("sequencer, storage, observer"));
}

#[test]
fn parse_options_rejects_invalid_chain_p2p_user_mode() {
    let err =
        parse_options(["--chain-p2p-user-mode", "wild"].into_iter()).expect_err("should fail");
    assert!(err.contains("auto_join, private_safe, public_entry"));
}

#[test]
fn parse_options_rejects_invalid_chain_link_policy() {
    let err = parse_options(["--chain-link-policy", "observe"].into_iter())
        .expect_err("invalid chain link policy should fail");
    assert!(err.contains("enforcing|shadow"));
}

#[test]
fn parse_options_rejects_invalid_chain_replication_network_peer() {
    let err = parse_options(["--chain-replication-network-peer", "127.0.0.1:4100"].into_iter())
        .expect_err("should fail");
    assert!(err.contains("multiaddr"));
}

#[test]
fn parse_options_ignores_chain_tuning_when_hosted_public_join_disables_chain() {
    let options = parse_options(
        [
            "--chain-pos-ticks-per-slot",
            "4",
            "--chain-pos-proposal-tick-phase",
            "4",
        ]
        .into_iter(),
    )
    .expect("hosted public join disables local chain validation");
    assert_eq!(options.deployment_mode, "hosted_public_join");
    assert!(!options.chain_enabled);
    assert_eq!(options.chain_pos_ticks_per_slot, 4);
    assert_eq!(options.chain_pos_proposal_tick_phase, 4);
}

#[test]
fn parse_options_rejects_unknown_option() {
    let err = parse_options(["--unknown"].into_iter()).expect_err("should fail");
    assert!(err.contains("unknown option"));
}

#[test]
fn parse_options_rejects_unknown_agent_provider_mode() {
    let err = parse_options(["--agent-provider-mode", "wat-provider"].into_iter())
        .expect_err("should fail");
    assert!(err.contains("builtin_llm"));
    assert!(err.contains("provider_backed"));
}

#[test]
fn parse_options_rejects_invalid_provider_execution_lane() {
    let err = parse_options(
        [
            "--with-llm",
            "--agent-decision-source",
            "provider_backed",
            "--agent-execution-lane",
            "gpu_only",
        ]
        .into_iter(),
    )
    .expect_err("should fail");
    assert!(err.contains("player_parity"));
    assert!(err.contains("headless_agent"));
}

#[test]
fn parse_options_rejects_unknown_chain_storage_profile() {
    let err =
        parse_options(["--chain-storage-profile", "unknown"].into_iter()).expect_err("should fail");
    assert!(err.contains("dev_local"));
    assert!(err.contains("release_default"));
    assert!(err.contains("soak_forensics"));
}

#[test]
fn parse_options_rejects_missing_value() {
    let err = parse_options(["--viewer-port"].into_iter()).expect_err("should fail");
    assert!(err.contains("requires a value"));
}

#[test]
fn query_runtime_bound_players_reads_snapshot_bindings() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind probe mock");
    let addr = listener.local_addr().expect("local addr");
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let reader_stream = stream.try_clone().expect("clone stream");
        let mut reader = BufReader::new(reader_stream);
        let mut writer = BufWriter::new(stream);

        let mut raw_hello = String::new();
        reader.read_line(&mut raw_hello).expect("read hello");
        let hello_request: ViewerRequest =
            serde_json::from_str(raw_hello.trim_end()).expect("decode hello request");
        assert!(matches!(
            hello_request,
            ViewerRequest::Hello {
                version: VIEWER_PROTOCOL_VERSION,
                ..
            }
        ));
        serde_json::to_writer(
            &mut writer,
            &ViewerResponse::HelloAck {
                server: "oasis7".to_string(),
                version: VIEWER_PROTOCOL_VERSION,
                min_version: 1,
                max_version: VIEWER_PROTOCOL_VERSION,
                capabilities: Vec::new(),
                world_id: "test-world".to_string(),
                control_profile: oasis7::viewer::ViewerControlProfile::Playback,
                authority_epoch: None,
            },
        )
        .expect("write hello ack");
        writer.write_all(b"\n").expect("write newline");
        writer.flush().expect("flush hello ack");

        let mut snapshot_line = String::new();
        reader
            .read_line(&mut snapshot_line)
            .expect("read snapshot request");
        let snapshot_request: ViewerRequest =
            serde_json::from_str(snapshot_line.trim_end()).expect("decode snapshot request");
        assert!(matches!(snapshot_request, ViewerRequest::RequestSnapshot));

        let mut model = WorldModel::default();
        model
            .agent_player_bindings
            .insert("agent-1".to_string(), "player-a".to_string());
        model
            .agent_player_bindings
            .insert("agent-2".to_string(), "player-b".to_string());
        let snapshot = WorldSnapshot {
            version: 1,
            chunk_generation_schema_version: 1,
            time: 0,
            config: WorldConfig::default(),
            model,
            runtime_snapshot: None,
            player_gameplay: None,
            chain_resource_manifest: Default::default(),
            latest_chain_resource_delta: Default::default(),
            chunk_runtime: Default::default(),
            intel_ttl_ticks: 0,
            next_event_id: 0,
            next_action_id: 0,
            pending_actions: Vec::new(),
            journal_len: 0,
        };
        serde_json::to_writer(&mut writer, &ViewerResponse::Snapshot { snapshot })
            .expect("write snapshot");
        writer.write_all(b"\n").expect("write newline");
        writer.flush().expect("flush snapshot");
    });

    let players = query_runtime_bound_players(format!("{addr}").as_str()).expect("query players");
    assert!(players.contains("player-a"));
    assert!(players.contains("player-b"));
    assert_eq!(players.len(), 2);
    handle.join().expect("join mock server");
}
