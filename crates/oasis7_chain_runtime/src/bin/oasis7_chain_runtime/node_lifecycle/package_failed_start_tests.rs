#![cfg(unix)]

use super::Instance;
use crate::node_lifecycle::{
    actions, bootstrap, local_io as io,
    model::{self, ChainAnchor, NodeConfig},
};
use oasis7_runtime::{ChainResourceDerivationContext, Journal, Snapshot};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(super) const SUPERVISOR_ENTRY: &str =
    "node_lifecycle::process::package_failed_start_tests::package_supervisor_child_entry";
pub(super) const RUNTIME_ENTRY: &str =
    "node_lifecycle::process::package_failed_start_tests::package_runtime_child_entry";
pub(super) const MANAGER_ENTRY: &str =
    "node_lifecycle::process::package_failed_start_tests::package_operation_manager_child_entry";
pub(super) const SUPERVISOR_CONFIG_ENV: &str = "OASIS7_TEST_PACKAGE_SUPERVISOR_CONFIG";
pub(super) const SUPERVISOR_INCARNATION_ENV: &str = "OASIS7_TEST_PACKAGE_SUPERVISOR_INCARNATION";
pub(super) const SUPERVISOR_ACTION_ENV: &str = "OASIS7_TEST_PACKAGE_SUPERVISOR_ACTION";
pub(super) const FAIL_ACTION_ENV: &str = "OASIS7_TEST_PACKAGE_FAIL_ACTION";
pub(super) const RUNTIME_CHILD_ENV: &str = "OASIS7_TEST_PACKAGE_RUNTIME_CHILD";
pub(super) const MANAGER_REQUEST_ENV: &str = "OASIS7_TEST_PACKAGE_MANAGER_REQUEST";
pub(super) const MANAGER_STORE_ENV: &str = "OASIS7_TEST_PACKAGE_MANAGER_STORE";
pub(super) const MANAGER_OPERATION_ENV: &str = "OASIS7_TEST_PACKAGE_MANAGER_OPERATION";
pub(super) const MANAGER_PRIOR_RUNTIME_ENV: &str = "OASIS7_TEST_PACKAGE_MANAGER_PRIOR_RUNTIME";
pub(super) const MANAGER_PROGRESS_ENV: &str = "OASIS7_TEST_PACKAGE_MANAGER_PROGRESS";
pub(super) const INJECTED_EXIT_CODE: i32 = 42;

const OPERATION_ID: &str = "package-failed-start-os-child";
const FAILURE_TEXT: &str = "test-only exact package startup failure";

fn mark_manager_progress(phase: &str) {
    let Some(path) = std::env::var_os(MANAGER_PROGRESS_ENV) else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
    {
        let _ = writeln!(file, "{phase}");
        let _ = file.sync_all();
    }
}

#[test]
fn package_supervisor_child_entry() {
    let (Some(config), Some(incarnation), Some(action_id)) = (
        std::env::var_os(SUPERVISOR_CONFIG_ENV),
        std::env::var_os(SUPERVISOR_INCARNATION_ENV),
        std::env::var_os(SUPERVISOR_ACTION_ENV),
    ) else {
        return;
    };
    let config = config.to_string_lossy().into_owned();
    let incarnation = incarnation.to_string_lossy().into_owned();
    let action_id = action_id.to_string_lossy().into_owned();
    let args = [
        "_run".to_string(),
        "--config".into(),
        config,
        "--incarnation".into(),
        incarnation,
    ];
    let result = crate::node_lifecycle::run(args.iter().map(String::as_str));
    if std::env::var(FAIL_ACTION_ENV).ok().as_deref() == Some(action_id.as_str()) {
        let error = result.expect_err("the exact injected package action must fail startup");
        assert!(
            error.contains(FAILURE_TEXT),
            "unexpected startup failure: {error}"
        );
    } else {
        result.expect("the selected prior runtime supervisor should remain live until stopped");
    }
}

#[test]
fn package_runtime_child_entry() {
    if std::env::var_os(RUNTIME_CHILD_ENV).is_none() {
        return;
    }
    // The production supervisor owns and terminates this actual OS child.
    // SIGTERM ends the test process, after which the normal production
    // supervisor persists the child's exact stopped receipt.
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn package_operation_manager_child_entry() {
    if std::env::var_os(MANAGER_REQUEST_ENV).is_none()
        || std::env::var_os(MANAGER_STORE_ENV).is_none()
        || std::env::var_os(MANAGER_OPERATION_ENV).is_none()
        || std::env::var_os(MANAGER_PRIOR_RUNTIME_ENV).is_none()
    {
        return;
    }
    // The shipping CLI runs this operation on the OS main thread. libtest
    // dispatches test bodies on its smaller worker stack, so move only the
    // manager child body to a stack matching the usual native main-thread
    // budget instead of changing production stack policy.
    std::thread::Builder::new()
        .name("package-operation-manager".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(package_operation_manager_child_run)
        .expect("spawn package manager test body with CLI-sized stack")
        .join()
        .expect("package manager test body did not panic");
}

fn package_operation_manager_child_run() {
    let (Some(request_path), Some(store_path), Some(operation_id), Some(prior_runtime)) = (
        std::env::var_os(MANAGER_REQUEST_ENV),
        std::env::var_os(MANAGER_STORE_ENV),
        std::env::var_os(MANAGER_OPERATION_ENV),
        std::env::var_os(MANAGER_PRIOR_RUNTIME_ENV),
    ) else {
        return;
    };
    let request_path = PathBuf::from(request_path);
    let store_path = PathBuf::from(store_path);
    let operation_id = operation_id.to_string_lossy().into_owned();
    mark_manager_progress("manager_child_entered");
    let prior_runtime = fs::canonicalize(prior_runtime).expect("canonical prior runtime");
    let loaded = std::env::current_exe()
        .expect("manager child executable")
        .canonicalize()
        .expect("canonical manager executable");
    assert_eq!(
        loaded, prior_runtime,
        "OperationRecord manager must run from the selected prior package image"
    );
    mark_manager_progress("selected_prior_image_verified");

    let request: Value = io::read(&request_path).expect("read immutable operation request");
    let config_path = PathBuf::from(request["config"].as_str().unwrap());
    let config = NodeConfig::load(&config_path).expect("load exact local node config");
    let owner_instance: Instance = serde_json::from_value(request["expected_instance"].clone())
        .expect("request carries the exact original owner");
    let data = &config.identity.data_dir;

    let submit = [
        "submit".to_string(),
        "--request".into(),
        request_path.display().to_string(),
        "--store".into(),
        store_path.display().to_string(),
    ];
    mark_manager_progress("operation_submit_started");
    crate::node_operations::run(submit.iter().map(String::as_str))
        .expect("admit immutable package operation");
    mark_manager_progress("operation_submit_finished");

    let submitted = read_operation(&store_path, &operation_id);
    let request_digest = submitted["request_digest"]
        .as_str()
        .expect("persisted immutable request digest");
    let package_request: crate::node_lifecycle::package::PackageUpgradeV1 =
        serde_json::from_value(request["package_upgrade"].clone())
            .expect("decode exact package request");
    let current_before = fs::read_link(package_request.node_root.join("current"))
        .expect("read pre-stage current release");
    let config_digest_before = io::digest(&NodeConfig::load(&config_path).unwrap()).unwrap();
    let status_before = super::status(data).expect("read owner before stage");
    let owner_before: Instance = serde_json::from_value(status_before["instance"].clone()).unwrap();
    assert_eq!(owner_before, owner_instance);

    // Exercise the real package stage while the original OS owner still holds
    // its lifetime lock. The executor below will read this exact durable
    // receipt and continue the same OperationRecord transaction idempotently.
    mark_manager_progress("active_owner_stage_started");
    let active_owner_stage = crate::node_lifecycle::package::stage(
        &config_path,
        &package_request,
        &operation_id,
        request_digest,
        &owner_instance,
    )
    .expect("selected prior image must stage without stopping the exact owner");
    mark_manager_progress("active_owner_stage_finished");
    assert_eq!(
        active_owner_stage.expected_incarnation,
        owner_instance.incarnation
    );
    assert_eq!(
        active_owner_stage.expected_generation,
        owner_instance.generation
    );
    let status_after = super::status(data).expect("read owner after stage");
    let owner_after: Instance = serde_json::from_value(status_after["instance"].clone()).unwrap();
    assert_eq!(owner_after, owner_instance);
    assert!(status_after["blocker"].is_null());
    assert_eq!(
        fs::read_link(package_request.node_root.join("current")).unwrap(),
        current_before,
        "stage must leave the selected release pointer unchanged"
    );
    assert_eq!(
        io::digest(&NodeConfig::load(&config_path).unwrap()).unwrap(),
        config_digest_before,
        "stage must leave node config unchanged"
    );

    let mut record = read_operation(&store_path, &operation_id);
    for attempt in 0..8 {
        if record["state"] == "failed"
            && record["stage"] == "package_promoted_start_failed_rolled_back"
        {
            break;
        }
        mark_manager_progress(&format!("operation_resume_{attempt}_started"));
        let revision = record["revision"].as_u64().expect("operation revision");
        let resume = [
            "resume".to_string(),
            "--store".into(),
            store_path.display().to_string(),
            "--id".into(),
            operation_id.clone(),
            "--revision".into(),
            revision.to_string(),
        ];
        crate::node_operations::run(resume.iter().map(String::as_str))
            .expect("resume the exact package transaction");
        record = read_operation(&store_path, &operation_id);
        mark_manager_progress(&format!("operation_resume_{attempt}_finished"));
    }
    assert_eq!(
        record["state"], "failed",
        "operation did not reach a terminal rollback result: {record}"
    );
    assert_eq!(record["stage"], "package_promoted_start_failed_rolled_back");

    let failed_start_id = format!("op:{operation_id}:start");
    let failed_start = actions::query(data, &failed_start_id)
        .expect("query failed candidate action")
        .expect("failed candidate action receipt");
    assert_eq!(failed_start.target.state, "failed");
    assert_eq!(failed_start.target.exit_code, Some(INJECTED_EXIT_CODE));
    assert!(failed_start.target.runtime_pid.is_none());

    let rollback_start_id = format!("op:{operation_id}:rollback-start");
    let rollback_start = actions::query(data, &rollback_start_id)
        .expect("query rollback-start action")
        .expect("rollback-start receipt");
    assert_eq!(rollback_start.target.state, "running");
    assert_eq!(
        rollback_start
            .runtime_artifact
            .as_ref()
            .unwrap()
            .runtime_path,
        prior_runtime
    );
    assert_ne!(
        rollback_start.target.incarnation,
        owner_instance.incarnation
    );

    let rollback_instance = super::read_instance(data)
        .expect("read rollback instance")
        .expect("rollback instance exists");
    assert_eq!(
        rollback_instance.incarnation,
        rollback_start.target.incarnation
    );
    assert_eq!(rollback_instance.state, "running");
    let _ = actions::stop(
        data,
        "package-failed-start-test-cleanup",
        &rollback_instance,
        10_000,
    )
    .expect("stop actual rollback runtime child");
    mark_manager_progress("rollback_runtime_cleanup_finished");
    io::write(
        &store_path.join("package-failed-start-test-result.json"),
        &json!({
            "operation": record,
            "failed_candidate_start": failed_start,
            "rollback_start": rollback_start,
            "active_owner_stage": active_owner_stage,
            "prior_runtime": prior_runtime,
            "original_owner": owner_instance
        }),
    )
    .expect("write test result summary");
}

/// Real local OperationRecord/package-manager regression. Only the runtime's
/// startup entry is faulted, and only for the exact `op:<id>:start` action.
/// The injected error follows runtime-argument validation but precedes
/// `bootstrap::prepare`, so the failed candidate does not materialize an
/// unqualified seed World between the staged descriptor and prior recheck.
/// The candidate descriptor, prior descriptor, package bytes, helper state
/// machine, stop receipt, rollback helper, and restored-runtime OS child are
/// all exercised through their normal production paths. The runtime child is
/// a test harness process, so this proves lifecycle/process restoration, not
/// application gameplay or a deployment fault domain. Before resuming the
/// OperationRecord, the selected prior image stages the exact durable request
/// while the original owner is live; the test checks that stage leaves its
/// incarnation, generation, config, and current release unchanged.
/// This is a test-image transaction proof; shipping-binary different-SHA
/// compatibility and upgrade behavior remain covered by the package script.
#[test]
fn package_failed_candidate_start_rolls_back_and_restarts_exact_prior_image() {
    let mut fixture = PackageFixture::new();
    let mut config = fixture.fixture.initialize();
    fixture.bind_runtime_bundle(&mut config);
    let config_path = config.identity.data_dir.join(model::CONFIG_FILE);
    let original_config_digest = io::digest(&config).unwrap();

    let (ready_tx, ready_rx) = mpsc::channel();
    let owner_config = config.clone();
    let owner_intent = super::test_intent(&config).expect("create owner start identity");
    let owner_thread = std::thread::spawn(move || {
        let command = Command::new("/bin/sleep");
        let mut command = command;
        command.arg("120");
        super::run_test_receiver(&owner_config, owner_intent, command, ready_tx)
    });
    let mut owner_guard = ReceiverGuard {
        data_dir: config.identity.data_dir.clone(),
        handle: Some(owner_thread),
    };
    ready_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("receiver admission");
    let original_owner = wait_running_instance(&config.identity.data_dir);

    let package = fixture.install_package_artifacts();
    let binding_path = config.identity.data_dir.join("node-binding.json");
    let original_binding_digest = io::sha256(&binding_path).unwrap();
    let operation_request = json!({
        "api_version": "oasis7.local-operation/v1",
        "operation_id": OPERATION_ID,
        "world_id": config.identity.chain.world_id(),
        "chain_id": config.identity.chain.chain_id(),
        "kind": "package_upgrade",
        "config": config_path,
        "expected_generation": original_owner.generation,
        "expected_config_digest": original_config_digest,
        "expected_instance": original_owner,
        "package_upgrade": package,
    });
    let request_path = fixture.root().join("operation-request.json");
    let store_path = config
        .identity
        .data_dir
        .join("management-package-failed-start");
    io::write(&request_path, &operation_request).expect("write exact OperationRecord request");

    let manager_log_path = fixture.root().join("package-operation-manager.log");
    let manager_progress_path = fixture
        .root()
        .join("package-operation-manager-progress.log");
    let manager_log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&manager_log_path)
        .expect("create bounded test manager log");
    let mut manager = Command::new(&fixture.prior_runtime);
    manager
        .args(["--exact", MANAGER_ENTRY, "--nocapture"])
        .env(MANAGER_REQUEST_ENV, &request_path)
        .env(MANAGER_STORE_ENV, &store_path)
        .env(MANAGER_OPERATION_ENV, OPERATION_ID)
        .env(MANAGER_PRIOR_RUNTIME_ENV, &fixture.prior_runtime)
        .env(MANAGER_PROGRESS_ENV, &manager_progress_path)
        .env(FAIL_ACTION_ENV, format!("op:{OPERATION_ID}:start"))
        .stdout(Stdio::from(manager_log.try_clone().unwrap()))
        .stderr(Stdio::from(manager_log));
    manager.process_group(0);
    let mut manager = manager.spawn().expect("spawn prior-image manager child");
    let manager_status = wait_child(
        &mut manager,
        Duration::from_secs(90),
        &manager_log_path,
        &manager_progress_path,
    );
    assert!(
        manager_status.success(),
        "OperationRecord package child failed: {}\nprogress:\n{}",
        fs::read_to_string(&manager_log_path).unwrap_or_default(),
        fs::read_to_string(&manager_progress_path).unwrap_or_default()
    );

    let result: Value = io::read(&store_path.join("package-failed-start-test-result.json"))
        .expect("manager child wrote terminal result");
    assert_eq!(
        result["operation"]["stage"],
        "package_promoted_start_failed_rolled_back"
    );
    assert_eq!(
        result["failed_candidate_start"]["target"]["exit_code"],
        INJECTED_EXIT_CODE
    );
    assert_eq!(
        result["active_owner_stage"]["expected_incarnation"],
        original_owner.incarnation
    );
    assert_eq!(
        result["active_owner_stage"]["expected_generation"],
        original_owner.generation
    );
    assert_eq!(result["rollback_start"]["target"]["state"], "running");
    assert_eq!(
        serde_json::from_value::<PathBuf>(
            result["rollback_start"]["runtime_artifact"]["runtime_path"].clone()
        )
        .unwrap(),
        fixture.prior_runtime
    );

    let current_target = fs::read_link(fixture.node_root.join("current")).unwrap();
    assert_eq!(current_target, fixture.prior_release);
    let restored_config = NodeConfig::load(&config_path).unwrap();
    assert_eq!(
        io::digest(&restored_config).unwrap(),
        original_config_digest
    );
    assert_eq!(restored_config.identity, config.identity);
    assert_eq!(
        io::sha256(&binding_path).unwrap(),
        original_binding_digest,
        "rollback must restore the exact node binding bytes"
    );
    let prior_buildinfo = fs::read_to_string(fixture.prior_release.join("BUILDINFO")).unwrap();
    assert_eq!(
        fs::read_to_string(fixture.node_root.join("DEPLOYED_BUILDINFO")).unwrap(),
        prior_buildinfo
    );
    let world_path = config.identity.data_dir.join("state/world");
    assert_eq!(bootstrap::tree(&world_path).unwrap(), fixture.world_tree);

    let current = super::read_instance(&config.identity.data_dir)
        .unwrap()
        .expect("restored runtime instance");
    assert_eq!(
        current.state, "stopped",
        "manager cleans up its verified child before returning"
    );
    owner_guard
        .join()
        .expect("join exact original owner")
        .expect("original receiver result");
}

struct PackageFixture {
    fixture: crate::node_lifecycle::tests::Fixture,
    root: PathBuf,
    node_root: PathBuf,
    prior_release: PathBuf,
    prior_runtime: PathBuf,
    test_runtime_image: PathBuf,
    candidate_runtime: PathBuf,
    candidate_buildinfo: PathBuf,
    image_sha256: String,
    world_tree: (String, u64, u64),
}

impl PackageFixture {
    fn new() -> Self {
        let mut fixture = crate::node_lifecycle::tests::Fixture::new();
        fixture.retain_on_failure();
        let root = fixture
            .initialize()
            .identity
            .data_dir
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap();
        let source_image = fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
        let test_runtime_image = root.join("package-test-runtime-image");
        let image_sha256 = prepare_private_test_runtime_image(&source_image, &test_runtime_image);
        Self {
            fixture,
            root: root.clone(),
            node_root: root.join("package-root"),
            prior_release: root.join("package-root/releases/prior-v1"),
            prior_runtime: root.join("package-root/releases/prior-v1/bin/oasis7_chain_runtime"),
            test_runtime_image,
            candidate_runtime: root.join("candidate-runtime"),
            candidate_buildinfo: root.join("candidate-BUILDINFO"),
            image_sha256,
            world_tree: (String::new(), 0, 0),
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn bind_runtime_bundle(&mut self, config: &mut NodeConfig) {
        let compatibility = config.identity.chain.compatibility().unwrap().clone();
        let sidecar = self.root.join("existing-world");
        fs::create_dir_all(&sidecar).unwrap();
        let world_source = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../doc/testing/evidence/public-testnet-governed-bootstrap-world-2026-06-06/world",
        );
        let world = oasis7::runtime_host::world::from_snapshot(
            Snapshot::load_json(world_source.join("snapshot.json")).unwrap(),
            Journal::load_json(world_source.join("journal.json")).unwrap(),
        )
        .unwrap();
        let genesis_ref = format!("sha256:{}", compatibility.genesis.sha256);
        world
            .save_to_dir_with_chain_resource_context(
                &sidecar,
                ChainResourceDerivationContext {
                    world_id: &compatibility.world_id,
                    chain_id: &compatibility.world_id,
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
        let world_tree = bootstrap::tree(&sidecar).unwrap();
        let provenance = self.root.join("world-generation-provenance.json");
        fs::write(
            &provenance,
            br#"{"scenario_id":"package-failed-start-test"}"#,
        )
        .unwrap();
        let bundle = json!({
            "runtime_build": {"sha256": self.image_sha256},
            "generated_world_sidecar": {
                "kind": "directory",
                "resolved_path": sidecar,
                "sha256_tree": world_tree.0,
                "file_count": world_tree.1,
                "total_bytes": world_tree.2
            },
            "world_generation_provenance": {
                "kind": "file",
                "resolved_path": provenance,
                "sha256": io::sha256(&self.root.join("world-generation-provenance.json")).unwrap()
            }
        });
        io::write(&compatibility.release_bundle.path, &bundle).unwrap();
        if let ChainAnchor::Compatibility(anchor) = &mut config.identity.chain {
            anchor.release_bundle.sha256 = io::sha256(&anchor.release_bundle.path).unwrap();
        } else {
            panic!("package fixture requires compatibility chain anchor");
        }
        io::write(&config.identity.data_dir.join(model::CONFIG_FILE), config).unwrap();
        io::write(
            &config.identity.data_dir.join("node-binding.json"),
            &config.identity,
        )
        .unwrap();
        config.identity.chain.verify().unwrap();
        // Reuse the actual selected sidecar for the post-rollback equality
        // assertion; no test-only world bytes are installed by the manager.
        self.world_tree = world_tree;
    }

    fn install_package_artifacts(&self) -> Value {
        fs::create_dir_all(self.prior_release.join("bin")).unwrap();
        // Strip a private copy once, then give the prior and candidate runtime
        // paths hard links to that immutable test image. The production
        // verifier still hashes and opens both selected paths normally.
        let image_metadata = fs::symlink_metadata(&self.test_runtime_image).unwrap();
        assert!(image_metadata.is_file());
        assert_ne!(
            image_metadata.mode() & 0o111,
            0,
            "test image must be executable"
        );
        fs::hard_link(&self.test_runtime_image, &self.prior_runtime)
            .expect("link prior fixture to the immutable test image");
        let prior_metadata = fs::metadata(&self.prior_runtime).unwrap();
        assert_eq!(prior_metadata.dev(), image_metadata.dev());
        assert_eq!(prior_metadata.ino(), image_metadata.ino());
        assert_eq!(prior_metadata.mode() & 0o777, image_metadata.mode() & 0o777);
        fs::create_dir_all(&self.node_root).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&self.prior_release, self.node_root.join("current")).unwrap();
        let prior_sha = &self.image_sha256;
        let prior_buildinfo = format!(
            "workflow=Local Native Package\ncommit=prior-commit\npackage_version=prior-v1\nrun_id=prior-run\nplatform=local-native\nruntime_sha256={prior_sha}\n"
        );
        fs::write(self.prior_release.join("BUILDINFO"), &prior_buildinfo).unwrap();
        fs::write(self.node_root.join("CURRENT_VERSION"), "prior-v1\n").unwrap();
        fs::write(self.node_root.join("DEPLOYED_BUILDINFO"), &prior_buildinfo).unwrap();
        fs::hard_link(&self.test_runtime_image, &self.candidate_runtime)
            .expect("link candidate fixture to the immutable test image");
        let candidate_metadata = fs::metadata(&self.candidate_runtime).unwrap();
        assert_eq!(candidate_metadata.dev(), image_metadata.dev());
        assert_eq!(candidate_metadata.ino(), image_metadata.ino());
        assert_eq!(
            candidate_metadata.mode() & 0o777,
            image_metadata.mode() & 0o777
        );
        let candidate_sha = &self.image_sha256;
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let package_version = "candidate-v2";
        let run_id = "package-failed-start-run";
        fs::write(
            &self.candidate_buildinfo,
            format!(
                "workflow=Local Native Package\ncommit={commit}\npackage_version={package_version}\nrun_id={run_id}\nplatform=local-native\nruntime_sha256={candidate_sha}\n"
            ),
        )
        .unwrap();
        let production_helper = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/p2p-public-testnet-package-node-upgrade.sh")
            .canonicalize()
            .unwrap();
        let descriptor_runner = self.root.join("package-test-descriptor-runner.py");
        write_private_executable(
            &descriptor_runner,
            include_str!("package_test_descriptor_runner.py"),
        );
        let helper = self.root.join("package-test-upgrade-helper.sh");
        let production_source = fs::read_to_string(&production_helper).unwrap();
        let command_prefix = "command = [\n        str(binary),";
        assert_eq!(
            production_source.matches(command_prefix).count(),
            1,
            "test helper adaptation must target exactly the descriptor invocation"
        );
        let descriptor_runner_literal =
            serde_json::to_string(&descriptor_runner.to_string_lossy()).unwrap();
        let adapted_source = production_source.replacen(
            command_prefix,
            &format!("command = [\n        {descriptor_runner_literal},\n        str(binary),"),
            1,
        );
        // The copied helper changes only how libtest is asked to run the
        // real compatibility decoder. The production helper/CLI remains
        // untouched; the child still reports its actual loaded image and
        // decoded World/CAS state to the helper's normal validators.
        write_private_executable(&helper, &adapted_source);
        json!({
            "api_version": "oasis7.local-package-upgrade-request/v1",
            "profile": "local_native_package_v1",
            "node_root": self.node_root,
            "helper_script": helper,
            "helper_sha256": io::sha256(&helper).unwrap(),
            "package_deb": null,
            "package_deb_sha256": null,
            "ops_tools_tar": null,
            "ops_tools_tar_sha256": null,
            "native_artifact": {
                "runtime_binary": self.candidate_runtime,
                "runtime_sha256": candidate_sha,
                "buildinfo": self.candidate_buildinfo,
                "buildinfo_sha256": io::sha256(&self.candidate_buildinfo).unwrap()
            },
            "candidate_chain_config": null,
            "package_version": package_version,
            "commit": commit,
            "run_id": run_id,
            "artifact_ref": "test-harness-runtime-image"
        })
    }
}

fn prepare_private_test_runtime_image(source: &Path, destination: &Path) -> String {
    let source_size = fs::metadata(source)
        .expect("inspect original test image")
        .len();
    fs::copy(source, destination).expect("copy test runtime into its private package fixture");

    let strip_program = match std::env::consts::OS {
        "macos" => Path::new("/usr/bin/strip"),
        "linux" => Path::new("strip"),
        other => panic!("package fixture does not support private image stripping on {other}"),
    };
    let strip = Command::new(strip_program)
        .arg(destination)
        .output()
        .expect("run platform strip on the private test image");
    assert!(
        strip.status.success(),
        "strip failed for private test image: {}",
        String::from_utf8_lossy(&strip.stderr)
    );

    let metadata = fs::symlink_metadata(destination).expect("inspect stripped test image");
    assert!(
        metadata.file_type().is_file(),
        "stripped test image is not a regular file"
    );
    assert!(
        metadata.len() <= source_size,
        "platform strip increased the private test image size"
    );
    assert_ne!(
        metadata.mode() & 0o111,
        0,
        "stripped test image lost executable permissions"
    );
    let listing = Command::new(destination)
        .arg("--list")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("run the stripped test image list smoke check");
    assert!(
        listing.status.success(),
        "stripped test image cannot run its test harness: {}",
        String::from_utf8_lossy(&listing.stderr)
    );

    // CI may already supply a fully stripped input; retain its exact bytes.
    eprintln!(
        "package-test-image source_bytes={source_size} stripped_bytes={}",
        metadata.len()
    );
    io::sha256(destination).expect("hash the immutable stripped test image")
}

fn write_private_executable(path: &Path, contents: &str) {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o700);
    let mut file = options
        .open(path)
        .expect("create private executable fixture");
    file.write_all(contents.as_bytes())
        .expect("write private executable fixture");
    file.sync_all().expect("sync private executable fixture");
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .expect("set private executable fixture mode");
}

fn wait_running_instance(data_dir: &Path) -> Instance {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(Some(instance)) = super::read_instance(data_dir)
            && instance.state == "running"
            && instance.runtime_pid.is_some()
        {
            return instance;
        }
        assert!(
            Instant::now() < deadline,
            "local receiver did not become running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct ReceiverGuard {
    data_dir: PathBuf,
    handle: Option<JoinHandle<Result<(), String>>>,
}
impl ReceiverGuard {
    fn join(&mut self) -> Result<Result<(), String>, String> {
        self.handle
            .take()
            .expect("receiver thread exists")
            .join()
            .map_err(|_| "original receiver thread panicked".into())
    }
}
impl Drop for ReceiverGuard {
    fn drop(&mut self) {
        if let Ok(Some(instance)) = super::read_instance(&self.data_dir)
            && !matches!(instance.state.as_str(), "stopped" | "failed")
        {
            let _ = actions::stop(
                &self.data_dir,
                "package-failed-start-test-original-owner-cleanup",
                &instance,
                10_000,
            );
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_operation(store_path: &Path, operation_id: &str) -> Value {
    let store: Value = io::read(&store_path.join("operations.json")).expect("read operation store");
    store["records"]
        .as_array()
        .and_then(|records| {
            records
                .iter()
                .find(|record| record["request"]["operation_id"] == operation_id)
        })
        .cloned()
        .expect("operation record persisted")
}

fn wait_child(
    child: &mut Child,
    timeout: Duration,
    manager_log: &Path,
    manager_progress: &Path,
) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("manager child wait") {
            return status;
        }
        if Instant::now() >= deadline {
            let process_group = -(child.id() as libc::pid_t);
            unsafe {
                libc::kill(process_group, libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            let progress = fs::read_to_string(manager_progress)
                .unwrap_or_else(|error| format!("<unavailable: {error}>"));
            let log = fs::read_to_string(manager_log)
                .unwrap_or_else(|error| format!("<unavailable: {error}>"));
            panic!(
                "package OperationRecord child exceeded bounded test timeout; progress:\n{progress}\nmanager log:\n{log}"
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
