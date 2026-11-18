use super::super::{cli, development_runtime, replication_network, resolve_runtime_paths, traffic_status};
use super::{CliOptions, SigningKey, node_keypair_config, unused_loopback_tcp_addr};

#[test]
fn typed_resource_budget_reaches_live_host_replication_status_and_rejects_invalid_limits() {
    use oasis7::development_node_config::P2pResourceBudget;

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
    let options = development_runtime::into_options(input.clone(), false).unwrap();
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
