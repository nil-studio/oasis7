use super::super::checkpoint::load_execution_bridge_record;
use super::super::driver::{NodeRuntimeExecutionDriver, persist_execution_world};
use super::temp_dir;
use oasis7_node::{NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::{ReleaseSecurityPolicy, production_hardened_main_token_config};
use std::fs;

#[test]
fn production_release_policy_release_default_applies_hardened_policy() {
    let dir = temp_dir("execution-driver-release-policy-release-default");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::ReleaseDefault);
    let driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir,
        records_dir,
        storage_root,
        &storage_profile,
    )
    .expect("driver");

    assert_eq!(
        driver.execution_world.release_security_policy(),
        &ReleaseSecurityPolicy::production_hardened()
    );
    assert!(
        driver
            .execution_world
            .release_security_policy()
            .is_production_hardened()
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn dev_local_storage_profile_keeps_generic_supply_for_missing_execution_world() {
    let dir = temp_dir("execution-driver-release-policy-dev-local");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::DevLocal);
    let driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir,
        records_dir,
        storage_root,
        &storage_profile,
    )
    .expect("driver");

    assert_eq!(
        driver.execution_world.release_security_policy(),
        &ReleaseSecurityPolicy::default()
    );
    assert_eq!(driver.execution_world.main_token_config().initial_supply, 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn new_driver_publishes_fresh_execution_world_persistence_before_commit() {
    let dir = temp_dir("execution-driver-bootstrap-persistence");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::DevLocal);

    let driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir.clone(),
        records_dir,
        storage_root,
        &storage_profile,
    )
    .expect("driver");

    assert!(world_dir.join("snapshot.json").exists());
    assert!(world_dir.join("journal.json").exists());
    assert!(driver.simulator_world_dir.join("snapshot.json").exists());
    assert!(driver.simulator_world_dir.join("journal.json").exists());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn new_driver_repairs_partial_execution_world_persistence_files() {
    let dir = temp_dir("execution-driver-repair-partial-persistence");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::DevLocal);

    fs::create_dir_all(world_dir.as_path()).expect("create world dir");
    fs::write(world_dir.join("snapshot.json"), "{}").expect("write partial snapshot");

    let driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir.clone(),
        records_dir,
        storage_root,
        &storage_profile,
    )
    .expect("driver");

    assert!(world_dir.join("snapshot.json").exists());
    assert!(world_dir.join("journal.json").exists());
    assert!(driver.simulator_world_dir.join("snapshot.json").exists());
    assert!(driver.simulator_world_dir.join("journal.json").exists());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn dev_local_storage_profile_clears_pristine_frozen_supply_from_existing_execution_world() {
    let dir = temp_dir("execution-driver-release-policy-dev-local-existing-world");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut world = oasis7::runtime_host::world::new();
    world.set_main_token_config(production_hardened_main_token_config());
    persist_execution_world(world_dir.as_path(), &world).expect("persist release-like world");

    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::DevLocal);
    let driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir,
        records_dir,
        storage_root,
        &storage_profile,
    )
    .expect("driver");

    assert_eq!(
        driver.execution_world.release_security_policy(),
        &ReleaseSecurityPolicy::default()
    );
    assert_eq!(driver.execution_world.main_token_config().initial_supply, 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn normal_commit_keeps_world_cache_unchanged_while_advancing_cas_record() {
    let dir = temp_dir("execution-driver-normal-commit-cas-authoritative-cache");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::DevLocal);
    let mut driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir.clone(),
        records_dir.clone(),
        storage_root,
        &storage_profile,
    )
    .expect("driver");
    let snapshot_before = fs::read(world_dir.join("snapshot.json")).expect("cache snapshot");
    let journal_before = fs::read(world_dir.join("journal.json")).expect("cache journal");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");

    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect("commit");

    assert_eq!(
        fs::read(world_dir.join("snapshot.json")).expect("cache snapshot after commit"),
        snapshot_before,
        "normal commits must leave the materialized world cache untouched"
    );
    assert_eq!(
        fs::read(world_dir.join("journal.json")).expect("cache journal after commit"),
        journal_before,
        "normal commits must leave the materialized world cache untouched"
    );
    let record =
        load_execution_bridge_record(records_dir.join("00000000000000000001.json").as_path())
            .expect("CAS-backed record advances at committed height");
    assert_eq!(record.height, 1);
    assert!(
        record
            .snapshot_ref
            .as_deref()
            .is_some_and(|value| !value.is_empty())
    );
    assert!(
        record
            .journal_ref
            .as_deref()
            .is_some_and(|value| !value.is_empty())
    );

    let _ = fs::remove_dir_all(dir);
}
