//! Shipping V3 CLI/HTTP/client evidence over a separately installed, GC-safe
//! three-height ledger. The current private authority remains separate.
use super::prefix_ledger_tests::{actual_prefix, cli_files, open_prefix, private_file};
use super::*;
use ed25519_dalek::SigningKey;
use oasis7_client_api::world_service::{
    FORMAL_BFT_READ_PATH_V2, FORMAL_BFT_READ_PATH_V3, FormalBftReadRequestV3,
    FormalBftReadResponseV3, FormalBftReadTargetV3, MAX_FORMAL_BFT_READ_RESPONSE_BYTES_V3,
    SignedReadRequest,
};
use oasis7_game_client::{
    FormalBftReadClientConfigV3, FormalBftReadClientV3, FormalBftReadVerifierV3,
    WorldServiceClientConfig, WorldServiceClientError, WorldServiceTransport,
};
use oasis7_runtime::LocalCasStore;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[test]
fn formal_prefix_v3_shipping_cli_http_verifies_the_complete_actual_world_prefix() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(read_case)
        .unwrap()
        .join()
        .unwrap();
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
                crate::formal_bft_prefix_cli::run_with_shutdown(
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
        client: &FormalBftReadClientV3,
    ) -> oasis7_game_client::VerifiedFormalBftReadPageV3<serde_json::Value> {
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
                        "V3 CLI never served the actual cut: {error}"
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
    h: &QualifiedH,
    bind: SocketAddr,
    service: &SigningKey,
    reader: &SigningKey,
    target: FormalBftReadTargetV3,
    scope: &str,
) -> FormalBftReadClientV3 {
    FormalBftReadClientV3::new(
        FormalBftReadClientConfigV3 {
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
    let h = qualified_handoff();
    let builder = Scratch::new();
    let prefix = actual_prefix(&h, &builder);
    let tip = prefix.entries.last().unwrap();
    let height = tip.certificate.proposal.block.height;
    assert_eq!(height, 5);
    let block_id = tip.certificate.proposal.block.block_id().unwrap();
    let exact = FormalBftReadTargetV3 {
        height,
        block_id: block_id.clone(),
    };
    let (root, config, proof_file) = cli_files(&h, &prefix, height, &block_id);
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
            "bft-prefix",
            "install",
            "--config",
            config.to_str().unwrap(),
            "--prefix",
            proof_file.to_str().unwrap(),
        ]
        .into_iter(),
    )
    .unwrap()
    .unwrap();
    let store = LocalCasStore::new(root.path().join("cas"));
    store.prune_orphan_blobs().unwrap();
    let public_client = client(&h, bind, &service, &reader, exact.clone(), "public");
    let mut server = Server::start(&config);
    let page = server.read(&public_client);
    assert_eq!(page.proof().heights(), 3);
    assert_eq!(page.response().head.height, 5);
    assert_eq!(page.response().head.logical_tick, 5);
    assert_eq!(page.view()["state"]["time"], serde_json::json!(5));
    assert!(
        open_prefix(&h, &root, Arc::new(EmptyIndexReader), false).is_err(),
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
        max_response_bytes: MAX_FORMAL_BFT_READ_RESPONSE_BYTES_V3,
    })
    .unwrap();
    let verifier =
        FormalBftReadVerifierV3::new(h.anchor.clone(), h.target.clone(), exact.clone()).unwrap();
    let request = verifier.request("public", [9; 32]).unwrap();
    let signed: SignedReadRequest<FormalBftReadRequestV3> =
        oasis7::world_service::authority::sign_read_request(
            FORMAL_BFT_READ_PATH_V3,
            request.clone(),
            &hex::encode(reader.to_bytes()),
        )
        .unwrap();
    let response: FormalBftReadResponseV3<serde_json::Value> =
        transport.call(FORMAL_BFT_READ_PATH_V3, &signed).unwrap();
    verifier.verify_response(&signed, response.clone()).unwrap();
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

    // The old path and its signature domain cannot be accepted as V3.
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
        transport.call::<_, serde_json::Value>(FORMAL_BFT_READ_PATH_V3, &old_signed),
        400,
    );
    assert!(
        oasis7_game_client::FormalBftReadVerifierV2::new(
            h.anchor.clone(),
            h.target.clone(),
            oasis7_client_api::world_service::FormalBftReadTargetV2 {
                height,
                block_id: block_id.clone()
            }
        )
        .is_err()
    );
    let private_client = client(&h, bind, &service, &reader, exact.clone(), "agent:alice");
    assert_status(private_client.read::<serde_json::Value>(), 503);

    let earlier = &prefix.entries[1].certificate.proposal.block;
    let earlier_target = FormalBftReadTargetV3 {
        height: 4,
        block_id: earlier.block_id().unwrap(),
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
    config_value["exact_target"] = serde_json::to_value(&earlier_target).unwrap();
    private_file(&config, &serde_json::to_vec(&config_value).unwrap());
    let mut server = Server::start(&config);
    let earlier_page = server.read(&earlier_client);
    assert_eq!(earlier_page.proof().heights(), 2);
    assert_eq!(earlier_page.view()["state"]["time"], serde_json::json!(4));
    assert_status(public_client.read::<serde_json::Value>(), 400);
    server.stop();

    // Fresh serve cannot repair a missing retained receipt after installation.
    let shard = store
        .list_pin_scope_shards("formal-bft-prefix-v1")
        .unwrap()
        .into_iter()
        .find(|value| value.ends_with("00000000000000000004"))
        .unwrap();
    assert!(
        store
            .remove_pin_scope_shard("formal-bft-prefix-v1", &shard)
            .unwrap()
    );
    for _ in 0..2 {
        let stopped = Arc::new(AtomicBool::new(true));
        assert!(
            crate::formal_bft_prefix_cli::run_with_shutdown(
                ["serve", "--config", config.to_str().unwrap()].into_iter(),
                Some(stopped)
            )
            .is_err()
        );
        assert!(
            !store
                .list_pin_scope_shards("formal-bft-prefix-v1")
                .unwrap()
                .contains(&shard)
        );
    }
}
