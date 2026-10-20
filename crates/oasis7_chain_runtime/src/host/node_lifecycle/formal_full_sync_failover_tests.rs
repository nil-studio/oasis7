use super::*;
use oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole;
use oasis7_runtime::{BlobStore, LocalCasStore};
use std::net::{TcpListener, TcpStream};

fn local_instance(label: &str) -> LocalInstance {
    LocalInstance {
        api_version: "oasis7.local-node-instance/v1".into(),
        action_id: format!("{label}-action"),
        generation: 1,
        incarnation: format!("{label}-incarnation"),
        config_digest: "c".repeat(64),
        supervisor_pid: std::process::id(),
        runtime_pid: None,
        started_unix_ms: 1,
        state: "running".into(),
        exit_code: None,
    }
}

fn free_loopback_endpoint() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{address}")
}

fn config(
    endpoint: String,
    service_key: String,
    alternates: Vec<FormalFullSyncEndpointV1>,
    trust: oasis7::world_service::client::FormalReadTrustConfigV1,
    node_key: &ed25519_dalek::SigningKey,
) -> FormalFullSyncConfigV1 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    FormalFullSyncConfigV1 {
        api_version: API_VERSION.into(),
        endpoint,
        trusted_service_public_key: service_key,
        alternate_endpoints: alternates,
        world_id: trust.initial_trust.world.world_id.clone(),
        chain_id: trust.initial_trust.world.chain_id.clone(),
        genesis_digest: trust.initial_trust.world.genesis_digest.clone(),
        formal_read_trust: trust,
        grant: Some(signed_grant(node_key, now, 1)),
        expected_role: FormalNodeReadRoleV1::StateSync,
        poll_interval_ms: 100,
        request_timeout_ms: 2_000,
        max_total_sync_bytes: 64 * 1024 * 1024,
    }
}

#[test]
fn formal_full_sync_uses_independently_pinned_alternate_when_seed_is_unavailable() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let (mut server, alternate_endpoint, alternate_key, node_runtime, _, _) =
        start_live_service(world, trust.clone(), anchor.clone(), material, proof);
    let node_key = test_key(92);
    let config = config(
        free_loopback_endpoint(),
        hex::encode(test_key(93).verifying_key().to_bytes()),
        vec![FormalFullSyncEndpointV1 {
            endpoint: alternate_endpoint,
            trusted_service_public_key: alternate_key,
        }],
        trust,
        &node_key,
    );
    let state_dir = TestPrivateDir::new();
    let mut session = FormalFullSyncSession::new(
        config,
        hex::encode(node_key.to_bytes()),
        &state_dir.0.join("full-sync"),
        &local_instance("seed-failover"),
    )
    .unwrap();

    let restored = session.sync_latest().unwrap();

    assert_eq!(restored.cut, cut(&anchor));
    assert_eq!(session.active_provider_index, 1);
    assert!(session.status().ready);
    assert!(session.status().state_root_verified);
    assert!(!session.status().can_propose);
    server.stop().unwrap();
    node_runtime.lock().unwrap().stop().unwrap();
}

#[test]
fn formal_full_sync_rejects_bad_provider_material_then_replays_fixed_c0_for_alternate() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let mut bad_server_material = material.clone();
    bad_server_material
        .roots
        .insert(ArtifactRole::Snapshot, test_hash(117));
    let (mut bad_server, bad_endpoint, bad_key, bad_runtime, bad_published, bad_history) =
        start_live_service(
            world.clone(),
            trust.clone(),
            anchor.clone(),
            material.clone(),
            proof.clone(),
        );
    let (mut good_server, good_endpoint, good_key, good_runtime, good_published, good_history) =
        start_live_service(
            world.clone(),
            trust.clone(),
            anchor.clone(),
            material.clone(),
            proof,
        );

    let successor = same_epoch_successor_fixture(&world, &anchor, &material);
    let mut bad_successor = successor.clone();
    bad_successor.material = bad_server_material;
    bad_history.push(bad_successor);
    bad_published
        .publish(successor.world.clone(), successor.anchor.clone())
        .unwrap();
    good_history.push(successor.clone());
    good_published
        .publish(successor.world.clone(), successor.anchor.clone())
        .unwrap();

    let node_key = test_key(92);
    let config = config(
        bad_endpoint,
        bad_key,
        vec![FormalFullSyncEndpointV1 {
            endpoint: good_endpoint,
            trusted_service_public_key: good_key,
        }],
        trust,
        &node_key,
    );
    let state_dir = TestPrivateDir::new();
    let mut session = FormalFullSyncSession::new(
        config,
        hex::encode(node_key.to_bytes()),
        &state_dir.0.join("full-sync"),
        &local_instance("bad-proof-failover"),
    )
    .unwrap();

    let restored = session.sync_latest().unwrap();

    assert_eq!(restored.cut, cut(&successor.anchor));
    assert_eq!(session.active_provider_index, 1);
    assert_eq!(
        session.status().last_verified_cut,
        Some(cut(&successor.anchor))
    );
    assert!(session.status().ready);
    assert!(!session.status().can_propose);

    bad_server.stop().unwrap();
    good_server.stop().unwrap();
    bad_runtime.lock().unwrap().stop().unwrap();
    good_runtime.lock().unwrap().stop().unwrap();
}

#[test]
fn formal_full_sync_alternate_service_signature_must_match_its_own_pin() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let (mut server, endpoint, _service_key, node_runtime, _, _) =
        start_live_service(world, trust.clone(), anchor, material, proof);
    let node_key = test_key(92);
    let config = config(
        free_loopback_endpoint(),
        hex::encode(test_key(93).verifying_key().to_bytes()),
        vec![FormalFullSyncEndpointV1 {
            endpoint,
            trusted_service_public_key: hex::encode(test_key(94).verifying_key().to_bytes()),
        }],
        trust,
        &node_key,
    );
    let state_dir = TestPrivateDir::new();
    let mut session = FormalFullSyncSession::new(
        config,
        hex::encode(node_key.to_bytes()),
        &state_dir.0.join("full-sync"),
        &local_instance("wrong-alternate-pin"),
    )
    .unwrap();

    assert!(session.sync_latest().is_err());
    assert_eq!(session.active_provider_index, 0);
    assert!(!session.status().ready);
    assert!(!session.status().state_root_verified);

    server.stop().unwrap();
    node_runtime.lock().unwrap().stop().unwrap();
}

#[test]
fn formal_full_sync_compatibility_probe_replays_local_history_when_all_endpoints_are_unavailable() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let (mut server, live_endpoint, service_key, node_runtime, published, history) =
        start_live_service(
            world.clone(),
            trust.clone(),
            anchor.clone(),
            material.clone(),
            proof,
        );
    let successor = same_epoch_successor_fixture(&world, &anchor, &material);
    history.push(successor.clone());
    published
        .publish(successor.world.clone(), successor.anchor.clone())
        .unwrap();

    let root = TestPrivateDir::new();
    let data_dir = root.0.join("data");
    super::super::super::local_io::private_dir(&data_dir).unwrap();
    let keys_dir = data_dir.join("keys");
    super::super::super::local_io::private_dir(&keys_dir).unwrap();
    let (keypair, _) =
        crate::node_keypair_config::ensure_node_keypair_in_secure_config_dir(&keys_dir).unwrap();
    let key_seed: [u8; 32] = hex::decode(&keypair.private_key_hex)
        .unwrap()
        .try_into()
        .unwrap();
    let node_key = ed25519_dalek::SigningKey::from_bytes(&key_seed);

    let state_dir = data_dir.join("state");
    super::super::super::local_io::private_dir(&state_dir).unwrap();
    let cas_root = state_dir.join("cas");
    let live_config = config(
        live_endpoint,
        service_key.clone(),
        vec![],
        trust.clone(),
        &node_key,
    );
    let node_private_key = keypair.private_key_hex.clone();
    let mut session = FormalFullSyncSession::new(
        live_config,
        node_private_key,
        &cas_root,
        &local_instance("compatibility-history-seed"),
    )
    .unwrap();
    let restored = session.sync_latest().unwrap();
    assert_eq!(restored.cut, cut(&successor.anchor));
    drop(session);
    server.stop().unwrap();
    node_runtime.lock().unwrap().stop().unwrap();

    let unavailable_primary = "http://127.0.0.1:0".to_owned();
    let unavailable_alternate = "http://127.0.0.2:0".to_owned();
    assert!(TcpStream::connect("127.0.0.1:0").is_err());
    assert!(TcpStream::connect("127.0.0.2:0").is_err());
    let offline_config = config(
        unavailable_primary,
        service_key.clone(),
        vec![FormalFullSyncEndpointV1 {
            endpoint: unavailable_alternate,
            trusted_service_public_key: service_key,
        }],
        trust,
        &node_key,
    );
    let node: super::super::super::model::NodeConfig = serde_json::from_value(serde_json::json!({
        "api_version": "oasis7.local-node/v3",
        "identity": {
            "api_version": "oasis7.local-node/v3",
            "chain": {
                "runtime_profile": PROFILE,
                "world_id": offline_config.world_id.clone(),
                "chain_id": offline_config.chain_id.clone(),
                "formal_full_sync": offline_config.clone()
            },
            "data_dir": data_dir.clone(),
            "node_id": "offline-compatibility-probe",
            "root_public_key": keypair.public_key_hex.clone(),
            "peer_id": "unused-in-read-only-probe"
        },
        "role": "observer",
        "rpc_bind": "127.0.0.1:0",
        "additional_peers": []
    }))
    .unwrap();

    let descriptor = super::super::compatibility::probe_runtime_compatibility(
        node.identity.chain.formal_full_sync().unwrap(),
        &node,
        &data_dir.join(STATUS_FILE),
        &data_dir,
        None,
    )
    .expect("the locally retained qualified history is replayed without contacting providers");

    assert_eq!(descriptor["history_head_verified"], true);
    assert_eq!(descriptor["history_source"], "local_qualified_history");
    assert!(descriptor.get("provider_index").is_none());
    assert_eq!(descriptor["history"]["page_count"], 1);
    assert_eq!(
        descriptor["history"]["current_cut"],
        serde_json::to_value(cut(&successor.anchor)).unwrap()
    );
}

#[test]
fn formal_full_sync_local_cas_corruption_poisons_without_provider_failover() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let (mut seed_server, seed_endpoint, seed_key, seed_runtime, _, _) = start_live_service(
        world.clone(),
        trust.clone(),
        anchor.clone(),
        material.clone(),
        proof.clone(),
    );
    let (mut alternate_server, alternate_endpoint, alternate_key, alternate_runtime, _, _) =
        start_live_service(world, trust.clone(), anchor, material.clone(), proof);
    let node_key = test_key(92);
    let state_dir = TestPrivateDir::new();
    let cas_root = state_dir.0.join("full-sync");
    super::super::super::local_io::private_dir(&cas_root).unwrap();

    let snapshot_hash = material.roots[&ArtifactRole::Snapshot].clone();
    let snapshot = material
        .objects
        .iter()
        .find(|object| object.content_hash == snapshot_hash)
        .expect("fixture Snapshot object")
        .bytes
        .clone();
    let cas = LocalCasStore::new(&cas_root);
    assert_eq!(cas.put_bytes(&snapshot).unwrap(), snapshot_hash);
    std::fs::write(
        cas.blobs_dir().join(format!("{snapshot_hash}.blob")),
        b"corrupt local CAS object",
    )
    .unwrap();
    super::super::cas_lifecycle::replace_loaded_world_pins(
        &cas,
        &std::collections::BTreeSet::from([snapshot_hash]),
    )
    .expect("retain the damaged object so startup GC cannot hide the failure");

    let config = config(
        seed_endpoint,
        seed_key,
        vec![FormalFullSyncEndpointV1 {
            endpoint: alternate_endpoint,
            trusted_service_public_key: alternate_key,
        }],
        trust,
        &node_key,
    );
    let mut session = FormalFullSyncSession::new(
        config,
        hex::encode(node_key.to_bytes()),
        &cas_root,
        &local_instance("corrupt-local-cas"),
    )
    .unwrap();

    let error = match session.sync_latest() {
        Err(error) => error,
        Ok(_) => panic!("a hash-invalid local object must poison provider failover"),
    };

    assert!(
        error.contains("local content-addressed object is invalid"),
        "{error}"
    );
    assert_eq!(session.active_provider_index, 0);
    assert!(!session.status().ready);
    assert!(!session.status().state_root_verified);

    seed_server.stop().unwrap();
    alternate_server.stop().unwrap();
    seed_runtime.lock().unwrap().stop().unwrap();
    alternate_runtime.lock().unwrap().stop().unwrap();
}

#[test]
fn formal_full_sync_alternates_are_bounded_and_legacy_config_bytes_stay_unchanged() {
    let (_, trust, _, _, _) = full_sync_fixture();
    let node_key = test_key(92);
    let seed_endpoint = free_loopback_endpoint();
    let base = config(
        seed_endpoint.clone(),
        hex::encode(test_key(93).verifying_key().to_bytes()),
        vec![],
        trust.clone(),
        &node_key,
    );

    let serialized = serde_json::to_value(&base).unwrap();
    assert!(serialized.get("alternate_endpoints").is_none());
    let round_trip: FormalFullSyncConfigV1 = serde_json::from_value(serialized).unwrap();
    assert_eq!(round_trip, base);

    let mut duplicate = base.clone();
    duplicate
        .alternate_endpoints
        .push(FormalFullSyncEndpointV1 {
            endpoint: seed_endpoint,
            trusted_service_public_key: hex::encode(test_key(94).verifying_key().to_bytes()),
        });
    assert!(duplicate.validate().is_err());

    let mut too_many = base;
    too_many.alternate_endpoints = (0..MAX_SYNC_PROVIDERS)
        .map(|seed| FormalFullSyncEndpointV1 {
            endpoint: free_loopback_endpoint(),
            trusted_service_public_key: hex::encode(
                test_key(100 + seed as u8).verifying_key().to_bytes(),
            ),
        })
        .collect();
    assert!(too_many.validate().is_err());
}
