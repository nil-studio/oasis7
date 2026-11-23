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

    use oasis7_net::{Libp2pNetwork, Libp2pNetworkConfig};
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

    // Match the existing DevLocal bootstrap inputs: its starter World contains
    // this live agent; a made-up identifier would fail before any network work.
    fn provider(value: &mut Value) {
        value["local_provider"] = json!({
            "authority_path": "provider-authority.json",
            "wasm_path": "provider.wasm",
            "metadata_path": "provider-metadata.json",
            "agent_id": "starter-agent-0",
            "owner_binding": "local-test-owner-0",
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
                let mut child = Command::new(binary())
                    .args(["dev", command, "--config", path.to_str().unwrap()])
                    .current_dir(&root.0)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                let status = loop {
                    if let Some(status) = child.try_wait().unwrap() {
                        break status;
                    }
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("shipping {command} continued after invalid {case}");
                    }
                    std::thread::sleep(Duration::from_millis(10));
                };
                assert!(
                    !status.success(),
                    "shipping {command} accepted invalid {case}"
                );
                assert!(
                    !root.0.join("node.toml").exists() && !root.0.join("runtime").exists(),
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
            Self::start_command(root, config, label, "node")
        }

        fn start_command(root: &Path, config: &Path, label: &str, command: &str) -> Self {
            let stderr = root.join(format!("process-{label}.stderr"));
            let child = Command::new(binary())
                .args(["dev", command, "--config", config.to_str().unwrap()])
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
        stream
            .set_read_timeout(Some(Duration::from_millis(600)))
            .ok()?;
        stream
            .set_write_timeout(Some(Duration::from_millis(600)))
            .ok()?;
        stream
            .write_all(
                b"GET /v1/chain/status HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
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

    // The original session must actually consume metered substream bytes.
    // Otherwise checking a zero byte ledger on restart would not detect an
    // accidental persistence or reuse of the original resource counters.
    fn charge_shipping_budget(
        original_status: &Value,
        status_address: SocketAddr,
        node_tcp_port: u16,
    ) {
        let peer_id = original_status["replication"]["local_peer_id"]
            .as_str()
            .expect("public status must expose the real replication peer identity");
        let bootstrap = format!("/ip4/127.0.0.1/tcp/{node_tcp_port}/p2p/{peer_id}")
            .parse()
            .unwrap();
        let client = Libp2pNetwork::try_new(Libp2pNetworkConfig {
            bootstrap_peers: vec![bootstrap],
            enable_autonat: false,
            ..Libp2pNetworkConfig::default()
        })
        .expect("connect independent authenticated budget consumer");
        let connect_deadline = Instant::now() + Duration::from_secs(15);
        let peer = loop {
            if let Some(peer) = client
                .connected_peers()
                .into_iter()
                .find(|peer| peer.to_string() == peer_id)
            {
                break peer;
            }
            assert!(
                Instant::now() < connect_deadline,
                "shipping budgeted peer never connected: {:?}",
                client.debug_errors()
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        // The observer legitimately serves the discovery control lane. Fetch
        // and independently decode its original signed record through a real
        // authenticated RR; connection or unsupported-protocol errors are not
        // successful consumption of this shipping instance.
        let response = client
            .request_to_peer_with_timeout(
                "/aw/rr/1.0.0/get_local_peer_record",
                &[],
                peer,
                Duration::from_secs(3),
            )
            .expect("shipping development host must serve a valid discovery request");
        let signed: oasis7_proto::distributed_dht::SignedPeerRecord =
            serde_cbor::from_slice(&response)
                .expect("decode the shipping host's actual RR response");
        assert_eq!(signed.record.peer_id, peer_id);
        assert_eq!(signed.record.world_id, "budget-process-world");
        signed.record.validate_policy().unwrap();
        assert!(!signed.identity_public_key_protobuf_hex.is_empty());
        assert!(!signed.signature_hex.is_empty());
        let charged_deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = fetch_status(status_address)
                && let Some(budget) = find_budget(&status)
                && budget["bytes"]["charged_bytes"]["total"]
                    .as_u64()
                    .is_some_and(|count| count > 0)
            {
                assert!(
                    budget["bytes"]["actual_bytes"]["inbound"]
                        .as_u64()
                        .unwrap_or_default()
                        > 0
                );
                assert!(
                    budget["bytes"]["actual_bytes"]["outbound"]
                        .as_u64()
                        .unwrap_or_default()
                        > 0
                );
                return;
            }
            assert!(
                Instant::now() < charged_deadline,
                "actual shipping network never charged a real substream"
            );
            std::thread::sleep(Duration::from_millis(20));
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
        charge_shipping_budget(&first_status, status_addr, node_tcp);
        first.stop();

        // The same fixed typed config must reopen against the same original
        // world/key identity. A 60-second ledger is strictly per instance,
        // so cold restart creates a new instance without claiming old bytes.
        let mut reopened = LiveProcess::start(&root.0, &path, "cold");
        let second_status = status_until_ready(&mut reopened, status_addr);
        let second = find_budget(&second_status).unwrap();
        assert_eq!(second["profile"], PROFILE);
        assert_ne!(second["instance_id"], first_instance);
        assert_eq!(
            second_status["replication"]["local_peer_id"],
            first_status["replication"]["local_peer_id"]
        );
        assert_eq!(second["connections"]["total"], 0);
        assert_eq!(second["bytes"]["charged_bytes"]["total"], 0);
        reopened.stop();
    }

    #[cfg(feature = "wasmtime")]
    #[test]
    fn shipping_local_provider_retains_executable_wasm_and_authority_across_budget_restart() {
        use oasis7_runtime::{
            LocalTestProviderAuthorityConfig, LocalTestProviderModuleArtifact,
            LocalTestProviderSessionMode, ProviderBackedBootstrapAuthorityV1,
        };
        use oasis7_wasm_abi::{ModuleCallRequest, ModuleLimits, ModuleSandbox};
        use sha2::{Digest, Sha256};

        // This is a real, executable Core WASM module with the same pure `call`
        // ABI as the DevLocal provider. It returns a typed empty ModuleOutput.
        // Execute the original bytes before admitting them through the shipping
        // process: arbitrary "wasm" text would otherwise also test only storage.
        let wasm = executable_provider_wasm();
        let wasm_hash = hex::encode(Sha256::digest(&wasm));
        let mut executor = oasis7_wasm_executor::WasmExecutor::new(Default::default()).unwrap();
        let result = executor
            .call(&ModuleCallRequest {
                module_id: "module.runtime.local-test-provider".into(),
                wasm_hash: wasm_hash.clone(),
                trace_id: "shipping-provider-budget".into(),
                entrypoint: "call".into(),
                input: vec![],
                limits: ModuleLimits {
                    max_mem_bytes: 64 * 1024,
                    max_gas: 100_000,
                    max_call_rate: 1,
                    max_output_bytes: 1024,
                    max_effects: 0,
                    max_emits: 0,
                },
                wasm_bytes: wasm.clone().into(),
            })
            .expect("the original provider fixture must really execute in Wasmtime");
        assert!(result.effects.is_empty() && result.emits.is_empty());

        let root = PrivateRoot::new();
        let status_addr: SocketAddr = format!("127.0.0.1:{}", tcp_port()).parse().unwrap();
        let node_tcp = tcp_port();
        let path = root.0.join("provider.json");
        let mut input = config(&root.0, status_addr, node_tcp, udp_port());
        input["runtime"]["execution_world_dir"] = json!("provider-world");
        provider(&mut input);
        write_config(&path, &input);
        fs::write(root.0.join("provider.wasm"), &wasm).unwrap();
        let metadata = json!({
            "source_hash": wasm_hash,
            "build_manifest_hash": hex::encode(Sha256::digest(b"shipping-budget-executable-provider-fixture-v1"))
        });
        write_config(&root.0.join("provider-metadata.json"), &metadata);

        let mut original =
            LiveProcess::start_command(&root.0, &path, "provider-original", "local-provider");
        let first = status_until_ready(&mut original, status_addr);
        let first_budget = find_budget(&first).unwrap();
        let mut expected_limits = limits();
        expected_limits.as_object_mut().unwrap().remove("profile");
        assert_eq!(first_budget["limits"], expected_limits);
        let first_instance = first_budget["instance_id"].clone();
        charge_shipping_budget(&first, status_addr, node_tcp);
        original.stop();

        let authority_path = root.0.join("provider-authority.json");
        let authority_bytes =
            fs::read(&authority_path).expect("shipping provider persisted its authority");
        let authority: ProviderBackedBootstrapAuthorityV1 =
            serde_json::from_slice(&authority_bytes).unwrap();
        assert_eq!(authority.agent_id, "starter-agent-0");
        assert_eq!(authority.owner_binding, "local-test-owner-0");
        assert_eq!(authority.world_id, "budget-process-world");
        let mut requested = LocalTestProviderAuthorityConfig::hosted_public_join(
            "starter-agent-0",
            "local-test-owner-0",
            format!("blake3:{}", "0".repeat(64)),
        );
        requested.session_mode = LocalTestProviderSessionMode::Loopback;
        requested.presenter.session_id = Some("loopback:starter-agent-0".into());
        let artifact = LocalTestProviderModuleArtifact {
            wasm_bytes: wasm,
            source_hash: metadata["source_hash"].as_str().unwrap().into(),
            build_manifest_hash: metadata["build_manifest_hash"].as_str().unwrap().into(),
        };
        let verify_original = || {
            let mut world =
                oasis7::runtime_host::world::load_from_dir(root.0.join("provider-world"))
                    .expect("independent reader opens the actual provider World");
            let persisted = world
                .load_module(&wasm_hash)
                .expect("the original provider artifact is available after reopening the World");
            assert_eq!(persisted.bytes.as_ref(), artifact.wasm_bytes.as_slice());
            assert!(
                world
                    .local_test_provider_grant_status(&requested, &artifact, &authority)
                    .expect("original WASM and Runtime authority remain bound")
                    .is_some()
            );
            world
                .validate_local_test_main_token_funding("starter-agent-0")
                .expect("shipping Runtime performed the required DevLocal funding");
        };
        verify_original();

        let mut reopened =
            LiveProcess::start_command(&root.0, &path, "provider-cold", "local-provider");
        let second = status_until_ready(&mut reopened, status_addr);
        let budget = find_budget(&second).unwrap();
        assert_eq!(budget["limits"], expected_limits);
        assert_ne!(budget["instance_id"], first_instance);
        assert_eq!(budget["bytes"]["charged_bytes"]["total"], 0);
        assert_eq!(
            second["replication"]["local_peer_id"],
            first["replication"]["local_peer_id"]
        );
        // Resuming the original provider remains useful: authenticate and charge
        // real RR again, while proving that no new authority replaced the old one.
        charge_shipping_budget(&second, status_addr, node_tcp);
        reopened.stop();
        assert_eq!(fs::read(&authority_path).unwrap(), authority_bytes);
        verify_original();
    }

    #[cfg(feature = "wasmtime")]
    fn executable_provider_wasm() -> Vec<u8> {
        fn leb(mut value: usize) -> Vec<u8> {
            let mut bytes = Vec::new();
            loop {
                let byte = (value & 0x7f) as u8;
                value >>= 7;
                bytes.push(byte | if value == 0 { 0 } else { 0x80 });
                if value == 0 {
                    return bytes;
                }
            }
        }
        fn section(wasm: &mut Vec<u8>, tag: u8, bytes: Vec<u8>) {
            wasm.push(tag);
            wasm.extend(leb(bytes.len()));
            wasm.extend(bytes);
        }
        let output = serde_cbor::to_vec(&oasis7_wasm_abi::ModuleOutput {
            new_state: None,
            effects: vec![],
            emits: vec![],
            tick_lifecycle: None,
            output_bytes: 0,
        })
        .unwrap();
        let mut wasm = b"\0asm\x01\0\0\0".to_vec();
        section(
            &mut wasm,
            1,
            vec![
                2, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 2, 0x7f, 0x7f, 2, 0x7f, 0x7f,
            ],
        );
        section(&mut wasm, 3, vec![2, 0, 1]);
        section(&mut wasm, 5, vec![1, 0, 1]);
        section(
            &mut wasm,
            7,
            b"\x03\x06memory\x02\x00\x05alloc\x00\x00\x04call\x00\x01".to_vec(),
        );
        let alloc = vec![0, 0x41, 0x80, 0x08, 0x0b];
        let mut call = vec![0, 0x41, 16, 0x41];
        let mut length = leb(output.len());
        if length.last().unwrap() & 0x40 != 0 {
            *length.last_mut().unwrap() |= 0x80;
            length.push(0);
        }
        call.extend(length);
        call.push(0x0b);
        let mut code = vec![2];
        code.extend(leb(alloc.len()));
        code.extend(alloc);
        code.extend(leb(call.len()));
        code.extend(call);
        section(&mut wasm, 10, code);
        let mut data = vec![1, 0, 0x41, 16, 0x0b];
        data.extend(leb(output.len()));
        data.extend(output);
        section(&mut wasm, 11, data);
        wasm
    }
}
