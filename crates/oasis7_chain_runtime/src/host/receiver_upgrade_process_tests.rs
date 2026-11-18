//! OS-process coverage of the real CLI dispatcher and development network.
//! This is not a test of an installed release executable or public networking.
use super::*;
use oasis7::development_node_config::{
    DevelopmentPeerRecordPublicationModeV1, DevelopmentPeerRecordReceptionModeV1,
};
use oasis7_proto::distributed_dht::{
    DistributedDht, PeerRecordPublicationStateV2, PeerRecordPublicationV2, SignedPeerRecord,
};
use serde::Serialize;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn receiver_files(directory: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            (entry.file_name().into_string().unwrap(), fs::read(entry.path()).unwrap())
        })
        .collect()
}

const CHILD_ENV: &str = "OASIS7_TEST_RECEIVER_UPGRADE_PROCESS";

struct Process(Child);
impl Process {
    fn wait(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success(), "receiver child failed: {status}");
                return;
            }
            assert!(Instant::now() < deadline, "receiver child timed out");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // Reap only this owned process, including parent assertion failures.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn(root: &Path, role: &str) -> Process {
    Process(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "receiver_upgrade_process_child_entry",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_ENV, serde_json::to_string(&(root, role)).unwrap())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    )
}

fn options(root: &Path) -> CliOptions {
    CliOptions {
        runtime_root: Some(root.join("runtime")),
        config_path: root.join("config.json").to_str().unwrap().to_owned(),
        replication_network_listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".into()],
        p2p_peer_record_reception: DevelopmentPeerRecordReceptionModeV1::V2DurableCompatible,
        ..CliOptions::default()
    }
}

fn cli(root: &Path, role: &str) -> Result<(), String> {
    let mut args = vec![
        "--runtime-root".into(),
        root.join("runtime").to_str().unwrap().into(),
        "--config".into(),
        root.join("config.json").to_str().unwrap().into(),
        "--p2p-peer-record-reception".into(),
    ];
    if role == "initialize" {
        args.extend([
            "v2_durable_compatible".into(),
            "--initialize-peer-record-receiver".into(),
        ]);
    } else {
        args.extend(["v2_only".into(), "--upgrade-peer-record-receiver".into()]);
    }
    super::run_cli(args)
}

#[test]
fn receiver_upgrade_process_child_entry() {
    let Ok(encoded) = std::env::var(CHILD_ENV) else {
        return;
    };
    let (root, role): (PathBuf, String) = serde_json::from_str(&encoded).unwrap();
    match role.as_str() {
        "initialize" | "upgrade" => cli(&root, &role).unwrap(),
        "reject-locked" => {
            let error = cli(&root, "upgrade").unwrap_err();
            assert!(error.contains("already in use"), "wrong rejection: {error}");
        }
        "hold" => {
            let options = options(&root);
            let key = node_keypair_config::read_existing_legacy_keypair(Path::new(
                &options.config_path,
            ))
            .unwrap();
            let paths = super::resolve_runtime_paths(&options);
            let network =
                super::create_default_replication_network(&options, &key, &paths).unwrap();
            fs::write(root.join("ready"), b"locked").unwrap();
            let deadline = Instant::now() + Duration::from_secs(40);
            while !root.join("release").exists() {
                assert!(Instant::now() < deadline, "writer release timed out");
                std::thread::sleep(Duration::from_millis(20));
            }
            drop(network);
        }
        "seed" => seed_nonempty(&root),
        "cold" => cold_consume(&root),
        _ => panic!("unknown child role"),
    }
    fs::write(root.join(format!("done-{role}")), b"complete").unwrap();
}

fn cold_consume(root: &Path) {
    let mut options = options(root);
    let key =
        node_keypair_config::read_existing_legacy_keypair(Path::new(&options.config_path))
            .unwrap();
    let paths = super::resolve_runtime_paths(&options);
    let error = super::create_default_replication_network(&options, &key, &paths)
        .err()
        .expect("old mode must reject the upgraded ledger");
    assert!(
        error.contains("mode mismatch"),
        "wrong old-mode rejection: {error}"
    );
    options.p2p_peer_record_reception = DevelopmentPeerRecordReceptionModeV1::V2Only;
    options.p2p_peer_record_publication = DevelopmentPeerRecordPublicationModeV1::V2Durable;
    super::initialize_default_peer_record_publication(&options).unwrap();
    let network = super::create_default_replication_network(&options, &key, &paths).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(Some(record)) =
            network.get_peer_record(&options.world_id, &network.peer_id().to_string())
        {
            assert!(
                record.publication.is_some(),
                "V2-only must consume a V2 signed record"
            );
            assert_eq!(record.record.peer_id, network.peer_id().to_string());
            fs::write(
                root.join("consumed.cbor"),
                serde_cbor::to_vec(&record).unwrap(),
            )
            .unwrap();
            break;
        }
        assert!(
            Instant::now() < deadline,
            "local signed V2 publication was not consumed"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_cold_matrix(root, &network);
}

#[test]
fn receiver_upgrade_rejects_live_process_then_cold_reopens_v2() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir()
        .join(format!("receiver-process-{}-{nonce}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let root = root.canonicalize().unwrap();
    spawn(&root, "initialize").wait();
    assert!(root.join("done-initialize").exists());
    spawn(&root, "seed").wait();
    assert!(root.join("done-seed").exists());
    let state = root.join("runtime/p2p/peer-record-receiver/peer-record-receiver-v2.state.cbor");
    let original = fs::read(&state).unwrap();
    fs::write(root.join("original.cbor"), &original).unwrap();
    let original_config = fs::read(root.join("config.json")).unwrap();
    let receiver_directory = state.parent().unwrap();
    let original_files = receiver_files(receiver_directory);
    let mut holder = spawn(&root, "hold");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !root.join("ready").exists() {
        assert!(
            holder.0.try_wait().unwrap().is_none(),
            "writer exited before readiness"
        );
        assert!(Instant::now() < deadline, "writer did not hold receiver");
        std::thread::sleep(Duration::from_millis(20));
    }
    let locked_files = receiver_files(receiver_directory);
    assert_eq!(
        original_files.keys().collect::<Vec<_>>(),
        locked_files.keys().collect::<Vec<_>>()
    );
    let original_cbor: serde_cbor::Value = serde_cbor::from_slice(&original).unwrap();
    let locked_state = fs::read(&state).unwrap();
    let locked_cbor: serde_cbor::Value = serde_cbor::from_slice(&locked_state).unwrap();
    let serde_cbor::Value::Map(ref orig_map) = original_cbor else { panic!("orig map"); };
    let serde_cbor::Value::Map(ref locked_map) = locked_cbor else { panic!("locked map"); };
    for (k, v) in orig_map {
        if matches!(k, serde_cbor::Value::Text(s) if s == "pre_migration_v1_expiries") {
            continue;
        }
        assert_eq!(Some(v), locked_map.get(k), "field must remain unchanged during hold");
    }
    spawn(&root, "reject-locked").wait();
    assert!(root.join("done-reject-locked").exists());
    let post_reject_files = receiver_files(receiver_directory);
    assert_eq!(
        locked_files.keys().collect::<Vec<_>>(),
        post_reject_files.keys().collect::<Vec<_>>()
    );
    let post_reject_state = fs::read(&state).unwrap();
    let post_reject_cbor: serde_cbor::Value = serde_cbor::from_slice(&post_reject_state).unwrap();
    let serde_cbor::Value::Map(ref post_reject_map) = post_reject_cbor else { panic!("post reject map"); };
    for (k, v) in locked_map {
        if matches!(k, serde_cbor::Value::Text(s) if s == "pre_migration_v1_expiries") {
            continue;
        }
        assert_eq!(Some(v), post_reject_map.get(k), "rejected upgrade must not change ledger fields");
    }
    assert_eq!(original_config, fs::read(root.join("config.json")).unwrap());
    fs::write(root.join("release"), b"release").unwrap();
    holder.wait();
    let stable_state = fs::read(&state).unwrap();
    let stable_files = receiver_files(receiver_directory);
    spawn(&root, "upgrade").wait();
    assert!(root.join("done-upgrade").exists());
    let upgraded = fs::read(&state).unwrap();
    assert_ne!(stable_state, upgraded);
    let mut expected_ledger: serde_cbor::Value = serde_cbor::from_slice(&stable_state).unwrap();
    let serde_cbor::Value::Map(ref mut fields) = expected_ledger else {
        panic!("ledger map");
    };
    fields.insert(serde_cbor::Value::Text("mode".into()), serde_cbor::value::to_value(oasis7_node::PeerRecordReceiverMode::V2Only).unwrap());
    assert_eq!(expected_ledger, serde_cbor::from_slice::<serde_cbor::Value>(&upgraded).unwrap());
    let mut expected_files = stable_files;
    expected_files.remove("peer-record-receiver-v2.lock");
    expected_files.insert(state.file_name().unwrap().to_str().unwrap().into(), upgraded.clone());
    assert_eq!(expected_files, receiver_files(receiver_directory));
    assert_eq!(original_config, fs::read(root.join("config.json")).unwrap());
    spawn(&root, "upgrade").wait();
    assert_eq!(upgraded, fs::read(&state).unwrap(), "repeat changed state");
    assert_eq!(expected_files, receiver_files(receiver_directory));
    spawn(&root, "cold").wait();
    assert!(root.join("done-cold").exists());
    assert_eq!(original_config, fs::read(root.join("config.json")).unwrap());
    assert_eq!(
        expected_files.keys().collect::<Vec<_>>(),
        receiver_files(receiver_directory).keys().collect::<Vec<_>>()
    );
    let consumed: oasis7_proto::distributed_dht::SignedPeerRecord =
        serde_cbor::from_slice(&fs::read(root.join("consumed.cbor")).unwrap()).unwrap();
    let ledger: serde_cbor::Value = serde_cbor::from_slice(&fs::read(&state).unwrap()).unwrap();
    let serde_cbor::Value::Map(fields) = ledger else {
        panic!("ledger map");
    };
    let Some(serde_cbor::Value::Map(floors)) =
        fields.get(&serde_cbor::Value::Text("floors".into()))
    else {
        panic!("receiver floors map");
    };
    assert!(floors.contains_key(&serde_cbor::Value::Text(consumed.record.peer_id)));
    fs::remove_dir_all(root).unwrap();
}

// Independent signed wire fixtures enter the real DHT admission path. Never edit
// receiver state to manufacture floors or isolation evidence.
fn signed_fixture(
    root: &Path,
    peer: u8,
    sequence: Option<u64>,
    state: PeerRecordPublicationStateV2,
    label: &str,
) -> SignedPeerRecord {
    let key = oasis7_node::derive_libp2p_identity_keypair(&hex::encode([peer; 32])).unwrap();
    let mut record = super::replication_network::build_default_peer_record(&options(root));
    record.peer_id = key.public().to_peer_id().to_string();
    record.node_id = label.into();
    record.published_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap();
    let publication = sequence.map(|sequence| PeerRecordPublicationV2 {
        schema_version: 2,
        sequence,
        state,
    });
    let mut payload = if publication.is_some() {
        b"oasis7-peer-record-v2|".to_vec()
    } else {
        b"oasis7-peer-record-v1|".to_vec()
    };
    let value = match &publication {
        Some(p) => serde_cbor::value::to_value((p.schema_version, p.sequence, p.state, &record)).unwrap(),
        None => serde_cbor::value::to_value(&record).unwrap(),
    };
    let mut canonical = Vec::new();
    let mut serializer = serde_cbor::ser::Serializer::new(&mut canonical);
    serializer.self_describe().unwrap();
    value.serialize(&mut serializer).unwrap();
    payload.extend(canonical);
    SignedPeerRecord {
        record,
        identity_public_key_protobuf_hex: hex::encode(key.public().encode_protobuf()),
        signature_hex: hex::encode(key.sign(&payload).unwrap()),
        publication,
    }
}

fn put_accepted(network: &oasis7_node::Libp2pReplicationNetwork, record: &SignedPeerRecord) {
    if let Err(error) = network.put_peer_record(&record.record.world_id, record) {
        assert!(format!("{error:?}").contains("kad put_record failed: the quorum failed; needed 1 peers"), "unexpected admission error: {error:?}");
    }
}

fn put_rejected(network: &oasis7_node::Libp2pReplicationNetwork, record: &SignedPeerRecord) {
    let error = network.put_peer_record(&record.record.world_id, record).unwrap_err();
    assert!(format!("{error:?}").contains("the value is too large to be stored"), "wrong rejection: {error:?}");
}

fn seed_nonempty(root: &Path) {
    let options = options(root);
    let key = node_keypair_config::read_existing_legacy_keypair(Path::new(&options.config_path)).unwrap();
    let paths = super::resolve_runtime_paths(&options);
    let network = super::create_default_replication_network(&options, &key, &paths).unwrap();
    use PeerRecordPublicationStateV2::{Live, Tombstone};
    let legacy = signed_fixture(root, 11, None, Live, "retired");
    put_accepted(&network, &legacy);
    assert_eq!(network.get_peer_record(&options.world_id, &legacy.record.peer_id).unwrap(), Some(legacy.clone()));
    put_accepted(&network, &signed_fixture(root, 11, Some(7), Live, "live"));
    put_rejected(&network, &legacy);
    put_accepted(&network, &signed_fixture(root, 12, Some(3), Tombstone, "withdrawn"));
    put_accepted(&network, &signed_fixture(root, 13, Some(4), Live, "first"));
    put_rejected(&network, &signed_fixture(root, 13, Some(4), Live, "fork"));
    let state = root.join("runtime/p2p/peer-record-receiver/peer-record-receiver-v2.state.cbor");
    let value: serde_cbor::Value = serde_cbor::from_slice(&fs::read(state).unwrap()).unwrap();
    let serde_cbor::Value::Map(fields) = value else {
        panic!("ledger map");
    };
    let serde_cbor::Value::Map(floors) = &fields[&serde_cbor::Value::Text("floors".into())] else {
        panic!("floors map");
    };
    assert_eq!(floors.len(), 3, "all three real signed fixtures must persist");
    for (peer, sequence, isolated) in [(11, 7, false), (12, 3, false), (13, 4, true)] {
        let id = signed_fixture(root, peer, Some(sequence), Live, "probe").record.peer_id;
        let serde_cbor::Value::Map(floor) = &floors[&serde_cbor::Value::Text(id)] else {
        panic!("floor map");
    };
        assert_eq!(floor[&serde_cbor::Value::Text("sequence".into())], serde_cbor::Value::Integer(sequence.into()));
        assert_eq!(floor[&serde_cbor::Value::Text("isolated".into())], serde_cbor::Value::Bool(isolated));
    }
}

fn assert_cold_matrix(root: &Path, network: &oasis7_node::Libp2pReplicationNetwork) {
    use PeerRecordPublicationStateV2::Live;
    let state = root.join("runtime/p2p/peer-record-receiver/peer-record-receiver-v2.state.cbor");
    let before = fs::read(&state).unwrap();
    assert_original_floors(root, &before, false);
    for record in [
        signed_fixture(root, 11, None, Live, "retired"),
        signed_fixture(root, 14, None, Live, "unseen-v1"),
        signed_fixture(root, 11, Some(6), Live, "regressed"),
        signed_fixture(root, 12, Some(2), Live, "stale-live"),
        signed_fixture(root, 13, Some(5), Live, "isolated-higher"),
    ] {
        put_rejected(network, &record);
    }
    assert_eq!(before, fs::read(&state).unwrap(), "rejections must not change original floors");
    let higher = signed_fixture(root, 11, Some(8), Live, "higher");
    put_accepted(network, &higher);
    assert_eq!(network.get_peer_record(&higher.record.world_id, &higher.record.peer_id).unwrap(), Some(higher));
    assert_original_floors(root, &fs::read(&state).unwrap(), true);
}

fn assert_original_floors(root: &Path, current: &[u8], advanced: bool) {
    let original: serde_cbor::Value = serde_cbor::from_slice(&fs::read(root.join("original.cbor")).unwrap()).unwrap();
    let current: serde_cbor::Value = serde_cbor::from_slice(current).unwrap();
    let serde_cbor::Value::Map(original) = original else {
        panic!("original map");
    };
    let serde_cbor::Value::Map(current) = current else {
        panic!("current map");
    };
    for field in ["schema_version", "binding"] {
        let field = serde_cbor::Value::Text(field.into());
        assert_eq!(original[&field], current[&field]);
    }
    let field = serde_cbor::Value::Text("floors".into());
    let serde_cbor::Value::Map(original) = &original[&field] else {
        panic!("original floors");
    };
    let serde_cbor::Value::Map(current) = &current[&field] else {
        panic!("current floors");
    };
    let live = serde_cbor::Value::Text(signed_fixture(root, 11, Some(8), PeerRecordPublicationStateV2::Live, "probe").record.peer_id);
    for (peer, floor) in original {
        if advanced && peer == &live {
            let serde_cbor::Value::Map(updated) = &current[peer] else {
        panic!("updated floor");
    };
            assert_eq!(updated[&serde_cbor::Value::Text("sequence".into())], serde_cbor::Value::Integer(8));
        } else {
            assert_eq!(*floor, current[peer], "cold process changed original signed floor");
        }
    }
}
