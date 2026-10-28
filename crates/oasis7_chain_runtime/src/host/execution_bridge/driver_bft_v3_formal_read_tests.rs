//! Shipping contract-4 CLI/HTTP/client evidence over a private wire-3
//! three-height ledger. The current private authority remains separate.
use super::super::prefix_ledger_tests::private_file;
use super::ledger_tests::{cas, fixture, open};
use super::*;
use ed25519_dalek::SigningKey;
use oasis7_client_api::world_service::{
    FORMAL_BFT_HEADER_READ_PATH_V4, FORMAL_BFT_READ_PATH_V2, FORMAL_BFT_READ_PATH_V3,
    FormalBftHeaderReadRequestV4, FormalBftHeaderReadResponseV4, FormalBftHeaderReadTargetV4,
    MAX_FORMAL_BFT_HEADER_READ_RESPONSE_BYTES_V4, SignedReadRequest,
};
use oasis7_game_client::{
    FormalBftHeaderReadClientConfigV4, FormalBftHeaderReadClientV4, FormalBftHeaderReadVerifierV4,
    WorldServiceClientConfig, WorldServiceClientError, WorldServiceTransport,
};
use oasis7_runtime::LocalCasStore;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[test]
fn wire3_formal_read_v4_shipping_cli_http_reads_nonempty_actual_world_prefix() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(read_case)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn wire3_formal_read_v4_signed_execution_lie_cannot_start_listener() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(signed_lie_case)
        .unwrap()
        .join()
        .unwrap();
}

fn signed_lie_case() {
    use super::super::super::formal_header::{
        FormalBftHeaderExecutionV3, PrivateHeaderEntryV3, PrivateHeaderPrefixV3,
    };
    let h = qualified_handoff_v3();
    let execution = FormalBftHeaderExecutionV3::from_handoff(
        h.anchor.clone(),
        h.target.clone(),
        &h.material,
        h.reader.clone(),
        Vec::new(),
        false,
    )
    .unwrap();
    let actions = super::execution_tests::private_actions();
    let mut candidate = execution.build_candidate(&actions).unwrap();
    candidate.commitment.state_root = hash("authenticated-read4-false-execution");
    candidate.header.execution_commitment_hash = candidate.commitment.digest().unwrap();
    let certificate = super::execution_tests::certify(execution.schedule(), &candidate);
    let height = certificate.execution_commitment.height;
    let id = certificate.header.block_id().unwrap();
    let input = PrivateHeaderPrefixV3::from_entries(vec![PrivateHeaderEntryV3 {
        certificate,
        actions_cbor: actions,
    }])
    .unwrap();
    let handoff =
        oasis7_formal_protocol::bft::verify_formal_bft_handoff_v2(&h.anchor, h.target.clone())
            .unwrap();
    input
        .public_prefix()
        .unwrap()
        .verify(&handoff)
        .expect("the real quorum authenticated this self-consistent execution lie");
    let (root, config, history) = cli_files(&h, &input, height, &id);
    let key = root.path().join("service.hex");
    private_file(&key, hex::encode(fixtures::key(118).to_bytes()).as_bytes());
    let bind = address();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    value["bind"] = serde_json::json!(bind.to_string());
    value["service_signing_key_path"] = serde_json::json!(key);
    private_file(&config, &serde_json::to_vec(&value).unwrap());
    assert!(
        crate::identity_receipt::dispatch(
            [
                "bft-header",
                "install",
                "--config",
                config.to_str().unwrap(),
                "--history",
                history.to_str().unwrap(),
            ]
            .into_iter()
        )
        .unwrap()
        .is_err()
    );
    assert!(
        crate::formal_bft_header_cli::run_with_shutdown(
            ["serve", "--config", config.to_str().unwrap()].into_iter(),
            Some(Arc::new(AtomicBool::new(true))),
        )
        .is_err()
    );
    assert!(std::net::TcpStream::connect_timeout(&bind, Duration::from_millis(100)).is_err());
}

struct Server {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<Result<(), String>>>,
}
impl Server {
    fn start(config: &Path) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let shutdown = stop.clone();
        let config = config.to_path_buf();
        let join = thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                crate::formal_bft_header_cli::run_with_shutdown(
                    ["serve", "--config", config.to_str().unwrap()].into_iter(),
                    Some(shutdown),
                )
            })
            .unwrap();
        Self {
            stop,
            join: Some(join),
        }
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.join.take().unwrap().join().unwrap().unwrap();
    }
    fn read(
        &mut self,
        client: &FormalBftHeaderReadClientV4,
    ) -> oasis7_game_client::VerifiedFormalBftHeaderReadPageV4<serde_json::Value> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            match client.read() {
                Ok(page) => return page,
                Err(error) => {
                    if self.join.as_ref().unwrap().is_finished() {
                        self.stop();
                        panic!("CLI ended before serving: {error}");
                    }
                    assert!(
                        Instant::now() < deadline,
                        "V4 CLI never served the actual cut: {error}"
                    );
                    thread::sleep(Duration::from_millis(40));
                }
            }
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
fn address() -> SocketAddr {
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    reservation.local_addr().unwrap()
}
fn client(
    h: &QualifiedH<BftProfileTargetV2>,
    bind: SocketAddr,
    service: &SigningKey,
    reader: &SigningKey,
    target: FormalBftHeaderReadTargetV4,
    scope: &str,
) -> FormalBftHeaderReadClientV4 {
    FormalBftHeaderReadClientV4::new(
        FormalBftHeaderReadClientConfigV4 {
            endpoint: format!("http://{bind}"),
            trusted_service_public_key: hex::encode(service.verifying_key().to_bytes()),
            read_private_key_hex: hex::encode(reader.to_bytes()),
            timeout: Duration::from_secs(3),
            scope_id: scope.into(),
            exact_target: target,
        },
        h.anchor.clone(),
        h.target.clone(),
    )
    .unwrap()
}
fn assert_status<T>(result: Result<T, WorldServiceClientError>, expected: u16) {
    match result {
        Err(WorldServiceClientError::Rejected { status, .. }) => assert_eq!(status, expected),
        Err(error) => panic!("expected HTTP {expected}, got {error}"),
        Ok(_) => panic!("unexpected successful read"),
    }
}

fn read_case() {
    let h = qualified_handoff_v3();
    let private_prefix = fixture(&h);
    let prefix = private_prefix.public_prefix().unwrap();
    let tip = prefix.entries.last().unwrap();
    let height = tip.execution_commitment.height;
    assert_eq!(height, 5);
    let block_id = tip.header.block_id().unwrap();
    let exact = FormalBftHeaderReadTargetV4 {
        height,
        header_id: block_id.clone(),
    };
    let (root, config, proof_file) = cli_files(&h, &private_prefix, height, &block_id);
    let service = fixtures::key(118);
    let reader = fixtures::key(119);
    let service_key = root.path().join("service.hex");
    private_file(&service_key, hex::encode(service.to_bytes()).as_bytes());
    let bind = address();
    let mut config_value: serde_json::Value =
        serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    config_value["bind"] = serde_json::json!(bind.to_string());
    config_value["service_signing_key_path"] = serde_json::json!(service_key);
    private_file(&config, &serde_json::to_vec(&config_value).unwrap());
    crate::identity_receipt::dispatch(
        [
            "bft-header",
            "install",
            "--config",
            config.to_str().unwrap(),
            "--history",
            proof_file.to_str().unwrap(),
        ]
        .into_iter(),
    )
    .unwrap()
    .unwrap();
    let store = LocalCasStore::new(root.path().join("cas"));
    let garbage = store
        .put_bytes(b"header-read-v4-unretained-gc-canary")
        .unwrap();
    assert!(store.prune_orphan_blobs().unwrap() > 0);
    assert!(store.get_verified(&garbage).is_err());
    let public_client = client(&h, bind, &service, &reader, exact.clone(), "public");
    let mut server = Server::start(&config);
    let page = server.read(&public_client);
    assert_eq!(page.proof().heights(), 3);
    assert_eq!(page.response().head.height, 5);
    assert_eq!(page.response().head.logical_tick, 5);
    assert_eq!(page.view()["state"]["time"], serde_json::json!(5));
    assert!(
        open(&h, root.path(), &store, false).is_err(),
        "serve must keep the ledger's OS writer claim alive"
    );

    let world = oasis7_client_api::world_service::WorldIdentity {
        world_id: h.anchor.head().record().world.world_id.clone(),
        genesis_digest: h.anchor.head().record().world.genesis_digest.clone(),
    };
    let transport = WorldServiceTransport::new(WorldServiceClientConfig {
        endpoint: format!("http://{bind}"),
        trusted_service_public_key: hex::encode(service.verifying_key().to_bytes()),
        expected_world: world,
        scope_id: "public".into(),
        read_private_key_hex: hex::encode(reader.to_bytes()),
        timeout: Duration::from_secs(3),
        max_response_bytes: MAX_FORMAL_BFT_HEADER_READ_RESPONSE_BYTES_V4,
    })
    .unwrap();
    let verifier =
        FormalBftHeaderReadVerifierV4::new(h.anchor.clone(), h.target.clone(), exact.clone())
            .unwrap();
    let request = verifier.request("public", [9; 32]).unwrap();
    let signed: SignedReadRequest<FormalBftHeaderReadRequestV4> =
        oasis7::world_service::authority::sign_read_request(
            FORMAL_BFT_HEADER_READ_PATH_V4,
            request.clone(),
            &hex::encode(reader.to_bytes()),
        )
        .unwrap();
    let response: FormalBftHeaderReadResponseV4<serde_json::Value> = transport
        .call(FORMAL_BFT_HEADER_READ_PATH_V4, &signed)
        .unwrap();
    verifier.verify_response(&signed, response.clone()).unwrap();
    let proof_bytes = hex::decode(&response.certified_prefix_cbor_hex).unwrap();
    let decoded = oasis7_formal_protocol::bft::FormalBftCertifiedHeaderPrefixV3::decode_canonical(
        &proof_bytes,
    )
    .unwrap();
    assert_eq!(decoded, prefix);
    let encoded_private = serde_cbor::to_vec(&private_prefix.entries()[0].actions_cbor).unwrap();
    assert!(
        !proof_bytes
            .windows(encoded_private.len())
            .any(|part| part == encoded_private)
    );
    assert!(
        page.view()["state"]["agents"]
            .get("private-wire3-world-agent-4975")
            .is_some(),
        "the real nonempty action is visible in the authorized public projection"
    );
    for field in ["actions_cbor", "payload_cbor", "snapshot"] {
        assert!(
            serde_json::to_value(&response)
                .unwrap()
                .get(field)
                .is_none()
        );
    }
    let mut forged = response.clone();
    forged.observation_nonce = "cd".repeat(32);
    assert!(verifier.verify_response(&signed, forged).is_err());
    let mut forged = response.clone();
    forged.handoff_anchor.record_id = "cd".repeat(32);
    assert!(verifier.verify_response(&signed, forged).is_err());
    let mut forged = response.clone();
    forged.target_config_digest = "cd".repeat(32);
    assert!(verifier.verify_response(&signed, forged).is_err());
    let mut forged = response.clone();
    forged.head.execution_commitment_hash = "cd".repeat(32);
    assert!(verifier.verify_response(&signed, forged).is_err());
    let mut forged = response.clone();
    forged.head.logical_tick -= 1;
    assert!(verifier.verify_response(&signed, forged).is_err());
    let mut reordered = prefix.clone();
    reordered.entries.swap(0, 1);
    let mut forged = response.clone();
    forged.certified_prefix_cbor_hex = hex::encode(reordered.canonical_bytes().unwrap());
    assert!(verifier.verify_response(&signed, forged).is_err());

    let mut forged = response.clone();
    forged.projection.audience.reader_public_key =
        Some(hex::encode(reader.verifying_key().to_bytes()));
    assert!(verifier.verify_response(&signed, forged).is_err());
    let mut forged = response.clone();
    forged.projection.audience.scope_id = "agent:another-reader".into();
    assert!(verifier.verify_response(&signed, forged).is_err());
    let mut forged = response.clone();
    forged.view = serde_json::json!({"forged":true});
    assert!(verifier.verify_response(&signed, forged).is_err());
    let mut truncated = prefix.clone();
    truncated.entries.remove(1);
    let mut forged = response.clone();
    forged.certified_prefix_cbor_hex = hex::encode(truncated.canonical_bytes().unwrap());
    assert!(verifier.verify_response(&signed, forged).is_err());
    let mut forged = response.clone();
    forged.head.projection_root = "ab".repeat(32);
    assert!(verifier.verify_response(&signed, forged).is_err());
    let mut forged = response;
    forged.exact_target.height = 4;
    assert!(verifier.verify_response(&signed, forged).is_err());

    // The old path and its signature domain cannot be accepted as V4.
    assert_status(
        transport.call::<_, serde_json::Value>(FORMAL_BFT_READ_PATH_V3, &signed),
        404,
    );
    assert_status(
        transport.call::<_, serde_json::Value>(FORMAL_BFT_READ_PATH_V2, &signed),
        404,
    );
    let old_signed = oasis7::world_service::authority::sign_read_request(
        FORMAL_BFT_READ_PATH_V2,
        request,
        &hex::encode(reader.to_bytes()),
    )
    .unwrap();
    assert_status(
        transport.call::<_, serde_json::Value>(FORMAL_BFT_HEADER_READ_PATH_V4, &old_signed),
        400,
    );
    let private_client = client(&h, bind, &service, &reader, exact.clone(), "agent:alice");
    assert_status(private_client.read::<serde_json::Value>(), 503);

    let earlier = &prefix.entries[1];
    let earlier_target = FormalBftHeaderReadTargetV4 {
        height: earlier.execution_commitment.height,
        header_id: earlier.header.block_id().unwrap(),
    };
    let earlier_client = client(
        &h,
        bind,
        &service,
        &reader,
        earlier_target.clone(),
        "public",
    );
    assert_status(earlier_client.read::<serde_json::Value>(), 400);
    server.stop();
    // The same CLI can reopen a specifically configured earlier historical cut.
    config_value["exact_height"] = serde_json::json!(earlier_target.height);
    config_value["exact_header_id"] = serde_json::json!(earlier_target.header_id);
    private_file(&config, &serde_json::to_vec(&config_value).unwrap());
    let mut server = Server::start(&config);
    let earlier_page = server.read(&earlier_client);
    assert_eq!(earlier_page.proof().heights(), 2);
    assert_eq!(earlier_page.view()["state"]["time"], serde_json::json!(4));
    assert_status(public_client.read::<serde_json::Value>(), 400);
    server.stop();

    // Fresh serve cannot repair a missing retained receipt after installation.
    let shard = store
        .list_pin_scope_shards("formal-bft-header-prefix-v1")
        .unwrap()
        .into_iter()
        .find(|value| value.ends_with("00000000000000000004"))
        .unwrap();
    assert!(
        store
            .remove_pin_scope_shard("formal-bft-header-prefix-v1", &shard)
            .unwrap()
    );
    for _ in 0..2 {
        let stopped = Arc::new(AtomicBool::new(true));
        assert!(
            crate::formal_bft_header_cli::run_with_shutdown(
                ["serve", "--config", config.to_str().unwrap()].into_iter(),
                Some(stopped)
            )
            .is_err()
        );
        assert!(
            !store
                .list_pin_scope_shards("formal-bft-header-prefix-v1")
                .unwrap()
                .contains(&shard)
        );
    }
}

fn cli_files(
    h: &QualifiedH<BftProfileTargetV2>,
    prefix: &super::super::super::formal_header::PrivateHeaderPrefixV3,
    height: u64,
    header_id: &str,
) -> (Scratch, PathBuf, PathBuf) {
    let root = Scratch::new();
    let store = cas(root.path());
    for (digest, bytes) in &h.reader.0 {
        assert_eq!(store.put_bytes(bytes).unwrap(), *digest);
    }
    let trust = root.path().join("trust.json");
    let material = root.path().join("H.cbor");
    let history = root.path().join("history.cbor");
    let config = root.path().join("read.json");
    private_file(&trust, &serde_json::to_vec(&h.trust).unwrap());
    private_file(
        &material,
        &oasis7_wasm_abi::encode_canonical_cbor(&h.material).unwrap(),
    );
    private_file(&history, &prefix.canonical_private_bytes().unwrap());
    private_file(&config, &serde_json::to_vec(&serde_json::json!({
        "schema_version":1, "trust_config_path":trust, "target":h.target,
        "handoff_material_path":material, "cas_directory":store.root(),
        "ledger_directory":root.path().join("ledger"), "exact_height":height,"exact_header_id":header_id,
    })).unwrap());
    (root, config, history)
}
