use super::cli::TrafficProfile;
use super::cli::parse_validator_signer_public_key_spec;
use super::status_server_support::status_http_request_is_complete;
use super::{
    CliOptions, DEFAULT_NODE_ID, DEFAULT_REPLICATION_NETWORK_LISTEN, DEFAULT_STATUS_BIND,
    DEFAULT_WORLD_ID, apply_traffic_profile_to_node_config,
    build_chain_balances_payload_from_world, build_default_peer_record,
    build_default_replication_network_config, build_live_node_network_policy_recommendation,
    build_node_replication_config, build_replication_fetch_requester_allowlist,
    build_replication_remote_writer_allowlist, node_keypair_config, parse_options,
    parse_validator_spec, reserve_startup_reconcile_bind_guard,
};
use ed25519_dalek::SigningKey;
use oasis7_node::{
    Libp2pReachabilitySnapshot, LiveAutoNatStatus, LiveHolePunchState, LivePublicPortReachability,
    LiveTransportKind, NodeAutoNatStatus, NodeConfig, NodeHolePunchViability, NodeNetworkPolicy,
    NodePublicPortReachability, NodeRole, NodeUserMode,
};
use oasis7_proto::distributed_dht::{
    PeerDeploymentMode, PeerDiscoverySource, PeerNodeRole, PeerReachabilityClass,
};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use std::net::{TcpListener, UdpSocket};
use std::path::Path;

fn unused_loopback_tcp_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve unused tcp port");
    listener.local_addr().expect("tcp addr").to_string()
}

#[test]
fn status_http_request_waits_for_full_content_length_body() {
    let partial = b"POST /v1/chain/gameplay/submit HTTP/1.1\r\nContent-Length: 11\r\n\r\n{\"ok\":";
    assert!(
        !status_http_request_is_complete(partial).expect("partial request should parse headers")
    );

    let complete =
        b"POST /v1/chain/gameplay/submit HTTP/1.1\r\nContent-Length: 11\r\n\r\n{\"ok\":true}";
    assert!(status_http_request_is_complete(complete).expect("complete request should parse"));
}

#[test]
fn status_http_request_without_body_is_complete_after_headers() {
    let request = b"GET /v1/chain/status HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    assert!(status_http_request_is_complete(request).expect("GET request should parse"));
}

#[test]
fn parse_options_defaults() {
    let options = parse_options(std::iter::empty()).expect("parse should succeed");
    assert_eq!(options.node_id, DEFAULT_NODE_ID);
    assert_eq!(options.world_id, DEFAULT_WORLD_ID);
    assert_eq!(options.status_bind, DEFAULT_STATUS_BIND);
    assert_eq!(options.storage_profile, StorageProfile::DevLocal);
    assert_eq!(options.traffic_profile, TrafficProfile::Default);
    assert_eq!(options.p2p_user_mode, NodeUserMode::AutoJoin);
    assert!(!options.p2p_accept_public_entry);
    assert_eq!(options.p2p_detected_reachability, None);
    assert_eq!(
        options.p2p_detected_hole_punch_viability,
        NodeHolePunchViability::Unknown
    );
    assert!(!options.p2p_detected_relay_available);
    assert!(!options.p2p_detected_probe_stable);
    assert!(!options.p2p_detected_reachability_explicit);
    assert!(!options.p2p_detected_hole_punch_viability_explicit);
    assert!(!options.p2p_detected_relay_available_explicit);
    assert!(!options.p2p_detected_probe_stable_explicit);
    assert!(!options.node_auto_attest_all_validators);
    assert!(options.node_validators.is_empty());
    assert!(options.reward_runtime_enabled);
    assert!(options.reward_runtime_epoch_duration_secs.is_none());
    let pos_defaults = oasis7::chain_pos_defaults::defaults();
    assert_eq!(options.pos_slot_duration_ms, pos_defaults.slot_duration_ms);
    assert_eq!(options.pos_ticks_per_slot, pos_defaults.ticks_per_slot);
    assert_eq!(
        options.pos_proposal_tick_phase,
        pos_defaults.proposal_tick_phase
    );
    assert!(!options.pos_adaptive_tick_scheduler_enabled);
    assert!(options.pos_slot_clock_genesis_unix_ms.is_none());
    assert_eq!(
        options.pos_max_past_slot_lag,
        pos_defaults.max_past_slot_lag
    );
    assert!(options.replication_network_listen_addrs.is_empty());
    assert!(options.replication_network_bootstrap_peers.is_empty());
    assert!(options.replication_fetch_requester_public_keys.is_empty());
    assert!(options.provider_backed_bootstrap_authority_paths.is_empty());
    assert!(options.deployment_inventory_path.is_none());
    assert!(options.runtime_root.is_none());
    assert!(options.replication_root.is_none());
    assert_eq!(options.p2p_max_ipv4_subnet_active_peers, None);
}

#[test]
fn parse_options_reads_custom_values() {
    let options = parse_options(
        [
            "--node-id",
            "node-a",
            "--world-id",
            "live-foo",
            "--storage-profile",
            "soak_forensics",
            "--traffic-profile",
            "triad_low_traffic",
            "--status-bind",
            "127.0.0.1:6221",
            "--node-role",
            "storage",
            "--node-tick-ms",
            "350",
            "--pos-slot-duration-ms",
            "8000",
            "--pos-ticks-per-slot",
            "10",
            "--pos-proposal-tick-phase",
            "9",
            "--pos-adaptive-tick-scheduler",
            "--pos-slot-clock-genesis-unix-ms",
            "1700000000000",
            "--pos-max-past-slot-lag",
            "32",
            "--node-validator",
            "node-a:55",
            "--node-validator",
            "node-b:45",
            "--node-auto-attest-all",
            "--p2p-max-ipv4-subnet-active-peers",
            "3",
            "--execution-world-dir",
            "custom/world",
            "--provider-bootstrap-authority",
            "authority-a.json",
            "--provider-bootstrap-authority",
            "authority-b.json",
            "--runtime-root",
            "custom/runtime",
            "--replication-root",
            "custom/replication",
            "--reward-runtime-epoch-duration-secs",
            "60",
            "--reward-points-per-credit",
            "100",
            "--reward-runtime-auto-redeem",
            "--reward-initial-reserve-power-units",
            "50000",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    assert_eq!(options.node_id, "node-a");
    assert_eq!(options.world_id, "live-foo");
    assert_eq!(options.storage_profile, StorageProfile::SoakForensics);
    assert_eq!(options.traffic_profile, TrafficProfile::TriadLowTraffic);
    assert_eq!(options.status_bind, "127.0.0.1:6221");
    assert_eq!(options.node_role.as_str(), "storage");
    assert_eq!(options.p2p_deployment_mode, PeerDeploymentMode::Private);
    assert_eq!(options.p2p_node_role, PeerNodeRole::FullStorage);
    assert_eq!(options.node_tick_ms, 350);
    assert_eq!(options.pos_slot_duration_ms, 8_000);
    assert_eq!(options.pos_ticks_per_slot, 10);
    assert_eq!(options.pos_proposal_tick_phase, 9);
    assert!(options.pos_adaptive_tick_scheduler_enabled);
    assert_eq!(
        options.pos_slot_clock_genesis_unix_ms,
        Some(1_700_000_000_000)
    );
    assert_eq!(options.pos_max_past_slot_lag, 32);
    assert_eq!(options.node_validators.len(), 2);
    assert!(options.node_validator_signer_public_keys.is_empty());
    assert!(options.node_auto_attest_all_validators);
    assert_eq!(options.p2p_max_ipv4_subnet_active_peers, Some(3));
    assert_eq!(options.reward_runtime_epoch_duration_secs, Some(60));
    assert_eq!(options.reward_points_per_credit, 100);
    assert!(options.reward_runtime_auto_redeem);
    assert_eq!(options.reward_initial_reserve_power_units, 50_000);
    assert_eq!(
        options.provider_backed_bootstrap_authority_paths,
        vec![
            Path::new("authority-a.json").to_path_buf(),
            Path::new("authority-b.json").to_path_buf(),
        ]
    );
    assert!(options.deployment_inventory_path.is_none());
    assert_eq!(
        options
            .execution_world_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        Some("custom/world".to_string())
    );
    assert_eq!(
        options
            .runtime_root
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        Some("custom/runtime".to_string())
    );
    assert_eq!(
        options
            .replication_root
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        Some("custom/replication".to_string())
    );
}

#[test]
fn parse_options_rejects_invalid_ipv4_subnet_active_peer_limit() {
    let err = parse_options(["--p2p-max-ipv4-subnet-active-peers", "0"].into_iter())
        .expect_err("should reject zero peer limit");
    assert!(err.contains("--p2p-max-ipv4-subnet-active-peers requires a positive integer"));
}

#[test]
fn parse_options_rejects_invalid_status_bind() {
    let err = parse_options(["--status-bind", "127.0.0.1"].into_iter())
        .expect_err("should reject invalid bind");
    assert!(err.contains("<host:port>"));
}

#[test]
fn parse_options_rejects_peer_without_bind() {
    let err = parse_options(["--node-gossip-peer", "127.0.0.1:9001"].into_iter())
        .expect_err("should reject peer without bind");
    assert!(err.contains("requires --node-gossip-bind"));
}

#[test]
fn startup_reconcile_bind_guard_rejects_occupied_status_bind() {
    let occupied = TcpListener::bind("127.0.0.1:0").expect("reserve status port");
    let status_addr = occupied.local_addr().expect("status addr").to_string();
    let mut options =
        parse_options(["--status-bind", status_addr.as_str()].into_iter()).expect("parse options");
    options.replication_network_listen_addrs = vec!["/ip4/127.0.0.1/tcp/0".to_string()];

    let err = reserve_startup_reconcile_bind_guard(&options)
        .expect_err("occupied status bind must fail before startup reconcile");
    assert!(err.contains("startup reconcile preflight failed: status bind"));
}

#[test]
fn startup_reconcile_bind_guard_rejects_occupied_gossip_bind() {
    let occupied = UdpSocket::bind("127.0.0.1:0").expect("reserve gossip port");
    let gossip_addr = occupied.local_addr().expect("gossip addr").to_string();
    let mut options = parse_options(
        [
            "--status-bind",
            unused_loopback_tcp_addr().as_str(),
            "--node-gossip-bind",
            gossip_addr.as_str(),
        ]
        .into_iter(),
    )
    .expect("parse options");
    options.replication_network_listen_addrs = vec!["/ip4/127.0.0.1/tcp/0".to_string()];

    let err = reserve_startup_reconcile_bind_guard(&options)
        .expect_err("occupied gossip bind must fail before startup reconcile");
    assert!(err.contains("startup reconcile preflight failed: gossip bind"));
}

#[test]
fn startup_reconcile_bind_guard_rejects_occupied_replication_tcp_listen() {
    let occupied = TcpListener::bind("127.0.0.1:0").expect("reserve replication port");
    let port = occupied.local_addr().expect("replication addr").port();
    let status_addr = unused_loopback_tcp_addr();
    let mut options =
        parse_options(["--status-bind", status_addr.as_str()].into_iter()).expect("parse options");
    options.replication_network_listen_addrs = vec![format!("/ip4/127.0.0.1/tcp/{port}")];

    let err = reserve_startup_reconcile_bind_guard(&options)
        .expect_err("occupied replication listen must fail before startup reconcile");
    assert!(err.contains("startup reconcile preflight failed: replication tcp listen"));
}

#[test]
fn startup_reconcile_bind_guard_rejects_occupied_replication_ipv6_tcp_listen() {
    let Ok(occupied) = TcpListener::bind("[::1]:0") else {
        return;
    };
    let port = occupied.local_addr().expect("replication addr").port();
    let status_addr = unused_loopback_tcp_addr();
    let mut options =
        parse_options(["--status-bind", status_addr.as_str()].into_iter()).expect("parse options");
    options.replication_network_listen_addrs = vec![format!("/ip6/::1/tcp/{port}")];

    let err = reserve_startup_reconcile_bind_guard(&options)
        .expect_err("occupied ipv6 replication listen must fail before startup reconcile");
    assert!(err.contains("startup reconcile preflight failed: replication tcp listen"));
}

#[test]
fn startup_reconcile_bind_guard_rejects_unsupported_replication_listen_addr() {
    let status_addr = unused_loopback_tcp_addr();
    let mut options =
        parse_options(["--status-bind", status_addr.as_str()].into_iter()).expect("parse options");
    options.replication_network_listen_addrs = vec!["/dns4/example.test/tcp/30333".to_string()];

    let err = reserve_startup_reconcile_bind_guard(&options)
        .expect_err("unsupported replication listen must fail closed before startup reconcile");
    assert!(err.contains("unsupported replication listen address"));
}

#[test]
fn parse_options_reads_validator_signer_public_key_overrides() {
    let options = parse_options(
        [
            "--node-validator",
            "node-a:55",
            "--node-validator-signer-public-key",
            "node-a:1111111111111111111111111111111111111111111111111111111111111111",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    assert_eq!(
        options
            .node_validator_signer_public_keys
            .get("node-a")
            .map(String::as_str),
        Some("1111111111111111111111111111111111111111111111111111111111111111")
    );
}

#[test]
fn parse_validator_signer_public_key_spec_rejects_missing_public_key() {
    let err = parse_validator_signer_public_key_spec("node-a:")
        .expect_err("missing signer public key must fail");
    assert!(err.contains("public_key_hex cannot be empty"));
}

#[test]
fn sequencer_requires_execution_commit_but_all_runtime_roles_materialize_execution_state() {
    assert!(super::node_role_requires_execution_commit(
        NodeRole::Sequencer
    ));
    assert!(!super::node_role_requires_execution_commit(
        NodeRole::Observer
    ));
    assert!(!super::node_role_requires_execution_commit(
        NodeRole::Storage
    ));

    for role in [NodeRole::Sequencer, NodeRole::Observer, NodeRole::Storage] {
        assert!(
            super::node_role_materializes_execution_state(role),
            "expected {role:?} to materialize execution state"
        );
    }
}

#[test]
fn parse_options_reads_replication_network_addresses() {
    let options = parse_options(
        [
            "--replication-network-listen",
            "/ip4/127.0.0.1/tcp/19641",
            "--replication-network-listen",
            "/ip4/127.0.0.1/udp/19642/quic-v1",
            "--replication-network-peer",
            "/ip4/127.0.0.1/tcp/19651",
            "--replication-network-peer",
            "/ip4/127.0.0.1/udp/19652/quic-v1",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    assert_eq!(
        options
            .replication_network_listen_addrs
            .iter()
            .map(|addr| addr.to_string())
            .collect::<Vec<_>>(),
        vec![
            "/ip4/127.0.0.1/tcp/19641".to_string(),
            "/ip4/127.0.0.1/udp/19642/quic-v1".to_string(),
        ]
    );
    assert_eq!(
        options
            .replication_network_bootstrap_peers
            .iter()
            .map(|addr| addr.to_string())
            .collect::<Vec<_>>(),
        vec![
            "/ip4/127.0.0.1/tcp/19651".to_string(),
            "/ip4/127.0.0.1/udp/19652/quic-v1".to_string(),
        ]
    );
}

#[test]
fn parse_options_reads_replication_fetch_requester_public_keys() {
    let options = parse_options(
        [
            "--replication-fetch-requester-public-key",
            "deadbeef",
            "--replication-fetch-requester-public-key",
            "cafebabe",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    assert_eq!(
        options.replication_fetch_requester_public_keys,
        vec!["deadbeef".to_string(), "cafebabe".to_string()]
    );
}

#[test]
fn parse_options_rejects_proposal_tick_phase_out_of_range() {
    let err = parse_options(
        [
            "--pos-ticks-per-slot",
            "4",
            "--pos-proposal-tick-phase",
            "4",
        ]
        .into_iter(),
    )
    .expect_err("proposal tick phase out of range");
    assert!(err.contains("--pos-proposal-tick-phase"));
}

#[test]
fn parse_options_rejects_unknown_storage_profile() {
    let err = parse_options(["--storage-profile", "unknown"].into_iter())
        .expect_err("invalid storage profile should fail");
    assert!(err.contains("dev_local"));
    assert!(err.contains("soak_forensics"));
}

#[test]
fn parse_options_rejects_unknown_traffic_profile() {
    let err = parse_options(["--traffic-profile", "burst_mode"].into_iter())
        .expect_err("invalid traffic profile should fail");
    assert!(err.contains("triad_low_traffic"));
}

#[test]
fn parse_validator_spec_rejects_zero_stake() {
    let err = parse_validator_spec("node-a:0").expect_err("should reject");
    assert!(err.contains("positive integer"));
}

#[test]
fn balances_payload_reports_empty_world_without_error() {
    let world = oasis7::runtime_host::world::new();
    let payload = build_chain_balances_payload_from_world(
        "node-a",
        "live-a",
        std::path::Path::new("/tmp/empty"),
        &world,
    );
    assert!(payload.ok);
    assert!(payload.load_error.is_none());
    assert_eq!(payload.node_power_credit_balance, 0);
    assert_eq!(payload.reward_mint_record_count, 0);
    assert!(payload.recent_reward_mint_records.is_empty());
}

#[test]
fn parse_options_rejects_unknown_option() {
    let err = parse_options(["--unknown"].into_iter()).expect_err("should fail");
    assert!(err.contains("unknown option"));
}

fn hosted_local_mock_chain_args() -> Vec<String> {
    vec![
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
        "--agent-execution-lane".to_string(),
        "player_parity".to_string(),
    ]
}

#[cfg(feature = "test_tier_required")]
#[test]
fn parse_options_accepts_hosted_local_mock_funding_tuple() {
    let args = hosted_local_mock_chain_args();
    let options = parse_options(args.iter().map(String::as_str))
        .expect("chain runtime should admit the explicit test-tier tuple");
    assert!(options.chain_local_standalone_test);
    assert_eq!(
        options.agent_decision_source.as_deref(),
        Some("provider_backed")
    );
    assert_eq!(
        options.agent_provider_backend.as_deref(),
        Some("provider_local_mock")
    );
    assert_eq!(
        options.agent_provider_contract.as_deref(),
        Some("worldsim_provider_v1")
    );
    assert_eq!(
        options.agent_provider_transport.as_deref(),
        Some("loopback_http")
    );
    assert_eq!(
        options.agent_execution_lane.as_deref(),
        Some("player_parity")
    );
}

#[cfg(not(feature = "test_tier_required"))]
#[test]
fn parse_options_rejects_hosted_local_mock_funding_without_test_tier() {
    let args = hosted_local_mock_chain_args();
    let error = parse_options(args.iter().map(String::as_str))
        .expect_err("production chain runtime must reject Hosted local-mock funding");
    assert!(error.contains("test_tier_required"), "{error}");
}

#[cfg(feature = "test_tier_required")]
#[test]
fn parse_options_rejects_hosted_local_mock_funding_with_wrong_tuple() {
    let mut args = hosted_local_mock_chain_args();
    let index = args
        .iter()
        .position(|arg| arg == "player_parity")
        .expect("execution lane argument");
    args[index] = "headless_agent".to_string();
    let error = parse_options(args.iter().map(String::as_str))
        .expect_err("Hosted local-mock funding must require player_parity");
    assert!(error.contains("player_parity"), "{error}");
}

#[cfg(feature = "test_tier_required")]
#[test]
fn parse_options_rejects_hosted_local_mock_funding_without_authority_setup() {
    let mut args = hosted_local_mock_chain_args();
    for flag in [
        "--local-test-provider-authority",
        "--local-test-provider-wasm",
        "--local-test-provider-metadata",
        "--local-test-provider-finality-block-hash",
        "--local-test-provider-session-mode",
    ] {
        let index = args
            .iter()
            .position(|arg| arg == flag)
            .expect("local provider argument");
        args.drain(index..=index + 1);
    }
    let error = parse_options(args.iter().map(String::as_str))
        .expect_err("provider tuple must not grant synthetic local authority");
    assert!(
        error.contains(
            "explicit local-provider/agent engineering options require complete provider setup"
        ),
        "{error}"
    );
}

#[test]
fn default_runtime_paths_depend_on_node_id() {
    let options = CliOptions {
        node_id: "node-z".to_string(),
        ..CliOptions::default()
    };
    let paths = super::resolve_runtime_paths(&options);
    assert!(
        paths
            .execution_world_dir
            .to_string_lossy()
            .contains("output/chain-runtime/node-z")
    );
    assert!(
        paths
            .runtime_root
            .to_string_lossy()
            .contains("output/chain-runtime/node-z")
    );
    assert!(
        paths
            .replication_root
            .to_string_lossy()
            .contains("output/node-distfs/node-z")
    );
}

#[test]
fn custom_runtime_root_overrides_default_runtime_paths() {
    let options = CliOptions {
        node_id: "node-z".to_string(),
        runtime_root: Some("custom/runtime".into()),
        ..CliOptions::default()
    };
    let paths = super::resolve_runtime_paths(&options);
    assert_eq!(paths.runtime_root, Path::new("custom/runtime"));
    assert_eq!(
        paths.reward_runtime_report_dir,
        Path::new("custom/runtime").join("reward-runtime-report")
    );
    assert_eq!(
        paths.reward_runtime_state_path,
        Path::new("custom/runtime").join("reward-runtime-state.json")
    );
}

#[test]
fn custom_replication_root_overrides_default_runtime_path() {
    let options = CliOptions {
        node_id: "node-z".to_string(),
        replication_root: Some("custom/replication".into()),
        ..CliOptions::default()
    };
    let paths = super::resolve_runtime_paths(&options);
    assert_eq!(paths.replication_root, Path::new("custom/replication"));
}

#[test]
fn default_replication_network_config_uses_loopback_ephemeral_listen() {
    let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let config = build_default_replication_network_config(&CliOptions::default(), &keypair)
        .expect("default replication network config should build");
    assert_eq!(config.listen_addrs.len(), 1);
    assert_eq!(
        config.listen_addrs[0].to_string(),
        DEFAULT_REPLICATION_NETWORK_LISTEN
    );
    assert!(config.bootstrap_peers.is_empty());
    assert!(config.enable_autonat);
    assert!(!config.allow_local_handler_fallback_when_no_peers);
    assert_eq!(config.bootstrap_redial_interval_ms, 1_000);
    assert_eq!(config.discovery_query_interval_ms, 15_000);
    assert_eq!(config.republish_interval_ms, 5 * 60 * 1000);
    assert!(config.keypair.is_some());
    let peer_record = config.peer_record.expect("peer record");
    assert_eq!(peer_record.node_id, DEFAULT_NODE_ID);
    assert_eq!(peer_record.node_role, "validator_core");
    assert_eq!(peer_record.deployment_mode, PeerDeploymentMode::Private);
    assert_eq!(
        peer_record.reachability_class,
        PeerReachabilityClass::Private
    );
    assert_eq!(
        peer_record.discovery_sources,
        vec![
            PeerDiscoverySource::StaticBootstrap,
            PeerDiscoverySource::Dht
        ]
    );
}

#[test]
fn default_replication_network_config_uses_explicit_topology_addrs() {
    let signing_key = SigningKey::from_bytes(&[10_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let options = parse_options(
        [
            "--replication-network-listen",
            "/ip4/127.0.0.1/tcp/19741",
            "--replication-network-peer",
            "/ip4/127.0.0.1/tcp/19742",
            "--replication-network-peer",
            "/ip4/127.0.0.1/tcp/19743",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");
    let config = build_default_replication_network_config(&options, &keypair)
        .expect("explicit replication network config should build");

    assert_eq!(
        config
            .listen_addrs
            .iter()
            .map(|addr| addr.to_string())
            .collect::<Vec<_>>(),
        vec!["/ip4/127.0.0.1/tcp/19741".to_string()]
    );
    assert_eq!(
        config
            .bootstrap_peers
            .iter()
            .map(|addr| addr.to_string())
            .collect::<Vec<_>>(),
        vec![
            "/ip4/127.0.0.1/tcp/19742".to_string(),
            "/ip4/127.0.0.1/tcp/19743".to_string(),
        ]
    );
}

#[test]
fn triad_low_traffic_replication_network_config_slows_control_plane_churn() {
    let signing_key = SigningKey::from_bytes(&[12_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let options = parse_options(
        [
            "--traffic-profile",
            "triad_low_traffic",
            "--replication-network-peer",
            "/ip4/39.104.204.172/tcp/5611",
            "--replication-network-peer",
            "/ip4/39.104.205.67/tcp/5612",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");
    let config = build_default_replication_network_config(&options, &keypair)
        .expect("low-traffic replication network config should build");

    assert_eq!(config.bootstrap_redial_interval_ms, 10_000);
    assert_eq!(config.discovery_query_interval_ms, 180_000);
    assert_eq!(config.republish_interval_ms, 30 * 60 * 1000);
    assert!(!config.enable_autonat);
}

#[test]
fn replication_network_config_applies_ipv4_subnet_active_peer_limit() {
    let signing_key = SigningKey::from_bytes(&[13_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let options = parse_options(
        [
            "--p2p-max-ipv4-subnet-active-peers",
            "3",
            "--replication-network-peer",
            "/ip4/39.104.204.172/tcp/5611",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");
    let config = build_default_replication_network_config(&options, &keypair)
        .expect("replication network config should build");

    assert_eq!(
        config.peer_manager_policy.max_ipv4_subnet_active_peers,
        Some(3)
    );
}

#[test]
fn triad_low_traffic_node_profile_reduces_gossip_and_feedback_budgets() {
    let options = parse_options(
        [
            "--node-role",
            "storage",
            "--traffic-profile",
            "triad_low_traffic",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");
    let base = NodeConfig::new("node-a", "world-a", NodeRole::Storage)
        .expect("base node config should build");
    let config = apply_traffic_profile_to_node_config(base, &options)
        .expect("profile application should succeed");

    assert_eq!(config.max_dynamic_gossip_peers, 8);
    assert_eq!(config.dynamic_gossip_peer_ttl_ms, 60 * 60 * 1000);
    let feedback = config
        .feedback_p2p
        .expect("storage role should keep feedback p2p");
    assert_eq!(feedback.max_incoming_announces_per_tick, 8);
    assert_eq!(feedback.max_outgoing_announces_per_tick, 8);
}

#[test]
fn build_default_peer_record_tracks_runtime_identity_boundary() {
    let options = CliOptions {
        node_id: "node-p2p".to_string(),
        world_id: "world-p2p".to_string(),
        node_role: NodeRole::Storage,
        p2p_node_role: PeerNodeRole::FullStorage,
        ..CliOptions::default()
    };
    let record = build_default_peer_record(&options);
    assert!(record.peer_id.is_empty());
    assert_eq!(record.node_id, "node-p2p");
    assert_eq!(record.world_id, "world-p2p");
    assert_eq!(record.network_id, "world-p2p");
    assert_eq!(record.node_role, "full_storage");
    assert_eq!(record.deployment_mode, PeerDeploymentMode::Private);
    assert_eq!(record.reachability_class, PeerReachabilityClass::Private);
}

#[test]
fn parse_options_reads_explicit_p2p_policy_overrides() {
    let options = parse_options(
        [
            "--node-role",
            "observer",
            "--p2p-deployment-mode",
            "public",
            "--p2p-node-role",
            "relay",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");
    assert_eq!(options.node_role, NodeRole::Observer);
    assert_eq!(options.p2p_deployment_mode, PeerDeploymentMode::Public);
    assert_eq!(options.p2p_node_role, PeerNodeRole::Relay);
}

#[test]
fn parse_options_reads_user_mode_detection_hints() {
    let options = parse_options(
        [
            "--node-role",
            "storage",
            "--p2p-user-mode",
            "auto_join",
            "--p2p-accept-public-entry",
            "--p2p-detected-reachability",
            "public",
            "--p2p-detected-hole-punch",
            "viable",
            "--p2p-detected-probe-unstable",
            "--p2p-detected-relay-unavailable",
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    assert_eq!(options.p2p_user_mode, NodeUserMode::AutoJoin);
    assert!(options.p2p_accept_public_entry);
    assert_eq!(
        options.p2p_detected_reachability,
        Some(PeerReachabilityClass::Public)
    );
    assert_eq!(
        options.p2p_detected_hole_punch_viability,
        NodeHolePunchViability::Viable
    );
    assert!(!options.p2p_detected_probe_stable);
    assert!(!options.p2p_detected_relay_available);
    assert!(options.p2p_detected_reachability_explicit);
    assert!(options.p2p_detected_hole_punch_viability_explicit);
    assert!(options.p2p_detected_relay_available_explicit);
    assert!(options.p2p_detected_probe_stable_explicit);
}

#[test]
fn live_reachability_snapshot_upgrades_detection_when_cli_hints_are_absent() {
    let options = parse_options(["--node-role", "observer"].into_iter()).expect("parse");
    let (recommendation, detection) = build_live_node_network_policy_recommendation(
        &options,
        Some(&Libp2pReachabilitySnapshot {
            active_transport_kind: Some(LiveTransportKind::HolePunched),
            active_direct_path_count: 0,
            active_hole_punch_path_count: 1,
            active_relay_path_count: 0,
            relay_reservation_active: false,
            hole_punch_state: LiveHolePunchState::Viable,
            ..Libp2pReachabilitySnapshot::default()
        }),
    )
    .expect("recommendation");

    assert_eq!(
        detection.observed_reachability,
        Some(PeerReachabilityClass::Hybrid)
    );
    assert_eq!(
        detection.hole_punch_viability,
        NodeHolePunchViability::Viable
    );
    assert!(!detection.relay_available);
    assert!(detection.probe_stable);
    assert_eq!(
        recommendation.recommended_user_mode,
        NodeUserMode::PublicEntry
    );
    assert_eq!(
        recommendation.effective_user_mode,
        NodeUserMode::PrivateSafe
    );
}

#[test]
fn relay_reservation_without_active_relay_path_does_not_claim_relay_only_reachability() {
    let options = parse_options(["--node-role", "observer"].into_iter()).expect("parse");
    let (_, detection) = build_live_node_network_policy_recommendation(
        &options,
        Some(&Libp2pReachabilitySnapshot {
            active_transport_kind: None,
            active_direct_path_count: 0,
            active_hole_punch_path_count: 0,
            active_relay_path_count: 0,
            relay_reservation_active: true,
            hole_punch_state: LiveHolePunchState::Unknown,
            ..Libp2pReachabilitySnapshot::default()
        }),
    )
    .expect("recommendation");

    assert_eq!(detection.observed_reachability, None);
    assert!(detection.relay_available);
    assert!(detection.probe_stable);
}

#[test]
fn direct_and_relay_active_paths_do_not_auto_promote_to_public_entry() {
    let options = parse_options(["--node-role", "observer"].into_iter()).expect("parse");
    let (recommendation, detection) = build_live_node_network_policy_recommendation(
        &options,
        Some(&Libp2pReachabilitySnapshot {
            active_transport_kind: Some(LiveTransportKind::RelayReserved),
            active_direct_path_count: 1,
            active_hole_punch_path_count: 0,
            active_relay_path_count: 1,
            relay_reservation_active: true,
            hole_punch_state: LiveHolePunchState::Unknown,
            ..Libp2pReachabilitySnapshot::default()
        }),
    )
    .expect("recommendation");

    assert_eq!(
        detection.observed_reachability,
        Some(PeerReachabilityClass::RelayOnly)
    );
    assert_eq!(
        recommendation.recommended_user_mode,
        NodeUserMode::PrivateSafe
    );
    assert_eq!(
        recommendation.effective_user_mode,
        NodeUserMode::PrivateSafe
    );
    assert!(detection.relay_available);
    assert!(detection.probe_stable);
}

#[test]
fn failed_hole_punch_snapshot_does_not_force_blocked_viability() {
    let options = parse_options(["--node-role", "observer"].into_iter()).expect("parse");
    let (_, detection) = build_live_node_network_policy_recommendation(
        &options,
        Some(&Libp2pReachabilitySnapshot {
            active_transport_kind: None,
            active_direct_path_count: 0,
            active_hole_punch_path_count: 0,
            active_relay_path_count: 0,
            relay_reservation_active: false,
            hole_punch_state: LiveHolePunchState::Unknown,
            ..Libp2pReachabilitySnapshot::default()
        }),
    )
    .expect("recommendation");

    assert_eq!(
        detection.hole_punch_viability,
        NodeHolePunchViability::Unknown
    );
    assert!(!detection.probe_stable);
}

#[test]
fn explicit_cli_detection_hints_override_live_snapshot() {
    let options = parse_options(
        [
            "--node-role",
            "observer",
            "--p2p-detected-reachability",
            "private",
            "--p2p-detected-hole-punch",
            "blocked",
            "--p2p-detected-relay-unavailable",
            "--p2p-detected-probe-unstable",
        ]
        .into_iter(),
    )
    .expect("parse");
    let (_, detection) = build_live_node_network_policy_recommendation(
        &options,
        Some(&Libp2pReachabilitySnapshot {
            active_transport_kind: Some(LiveTransportKind::HolePunched),
            active_direct_path_count: 0,
            active_hole_punch_path_count: 1,
            active_relay_path_count: 1,
            relay_reservation_active: true,
            hole_punch_state: LiveHolePunchState::Viable,
            ..Libp2pReachabilitySnapshot::default()
        }),
    )
    .expect("recommendation");

    assert_eq!(
        detection.observed_reachability,
        Some(PeerReachabilityClass::Private)
    );
    assert_eq!(
        detection.hole_punch_viability,
        NodeHolePunchViability::Blocked
    );
    assert!(!detection.relay_available);
    assert!(!detection.probe_stable);
}

#[test]
fn node_network_policy_rejects_incompatible_runtime_role_combo() {
    let err = NodeConfig::new("node-a", "world-a", NodeRole::Observer)
        .expect("config")
        .with_network_policy(NodeNetworkPolicy {
            deployment_mode: PeerDeploymentMode::ValidatorHidden,
            node_role_claim: PeerNodeRole::ValidatorCore,
        })
        .expect_err("observer cannot claim validator_core");
    assert!(
        matches!(err, oasis7_node::NodeError::InvalidConfig { reason } if reason.contains("cannot use network node_role_claim"))
    );
}

#[test]
fn build_node_replication_config_uses_storage_profile_budget() {
    let mut private_key_bytes = [0_u8; 32];
    getrandom::fill(&mut private_key_bytes).expect("OS randomness unavailable");
    let signing_key = SigningKey::from_bytes(&private_key_bytes);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::ReleaseDefault);
    let config = build_node_replication_config(
        "node-a",
        Path::new("custom/replication"),
        &keypair,
        &storage_profile,
        &[],
        &[],
        false,
    )
    .expect("replication config should build");

    assert_eq!(
        config.max_hot_commit_messages(),
        storage_profile.replication_max_hot_commit_messages
    );
    assert_eq!(config.root_dir, Path::new("custom/replication"));
}

#[test]
fn live_reachability_snapshot_promotes_public_entry_from_autonat_probe() {
    let options = parse_options(["--node-role", "observer"].into_iter()).expect("parse");
    let (recommendation, detection) = build_live_node_network_policy_recommendation(
        &options,
        Some(&Libp2pReachabilitySnapshot {
            autonat_status: LiveAutoNatStatus::Public,
            public_port_reachability: LivePublicPortReachability::Reachable,
            observed_public_addr: Some("/dns4/public.example/tcp/4001".to_string()),
            confirmed_external_direct_addrs: vec!["/dns4/public.example/tcp/4001".to_string()],
            ..Libp2pReachabilitySnapshot::default()
        }),
    )
    .expect("recommendation");

    assert_eq!(
        detection.observed_reachability,
        Some(PeerReachabilityClass::Public)
    );
    assert_eq!(detection.autonat_status, NodeAutoNatStatus::Public);
    assert_eq!(
        detection.public_port_reachability,
        NodePublicPortReachability::Reachable
    );
    assert!(detection.probe_stable);
    assert_eq!(
        recommendation.recommended_user_mode,
        NodeUserMode::PublicEntry
    );
}

#[test]
fn build_replication_remote_writer_allowlist_uses_only_validator_keys() {
    let validator_signers = ["bbbb".to_string(), "aaaa".to_string(), "bbbb".to_string()];

    let allowlist = build_replication_remote_writer_allowlist(validator_signers.iter());

    assert_eq!(allowlist, vec!["aaaa".to_string(), "bbbb".to_string()]);
}

#[test]
fn build_replication_fetch_requester_allowlist_combines_validator_and_explicit_keys() {
    let validator_signers = ["bbbb".to_string(), "aaaa".to_string(), "bbbb".to_string()];
    let explicit = vec!["cccc".to_string(), "aaaa".to_string(), "cccc".to_string()];

    let allowlist =
        build_replication_fetch_requester_allowlist(validator_signers.iter(), explicit.as_slice());

    assert_eq!(
        allowlist,
        vec!["aaaa".to_string(), "bbbb".to_string(), "cccc".to_string()]
    );
}

#[path = "oasis7_chain_runtime_tests_network.rs"]
mod network_tests;

#[path = "cli_contract_tests.rs"]
mod cli_contract_tests;

#[test]
fn engineering_parser_rejects_deployment_authority_and_malformed_signer_keys() {
    let error =
        parse_options(["--deployment-inventory", "custom/inventory.json"].into_iter()).unwrap_err();
    assert!(error.contains("engineering typed runtime cannot consume manifest/formal/guarded/deployment authority inputs"));
    let error = parse_options(
        [
            "--node-validator",
            "node-a:55",
            "--node-validator-signer-public-key",
            "node-a:deadbeef",
        ]
        .into_iter(),
    )
    .unwrap_err();
    assert!(error.contains("engineering validator signer identity is invalid or rebound"));
}
