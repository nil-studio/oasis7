//! Shipping Node CLI over a real provider TCP session and retained actual H.
//! The backend remains the explicit in-memory test double from the parent.
use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn wire3_formal_read_shipping_node_cli_commits_private_input_via_native_provider() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(cli_case)
        .unwrap()
        .join()
        .unwrap();
}
fn private_file(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn cli_case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    for (digest, bytes) in &h.reader.0 {
        assert_eq!(store.put_bytes(bytes).unwrap(), *digest);
    }
    let records = root.path().join("ledger");
    let replication = root.path().join("replication");
    for dir in [&records, &replication] {
        fs::create_dir(dir).unwrap();
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let trust = root.path().join("trust.json");
    let material = root.path().join("H.cbor");
    let requester = root.path().join("requester.hex");
    let config = root.path().join("node.json");
    let input_path = root.path().join("actions.cbor");
    private_file(&trust, &serde_json::to_vec(&h.trust).unwrap());
    private_file(
        &material,
        &oasis7_wasm_abi::encode_canonical_cbor(&h.material).unwrap(),
    );
    private_file(
        &requester,
        hex::encode(fixtures::key(117).to_bytes()).as_bytes(),
    );
    let actions = vec![
        oasis7_node::NodeConsensusAction::from_payload(
            9010,
            "shipping-node-player",
            payload("shipping-wire3-node-cli-agent"),
        )
        .unwrap(),
    ];
    let input = serde_cbor::to_vec(&actions).unwrap();
    private_file(&input_path, &input);
    let action_root = oasis7_node::compute_consensus_action_root(&actions).unwrap();
    let unused = TcpListener::bind("127.0.0.1:0").unwrap();
    let unlistened = unused.local_addr().unwrap();
    drop(unused);
    assert!(std::net::TcpStream::connect_timeout(&unlistened, Duration::from_millis(100)).is_err());
    let mut config_value = serde_json::json!({
        "schema_version":1, "trust_config_path":trust, "target":h.target,
        "handoff_material_path":material, "cas_directory":store.root(),
        "records_directory":records, "replication_directory":replication,
        "validator_id":validator_id(0), "player_id":"shipping-node-player",
        "operator_instance_id":"actual-node-object-restart-4975",
        "provisioned_signing_keys":[{"validator_id":validator_id(0),"public_key_hex":hex::encode(fixtures::key(120).verifying_key().to_bytes())}],
        "timeouts":{"propose_ms":2000,"prevote_ms":2000,"precommit_ms":2000,"round_delta_ms":100},
        "provider_address":unlistened, "requester_signing_key_path":requester,
        "provider_timeout_ms":10000, "completion_timeout_ms":120000,
    });
    private_file(&config, &serde_json::to_vec(&config_value).unwrap());
    // Normal operation without a pinned provider must fail before initializing
    // either the private identity or the canonical ledger.
    assert!(
        crate::identity_receipt::dispatch(
            [
                "bft-header",
                "node",
                "--config",
                config.to_str().unwrap(),
                "--actions",
                input_path.to_str().unwrap(),
            ]
            .into_iter()
        )
        .unwrap()
        .unwrap_err()
        .contains("provider_trust")
    );
    assert!(fs::read_dir(&records).unwrap().next().is_none());
    // This is the real first-use provisioning command, with no provider yet.
    crate::identity_receipt::dispatch(
        [
            "bft-header",
            "node",
            "--config",
            config.to_str().unwrap(),
            "--print-binding",
        ]
        .into_iter(),
    )
    .unwrap()
    .unwrap();
    let report = crate::formal_bft_header_cli::prepare_node_binding_report(&config).unwrap();
    report.binding.validate().unwrap();
    assert_eq!(
        report.schema_version,
        "oasis7.formal_bft_header_node_binding.v3"
    );
    assert_eq!(report.binding.digest().unwrap(), report.binding_digest);
    let descriptor = serde_json::to_value(&report).unwrap();
    assert_eq!(descriptor.as_object().unwrap().len(), 3);
    let descriptor_json = serde_json::to_string(&descriptor).unwrap();
    assert!(!descriptor_json.contains(&hex::encode(fixtures::key(117).to_bytes())));
    for key in [
        r#""actions_cbor":"#,
        r#""payload_cbor":"#,
        r#""snapshot":"#,
        r#""requester_signing_key_path":"#,
    ] {
        assert!(!descriptor_json.contains(key));
    }
    let second_report = crate::formal_bft_header_cli::prepare_node_binding_report(&config).unwrap();
    assert_eq!(second_report.binding_digest, report.binding_digest);
    // Independently restore actual H rather than deserializing the descriptor
    // into a trust capability. Its retained H closure needs no fixture reader.
    let original = driver_with_source(&h, root.path(), &store, Arc::new(EmptyIndexReader));
    let binding = signer_binding(&original);
    let binding_digest = binding.digest().unwrap();
    assert_eq!(binding_digest, report.binding_digest);
    // Reserve a different exact private value before any provider WAL exists.
    // A cold CLI invocation must finish this value, then execute its own input.
    let original_actions = vec![
        oasis7_node::NodeConsensusAction::from_payload(
            9009,
            "shipping-node-player",
            payload("shipping-wire3-original-reservation-agent"),
        )
        .unwrap(),
    ];
    let original_action_root =
        oasis7_node::compute_consensus_action_root(&original_actions).unwrap();
    let original_value = {
        let mut actual = original.lock().unwrap();
        actual.configure_bft_v3_execution(&binding).unwrap();
        let context = actual
            .current_bft_v3_execution_context(binding.initial_schedule())
            .unwrap();
        let candidate = actual
            .build_bft_v3_candidate(NodeBftBuildContextV3 {
                execution: context.clone(),
                actions: original_actions,
            })
            .unwrap();
        actual
            .preview_bft_v3_candidate(&context, &candidate)
            .unwrap();
        assert_eq!(
            actual
                .retain_bft_v3_private_candidate(&context, &candidate)
                .unwrap()
                .len(),
            64
        );
        candidate.public_value
    };
    let original_path = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1/node-private-v1")
        .join(format!(
            "candidate-{:020}.cbor",
            original_value.execution_commitment.height
        ));
    let original_bytes = fs::read(&original_path).unwrap();
    drop(original);
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    assert!(snapshot.lock().unwrap().state.history.is_empty());
    let (address, server) = tcp(service(&binding, snapshot.clone()));
    config_value["provider_address"] = serde_json::json!(address);
    config_value["provider_trust"] = serde_json::json!({
        "schema_version":3, "provider_public_key":hex::encode(fixtures::key(116).verifying_key().to_bytes()),
        "binding_digest":binding_digest,
    });
    private_file(&config, &serde_json::to_vec(&config_value).unwrap());
    // No external fixture reader is passed to this production command. It
    // must restore the original Node H closure from its retained CAS receipt.
    crate::identity_receipt::dispatch(
        [
            "bft-header",
            "node",
            "--config",
            config.to_str().unwrap(),
            "--actions",
            input_path.to_str().unwrap(),
        ]
        .into_iter(),
    )
    .unwrap()
    .unwrap();
    server.join().unwrap().unwrap();
    let protected = snapshot.lock().unwrap().clone();
    assert!(protected.state.pending_driver_apply.is_none());
    assert!(protected.state.history.len() >= 2);
    let recovered = &protected.state.history[0];
    assert_eq!(recovered.header, original_value.header);
    assert_eq!(
        recovered.execution_commitment,
        original_value.execution_commitment
    );
    assert_eq!(
        recovered.execution_commitment.action_root,
        original_action_root
    );
    assert_eq!(fs::read(original_path).unwrap(), original_bytes);

    assert_eq!(
        protected.state.history.len(),
        protected.state.applied_installs.len()
    );
    let certificate = protected
        .state
        .history
        .iter()
        .find(|c| c.execution_commitment.action_root == action_root)
        .expect("the CLI committed the submitted nonempty input");
    let height = certificate.execution_commitment.height;
    assert!(
        height > recovered.execution_commitment.height,
        "this request must commit only after the original private reservation"
    );
    let id = certificate.header.block_id().unwrap();
    let applied = protected
        .state
        .applied_installs
        .iter()
        .find(|r| r.height == height)
        .unwrap();
    assert_eq!(applied.header_id, id);
    assert_eq!(applied.driver_install_digest.len(), 64);
    let public_json = serde_json::to_string(&protected.state).unwrap();
    assert!(!public_json.contains(r#""actions_cbor":"#));
    assert!(!public_json.contains(r#""payload_cbor":"#));
    assert!(!public_json.contains("shipping-wire3-node-cli-agent"));
    assert!(!public_json.contains("shipping-wire3-original-reservation-agent"));
    let public_bytes = serde_cbor::to_vec(&protected.state).unwrap();
    assert!(!public_bytes.windows(input.len()).any(|part| part == input));
    let mut ledger = FormalBftHeaderLedgerExecutionV3::open_from_handoff(
        &root.path().join("ledger"),
        store,
        h.anchor,
        h.target,
        h.material,
        Arc::new(EmptyIndexReader),
        false,
    )
    .unwrap();
    let cut = ledger.verified_cut(height, &id).unwrap();
    assert!(
        cut.world()
            .state()
            .agents
            .contains_key("shipping-wire3-node-cli-agent")
    );
    assert!(
        cut.world()
            .state()
            .agents
            .contains_key("shipping-wire3-original-reservation-agent")
    );
    assert_eq!(cut.world().state().time, height);
    assert_eq!(
        cut.world()
            .execution_state_identity()
            .unwrap()
            .state_root_hash,
        certificate.execution_commitment.state_root
    );
}
