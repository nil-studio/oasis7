//! OS-process coverage of the real CLI dispatcher and development network.
//! This is not a test of an installed release executable or public networking.
use super::*;
use oasis7::development_node_config::{
    DevelopmentPeerRecordPublicationModeV1, DevelopmentPeerRecordReceptionModeV1,
};
use oasis7_proto::distributed_dht::DistributedDht;
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
    let state = root.join("runtime/p2p/peer-record-receiver/peer-record-receiver-v2.state.cbor");
    let original = fs::read(&state).unwrap();
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
    assert_eq!(original_files, receiver_files(receiver_directory));
    assert_eq!(original_config, fs::read(root.join("config.json")).unwrap());
    spawn(&root, "reject-locked").wait();
    assert!(root.join("done-reject-locked").exists());
    assert_eq!(original_files, receiver_files(receiver_directory));
    assert_eq!(original_config, fs::read(root.join("config.json")).unwrap());
    assert_eq!(
        original,
        fs::read(&state).unwrap(),
        "rejected upgrade changed state"
    );
    fs::write(root.join("release"), b"release").unwrap();
    holder.wait();
    spawn(&root, "upgrade").wait();
    assert!(root.join("done-upgrade").exists());
    let upgraded = fs::read(&state).unwrap();
    assert_ne!(original, upgraded);
    let mut expected_files = original_files.clone();
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
