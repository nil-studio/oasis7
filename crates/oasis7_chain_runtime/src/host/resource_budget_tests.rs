use super::super::{cli, development_runtime, replication_network, resolve_runtime_paths, traffic_status};
use super::{CliOptions, SigningKey, node_keypair_config, unused_loopback_tcp_addr};

#[test]
fn typed_resource_budget_reaches_live_host_replication_status_and_rejects_invalid_limits() {
    use oasis7_client_api::development_node_config::P2pResourceBudget;

    let mut input = development_runtime::from_options(&CliOptions::default()).unwrap();
    let raw_profile = r#"{"profile":"v1-discovery-resource-budget-1",
        "connections_total":3,"pending_connections_total":2,"pending_inbound_unknown":1,
        "pending_outbound":2,"connections_per_peer":1,"handshake_timeout_ms":2000,
        "bytes_in_per_window":1024,"bytes_out_per_window":2048,"bytes_total_per_window":3072}"#;
    let profile: P2pResourceBudget = serde_json::from_str(raw_profile).unwrap();
    input.runtime.p2p_resource_budget = profile;
    let listener: std::net::SocketAddr = unused_loopback_tcp_addr().parse().unwrap();
    input.runtime.replication_network_listen_addrs =
        vec![format!("/ip4/127.0.0.1/tcp/{}", listener.port())];
    let config_bytes = serde_json::to_vec(&input).unwrap();
    let original = read_fixture_config(&config_bytes).unwrap();
    let options = development_runtime::into_options(original, false).unwrap();
    assert_eq!(
        development_runtime::from_options(&options).unwrap().runtime.p2p_resource_budget,
        profile
    );
    let signing_key = SigningKey::from_bytes(&[73_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let paths = resolve_runtime_paths(&options);
    let network = replication_network::create_default_replication_network(&options, &keypair, &paths).unwrap();
    // The host API exposes status rather than the node test-only listener
    // accessor. Observe the actual listener through a loopback TCP connection.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    loop {
        if std::net::TcpStream::connect_timeout(&listener, std::time::Duration::from_millis(100)).is_ok() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "typed budget host did not listen");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    while network.traffic_metrics_snapshot().resource_budget.connections.unwrap().total != 0 {
        assert!(std::time::Instant::now() < deadline, "probe connection did not release");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let metrics = traffic_status::build_chain_traffic_status(&network, None);
    let budget = metrics.libp2p_replication.resource_budget;
    assert_eq!(budget.profile, "v1-discovery-resource-budget-1");
    assert!(budget.instance_id.is_some());
    assert_eq!(budget.connections.unwrap().established, 0);
    assert_eq!(budget.bytes.unwrap().charged_bytes.total, 0);
    let P2pResourceBudget::SubstreamV1(limits) = profile else { unreachable!() };
    assert_eq!(budget.limits, Some(limits));
    let mut second_options = options.clone();
    second_options.replication_network_listen_addrs = vec!["/ip4/127.0.0.1/tcp/0".into()];
    let second = replication_network::create_default_replication_network(&second_options, &keypair, &paths).unwrap();
    assert_eq!(network.peer_id(), second.peer_id());
    assert_ne!(budget.instance_id, second.traffic_metrics_snapshot().resource_budget.instance_id);

    input.runtime.p2p_resource_budget = serde_json::from_str(
        &raw_profile.replace(r#""pending_connections_total":2"#, r#""pending_connections_total":4"#),
    ).unwrap();
    assert!(development_runtime::into_options(input, false).is_err());
    let mut non_development = options;
    non_development.development_runtime = false;
    assert!(cli::validate_effective_options(&non_development).is_err());
}

#[test]
fn typed_local_provider_budget_reaches_real_host_network_and_status() {
    use oasis7_client_api::development_node_config::{
        DevelopmentLocalProviderV1, DevelopmentProviderSessionV1, P2pResourceBudget,
    };

    // Exercise the positive local-provider path, not merely the dev node
    // variant. The typed provider declaration is validated before the same
    // real host network builder consumes its selected resource budget.
    let mut config = development_runtime::from_options(&CliOptions::default()).unwrap();
    let profile: P2pResourceBudget = serde_json::from_str(
        r#"{"profile":"v1-discovery-resource-budget-1",
            "connections_total":3,"pending_connections_total":2,
            "pending_inbound_unknown":1,"pending_outbound":2,
            "connections_per_peer":1,"handshake_timeout_ms":2000,
            "bytes_in_per_window":1024,"bytes_out_per_window":2048,
            "bytes_total_per_window":3072}"#,
    )
    .unwrap();
    config.runtime.p2p_resource_budget = profile;
    config.local_provider = Some(DevelopmentLocalProviderV1 {
        authority_path: "local-provider-authority.json".into(),
        wasm_path: "local-provider.wasm".into(),
        metadata_path: "local-provider-metadata.json".into(),
        agent_id: "local-provider-budget-agent".into(),
        owner_binding: "local-provider-budget-owner".into(),
        finality_block_hash: format!("blake3:{}", "0".repeat(64)),
        session_mode: DevelopmentProviderSessionV1::Loopback,
        funding_lane: None,
    });
    let listener: std::net::SocketAddr = unused_loopback_tcp_addr().parse().unwrap();
    config.runtime.replication_network_listen_addrs =
        vec![format!("/ip4/127.0.0.1/tcp/{}", listener.port())];
    let original = read_fixture_config(&serde_json::to_vec(&config).unwrap()).unwrap();
    let options = development_runtime::into_options(original, true)
        .expect("typed local-provider accepts the original nonzero budget");
    assert!(options.development_runtime);
    assert_eq!(options.p2p_resource_budget, profile);
    assert!(options.local_test_provider_authority_path.is_some());
    assert!(options.local_test_provider_wasm_path.is_some());
    assert!(options.local_test_provider_metadata_path.is_some());
    assert_eq!(
        development_runtime::from_options(&options)
            .unwrap()
            .runtime
            .p2p_resource_budget,
        profile
    );

    let signing_key = SigningKey::from_bytes(&[74_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let paths = resolve_runtime_paths(&options);
    let network =
        replication_network::create_default_replication_network(&options, &keypair, &paths)
            .expect("real local-provider network must accept the shared resource budget");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    loop {
        if std::net::TcpStream::connect_timeout(
            &listener,
            std::time::Duration::from_millis(100),
        )
        .is_ok()
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "local-provider typed budget listener never became reachable"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    loop {
        let snapshot = network.traffic_metrics_snapshot().resource_budget;
        let occupancy = snapshot.connections.expect("live connection budget status");
        if occupancy.total == 0 {
            assert_eq!(snapshot.profile, "v1-discovery-resource-budget-1");
            assert!(snapshot.instance_id.is_some());
            assert_eq!(snapshot.bytes.unwrap().charged_bytes.total, 0);
            let P2pResourceBudget::SubstreamV1(limits) = profile else {
                unreachable!();
            };
            assert_eq!(snapshot.limits, Some(limits));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "local-provider probe connection did not release"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn normal_config_file_rejects_invalid_resource_budget_before_start() {
    let input = development_runtime::from_options(&CliOptions::default()).unwrap();
    let mut wire = serde_json::to_value(input).unwrap();
    wire["runtime"]["p2p_resource_budget"] = serde_json::json!({
        "profile": "v1-discovery-resource-budget-1",
        "connections_total": 3, "pending_connections_total": 2, "pending_inbound_unknown": 1,
        "pending_outbound": 2, "connections_per_peer": 1, "handshake_timeout_ms": 2000,
        "bytes_in_per_window": 1024, "bytes_out_per_window": 2048, "bytes_total_per_window": 3072
    });
    let mut invalid = wire.clone();
    invalid["runtime"]["p2p_resource_budget"]["bytes_out_per_window"] = serde_json::json!(0);
    assert!(read_fixture_config(&serde_json::to_vec(&invalid).unwrap()).is_err());

    let mut missing = wire.clone();
    missing["runtime"]["p2p_resource_budget"]
        .as_object_mut()
        .unwrap()
        .remove("pending_inbound_unknown");
    assert!(read_fixture_config(&serde_json::to_vec(&missing).unwrap()).is_err());

    let mut unknown = wire.clone();
    unknown["runtime"]["p2p_resource_budget"]["profile"] = serde_json::json!("future-version");
    assert!(read_fixture_config(&serde_json::to_vec(&unknown).unwrap()).is_err());

    let valid = serde_json::to_string(&wire).unwrap();
    assert!(read_fixture_config(valid.as_bytes()).is_ok());
    let duplicate = valid.replace(
        r#""connections_total":3"#,
        r#""connections_total":3,"connections_total":3"#,
    );
    assert_ne!(duplicate, valid);
    assert!(read_fixture_config(duplicate.as_bytes()).is_err());
    let overlimit = vec![b' '; 1024 * 1024 + 1];
    assert!(
        read_fixture_config(&overlimit)
            .unwrap_err()
            .contains("exceeds 1 MiB")
    );
}

fn read_fixture_config(
    bytes: &[u8],
) -> Result<oasis7_client_api::development_node_config::DevelopmentNodeConfigV1, String> {
    use std::io::Write;

    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let path = std::env::temp_dir().join(format!(
        "oasis7-resource-budget-{}.json",
        hex::encode(nonce)
    ));
    let mut fixture = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .unwrap();
    fixture.write_all(bytes).unwrap();
    drop(fixture);
    let result = development_runtime::read_config(&path);
    std::fs::remove_file(path).unwrap();
    result
}
/// The two development roles consume the same typed budget through the real
/// Host -> Node -> libp2p network, not merely JSON conversion or a status mock.
/// This does not replace the separate standard CLI process / all-transport matrix.
#[test]
fn typed_node_and_local_provider_budget_complete_real_rr_rejection_and_recovery() {
    use oasis7_client_api::development_node_config::{
        DevelopmentLocalProviderV1, DevelopmentProviderSessionV1, P2pResourceBudget,
    };
    use oasis7_proto::distributed::DistributedErrorCode;
    use oasis7_proto::world_error::WorldError;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};

    let profile: P2pResourceBudget = serde_json::from_str(
        r#"{"profile":"v1-discovery-resource-budget-1",
            "connections_total":4,"pending_connections_total":3,
            "pending_inbound_unknown":2,"pending_outbound":2,
            "connections_per_peer":2,"handshake_timeout_ms":10000,
            "bytes_in_per_window":8388608,"bytes_out_per_window":8388608,
            "bytes_total_per_window":16777216}"#,
    )
    .unwrap();
    let P2pResourceBudget::SubstreamV1(limits) = profile else {
        unreachable!();
    };

    let mut node_config = development_runtime::from_options(&CliOptions::default()).unwrap();
    node_config.runtime.p2p_resource_budget = profile;
    node_config.runtime.replication_network_listen_addrs =
        vec!["/ip4/127.0.0.1/tcp/0".into()];
    let mut node_options = development_runtime::into_options(
        read_fixture_config(&serde_json::to_vec(&node_config).unwrap()).unwrap(),
        false,
    )
    .unwrap();
    node_options.node_id = "budget-rr-node".into();

    let node_signer = SigningKey::from_bytes(&[75_u8; 32]);
    let node_key = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(node_signer.to_bytes()),
        public_key_hex: hex::encode(node_signer.verifying_key().to_bytes()),
    };
    let node_paths = resolve_runtime_paths(&node_options);
    let node = replication_network::create_default_replication_network(
        &node_options,
        &node_key,
        &node_paths,
    )
    .expect("typed dev-node listener");
    let deadline = Instant::now() + Duration::from_secs(15);
    let listen_addr = loop {
        if let Some(addr) = node
            .listening_addrs()
            .into_iter()
            .find(|addr| addr.to_string().contains("/tcp/"))
        {
            break addr;
        }
        assert!(
            Instant::now() < deadline,
            "typed dev-node did not bind: {:?}",
            node.debug_errors()
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    node.register_handler(
        "/aw/node/replication/ping",
        Box::new(|payload| {
            let mut out = payload.to_vec();
            out.extend_from_slice(b"-budget-ok");
            Ok(out)
        }),
    )
    .unwrap();
    let missing_count = Arc::new(AtomicUsize::new(0));
    node.register_handler(
        "/aw/node/replication/fetch-commit/1.0.0",
        Box::new({
            let missing_count = Arc::clone(&missing_count);
            move |_payload| {
                missing_count.fetch_add(1, Ordering::SeqCst);
                Err(WorldError::NetworkRequestFailed {
                    code: DistributedErrorCode::ErrNotFound,
                    message: "original commit not present".into(),
                    retryable: false,
                })
            }
        }),
    )
    .unwrap();

    let mut provider_config =
        development_runtime::from_options(&CliOptions::default()).unwrap();
    provider_config.runtime.p2p_resource_budget = profile;
    provider_config.runtime.replication_network_listen_addrs =
        vec!["/ip4/127.0.0.1/tcp/0".into()];
    provider_config.local_provider = Some(DevelopmentLocalProviderV1 {
        authority_path: "local-provider-authority.json".into(),
        wasm_path: "local-provider.wasm".into(),
        metadata_path: "local-provider-metadata.json".into(),
        agent_id: "budget-rr-provider-agent".into(),
        owner_binding: "budget-rr-provider-owner".into(),
        finality_block_hash: format!("blake3:{}", "0".repeat(64)),
        session_mode: DevelopmentProviderSessionV1::Loopback,
        funding_lane: None,
    });
    let mut provider_options = development_runtime::into_options(
        read_fixture_config(&serde_json::to_vec(&provider_config).unwrap()).unwrap(),
        true,
    )
    .expect("typed local-provider config");
    provider_options.node_id = "budget-rr-provider".into();
    provider_options.replication_network_bootstrap_peers =
        vec![format!("{listen_addr}/p2p/{}", node.peer_id())];

    let provider_signer = SigningKey::from_bytes(&[76_u8; 32]);
    let provider_key = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(provider_signer.to_bytes()),
        public_key_hex: hex::encode(provider_signer.verifying_key().to_bytes()),
    };
    let provider_paths = resolve_runtime_paths(&provider_options);
    let provider = replication_network::create_default_replication_network(
        &provider_options,
        &provider_key,
        &provider_paths,
    )
    .expect("typed local-provider real dialer");
    while !provider.connected_peers().contains(&node.peer_id()) {
        assert!(
            Instant::now() < deadline,
            "typed budget connection not established: dialer={:?}, node={:?}",
            provider.debug_errors(),
            node.debug_errors()
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    // Success, definitive remote rejection, and the next legitimate request
    // all pass through one authenticated live RR connection and budget.
    assert_eq!(
        provider.request("/aw/node/replication/ping", b"first").unwrap(),
        b"first-budget-ok"
    );
    let rejected = provider.request(
        "/aw/node/replication/fetch-commit/1.0.0",
        b"original-missing",
    );
    assert!(matches!(
        rejected.as_ref().map_err(|error| error.diagnostic_source()),
        Err(WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrNotFound,
            ..
        })
    ));
    assert_eq!(missing_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        provider.request("/aw/node/replication/ping", b"after-error").unwrap(),
        b"after-error-budget-ok"
    );

    for network in [&node, &provider] {
        let status = traffic_status::build_chain_traffic_status(network, None)
            .libp2p_replication
            .resource_budget;
        assert_eq!(status.profile, "v1-discovery-resource-budget-1");
        assert_eq!(status.limits, Some(limits));
        let bytes = status.bytes.expect("actual transport byte ledger");
        assert!(bytes.actual_bytes.inbound > 0);
        assert!(bytes.actual_bytes.outbound > 0);
        assert!(bytes.charged_bytes.total >= bytes.actual_bytes.total);
        assert!(bytes.charged_bytes.inbound <= limits.bytes_in_per_window.get());
        assert!(bytes.charged_bytes.outbound <= limits.bytes_out_per_window.get());
        assert!(bytes.charged_bytes.total <= limits.bytes_total_per_window.get());
        assert_eq!(
            bytes.charged_bytes.total,
            bytes.charged_bytes.inbound + bytes.charged_bytes.outbound
        );
    }
}

