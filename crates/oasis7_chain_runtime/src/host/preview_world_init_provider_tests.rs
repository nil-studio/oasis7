//! Explicit inputs only; actual-module test never loads model credentials.
use super::super::provider;
use super::*;

fn input(root: &Directory) -> provider::Inputs {
    provider::Inputs {
        wasm: root.0.join("provider.wasm"),
        metadata: root.0.join("provider.metadata.json"),
        agent_id: "starter-agent-0".into(),
        owner_binding: "preview-local-owner".into(),
        finality_block_hash: format!("blake3:{}", "12".repeat(32)),
        allowance: 8,
    }
}

#[test]
fn provider_profile_requires_explicit_complete_bounded_inputs() {
    let root = Directory::new();
    let input = input(&root);
    let mut values = std::collections::BTreeMap::new();
    assert!(provider::parse(&values).unwrap().is_none());
    values.insert("--provider-agent-id", input.agent_id.as_str());
    assert!(provider::parse(&values).is_err());
    values.insert("--profile", "devlocal-provider-v1");
    assert!(provider::parse(&values).is_err());
    values.insert("--provider-wasm", input.wasm.to_str().unwrap());
    values.insert("--provider-metadata", input.metadata.to_str().unwrap());
    values.insert("--provider-owner-binding", input.owner_binding.as_str());
    values.insert(
        "--provider-finality-block-hash",
        input.finality_block_hash.as_str(),
    );
    values.insert("--provider-allowance", "8");
    assert_eq!(provider::parse(&values).unwrap(), Some(input.clone()));
    for invalid in ["0", "129", "18446744073709551616"] {
        values.insert("--provider-allowance", invalid);
        assert!(provider::parse(&values).is_err());
    }
    values.insert("--provider-allowance", "8");
    values.insert("--profile", "formal-provider");
    assert!(provider::parse(&values).is_err());
}

#[test]
fn existing_public_preview_cannot_be_upgraded_to_provider() {
    let root = Directory::new();
    let mut options = root.options();
    initialize(&options).unwrap();
    let before = fs::read(options.root.join("initialization-receipt.json")).unwrap();
    assert!(
        serde_json::from_slice::<Value>(&before)
            .unwrap()
            .get("provider")
            .is_none()
    );
    options.provider = Some(input(&root));
    assert!(
        initialize(&options)
            .err()
            .unwrap()
            .contains("exact requested initialization")
    );
    assert_eq!(
        fs::read(options.root.join("initialization-receipt.json")).unwrap(),
        before
    );
    assert!(
        !options
            .root
            .join("private/provider-authority.json")
            .exists()
    );
}

#[test]
fn malformed_provider_module_cannot_publish_a_preview_root() {
    let root = Directory::new();
    let mut options = root.options();
    let input = input(&root);
    files::write_new(&input.wasm, b"not-a-WASM-module").unwrap();
    files::write_new(
        &input.metadata,
        format!(
            "{{\"source_hash\":\"{}\",\"build_manifest_hash\":\"{}\"}}",
            "11".repeat(32),
            "22".repeat(32)
        )
        .as_bytes(),
    )
    .unwrap();
    options.provider = Some(input);
    assert!(initialize(&options).is_err());
    assert!(!options.root.exists());
}

/// The caller must build the canonical suite for this candidate and explicitly
/// supply its WASM/metadata. No placeholder module or synthesized provenance.
#[test]
#[ignore = "requires explicit same-candidate canonical local-test-provider WASM and metadata"]
fn provider_preview_actual_module_commit_agent_read_and_immutable_rerun() {
    let root = Directory::new();
    let mut options = root.options();
    let mut input = input(&root);
    input.wasm = std::env::var_os("OASIS7_PREVIEW_TEST_PROVIDER_WASM")
        .expect("explicit canonical WASM input")
        .into();
    input.metadata = std::env::var_os("OASIS7_PREVIEW_TEST_PROVIDER_METADATA")
        .expect("explicit canonical build metadata input")
        .into();
    options.provider = Some(input.clone());
    let (hardened, _) = viewer_bootstrap_formal_release_runtime_world().unwrap();
    assert!(hardened.release_security_policy().is_production_hardened());
    let artifact = super::super::super::local_provider_setup::load_module_artifact(
        &input.wasm,
        &input.metadata,
    )
    .unwrap();
    let config = oasis7_runtime::LocalTestProviderAuthorityConfig::hosted_public_join(
        &input.agent_id,
        &input.owner_binding,
        &input.finality_block_hash,
    );
    let mut rejected = hardened;
    assert!(
        rejected
            .install_local_test_provider_module(&config, &artifact)
            .is_err(),
        "the ordinary production policy must still reject local authority"
    );
    let receipt = initialize(&options).unwrap();
    let original_receipt = fs::read(options.root.join("initialization-receipt.json")).unwrap();
    let original_authority =
        fs::read(options.root.join("private/provider-authority.json")).unwrap();
    let pinned = bridge::world_service_read::pin(
        &options.root.join("records"),
        &options.root.join("store"),
        &receipt.world,
        None,
    )
    .unwrap();
    let context = pinned
        .world
        .canonical_agent_service_context(&input.agent_id)
        .unwrap();
    assert_eq!(context.agent_id, input.agent_id);
    assert_eq!(
        pinned
            .world
            .current_agent_owner(&input.agent_id)
            .unwrap()
            .player_id,
        input.owner_binding
    );
    assert!(pinned.world.main_token_liquid_balance(&input.agent_id) > 0);
    let authority: oasis7_runtime::ProviderBackedBootstrapAuthorityV1 =
        serde_json::from_slice(&original_authority).unwrap();
    assert_eq!(authority.allowance, 8);
    assert!(
        authority
            .grant
            .expires_at_tick
            .is_some_and(|until| until > authority.grant.issued_at_tick)
    );
    authenticated_agent_http_read(&options, &receipt);
    assert_eq!(initialize(&options).unwrap().world, receipt.world);
    assert_eq!(
        fs::read(options.root.join("initialization-receipt.json")).unwrap(),
        original_receipt
    );
    assert_eq!(
        fs::read(options.root.join("private/provider-authority.json")).unwrap(),
        original_authority
    );
    let mut changed = options.clone();
    changed.provider.as_mut().unwrap().allowance += 1;
    assert!(initialize(&changed).is_err());
    let env = options.root.join("private/agent-viewer.env");
    fs::write(&env, b"OASIS7_WORLD_SERVICE_SCOPE=agent:other\n").unwrap();
    assert!(initialize(&options).is_err());
    assert_eq!(
        fs::read(options.root.join("initialization-receipt.json")).unwrap(),
        original_receipt
    );
}

fn authenticated_agent_http_read(options: &Options, receipt: &Receipt) {
    use oasis7::world_service::*;
    use std::{
        io::Read,
        net::{TcpListener, TcpStream},
        sync::Mutex,
    };
    let input = options.provider.as_ref().unwrap();
    let executor = hex::encode(fs::read(options.root.join("private/executor.key")).unwrap());
    let request = ReadWorldViewRequest {
        observation_nonce: Some("34".repeat(32)),
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        world: receipt.world.clone(),
        scope_id: format!("agent:{}", input.agent_id),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
        formal_anchor: None,
    };
    let signed = sign_read_request(VIEW_JSON_PATH, request.clone(), &executor).unwrap();
    let digest = request_digest(VIEW_JSON_PATH, &signed).unwrap();
    let body = serde_json::to_vec(&signed).unwrap();
    let mut raw = format!("POST {VIEW_JSON_PATH} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
    raw.extend(body);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let (mut server, _) = listener.accept().unwrap();
    let reader = std::thread::spawn(move || {
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        response
    });
    let (key, _) =
        super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &options.key_dir,
        )
        .unwrap();
    let signer = super::super::super::build_feedback_submit_signer(&options.node_id, &key).unwrap();
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(&options.node_id, &options.world_id, NodeRole::Sequencer).unwrap(),
    )));
    assert!(
        super::super::super::world_service_api::maybe_handle(
            &mut server,
            &raw,
            &runtime,
            "POST",
            VIEW_JSON_PATH,
            &options.world_id,
            &options.root.join("world"),
            &options.root.join("records"),
            &options.root.join("store"),
            &signer
        )
        .unwrap()
    );
    drop(server);
    let response = reader.join().unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200"), "Agent read failed");
    let offset = response.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
    let response: SignedServiceResponse<ReadWorldViewResponse<Value>> =
        serde_json::from_slice(&response[offset..]).unwrap();
    verify_service_response(
        VIEW_JSON_PATH,
        &digest,
        &response,
        &receipt.service_public_key,
    )
    .unwrap();
    response.payload.validate(&request).unwrap();
    assert!(
        serde_json::to_string(&response.payload.view)
            .unwrap()
            .contains("capability_invocation_context")
    );
}
