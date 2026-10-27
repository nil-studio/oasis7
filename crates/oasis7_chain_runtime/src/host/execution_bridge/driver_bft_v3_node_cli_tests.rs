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
    // Provision only the exact actual H identity, then release its OS claim.
    let original = driver(&h, root.path(), &store);
    let binding = signer_binding(&original);
    let binding_digest = binding.digest().unwrap();
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
    let replication = root.path().join("replication");
    fs::create_dir(&replication).unwrap();
    fs::set_permissions(&replication, fs::Permissions::from_mode(0o700)).unwrap();
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    assert!(snapshot.lock().unwrap().state.history.is_empty());
    let (address, server) = tcp(service(&binding, snapshot.clone()));
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
    private_file(&config, &serde_json::to_vec(&serde_json::json!({
        "schema_version":1, "trust_config_path":trust, "target":h.target,
        "handoff_material_path":material, "cas_directory":store.root(),
        "records_directory":root.path().join("ledger"), "replication_directory":replication,
        "validator_id":binding.local_validator_id, "player_id":"shipping-node-player",
        "operator_instance_id":binding.operator_instance_id,
        "provisioned_signing_keys":binding.provisioned_signing_keys, "timeouts":binding.timeouts,
        "provider_address":address, "provider_trust":{
            "schema_version":3, "provider_public_key":hex::encode(fixtures::key(116).verifying_key().to_bytes()),
            "binding_digest":binding_digest,
        },
        "requester_signing_key_path":requester, "provider_timeout_ms":10000,
        "completion_timeout_ms":120000,
    })).unwrap());
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
