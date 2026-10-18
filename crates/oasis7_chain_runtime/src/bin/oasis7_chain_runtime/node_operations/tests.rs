use super::super::node_lifecycle::{
    actions, local_io as io,
    tests::{operation_fixture, operation_fixture_with_stop_signal},
};
use super::{
    executor,
    model::{Kind, Record, Request},
    planner,
    store::Store,
};
fn safe_blocker_class(blocker: Option<&str>) -> &'static str {
    let Some(blocker) = blocker else {
        return "none";
    };
    if blocker == "LOCAL_ADMISSION_REJECTED: receiver receipt absent after action" {
        return "receipt_absent_after_action";
    }
    if blocker.starts_with("LOCAL_ADMISSION_REJECTED: exact instance control unavailable:") {
        return "control_query_unavailable";
    }
    if blocker.starts_with("LOCAL_ADMISSION_REJECTED: another process retains") {
        return "owner_lock_unavailable";
    }
    if blocker.starts_with("LOCAL_ADMISSION_REJECTED: stale stop target") {
        return "stale_target";
    }
    if blocker.starts_with("LOCAL_ADMISSION_REJECTED:") {
        return "local_admission_rejected_other";
    }
    if blocker.starts_with("ORIGINAL_OUTCOME_UNKNOWN:") {
        return "original_outcome_unknown";
    }
    "other"
}

fn safe_stage(stage: &str) -> &'static str {
    match stage {
        "prepared" => "prepared",
        "stop_verified" => "stop_verified",
        "maintenance_stopped" => "maintenance_stopped",
        "local_process_stopped" => "local_process_stopped",
        _ => "other",
    }
}

#[test]
fn operation_store_real_stop_readback_cas_identity_and_reserved_resources() {
    let (_fixture, config, receiver_cleanup, current) = operation_fixture_with_stop_signal();
    let binding = actions::binding(&config).unwrap();
    let root = binding.data_dir.join("management");
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "stop-one".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::Stop,
        config,
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let mut store = Store::open(&root).unwrap();
    assert!(Store::open(&root).is_err());
    let record = store
        .admit(planner::plan(request.clone()).unwrap())
        .unwrap();
    let mut other = request.clone();
    other.operation_id = "stop-two".into();
    assert!(store.admit(planner::plan(other).unwrap()).is_err());
    assert!(executor::resume(&mut store, "stop-one", 9).is_err());
    let result = executor::resume(&mut store, "stop-one", record.revision).unwrap();
    let stop_succeeded = result.state == "succeeded";
    let phase = safe_stage(&result.stage);
    let blocker_class = safe_blocker_class(result.blocker.as_deref());
    let receiver_cleanup_ok = receiver_cleanup.finish(!stop_succeeded).is_ok();
    assert_eq!(
        result.state,
        "succeeded",
        "phase={phase} blocker_class={blocker_class} dispatched={} native_results={} receiver_cleanup={receiver_cleanup_ok}",
        result.dispatched,
        result.results.len(),
    );
    assert!(receiver_cleanup_ok, "test receiver cleanup failed");
    assert_eq!(result.results[0].target.incarnation, current.incarnation);
    drop(store);
    let mut reopened = Store::open(&root).unwrap();
    let duplicate = reopened
        .admit(planner::plan(request.clone()).unwrap())
        .unwrap();
    assert_eq!(duplicate.revision, result.revision);
    let repeated = executor::resume(&mut reopened, "stop-one", result.revision).unwrap();
    assert_eq!(repeated.results[0].action_id, result.results[0].action_id);
    let mut changed = duplicate.clone();
    changed.request.kind = Kind::Restart;
    changed.request_digest = io::digest(&changed.request).unwrap();
    assert!(reopened.admit(changed).is_err());
}
#[test]
fn operation_prepared_cancel_has_no_process_effect() {
    let (_fixture, config, handle, current) = operation_fixture();
    let binding = actions::binding(&config).unwrap();
    let root = binding.data_dir.join("management");
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "cancel-before".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::Stop,
        config,
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let mut store = Store::open(&root).unwrap();
    let record = store.admit(planner::plan(request).unwrap()).unwrap();
    let cancelled = executor::cancel(&mut store, "cancel-before", record.revision).unwrap();
    assert_eq!(cancelled.state, "cancelled");
    assert!(cancelled.results.is_empty());
    assert_eq!(
        actions::inspect(&binding.data_dir).unwrap()["instance"]["state"],
        "running"
    );
    actions::stop(&binding.data_dir, "cleanup-after-cancel", &current, 2000).unwrap();
    handle.join().unwrap().unwrap();
}

#[test]
fn operation_cli_tail_help_and_strict_options() {
    assert!(super::run(["plan", "--help"].into_iter()).is_ok());
    assert!(super::run(["resume", "--store", "--id", "x"].into_iter()).is_err());
    assert!(super::run(["validator-replace"].into_iter()).is_err());
}

#[test]
fn operation_store_opens_pre_result_v1_records_without_rewriting_their_identity() {
    let (_fixture, config, handle, current) = operation_fixture();
    let binding = actions::binding(&config).unwrap();
    let root = binding.data_dir.join("management-legacy-result");
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "legacy-result-compat".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::Stop,
        config: config.clone(),
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let record = planner::plan(request).unwrap();
    let mut legacy_record = serde_json::to_value(&record).unwrap();
    legacy_record
        .as_object_mut()
        .unwrap()
        .remove("operation_result");
    io::private_dir(&root).unwrap();
    io::write(
        &root.join("operations.json"),
        &serde_json::json!({
            "api_version": "oasis7.local-operation-store/v1",
            "records": [legacy_record],
        }),
    )
    .unwrap();
    let store = Store::open(&root).unwrap();
    let loaded = store.get("legacy-result-compat").unwrap();
    assert_eq!(loaded.request_digest, record.request_digest);
    assert!(loaded.operation_result.is_none());
    actions::stop(
        &binding.data_dir,
        "cleanup-legacy-result-compat",
        &current,
        2000,
    )
    .unwrap();
    handle.join().unwrap().unwrap();
}

#[test]
fn operation_cancel_after_dispatch_intent_queries_real_absence_before_cancel() {
    let (_fixture, config, handle, current) = operation_fixture();
    let binding = actions::binding(&config).unwrap();
    let root = binding.data_dir.join("management");
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "cancel-no-admission".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::Stop,
        config,
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let mut store = Store::open(&root).unwrap();
    let mut record = store.admit(planner::plan(request).unwrap()).unwrap();
    record.dispatched = true;
    let rev = record.revision;
    record = store.cas(record, rev).unwrap();
    let cancelled = executor::cancel(&mut store, "cancel-no-admission", record.revision).unwrap();
    assert_eq!(cancelled.state, "cancelled");
    assert_eq!(cancelled.stage, "no_native_admission_verified");
    assert_eq!(
        actions::inspect(&binding.data_dir).unwrap()["instance"]["state"],
        "running"
    );
    actions::stop(&binding.data_dir, "cleanup-no-admission", &current, 2000).unwrap();
    handle.join().unwrap().unwrap();
}

#[test]
fn operation_timeout_cancel_reconciles_original_effect_then_forward_completes() {
    let (_fixture, config, handle, current) =
        super::super::node_lifecycle::tests::operation_stubborn_fixture();
    let binding = actions::binding(&config).unwrap();
    let root = binding.data_dir.join("management");
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "stop-pending".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::Stop,
        config,
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let mut store = Store::open(&root).unwrap();
    let record = store.admit(planner::plan(request).unwrap()).unwrap();
    let pending = executor::resume(&mut store, "stop-pending", record.revision).unwrap();
    assert_eq!(pending.state, "reconciling");
    let original_action = pending.results[0].action_id.clone();
    let cancelled = executor::cancel(&mut store, "stop-pending", pending.revision).unwrap();
    assert_eq!(cancelled.state, "reconciling");
    assert!(cancelled.cancel_requested);
    let observed = actions::inspect(&binding.data_dir).unwrap();
    assert_eq!(observed["instance"]["incarnation"], current.incarnation);
    assert_eq!(observed["instance"]["state"], "draining");
    // Cleanup a known unreaped child owned by this live TEST receiver only.
    unsafe {
        libc::kill(current.runtime_pid.unwrap() as i32, libc::SIGKILL);
    }
    handle.join().unwrap().unwrap();
    let completed = executor::resume(&mut store, "stop-pending", cancelled.revision).unwrap();
    assert_eq!(completed.state, "succeeded");
    assert_eq!(completed.results[0].action_id, original_action);
    assert_eq!(completed.results[0].target.incarnation, current.incarnation);
}

#[test]
fn operation_lost_ledger_query_only_recovery_keeps_original_goal_and_old_instance() {
    let (_fixture, config, receiver_cleanup, current) = operation_fixture_with_stop_signal();
    let binding = actions::binding(&config).unwrap();
    let root = binding.data_dir.join("management");
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "recover-old-stop".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::Stop,
        config: config.clone(),
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let mut store = Store::open(&root).unwrap();
    let admitted = store
        .admit(planner::plan(request.clone()).unwrap())
        .unwrap();
    let stopped = executor::resume(&mut store, &request.operation_id, admitted.revision).unwrap();
    let stopped_ok = stopped.state == "succeeded";
    let phase = safe_stage(&stopped.stage);
    let blocker_class = safe_blocker_class(stopped.blocker.as_deref());
    let receiver_cleanup_ok = receiver_cleanup.finish(!stopped_ok).is_ok();
    assert_eq!(
        stopped.state,
        "succeeded",
        "phase={phase} blocker_class={blocker_class} dispatched={} native_results={} receiver_cleanup={receiver_cleanup_ok}",
        stopped.dispatched,
        stopped.results.len(),
    );
    assert!(receiver_cleanup_ok, "test receiver cleanup failed");
    drop(store);
    std::fs::remove_file(root.join("operations.json")).unwrap();
    let (replacement, next) =
        super::super::node_lifecycle::tests::operation_replacement_receiver(&config);
    let recovered = super::recover::reconstruct(request.clone()).unwrap();
    assert!(recovered.recovered);
    assert_eq!(recovered.state, "succeeded");
    assert_eq!(recovered.results[0].target.incarnation, current.incarnation);
    let mut new_store = Store::open(&root).unwrap();
    let recovered = new_store.admit(recovered).unwrap();
    let repeated =
        executor::resume(&mut new_store, &request.operation_id, recovered.revision).unwrap();
    assert_eq!(repeated.results[0].target.incarnation, current.incarnation);
    assert_eq!(
        actions::inspect(&binding.data_dir).unwrap()["instance"]["incarnation"],
        next.incarnation
    );
    let mut changed = request;
    changed.kind = Kind::Restart;
    assert!(super::recover::reconstruct(changed).is_err());
    actions::stop(
        &binding.data_dir,
        "cleanup-after-query-recovery",
        &next,
        2000,
    )
    .unwrap();
    replacement.join().unwrap().unwrap();
}
#[test]
fn operation_lost_ledger_missing_native_receipt_remains_unknown_without_signal() {
    use super::super::node_lifecycle::management;
    let (_fixture, config, handle, current) = operation_fixture();
    let binding = actions::binding(&config).unwrap();
    let root = binding.data_dir.join("management");
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "recover-unknown".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::Stop,
        config: config.clone(),
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let admitted = planner::plan(request.clone()).unwrap();
    let context = management::ManagementRequestBinding {
        api_version: "oasis7.local-management-request/v1".into(),
        operation_id: request.operation_id.clone(),
        step_id: "stop".into(),
        action_id: admitted.action_id("stop"),
        request_digest: admitted.request_digest.clone(),
        request_json: serde_json::to_string(&request).unwrap(),
    };
    management::bind(&config, &context).unwrap();
    let recovered = super::recover::reconstruct(request).unwrap();
    assert_eq!(recovered.state, "reconciling");
    assert!(recovered.results.is_empty());
    let mut store = Store::open(&root).unwrap();
    let recovered = store.admit(recovered).unwrap();
    let still_unknown =
        executor::resume(&mut store, &context.operation_id, recovered.revision).unwrap();
    assert_eq!(still_unknown.state, "reconciling");
    assert!(still_unknown.blocker.unwrap().contains("query only"));
    assert!(
        actions::query(&binding.data_dir, &context.action_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        actions::inspect(&binding.data_dir).unwrap()["instance"]["state"],
        "running"
    );
    let mut conflicting = context.clone();
    conflicting.request_json = conflicting
        .request_json
        .replace("recover-unknown", "different-request");
    assert!(management::bind(&config, &conflicting).is_err());
    actions::stop(
        &binding.data_dir,
        "cleanup-unknown-recovery",
        &current,
        2000,
    )
    .unwrap();
    handle.join().unwrap().unwrap();
}

#[test]
fn operation_binding_cannot_race_or_retroactively_adopt_native_admission() {
    use super::super::node_lifecycle::management;
    let (_fixture, config, handle, current) = operation_fixture();
    let binding = actions::binding(&config).unwrap();
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "binding-race".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::Stop,
        config: config.clone(),
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let planned = planner::plan(request.clone()).unwrap();
    let context = management::ManagementRequestBinding {
        api_version: "oasis7.local-management-request/v1".into(),
        operation_id: request.operation_id,
        step_id: "stop".into(),
        action_id: planned.action_id("stop"),
        request_digest: planned.request_digest,
        request_json: serde_json::to_string(&planned.request).unwrap(),
    };
    // An independent admission owner blocks the complete check-and-bind
    // transaction. There is no unlocked interval in which intent can be added.
    let guard = actions::test_admission_lock(&binding.data_dir).unwrap();
    let other_config = config.clone();
    let other_context = context.clone();
    let attempt = std::thread::spawn(move || management::bind(&other_config, &other_context));
    assert!(
        attempt
            .join()
            .unwrap()
            .unwrap_err()
            .contains("admission writer busy")
    );
    assert!(
        management::read(&config, &context.action_id)
            .unwrap()
            .is_none()
    );
    drop(guard);
    // Native admission won first; a later parent goal cannot adopt that effect.
    actions::stop(&binding.data_dir, &context.action_id, &current, 2000).unwrap();
    handle.join().unwrap().unwrap();
    assert!(
        management::bind(&config, &context)
            .unwrap_err()
            .contains("retroactive adoption refused")
    );
    assert!(
        management::read(&config, &context.action_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn operation_result_collector_keeps_status_separate_from_protocol_and_runtime_application() {
    use super::result::{OperationOutcomeV1, OperationResultV1};
    let (_fixture, config, handle, current) = operation_fixture();
    let binding = actions::binding(&config).unwrap();
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "result-collector".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::Stop,
        config: config.clone(),
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let mut store = Store::open(
        &actions::binding(&config)
            .unwrap()
            .data_dir
            .join("management"),
    )
    .unwrap();
    let admitted = store.admit(planner::plan(request).unwrap()).unwrap();
    let stopped = executor::resume(
        &mut store,
        &admitted.request.operation_id,
        admitted.revision,
    )
    .unwrap();
    handle.join().unwrap().unwrap();
    let result = stopped.operation_result.clone().unwrap();
    assert_eq!(result.outcome, OperationOutcomeV1::DeploymentSucceeded);
    assert_eq!(result.protocol, None);
    assert_eq!(result.runtime_applied_commit, None);
    assert_eq!(
        result.instance.as_ref().unwrap().incarnation,
        current.incarnation
    );
    result.validate_record_binding(&stopped).unwrap();
    let _ = OperationResultV1::collect(&stopped).unwrap();
}

#[test]
fn critical_replica_removal_is_refused_before_any_operation_is_admitted() {
    let (_fixture, config, handle, current) = operation_fixture();
    let binding = actions::binding(&config).unwrap();
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "critical-remove-one".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::RemoveCriticalReplica,
        config: config.clone(),
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let concurrent = {
        let mut second = request.clone();
        second.operation_id = "critical-remove-two".into();
        // Even the formal request shape must fail without replacement authority.
        second.expected_instance = None;
        std::thread::spawn(move || planner::plan(second))
    };
    let first = planner::plan(request).unwrap_err();
    let second = concurrent.join().unwrap().unwrap_err();
    assert!(
        first.contains("healthy durable-set replacement is an exact local formal-owner operation")
    );
    assert!(second.contains("DURABILITY_WOULD_DROP"));
    assert_eq!(
        actions::inspect(&binding.data_dir).unwrap()["instance"]["state"],
        "running"
    );
    actions::stop(&binding.data_dir, "cleanup-critical-remove", &current, 2000).unwrap();
    handle.join().unwrap().unwrap();
}

#[test]
fn data_directory_migration_preserves_identity_and_retries_only_from_receipt() {
    let (_fixture, config, handle, current) = operation_fixture();
    let binding = actions::binding(&config).unwrap();
    let stopped =
        actions::stop(&binding.data_dir, "stop-before-migration", &current, 2000).unwrap();
    handle.join().unwrap().unwrap();
    let destination = config
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrated-node-data");
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "migration-once".into(),
        world_id: binding.world_id.clone(),
        chain_id: binding.chain_id.clone(),
        kind: Kind::MigrateDataDirectory,
        config: config.clone(),
        expected_generation: stopped.target.generation,
        expected_config_digest: binding.config_digest.clone(),
        expected_instance: Some(stopped.target.clone()),
        managed_target: None,
        migration: Some(super::model::DataDirectoryMigrationV1 {
            source: binding.data_dir.clone(),
            destination: destination.clone(),
        }),
        authorization: None,
        package_upgrade: None,
    };
    let mut store = Store::open(&binding.data_dir.join("management")).unwrap();
    let admitted = store
        .admit(planner::plan(request.clone()).unwrap())
        .unwrap();
    let migrated = executor::resume(&mut store, &request.operation_id, admitted.revision).unwrap();
    assert_eq!(migrated.state, "succeeded");
    let receipt = migrated.migration_receipt.clone().unwrap();
    assert_eq!(receipt.world_id, binding.world_id);
    assert_eq!(receipt.chain_id, binding.chain_id);
    assert!(!receipt.peer_id.is_empty());
    assert!(config.exists(), "source directory remains available");
    assert_eq!(
        actions::binding(&destination.join("node.json"))
            .unwrap()
            .data_dir,
        destination.clone()
    );
    let recovered = super::recover::reconstruct(request.clone()).unwrap();
    assert_eq!(recovered.state, "succeeded");
    assert_eq!(recovered.migration_receipt, Some(receipt.clone()));
    std::fs::remove_dir_all(&destination).unwrap();
    let unknown = super::super::node_lifecycle::migration::query_data_directory_migration(
        &config,
        &destination,
        &request.operation_id,
        &migrated.request_digest,
        &stopped.target,
    )
    .unwrap_err();
    assert!(unknown.contains("ORIGINAL_OUTCOME_UNKNOWN"));
}

#[test]
fn migration_dispatch_without_source_intent_stays_query_only() {
    let (_fixture, config, handle, current) = operation_fixture();
    let binding = actions::binding(&config).unwrap();
    let stopped = actions::stop(
        &binding.data_dir,
        "stop-before-unknown-migration",
        &current,
        2000,
    )
    .unwrap();
    handle.join().unwrap().unwrap();
    let destination = config
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migration-unknown-destination");
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "migration-no-intent".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::MigrateDataDirectory,
        config: config.clone(),
        expected_generation: stopped.target.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(stopped.target),
        managed_target: None,
        migration: Some(super::model::DataDirectoryMigrationV1 {
            source: binding.data_dir.clone(),
            destination: destination.clone(),
        }),
        authorization: None,
        package_upgrade: None,
    };
    let mut admitted = planner::plan(request.clone()).unwrap();
    admitted.dispatched = true;
    let migration_action = admitted.action_id("migration");
    admitted.action_attempts.push(migration_action);
    let mut store = Store::open(&binding.data_dir.join("management")).unwrap();
    let admitted = store.admit(admitted).unwrap();
    let result = executor::resume(&mut store, &request.operation_id, admitted.revision).unwrap();
    assert_eq!(result.state, "reconciling");
    assert!(result.blocker.unwrap().contains("query only"));
    assert!(!destination.exists());
    assert!(!binding.data_dir.join("management-migrations").exists());
    assert!(super::recover::reconstruct(request).is_err());
}

#[test]
fn package_plan_rejects_non_symlink_current_before_operation_admission() {
    use super::super::node_lifecycle::package::{
        LOCAL_NATIVE_PROFILE, LocalNativePackageArtifactV1, PackageUpgradeV1,
    };
    use std::fs;

    let (_fixture, config, handle, current) = operation_fixture();
    let binding = actions::binding(&config).unwrap();
    let node_root = config.parent().unwrap().join("package-plan-preimage");
    fs::create_dir_all(node_root.join("current")).unwrap();

    let runtime_binary = fs::canonicalize("/usr/bin/true").unwrap();
    let buildinfo = config.parent().unwrap().join("package-candidate-BUILDINFO");
    fs::write(&buildinfo, "candidate package provenance\n").unwrap();
    let helper_script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/p2p-public-testnet-package-node-upgrade.sh")
        .canonicalize()
        .unwrap();
    let package_upgrade = PackageUpgradeV1 {
        api_version: "oasis7.local-package-upgrade-request/v1".into(),
        profile: LOCAL_NATIVE_PROFILE.into(),
        node_root: node_root.canonicalize().unwrap(),
        helper_script: Some(helper_script.clone()),
        helper_sha256: Some(io::sha256(&helper_script).unwrap()),
        package_deb: None,
        package_deb_sha256: None,
        ops_tools_tar: None,
        ops_tools_tar_sha256: None,
        native_artifact: Some(LocalNativePackageArtifactV1 {
            runtime_sha256: io::sha256(&runtime_binary).unwrap(),
            runtime_binary,
            buildinfo_sha256: io::sha256(&buildinfo).unwrap(),
            buildinfo,
        }),
        candidate_chain_config: None,
        package_version: "0.0.1+preimage-test".into(),
        commit: "0123456789abcdef0123456789abcdef01234567".into(),
        run_id: "preimage-test-run".into(),
        artifact_ref: Some("local-test-artifact".into()),
    };
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "package-plan-nonsymlink".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::PackageUpgrade,
        config: config.clone(),
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: Some(package_upgrade),
    };

    let result = planner::plan(request);
    let cleanup = actions::stop(
        &binding.data_dir,
        "package-plan-nonsymlink-cleanup",
        &current,
        2_000,
    );
    let receiver_result = handle.join().unwrap();

    assert!(result.unwrap_err().contains("current to be a symlink"));
    cleanup.unwrap();
    receiver_result.unwrap();
    assert!(!node_root.join("package-upgrade-operations").exists());
}

#[test]
fn operation_result_binds_verified_formal_read_to_exact_operation_and_instance() {
    use crate::formal_read_fixtures as fixtures;
    use oasis7::world_service::{
        authority,
        client::{
            FormalReadTrustConfigV1, RemoteWorldServiceClient, WorldServiceClientConfig,
            WorldServicePort,
        },
        formal_read::FormalReadHistoryV1,
        formal_read_service::{FormalCommittedReadService, FormalReadPublication},
        wire::SignedReadRequest,
    };
    use oasis7_client_api::world_service::{
        ReadWorldViewRequest, WORLD_SERVICE_CONTRACT_VERSION, WorldIdentity,
    };
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    let (world, trust, anchor, _) = fixtures::actual_world_initial_fixture();
    let origin = FormalReadPublication::new(anchor.clone(), world.clone()).unwrap();
    let service = FormalCommittedReadService::new(
        trust.clone(),
        FormalReadPublication::new(anchor.clone(), world).unwrap(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let (offset, length) = loop {
            let mut chunk = [0; 4096];
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0, "formal client closed before its signed read");
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                assert!(header.starts_with("post /v1/world/view "));
                let length = header
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break (end + 4, length);
                }
            }
        };
        let signed: SignedReadRequest<ReadWorldViewRequest> =
            serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
        authority::verify_read_request("/v1/world/view", &signed).unwrap();
        let payload = service
            .read(
                &signed,
                &origin,
                &FormalReadHistoryV1 {
                    schema_version: 1,
                    steps: vec![],
                },
            )
            .unwrap();
        let digest = authority::request_digest("/v1/world/view", &signed).unwrap();
        let reply =
            authority::sign_service_response("/v1/world/view", digest, payload, &fixtures::h(9))
                .unwrap();
        let body = serde_json::to_vec(&reply).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(&body).unwrap();
    });
    let checkpoint = anchor.checkpoint();
    let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
        formal_read_trust: Some(FormalReadTrustConfigV1 {
            initial_trust: trust.clone(),
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        }),
        endpoint,
        trusted_service_public_key: fixtures::public(9),
        expected_world: WorldIdentity {
            world_id: trust.world.world_id.clone(),
            genesis_digest: trust.world.genesis_digest.clone(),
        },
        scope_id: "public".into(),
        read_private_key_hex: fixtures::h(7),
        timeout: Duration::from_secs(5),
        max_response_bytes: 16 * 1024 * 1024,
    })
    .unwrap();
    let view = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: WorldIdentity {
                world_id: trust.world.world_id.clone(),
                genesis_digest: trust.world.genesis_digest.clone(),
            },
            scope_id: "public".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    server.join().unwrap();

    let instance = super::super::node_lifecycle::LocalInstance {
        api_version: "oasis7.local-node-instance/v1".into(),
        action_id: "op:verified-operation:start".into(),
        generation: 4,
        incarnation: "0123456789abcdef0123456789abcdef".into(),
        config_digest: "b".repeat(64),
        supervisor_pid: 77,
        runtime_pid: Some(78),
        started_unix_ms: 99,
        state: "running".into(),
        exit_code: None,
    };
    let local_result = super::super::node_lifecycle::LocalActionRecord {
        api_version: "oasis7.local-node-action/v1".into(),
        action_id: instance.action_id.clone(),
        payload_digest: io::digest(&("start", instance.config_digest.clone(), 3_u64)).unwrap(),
        kind: "start".into(),
        expected_generation: 3,
        target: instance.clone(),
        runtime_artifact: None,
    };
    let observation = super::super::node_lifecycle::operation_read::VerifiedOperationReadV1 {
        view,
        node_id: "node-a".into(),
        world_id: trust.world.world_id.clone(),
        chain_id: trust.world.chain_id.clone(),
        instance: instance.clone(),
    };
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "verified-operation".into(),
        world_id: trust.world.world_id.clone(),
        chain_id: trust.world.chain_id.clone(),
        kind: Kind::Start,
        config: std::path::PathBuf::from("/tmp/verified-node.json"),
        expected_generation: 3,
        expected_config_digest: "b".repeat(64),
        expected_instance: None,
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let request_digest = io::digest(&request).unwrap();
    let record = Record {
        request,
        request_digest,
        revision: 1,
        state: "succeeded".into(),
        stage: "local_process_started".into(),
        dispatched: true,
        cancel_requested: false,
        recovered: false,
        blocker: None,
        results: vec![local_result],
        remote_results: vec![],
        action_attempts: vec!["op:verified-operation:start".into()],
        migration_receipt: None,
        migration_capacity_plan: None,
        protected_handoff_plan: None,
        formal_handoff: None,
        healthy_durable_set_plan: None,
        healthy_durable_set_progress: None,
        operation_result: None,
        package_upgrade_progress: None,
    };
    let mut result = super::result::OperationResultV1::collect(&record).unwrap();
    result
        .attach_verified_world_view(&record, &observation)
        .unwrap();
    assert!(result.protocol.is_some());
    assert!(
        result
            .protocol
            .as_ref()
            .unwrap()
            .verified_commitment_digest
            .starts_with("blake3:")
    );
    result.validate_record_binding(&record).unwrap();

    let private_temp_root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let store_path = private_temp_root.join(format!(
        "oasis7-operation-result-{}",
        io::random_id().unwrap()
    ));
    io::private_dir(&store_path).unwrap();
    let mut store = Store::open(&store_path).unwrap();
    store.admit(record.clone()).unwrap();
    let mut record_with_observation = record.clone();
    record_with_observation.operation_result = Some(result.clone());
    let persisted = store.cas(record_with_observation, 1).unwrap();
    assert_eq!(
        persisted
            .operation_result
            .as_ref()
            .unwrap()
            .protocol
            .as_ref(),
        result.protocol.as_ref()
    );
    let reopened = Store::snapshot(&store_path, "verified-operation").unwrap();
    assert_eq!(
        reopened
            .operation_result
            .as_ref()
            .unwrap()
            .protocol
            .as_ref(),
        result.protocol.as_ref()
    );
    std::fs::remove_dir_all(&store_path).unwrap();

    let mut changed = record.clone();
    changed.request.operation_id.push_str("-other");
    assert!(result.validate_record_binding(&changed).is_err());
    let wrong_instance = super::super::node_lifecycle::LocalInstance {
        generation: instance.generation + 1,
        ..instance.clone()
    };
    let wrong_observation = super::super::node_lifecycle::operation_read::VerifiedOperationReadV1 {
        view: observation.view.clone(),
        node_id: observation.node_id.clone(),
        world_id: observation.world_id.clone(),
        chain_id: observation.chain_id.clone(),
        instance: wrong_instance,
    };
    let mut wrong_result = super::result::OperationResultV1::collect(&record).unwrap();
    assert!(
        wrong_result
            .attach_verified_world_view(&record, &wrong_observation)
            .is_err()
    );
}

#[test]
fn protected_handoff_admission_persists_exact_plan_in_shared_result() {
    use super::super::formal_live_runtime::FormalRuntimeInstanceV1;
    use super::{model::OperationAuthorizationV1, protected_handoff::ProtectedSignerHandoffPlanV1};

    let predecessor = FormalRuntimeInstanceV1 {
        api_version: "oasis7.formal-runtime-owner/v1".into(),
        operation_id: "old-runtime-op".into(),
        request_digest: "a".repeat(64),
        incarnation: "0".repeat(32),
        config_digest: "b".repeat(64),
        node_id: "node-a".into(),
        world_id: "world-a".into(),
        chain_id: "chain-a".into(),
        authority_id: "c".repeat(64),
        authority_epoch: 7,
        profile_version: 2,
        process_id: 123,
    };
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "handoff-op".into(),
        world_id: predecessor.world_id.clone(),
        chain_id: predecessor.chain_id.clone(),
        kind: Kind::ProtectedSignerMigration,
        config: std::path::PathBuf::from("/private/old/formal.json"),
        expected_generation: predecessor.authority_epoch,
        expected_config_digest: predecessor.config_digest.clone(),
        expected_instance: None,
        managed_target: None,
        migration: None,
        authorization: Some(OperationAuthorizationV1::ProtectedSignerTransfer {
            formal_live_config_file: std::path::PathBuf::from("/private/new/formal.json"),
            target_signer_public_key: "d".repeat(64),
            expected_predecessor_instance: Some(Box::new(predecessor.clone())),
        }),
        package_upgrade: None,
    };
    let request_digest = io::digest(&request).unwrap();
    let plan = ProtectedSignerHandoffPlanV1 {
        api_version: "oasis7.protected-signer-handoff-plan/v1".into(),
        request_digest: request_digest.clone(),
        predecessor_config_file: request.config.clone(),
        predecessor_config_digest: predecessor.config_digest.clone(),
        predecessor_input_bundle_digest: "e".repeat(64),
        predecessor_recovery_digest: None,
        predecessor_writer_directory: std::path::PathBuf::from("/private/old/writer"),
        predecessor_instance: predecessor,
        successor_config_file: std::path::PathBuf::from("/private/new/formal.json"),
        successor_config_digest: "f".repeat(64),
        successor_input_bundle_digest: "1".repeat(64),
        recovery_permission_digest: "2".repeat(64),
        recovery_cut_position: 10,
        successor_writer_directory: std::path::PathBuf::from("/private/new/writer"),
        target_signer_public_key: "d".repeat(64),
    };
    let record = Record {
        request,
        request_digest,
        revision: 1,
        state: "queued".into(),
        stage: "prepared".into(),
        dispatched: false,
        cancel_requested: false,
        recovered: false,
        blocker: None,
        results: vec![],
        remote_results: vec![],
        action_attempts: vec![],
        migration_receipt: None,
        migration_capacity_plan: None,
        protected_handoff_plan: Some(plan),
        formal_handoff: None,
        healthy_durable_set_plan: None,
        healthy_durable_set_progress: None,
        operation_result: None,
        package_upgrade_progress: None,
    };
    let private_temp_root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let store_path = private_temp_root.join(format!(
        "oasis7-protected-handoff-{}",
        io::random_id().unwrap()
    ));
    io::private_dir(&store_path).unwrap();
    let mut store = Store::open(&store_path).unwrap();
    let admitted = store.admit(record).unwrap();
    let envelope = admitted.operation_result.as_ref().unwrap();
    assert_eq!(envelope.target_id, "node-a");
    assert!(envelope.instance.is_none());
    assert!(envelope.formal_handoff.is_none());
    drop(store);
    let persisted = Store::snapshot(&store_path, "handoff-op").unwrap();
    assert_eq!(persisted.state, "queued");
    assert_eq!(persisted.operation_result.unwrap().target_id, "node-a");
    std::fs::remove_dir_all(&store_path).unwrap();
}
