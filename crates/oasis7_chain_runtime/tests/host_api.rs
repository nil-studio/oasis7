#![cfg(all(not(target_arch = "wasm32"), feature = "node-libp2p"))]

use oasis7_chain_runtime::{RuntimeConfig, run_cli};
use std::path::Path;

#[test]
fn parsed_configuration_exposes_resolved_paths() {
    let config = RuntimeConfig::parse(
        [
            "--node-id",
            "library-node",
            "--runtime-root",
            "custom/runtime",
        ]
        .into_iter(),
    )
    .unwrap();
    let paths = config.paths();
    assert_eq!(paths.runtime_root, Path::new("custom/runtime"));
    assert_eq!(paths.storage_root, Path::new("custom/runtime/store"));
    assert_eq!(
        paths.replication_root,
        Path::new("output/node-distfs/library-node")
    );
}

#[test]
fn cli_errors_return_without_terminating_the_library_caller() {
    assert!(run_cli(vec!["--unknown-library-option".into()]).is_err());
    assert!(RuntimeConfig::parse(["--unknown-library-option"].into_iter()).is_err());
    assert!(run_cli(vec!["--help".into()]).is_ok());
}

#[cfg(unix)]
mod shipping_budget_process {
    use std::{
        fs,
        io::{Read, Write},
        net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use serde_json::{Value, json};

    const PROFILE: &str = "v1-discovery-resource-budget-1";

    struct PrivateRoot(PathBuf);

    impl PrivateRoot {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "oasis7-dev-budget-binary-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path.canonicalize().unwrap())
        }
    }

    impl Drop for PrivateRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn tcp_port() -> u16 {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        socket.local_addr().unwrap().port()
    }

    fn udp_port() -> u16 {
        UdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn limits() -> Value {
        json!({
            "profile": PROFILE,
            "connections_total": 4,
            "pending_connections_total": 3,
            "pending_inbound_unknown": 2,
            "pending_outbound": 2,
            "connections_per_peer": 2,
            "handshake_timeout_ms": 10000,
            "bytes_in_per_window": 8388608,
            "bytes_out_per_window": 8388608,
            "bytes_total_per_window": 16777216
        })
    }

    fn config(root: &Path, status: SocketAddr, tcp: u16, udp: u16) -> Value {
        json!({
            "api_version": "oasis7.development-node/v1",
            "working_directory": root,
            "runtime": {
                "node_id": "budget-process-observer",
                "world_id": "budget-process-world",
                "node_role": "observer",
                "reward_runtime_enabled": false,
                "status_bind": status,
                "config_path": "node.toml",
                "runtime_root": "runtime",
                "replication_network_listen_addrs": [
                    format!("/ip4/127.0.0.1/tcp/{tcp}"),
                    format!("/ip4/127.0.0.1/udp/{udp}/quic-v1")
                ],
                "p2p_resource_budget": limits()
            }
        })
    }

    fn write_config(path: &Path, value: &Value) {
        fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    }

    fn binary() -> &'static str {
        env!("CARGO_BIN_EXE_oasis7_chain_runtime")
    }

    fn provider(value: &mut Value) {
        value["local_provider"] = json!({
            "authority_path": "provider-authority.json",
            "wasm_path": "provider.wasm",
            "metadata_path": "provider-metadata.json",
            "agent_id": "budget-process-agent",
            "owner_binding": "budget-process-owner",
            "finality_block_hash": format!("blake3:{}", "0".repeat(64)),
            "session_mode": "loopback"
        });
    }

    #[test]
    fn shipping_node_and_provider_reject_invalid_resource_budgets_before_runtime_effects() {
        let root = PrivateRoot::new();
        let path = root.0.join("typed.json");
        let status: SocketAddr = format!("127.0.0.1:{}", tcp_port()).parse().unwrap();
        let original = config(&root.0, status, tcp_port(), udp_port());

        // Both shipping command modes must refuse invalid declarations before
        // creating the identity file, world directory or any network listener.
        // This exercises the real built executable, not just the Rust CLI parser.
        let mut invalid = Vec::new();
        for field in [
            "connections_total",
            "pending_connections_total",
            "pending_inbound_unknown",
            "pending_outbound",
            "connections_per_peer",
            "handshake_timeout_ms",
            "bytes_in_per_window",
            "bytes_out_per_window",
            "bytes_total_per_window",
        ] {
            let mut candidate = limits();
            candidate[field] = json!(0);
            invalid.push((format!("zero-{field}"), candidate));
        }
        for (label, field, value) in [
            ("pending-above-total", "pending_connections_total", 5_u64),
            ("unknown-above-pending", "pending_inbound_unknown", 4),
            ("outbound-above-pending", "pending_outbound", 4),
            ("per-peer-above-total", "connections_per_peer", 5),
        ] {
            let mut candidate = limits();
            candidate[field] = json!(value);
            invalid.push((label.into(), candidate));
        }
        let mut unknown = limits();
        unknown["unregistered_budget_override"] = json!(1);
        invalid.push(("unknown-field".into(), unknown));
        let mut missing = limits();
        missing
            .as_object_mut()
            .unwrap()
            .remove("pending_inbound_unknown");
        invalid.push(("missing-field".into(), missing));
        let mut future = limits();
        future["profile"] = json!("unsupported-future-profile");
        invalid.push(("future-profile".into(), future));

        for (case, budget) in invalid {
            let mut wire = original.clone();
            wire["runtime"]["p2p_resource_budget"] = budget;
            provider(&mut wire);
            write_config(&path, &wire);
            for command in ["node", "local-provider"] {
                let output = Command::new(binary())
                    .args(["dev", command, "--config", path.to_str().unwrap()])
                    .current_dir(&root.0)
                    .output()
                    .unwrap();
                assert!(
                    !output.status.success(),
                    "shipping {command} accepted invalid {case}: stdout={}",
                    String::from_utf8_lossy(&output.stdout)
                );
                assert!(
                    !root.0.join("node.toml").exists()
                        && !root.0.join("runtime").exists(),
                    "shipping {command} performed runtime effects before rejecting {case}"
                );
            }
        }
    }

    struct LiveProcess {
        child: Child,
        stderr: PathBuf,
    }

    impl LiveProcess {
        fn start(root: &Path, config: &Path, label: &str) -> Self {
            let stderr = root.join(format!("process-{label}.stderr"));
            let child = Command::new(binary())
                .args(["dev", "node", "--config", config.to_str().unwrap()])
                .current_dir(root)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::from(fs::File::create(&stderr).unwrap()))
                .spawn()
                .expect("launch actual development node executable");
            Self { child, stderr }
        }

        fn stop(&mut self) {
            let _ = self.child.kill();
            self.child.wait().expect("reap development node process");
        }

        fn diagnostics(&self) -> String {
            let content = fs::read_to_string(&self.stderr).unwrap_or_default();
            content
                .chars()
                .rev()
                .take(5000)
                .collect::<String>()
                .chars()
                .rev()
                .collect()
        }
    }

    impl Drop for LiveProcess {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn fetch_status(address: SocketAddr) -> Option<Value> {
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(200)).ok()?;
        stream.set_read_timeout(Some(Duration::from_millis(600))).ok()?;
        stream.set_write_timeout(Some(Duration::from_millis(600))).ok()?;
        stream
            .write_all(b"GET /v1/chain/status HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .ok()?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).ok()?;
        let separator = response.windows(4).position(|w| w == b"\r\n\r\n")?;
        let head = &response[..separator];
        if !head.starts_with(b"HTTP/1.1 200") && !head.starts_with(b"HTTP/1.0 200") {
            return None;
        }
        serde_json::from_slice(&response[separator + 4..]).ok()
    }

    fn find_budget(value: &Value) -> Option<&Value> {
        match value {
            Value::Object(fields) => {
                if let Some(budget) = fields.get("resource_budget")
                    && budget["profile"] == PROFILE
                {
                    return Some(budget);
                }
                fields.values().find_map(find_budget)
            }
            Value::Array(values) => values.iter().find_map(find_budget),
            _ => None,
        }
    }

    fn status_until_ready(process: &mut LiveProcess, address: SocketAddr) -> Value {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = fetch_status(address)
                && find_budget(&status).is_some()
            {
                return status;
            }
            if let Some(exit) = process.child.try_wait().unwrap() {
                panic!(
                    "shipping dev node exited before actual budgeted HTTP status: {exit}; {}",
                    process.diagnostics()
                );
            }
            assert!(
                Instant::now() < deadline,
                "shipping dev node did not expose live budgeted status: {}",
                process.diagnostics()
            );
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    #[test]
    fn shipping_dev_node_starts_with_both_transports_and_cold_reopens_new_budget_instance() {
        let root = PrivateRoot::new();
        let status_addr: SocketAddr = format!("127.0.0.1:{}", tcp_port()).parse().unwrap();
        let node_tcp = tcp_port();
        let node_quic = udp_port();
        let path = root.0.join("node.json");
        write_config(&path, &config(&root.0, status_addr, node_tcp, node_quic));

        let mut first = LiveProcess::start(&root.0, &path, "original");
        let first_status = status_until_ready(&mut first, status_addr);
        let original = find_budget(&first_status).unwrap();
        assert_eq!(original["profile"], PROFILE);
        let mut expected_limits = limits();
        expected_limits.as_object_mut().unwrap().remove("profile");
        assert_eq!(original["limits"], expected_limits);
        let first_instance = original["instance_id"]
            .as_str()
            .expect("typed live status must expose instance identity")
            .to_owned();
        assert_eq!(original["connections"]["total"], 0);
        first.stop();

        // The same fixed typed config must reopen against the same original
        // world/key identity. A 60-second ledger is strictly per instance,
        // so cold restart creates a new instance without claiming old bytes.
        let mut reopened = LiveProcess::start(&root.0, &path, "cold");
        let second_status = status_until_ready(&mut reopened, status_addr);
        let second = find_budget(&second_status).unwrap();
        assert_eq!(second["profile"], PROFILE);
        assert_ne!(second["instance_id"], first_instance);
        assert_eq!(second["connections"]["total"], 0);
        assert_eq!(second["bytes"]["charged_bytes"]["total"], 0);
        reopened.stop();
    }
}
