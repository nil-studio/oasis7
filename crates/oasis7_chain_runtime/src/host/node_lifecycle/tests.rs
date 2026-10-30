#![cfg(unix)]
use super::{Arguments, local_io as io, model, process};
use sha2::Digest;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[test]
fn local_node_config_check_is_read_only_and_preserves_existing_identity() {
    fn inventory(root: &Path) -> Vec<(PathBuf, Option<String>)> {
        let mut pending = vec![root.to_path_buf()];
        let mut entries = Vec::new();
        while let Some(path) = pending.pop() {
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            let digest = if metadata.is_dir() {
                pending.extend(
                    fs::read_dir(&path)
                        .unwrap()
                        .map(|entry| entry.unwrap().path()),
                );
                None
            } else {
                Some(io::sha256(&path).unwrap())
            };
            entries.push((path.strip_prefix(root).unwrap().to_path_buf(), digest));
        }
        entries.sort();
        entries
    }
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let before = inventory(&fixture.root);
    let path = config.identity.data_dir.join(model::CONFIG_FILE);
    let result = super::check_config(&path).unwrap();
    assert_eq!(result["peer_id"], config.identity.peer_id);
    assert_eq!(result["world_id"], config.identity.chain.world_id());
    assert_eq!(result["process_started"], false);
    assert_eq!(before, inventory(&fixture.root));
    assert!(
        Arguments::parse(
            [
                "config-check",
                "--config",
                path.to_str().unwrap(),
                "--node-id",
                "replacement"
            ]
            .into_iter()
        )
        .is_err()
    );
    assert!(
        Arguments::parse(["config-check", "--help"].into_iter())
            .unwrap()
            .is_none()
    );
    assert_eq!(before, inventory(&fixture.root));
}

pub(crate) struct Fixture {
    root: PathBuf,
    chain_file: PathBuf,
    chain: model::ChainAnchor,
    retain_on_failure: bool,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let root = fs::canonicalize("/tmp")
            .unwrap()
            .join(format!("oasis7-node-test-{}", io::random_id().unwrap()));
        io::private_dir(&root).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../doc/testing/evidence")
            .canonicalize()
            .unwrap();
        let genesis: serde_json::Value =
            io::read(&source.join("public-testnet-governed-bootstrap-genesis-2026-06-06.json"))
                .unwrap();
        let registry: serde_json::Value = io::read(
            &source.join("public-testnet-governed-bootstrap-validator-registry-2026-06-06.json"),
        )
        .unwrap();
        let mut manifest: serde_json::Value =
            io::read(&source.join("public-testnet-governed-bootstrap-manifest-2026-06-06.json"))
                .unwrap();
        io::write(&root.join("genesis.json"), &genesis).unwrap();
        io::write(&root.join("registry.json"), &registry).unwrap();
        io::write(&root.join("bundle.json"), &serde_json::json!({})).unwrap();
        fs::write(
            root.join("peers.txt"),
            "/ip4/127.0.0.1/tcp/19086/p2p/12D3KooWAuNCCEDu7CdUUDwALuAhuLekZHgVWxAYp4Ag5ti79fJj\n",
        )
        .unwrap();
        manifest["runtime_refs"]["genesis_ref"] = "genesis.json".into();
        manifest["runtime_refs"]["bootstrap_peer_ref"] = "peers.txt".into();
        manifest["runtime_refs"]["release_candidate_bundle_ref"] = "bundle.json".into();
        manifest["deployment_validator_registry"] = serde_json::json!({"ref":"registry.json","sha256":io::sha256(&root.join("registry.json")).unwrap(),"semantic_sha256":super::super::runtime_authority::semantic_registry_sha256(&super::super::runtime_authority::canonicalize_registry_for_authority(super::super::governance_registry::load_genesis_finality_registry(&root.join("registry.json")).unwrap())).unwrap()});
        io::write(&root.join("manifest.json"), &manifest).unwrap();
        let chain_file = root.join("chain.json");
        io::write(
            &chain_file,
            &model::CompatibilityChainConfigV1 {
                api_version: "oasis7.chain-config/v1".into(),
                runtime_profile: model::PROFILE.into(),
                world_id: genesis["world_id"].as_str().unwrap().into(),
                chain_id: genesis["chain_id"].as_str().unwrap().into(),
                genesis_sha256: io::sha256(&root.join("genesis.json")).unwrap(),
                network_tier_manifest: "manifest.json".into(),
                manifest_sha256: io::sha256(&root.join("manifest.json")).unwrap(),
            },
        )
        .unwrap();
        let chain = model::ChainConfig::load(&chain_file).unwrap();
        Self {
            root,
            chain_file,
            chain,
            retain_on_failure: false,
        }
    }
    pub(super) fn retain_on_failure(&mut self) {
        self.retain_on_failure = true;
    }
    pub(super) fn initialize(&self) -> model::NodeConfig {
        model::initialize(
            self.chain.clone(),
            &self.root.join("node"),
            Some("independent-observer"),
            "127.0.0.1:19083",
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.retain_on_failure && std::thread::panicking() {
            eprintln!(
                "preserving failed node lifecycle fixture at {}",
                self.root.display()
            );
            return;
        }
        if let Ok(dir) = process::control_dir(&self.root.join("node")) {
            let _ = fs::remove_dir_all(dir);
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn local_node_init_retains_identity_and_never_creates_world_or_authority() {
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let key = config.identity.data_dir.join("keys/node-keypair.toml");
    let original = io::sha256(&key).unwrap();
    let retry = model::initialize(
        fixture.chain.clone(),
        &config.identity.data_dir,
        Some("independent-observer"),
        &config.rpc_bind,
    )
    .unwrap();
    assert_eq!(retry, config);
    assert_eq!(io::sha256(&key).unwrap(), original);
    assert!(!config.identity.data_dir.join("state").exists());
    assert!(!config.identity.data_dir.join("genesis.json").exists());
    assert!(
        model::initialize(
            fixture.chain.clone(),
            &config.identity.data_dir,
            Some("changed-label"),
            &config.rpc_bind
        )
        .is_err()
    );
    let mut changed = config.clone();
    if let model::ChainAnchor::Compatibility(chain) = &mut changed.identity.chain {
        chain.world_id = "another-world".into();
    }
    io::write(&config.identity.data_dir.join(model::CONFIG_FILE), &changed).unwrap();
    assert!(model::NodeConfig::load(&config.identity.data_dir.join(model::CONFIG_FILE)).is_err());
    io::write(&config.identity.data_dir.join(model::CONFIG_FILE), &config).unwrap();
    fs::remove_file(&key).unwrap();
    assert!(
        model::initialize(
            fixture.chain.clone(),
            &config.identity.data_dir,
            None,
            &config.rpc_bind
        )
        .is_err()
    );
    assert!(
        !key.exists(),
        "existing identity is never silently regenerated"
    );
}
#[test]
fn local_node_untrusted_chain_foreign_data_and_modes_fail_before_identity_effects() {
    let fixture = Fixture::new();
    let mut formal: model::CompatibilityChainConfigV1 = io::read(&fixture.chain_file).unwrap();
    formal.runtime_profile = "controlled_single_authority".into();
    io::write(&fixture.chain_file, &formal).unwrap();
    assert!(model::ChainConfig::load(&fixture.chain_file).is_err());
    assert!(!fixture.root.join("node").exists());
    let foreign = fixture.root.join("foreign");
    io::private_dir(&foreign).unwrap();
    fs::write(foreign.join("snapshot.json"), "existing-state").unwrap();
    assert!(model::initialize(fixture.chain.clone(), &foreign, None, "127.0.0.1:9080").is_err());
    assert!(!foreign.join("initialization.lock").exists());
    assert!(!foreign.join("keys").exists());
    assert!(
        model::initialize(
            fixture.chain.clone(),
            &fixture.root.join("node"),
            None,
            "0.0.0.0:9080"
        )
        .is_err()
    );
    fs::write(
        &fixture.chain.compatibility().unwrap().registry.path,
        "changed",
    )
    .unwrap();
    assert!(
        model::initialize(
            fixture.chain.clone(),
            &fixture.root.join("node"),
            None,
            "127.0.0.1:9080"
        )
        .is_err()
    );
    assert!(!fixture.root.join("node").exists());
}
#[test]
fn local_node_cli_rejects_inapplicable_flags_and_runtime_bridge_has_one_layout() {
    assert!(
        Arguments::parse(
            [
                "start",
                "--config",
                "node.json",
                "--guarded-initial-config",
                "guarded.json"
            ]
            .into_iter()
        )
        .is_err()
    );
    assert!(Arguments::parse(["init", "--chain", "--data-dir", "/tmp/node"].into_iter()).is_err());
    assert!(
        Arguments::parse(["stop", "--data-dir", "/tmp/node", "--timeout-ms", "0"].into_iter())
            .is_err()
    );
    assert!(
        Arguments::parse(["status", "--help"].into_iter())
            .unwrap()
            .is_none()
    );
    assert!(
        Arguments::parse(
            [
                "runtime-compatibility",
                "--config",
                "node.json",
                "--minimum-full-sync-cut",
                "minimum.json"
            ]
            .into_iter()
        )
        .is_err()
    );
    let fixture = Fixture::new();
    let mut config = fixture.initialize();
    config.additional_peers = vec!["peer-one".into(), "peer-two".into()];
    let arguments = config.arguments().unwrap();
    assert!(arguments.contains(&"--node-no-auto-attest-all".into()));
    assert!(!arguments.contains(&"--node-auto-attest-all".into()));
    assert!(!arguments.contains(&"--deployment-inventory".into()));
    assert_eq!(
        arguments
            .iter()
            .filter(|a| a.as_str() == "--replication-network-peer")
            .count(),
        2
    );
    for flag in [
        "--runtime-root",
        "--execution-world-dir",
        "--execution-records-dir",
        "--storage-root",
        "--replication-root",
    ] {
        let index = arguments.iter().position(|v| v == flag).unwrap();
        assert!(
            Path::new(&arguments[index + 1]).starts_with(config.identity.data_dir.join("state"))
        );
    }
}

#[test]
fn candidate_chain_anchor_is_derived_from_the_exact_captured_input_bytes() {
    let fixture = Fixture::new();
    let input_path = fixture.chain_file.clone();
    let captured_bytes = fs::read_to_string(&input_path).unwrap();
    let input = super::package::candidate_chain::CandidateChainConfigInputV1 {
        api_version: "oasis7.operator-pinned-candidate-chain-config/v1".into(),
        path: input_path.clone(),
        sha256: hex::encode(sha2::Sha256::digest(captured_bytes.as_bytes())),
    };
    let captured_anchor = model::ChainConfig::load_bytes(&input_path, captured_bytes.as_bytes())
        .expect("parse captured candidate-chain bytes");
    assert_eq!(captured_anchor, fixture.chain);

    fs::write(&input_path, b"substituted mutable path contents").unwrap();
    super::package::candidate_chain::validate_captured_anchor(
        &input,
        &captured_bytes,
        &captured_anchor,
    )
    .expect("captured bytes remain the sole source of the candidate anchor");

    let changed_bytes = format!("{captured_bytes}\n");
    assert!(
        super::package::candidate_chain::validate_captured_anchor(
            &input,
            &changed_bytes,
            &captured_anchor,
        )
        .is_err()
    );

    let mut substituted_anchor = captured_anchor.clone();
    if let model::ChainAnchor::Compatibility(anchor) = &mut substituted_anchor {
        anchor.chain_id.push_str("-substituted");
    }
    assert!(
        super::package::candidate_chain::validate_captured_anchor(
            &input,
            &captured_bytes,
            &substituted_anchor,
        )
        .is_err()
    );
}

fn receiver(
    config: &model::NodeConfig,
    command: Command,
) -> std::thread::JoinHandle<Result<(), String>> {
    let config = config.clone();
    let intent = process::test_intent(&config).unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle =
        std::thread::spawn(move || process::run_test_receiver(&config, intent, command, sender));
    receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    handle
}
pub(crate) struct TestReceiverGuard {
    signal_stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<Result<(), String>>>,
    ready: Option<std::sync::mpsc::Receiver<()>>,
}

impl TestReceiverGuard {
    fn spawn(config: &model::NodeConfig, command: Command) -> Self {
        let config = config.clone();
        let intent = process::test_intent(&config).unwrap();
        let (sender, ready) = std::sync::mpsc::channel();
        let signal_stop = Arc::new(AtomicBool::new(false));
        let thread_signal_stop = signal_stop.clone();
        let handle = std::thread::spawn(move || {
            process::run_test_receiver_with_stop_signal(
                &config,
                intent,
                command,
                sender,
                thread_signal_stop,
            )
        });
        Self {
            signal_stop,
            handle: Some(handle),
            ready: Some(ready),
        }
    }

    fn wait_ready(&mut self) {
        self.ready
            .take()
            .expect("receiver readiness can only be awaited once")
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
    }

    pub(crate) fn finish(mut self, request_stop: bool) -> Result<(), String> {
        if request_stop {
            self.signal_stop.store(true, Ordering::SeqCst);
        }
        match self.handle.take().expect("receiver handle is owned").join() {
            Ok(result) => result,
            Err(_) => Err("test receiver thread panicked".into()),
        }
    }
}

impl Drop for TestReceiverGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.signal_stop.store(true, Ordering::SeqCst);
            let _ = handle.join();
        }
    }
}

fn receiver_with_stop_signal(config: &model::NodeConfig, command: Command) -> TestReceiverGuard {
    let mut receiver = TestReceiverGuard::spawn(config, command);
    receiver.wait_ready();
    receiver
}
fn running(config: &model::NodeConfig) -> process::Instance {
    let started = Instant::now();
    loop {
        let status = process::status(&config.identity.data_dir).unwrap();
        if status["blocker"].is_null() && status["instance"]["state"] == "running" {
            return serde_json::from_value(status["instance"].clone()).unwrap();
        }
        assert!(started.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn local_node_real_child_stop_preserves_data_and_late_instance_cannot_stop_replacement() {
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let key = config.identity.data_dir.join("keys/node-keypair.toml");
    let digest = io::sha256(&key).unwrap();
    let mut command = Command::new("/bin/sleep");
    command.arg("100");
    let handle = receiver(&config, command);
    let first = running(&config);
    let status = process::stop_instance(&config, &first, 2000).unwrap();
    assert_eq!(status["instance"]["state"], "stopped");
    handle.join().unwrap().unwrap();
    assert_eq!(io::sha256(&key).unwrap(), digest);
    let mut command = Command::new("/bin/sleep");
    command.arg("100");
    let next = receiver(&config, command);
    let second = running(&config);
    assert!(second.generation > first.generation);
    assert_ne!(second.incarnation, first.incarnation);
    assert!(
        process::stop_instance(&config, &first, 2000)
            .unwrap_err()
            .contains("stale stop target")
    );
    assert_eq!(running(&config).incarnation, second.incarnation);
    process::stop_instance(&config, &second, 2000).unwrap();
    next.join().unwrap().unwrap();
    assert_eq!(io::sha256(&key).unwrap(), digest);
    assert!(config.identity.data_dir.join("actions").exists());
    assert!(status["verified_sync_readiness"].is_null());
    assert_eq!(status["formal_readiness"], false);
}
#[test]
fn local_node_stop_timeout_keeps_original_action_and_queries_same_live_instance() {
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "trap '' TERM; exec /bin/sleep 100"]);
    let handle = receiver(&config, command);
    let current = running(&config);
    // Let the test child install its ignored signal disposition before stop.
    std::thread::sleep(Duration::from_millis(100));
    let pending = process::stop_instance(&config, &current, 30).unwrap();
    assert_eq!(pending["instance"]["incarnation"], current.incarnation);
    assert_eq!(pending["instance"]["state"], "draining");
    assert!(pending["blocker"].as_str().unwrap().contains("timed out"));
    let repeated = process::stop_instance(&config, &current, 30).unwrap();
    assert_eq!(repeated["instance"]["incarnation"], current.incarnation);
    // Test cleanup only: this PID is the unreaped child of our live receiver.
    unsafe {
        libc::kill(current.runtime_pid.unwrap() as i32, libc::SIGKILL);
    }
    handle.join().unwrap().unwrap();
    assert_eq!(
        process::status(&config.identity.data_dir).unwrap()["instance"]["state"],
        "stopped"
    );
}

#[test]
fn local_node_action_identity_retries_original_receipt_without_touching_later_generation() {
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let digest =
        super::actions::config_digest(&config.identity.data_dir.join(model::CONFIG_FILE)).unwrap();
    let mut command = Command::new("/bin/sleep");
    command.arg("100");
    let handle = receiver(&config, command);
    let original = running(&config);
    let stored = super::actions::query(&config.identity.data_dir, &original.action_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.target.incarnation, original.incarnation);
    assert_eq!(stored.expected_generation, 0);
    let stopped = super::actions::stop(
        &config.identity.data_dir,
        "operator-operation:stop-step",
        &original,
        2000,
    )
    .unwrap();
    assert_eq!(stopped.target.state, "stopped");
    handle.join().unwrap().unwrap();
    let mut command = Command::new("/bin/sleep");
    command.arg("100");
    let replacement = receiver(&config, command);
    let next = running(&config);
    let retry = super::actions::start(
        &config.identity.data_dir.join(model::CONFIG_FILE),
        &original.action_id,
        0,
        &digest,
    )
    .unwrap();
    assert_eq!(retry.target.incarnation, original.incarnation);
    assert_eq!(retry.target.state, "stopped");
    let stopped_retry = super::actions::stop(
        &config.identity.data_dir,
        "operator-operation:stop-step",
        &original,
        100,
    )
    .unwrap();
    assert_eq!(stopped_retry.target.incarnation, original.incarnation);
    assert_eq!(running(&config).incarnation, next.incarnation);
    assert!(
        super::actions::start(
            &config.identity.data_dir.join(model::CONFIG_FILE),
            &original.action_id,
            1,
            &digest
        )
        .is_err()
    );
    assert!(
        super::actions::stop(
            &config.identity.data_dir,
            "operator-operation:stop-step",
            &next,
            100
        )
        .is_err()
    );
    super::actions::stop(
        &config.identity.data_dir,
        "new-operation:stop-step",
        &next,
        2000,
    )
    .unwrap();
    replacement.join().unwrap().unwrap();
}

#[test]
fn local_node_queued_intent_without_receiver_is_unknown() {
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let intent = process::test_intent(&config).unwrap();
    io::write(&config.identity.data_dir.join("start-action.json"), &intent).unwrap();
    let status = process::status(&config.identity.data_dir).unwrap();
    assert_eq!(status["instance"]["incarnation"], intent.target.incarnation);
    assert!(
        status["blocker"]
            .as_str()
            .unwrap()
            .contains("without an admitted receiver")
    );
    assert_eq!(status["formal_readiness"], false);
}

#[test]
fn package_admission_fence_is_available_while_exact_owner_holds_lifetime_lock() {
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let mut command = Command::new("/bin/sleep");
    command.arg("100");
    let handle = receiver(&config, command);
    let expected = running(&config);
    let status_before = process::status(&config.identity.data_dir).unwrap();
    let before_owner: super::LocalInstance =
        serde_json::from_value(status_before["instance"].clone()).unwrap();
    assert_eq!(before_owner, expected);

    // A live supervisor owns process.lock for its whole lifetime. Package
    // staging needs only this transient admission fence and must not try to
    // acquire or steal the lifetime lock.
    let admission = process::admission_lock(&config.identity.data_dir)
        .expect("transient admission fence is available beside the live owner");
    assert!(
        process::lock(&config.identity.data_dir).is_err(),
        "the live receiver must retain the lifetime lock"
    );
    assert!(
        process::admission_lock(&config.identity.data_dir).is_err(),
        "the transient admission fence must serialize a second writer"
    );
    let status_after = process::status(&config.identity.data_dir).unwrap();
    let after_owner: super::LocalInstance =
        serde_json::from_value(status_after["instance"].clone()).unwrap();
    drop(admission);

    let cleanup = super::actions::stop(
        &config.identity.data_dir,
        "package-admission-fence-live-owner-cleanup",
        &expected,
        2_000,
    );
    let receiver_result = handle.join().unwrap();

    assert_eq!(before_owner, expected);
    assert_eq!(after_owner, expected);
    assert_eq!(status_after["blocker"], serde_json::Value::Null);
    assert!(
        cleanup.is_ok(),
        "test must stop the original live owner: {cleanup:?}"
    );
    assert!(
        receiver_result.is_ok(),
        "test receiver failed: {receiver_result:?}"
    );
}

pub(crate) fn operation_fixture() -> (
    Fixture,
    PathBuf,
    std::thread::JoinHandle<Result<(), String>>,
    super::LocalInstance,
) {
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let mut command = Command::new("/bin/sleep");
    command.arg("100");
    let handle = receiver(&config, command);
    let current = running(&config);
    (
        fixture,
        config.identity.data_dir.join(model::CONFIG_FILE),
        handle,
        current,
    )
}

pub(crate) fn operation_fixture_with_stop_signal()
-> (Fixture, PathBuf, TestReceiverGuard, super::LocalInstance) {
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let mut command = Command::new("/bin/sleep");
    command.arg("100");
    let receiver = receiver_with_stop_signal(&config, command);
    let current = running(&config);
    (
        fixture,
        config.identity.data_dir.join(model::CONFIG_FILE),
        receiver,
        current,
    )
}

#[test]
fn receiver_guard_reaps_child_when_fixture_setup_assertion_unwinds() {
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let data_dir = config.identity.data_dir.clone();
    let setup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut command = Command::new("/bin/sleep");
        command.arg("100");
        let _receiver = TestReceiverGuard::spawn(&config, command);
        panic!("injected fixture setup assertion before readiness");
    }));
    assert!(setup.is_err(), "injected setup assertion must unwind");
    let status = process::status(&data_dir).unwrap();
    assert_eq!(status["instance"]["state"], "stopped");
    assert!(status["instance"]["runtime_pid"].is_null());
}

pub(crate) fn operation_stubborn_fixture() -> (
    Fixture,
    PathBuf,
    std::thread::JoinHandle<Result<(), String>>,
    super::LocalInstance,
) {
    let fixture = Fixture::new();
    let config = fixture.initialize();
    let mut command = Command::new("/bin/sh");
    command.args([
        "-c",
        "trap '' TERM; printf ready >\"$READY_PATH\"; exec /bin/sleep 100",
    ]);
    let ready_path = fixture.root.join("term-handler-ready");
    command.env("READY_PATH", &ready_path);
    let handle = receiver(&config, command);
    let current = running(&config);
    let deadline = Instant::now() + Duration::from_secs(2);
    while fs::read_to_string(&ready_path).ok().as_deref() != Some("ready") {
        assert!(
            Instant::now() < deadline,
            "test child TERM handler readiness timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    (
        fixture,
        config.identity.data_dir.join(model::CONFIG_FILE),
        handle,
        current,
    )
}

fn export_bound_fixture_world(destination: &Path, world_id: &str, genesis_sha: &str) {
    use oasis7_runtime::{ChainResourceDerivationContext, Journal, Snapshot};
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../doc/testing/evidence/public-testnet-governed-bootstrap-world-2026-06-06/world",
    );
    let snapshot = Snapshot::load_json(source.join("snapshot.json")).unwrap();
    let journal = Journal::load_json(source.join("journal.json")).unwrap();
    let world = oasis7::runtime_host::world::from_snapshot(snapshot, journal).unwrap();
    let genesis_ref = format!("sha256:{genesis_sha}");
    io::private_dir(destination).unwrap();
    world
        .save_to_dir_with_chain_resource_context(
            destination,
            ChainResourceDerivationContext {
                world_id,
                chain_id: world_id,
                genesis_ref: Some(&genesis_ref),
                created_at_height: 0,
                manifest_height: 0,
                commit_block_hash: None,
                tick: world.state().time,
            },
            hex::encode([0x11u8; 32]),
            hex::encode([0x22u8; 32]),
        )
        .unwrap();
}
// Test-only typed fixture generation. The production node CLI cannot create
// or rebind a world. This derives a test checkpoint from committed fixture data.
#[test]
fn local_node_export_existing_world_fixture() {
    let Some(destination) = std::env::var_os("OASIS7_NODE_EXISTING_WORLD_FIXTURE_OUT") else {
        return;
    };
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(
            "../../doc/testing/evidence/public-testnet-governed-bootstrap-genesis-2026-06-06.json",
        )
        .canonicalize()
        .unwrap();
    let genesis: serde_json::Value = io::read(&source).unwrap();
    export_bound_fixture_world(
        &PathBuf::from(destination),
        genesis["world_id"].as_str().unwrap(),
        &io::sha256(&source).unwrap(),
    );
}
fn bootstrap_fixture() -> Fixture {
    let mut fixture = Fixture::new();
    let source = fixture.root.join("existing-world");
    export_bound_fixture_world(
        &source,
        fixture.chain.world_id(),
        fixture.chain.genesis_sha256().unwrap(),
    );
    let (sha, count, bytes) = super::bootstrap::tree(&source).unwrap();
    io::write(&fixture.root.join("bundle.json"),&serde_json::json!({"generated_world_sidecar":{"kind":"directory","resolved_path":source,"sha256_tree":sha,"file_count":count,"total_bytes":bytes}})).unwrap();
    fixture.chain = model::ChainConfig::load(&fixture.chain_file).unwrap();
    fixture
}
#[test]
fn local_node_bootstrap_copies_bound_existing_world_and_preserves_original_tree() {
    let fixture = bootstrap_fixture();
    let config = fixture.initialize();
    let source = fixture.root.join("existing-world");
    let before = super::bootstrap::tree(&source).unwrap();
    let incarnation = io::random_id().unwrap();
    super::bootstrap::prepare(&config, &incarnation).unwrap();
    assert_eq!(
        super::bootstrap::tree(&config.identity.data_dir.join("state/world")).unwrap(),
        before
    );
    assert_eq!(super::bootstrap::tree(&source).unwrap(), before);
    super::bootstrap::prepare(&config, &incarnation).unwrap();
    assert_eq!(super::bootstrap::tree(&source).unwrap(), before);
}
#[test]
fn local_node_bootstrap_rejects_artifact_drift_and_partial_existing_world() {
    let fixture = bootstrap_fixture();
    let config = fixture.initialize();
    let source = fixture.root.join("existing-world");
    fs::write(source.join("foreign-file"), b"drift").unwrap();
    assert!(super::bootstrap::prepare(&config, &io::random_id().unwrap()).is_err());
    assert!(!config.identity.data_dir.join("state").exists());
    fs::remove_file(source.join("foreign-file")).unwrap();
    io::private_dir(&config.identity.data_dir.join("state")).unwrap();
    io::private_dir(&config.identity.data_dir.join("state/world")).unwrap();
    assert!(super::bootstrap::prepare(&config, &io::random_id().unwrap()).is_err());
    assert_eq!(
        fs::read_dir(config.identity.data_dir.join("state/world"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn local_node_bootstrap_rejects_wrong_checkpoint_world_and_genesis_without_creating_state() {
    for wrong_genesis in [false, true] {
        let mut fixture = Fixture::new();
        let source = fixture.root.join("existing-world");
        let world = if wrong_genesis {
            fixture.chain.world_id()
        } else {
            "foreign-world"
        };
        let genesis = if wrong_genesis {
            hex::encode([0u8; 32])
        } else {
            fixture.chain.genesis_sha256().unwrap().to_string()
        };
        export_bound_fixture_world(&source, world, &genesis);
        let (sha, count, bytes) = super::bootstrap::tree(&source).unwrap();
        io::write(&fixture.root.join("bundle.json"),&serde_json::json!({"generated_world_sidecar":{"kind":"directory","resolved_path":source,"sha256_tree":sha,"file_count":count,"total_bytes":bytes}})).unwrap();
        fixture.chain = model::ChainConfig::load(&fixture.chain_file).unwrap();
        let config = fixture.initialize();
        assert!(super::bootstrap::prepare(&config, &io::random_id().unwrap()).is_err());
        assert!(!config.identity.data_dir.join("state").exists());
    }
}

pub(crate) fn operation_replacement_receiver(
    config_path: &Path,
) -> (
    std::thread::JoinHandle<Result<(), String>>,
    super::LocalInstance,
) {
    let config = model::NodeConfig::load(config_path).unwrap();
    let mut command = Command::new("/bin/sleep");
    command.arg("100");
    let handle = receiver(&config, command);
    let current = running(&config);
    (handle, current)
}
