//! Exercise the outer failover loop over real signed HTTP material responses.
use super::failover_tests::{config, local_instance};
use super::*;
use oasis7_client_api::world_service::SignedReadRequest;
use oasis7_client_api::world_service_full_sync::{
    FORMAL_FULL_SYNC_OBJECT_PATH, FormalFullSyncManifestV1, FormalFullSyncObjectDescriptorV1,
};
use std::sync::atomic::AtomicBool;

struct HostileProvider {
    endpoint: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for HostileProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.endpoint.trim_start_matches("http://"));
        self.thread.take().unwrap().join().unwrap();
    }
}

// Relay independently verified reads unchanged. Only the material provider is
// hostile: its service signature authenticates transport, not canonical roots.
fn hostile_provider(upstream: String, junk: Vec<u8>, late_hash: String) -> HostileProvider {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&requests);
    let thread = std::thread::spawn(move || {
        for connection in listener.incoming() {
            let mut stream = connection.unwrap();
            if stopped.load(Ordering::Acquire) {
                break;
            }
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
                assert!(header.len() < 16384);
            }
            let text = String::from_utf8(header.clone()).unwrap();
            let path = text.split_whitespace().nth(1).unwrap();
            let length: usize = text
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            assert!(length < 1024 * 1024);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            let junk_hash = blake3_hex(&junk);
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let object_hash = request["request"]["content_hash"].as_str();
            if let Some(hash) = object_hash {
                observed.lock().unwrap().push(hash.to_owned());
            }
            let response = if path == FORMAL_FULL_SYNC_OBJECT_PATH
                && object_hash == Some(junk_hash.as_str())
            {
                let request: SignedReadRequest<FormalFullSyncObjectRequestV1> =
                    serde_json::from_slice(&body).unwrap();
                let payload = FormalFullSyncObjectChunkV1 {
                    schema_version: 1,
                    cut: request.request.cut.clone(),
                    content_hash: junk_hash,
                    total_bytes: junk.len() as u64,
                    offset: 0,
                    bytes: junk.clone(),
                };
                let signed = authority::sign_service_response(
                    path,
                    authority::request_digest(path, &request).unwrap(),
                    payload,
                    &hex::encode(test_key(91).to_bytes()),
                )
                .unwrap();
                http_response(&serde_json::to_vec(&signed).unwrap())
            } else {
                let mut upstream =
                    TcpStream::connect(upstream.trim_start_matches("http://")).unwrap();
                upstream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                upstream.write_all(&header).unwrap();
                upstream.write_all(&body).unwrap();
                let mut response = Vec::new();
                upstream.read_to_end(&mut response).unwrap();
                if path == FORMAL_FULL_SYNC_MANIFEST_PATH {
                    let split = response
                        .windows(4)
                        .position(|bytes| bytes == b"\r\n\r\n")
                        .unwrap()
                        + 4;
                    let signed: SignedServiceResponse<FormalFullSyncManifestV1> =
                        serde_json::from_slice(&response[split..]).unwrap();
                    let mut manifest = signed.payload;
                    // Keep roots exact and the descriptor graph structurally closed.
                    // A later object's incorrect length fails only after junk is charged.
                    let late = manifest
                        .objects
                        .iter_mut()
                        .find(|o| o.content_hash == late_hash)
                        .unwrap();
                    late.size_bytes -= junk.len() as u64;
                    late.references.push(junk_hash.clone());
                    late.references.sort();
                    manifest.objects.push(FormalFullSyncObjectDescriptorV1 {
                        content_hash: junk_hash,
                        size_bytes: junk.len() as u64,
                        references: vec![],
                    });
                    manifest
                        .objects
                        .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
                    manifest.validate().unwrap();
                    let signed = authority::sign_service_response(
                        path,
                        signed.request_digest,
                        manifest,
                        &hex::encode(test_key(91).to_bytes()),
                    )
                    .unwrap();
                    http_response(&serde_json::to_vec(&signed).unwrap())
                } else {
                    response
                }
            };
            stream.write_all(&response).unwrap();
        }
    });
    HostileProvider {
        endpoint,
        requests,
        stop,
        thread: Some(thread),
    }
}

fn http_response(body: &[u8]) -> Vec<u8> {
    let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
    response.extend_from_slice(body);
    response
}

#[test]
fn formal_full_sync_real_provider_charges_survive_failover_and_fresh_call_recovers() {
    let (world, trust, initial_anchor, initial_material, proof) = full_sync_fixture();
    let target = same_epoch_successor_fixture(&world, &initial_anchor, &initial_material);
    let (mut server, endpoint, key, runtime, published, history) = start_live_service(
        world,
        trust.clone(),
        initial_anchor,
        initial_material,
        proof,
    );
    history.push(target.clone());
    published
        .publish(target.world.clone(), target.anchor.clone())
        .unwrap();
    // A real C0 -> C1 page must survive rejected material and client rebuild.
    // C0 alone intentionally has no persisted last_verified_cut.
    let anchor = target.anchor;
    let material = target.material;
    let declared: u64 = material.objects.iter().map(|o| o.bytes.len() as u64).sum();
    let late = material
        .objects
        .iter()
        .filter(|o| o.bytes.len() > 8)
        .max_by_key(|o| o.content_hash.clone())
        .unwrap();
    // Find a small real object ordered before the deliberately wrong descriptor.
    let junk = (0u64..10_000)
        .map(u64::to_be_bytes)
        .find(|bytes| blake3_hex(bytes) < late.content_hash)
        .expect("bounded fixture search must find an earlier digest")
        .to_vec();
    let junk_hash = blake3_hex(&junk);
    let late_hash = late.content_hash.clone();
    let snapshot_hash = material.roots[&ArtifactRole::Snapshot].clone();
    let hostile = hostile_provider(endpoint.clone(), junk.clone(), late_hash.clone());
    let node_key = test_key(92);
    // Size the newly retained canonical manifest from the real authenticated
    // provider, rather than padding the budget or duplicating its serializer.
    let probe_dir = TestPrivateDir::new();
    let probe = FormalFullSyncSession::new(
        config(
            endpoint.clone(),
            key.clone(),
            vec![],
            trust.clone(),
            &node_key,
        ),
        hex::encode(node_key.to_bytes()),
        &probe_dir.0.join("full-sync"),
        &local_instance("manifest-budget-probe"),
    )
    .unwrap();
    let manifest = probe.fetch_manifest(&cut(&anchor)).unwrap();
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let manifest_hash = blake3_hex(&manifest_bytes);
    let declared = declared
        + if material
            .objects
            .iter()
            .any(|o| o.content_hash == manifest_hash)
        {
            0
        } else {
            manifest_bytes.len() as u64
        };
    drop(probe);
    for spare in [0, junk.len() as u64] {
        hostile.requests.lock().unwrap().clear();
        let mut config = config(
            hostile.endpoint.clone(),
            key.clone(),
            vec![FormalFullSyncEndpointV1 {
                endpoint: endpoint.clone(),
                trusted_service_public_key: key.clone(),
            }],
            trust.clone(),
            &node_key,
        );
        config.max_total_sync_bytes = declared + spare;
        let directory = TestPrivateDir::new();
        let mut session = FormalFullSyncSession::new(
            config.clone(),
            hex::encode(node_key.to_bytes()),
            &directory.0.join("full-sync"),
            &local_instance("real-provider-budget"),
        )
        .unwrap();
        let result = session.sync_latest();
        let requests = hostile.requests.lock().unwrap().clone();
        let junk_request = requests.iter().position(|hash| hash == &junk_hash).unwrap();
        let late_request = requests.iter().position(|hash| hash == &late_hash).unwrap();
        assert!(junk_request < late_request);
        if spare == 0 {
            let error = result
                .err()
                .expect("failed-provider charge must consume this call's budget");
            assert!(
                error
                    .starts_with("provider[0]: full-sync object chunk has inconsistent total size"),
                "{error}"
            );
            assert!(
                error.contains("provider[1]")
                    && error.contains("exceeds configured protocol bound"),
                "{error}"
            );
            assert_eq!(session.status().loaded_cut, None);
            assert!(!session.status().ready);
            assert_eq!(session.status().last_verified_cut, Some(cut(&anchor)));
            assert!(
                !session
                    .cas
                    .list_effective_pins()
                    .unwrap()
                    .contains(&snapshot_hash)
            );
            assert!(
                !session
                    .cas
                    .list_effective_pins()
                    .unwrap()
                    .contains(&junk_hash)
            );
            // A reconstructed client and new call reuse the same CAS/history,
            // but do not inherit the previous call's material charges.
            drop(session);
            config.endpoint = endpoint.clone();
            config.alternate_endpoints.clear();
            session = FormalFullSyncSession::new(
                config,
                hex::encode(node_key.to_bytes()),
                &directory.0.join("full-sync"),
                &local_instance("real-provider-budget"),
            )
            .unwrap();
            let restored = session.sync_latest().unwrap();
            assert_eq!(restored.cut, cut(&anchor));
            let restored_bytes =
                oasis7_world_state::codec::snapshot_cbor(&restored.world.snapshot()).unwrap();
            assert_eq!(
                blake3_hex(&restored_bytes),
                anchor.head().record().after_state_root
            );
        } else {
            let restored = result.unwrap();
            assert_eq!(restored.cut, cut(&anchor));
            assert_eq!(session.active_provider_index, 1);
            assert!(!restored.retained_cas_hashes.contains(&junk_hash));
        }
        assert_eq!(session.status().loaded_cut, Some(cut(&anchor)));
        assert!(session.status().ready);
        assert!(
            session
                .cas
                .list_effective_pins()
                .unwrap()
                .contains(&manifest_hash)
        );
        assert!(
            !session
                .cas
                .list_effective_pins()
                .unwrap()
                .contains(&junk_hash)
        );
    }
    drop(hostile);
    server.stop().unwrap();
    runtime.lock().unwrap().stop().unwrap();
}
