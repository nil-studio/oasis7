//! Trusted setup scope for the private WorldRule gameplay fixture only.
use super::*;

pub(crate) fn pin_world_rule_resource_identity(
    driver: &mut NodeRuntimeExecutionDriver,
    identity: &WorldIdentity,
) {
    let manifest = driver.execution_world.chain_resource_manifest();
    assert_eq!(manifest.world_id, identity.world_id);
    assert_eq!(manifest.chain_id, "w1");
    assert_eq!(
        manifest.genesis_ref.as_deref(),
        Some(identity.genesis_digest.as_str())
    );
    driver.resource_identity = Some(crate::execution_bridge::driver::ExecutionResourceIdentity {
        chain_id: manifest.chain_id.clone(),
        genesis_ref: identity.genesis_digest.clone(),
    });
}

fn assert_cas_scope(dir: &std::path::Path, height: u64, identity: &WorldIdentity) {
    use oasis7_runtime::BlobStore;
    let record = crate::execution_bridge::checkpoint::load_execution_bridge_record(
        &crate::execution_bridge::checkpoint::execution_bridge_record_path(
            &dir.join("records"),
            height,
        ),
    )
    .unwrap();
    let store = oasis7_runtime::LocalCasStore::new(dir.join("store"));
    let bytes = store
        .get_verified(record.snapshot_ref.as_deref().unwrap())
        .unwrap();
    let snapshot: oasis7_runtime::Snapshot = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(snapshot.chain_resource_manifest.world_id, identity.world_id);
    assert_eq!(snapshot.chain_resource_manifest.chain_id, "w1");
    assert_eq!(
        snapshot.chain_resource_manifest.genesis_ref.as_deref(),
        Some(identity.genesis_digest.as_str())
    );
}

#[test]
fn world_rule_resource_scope_survives_commit_cas_and_restart() {
    let _permit = super::super::private_task_resource_permit();
    let _slot = super::super::prepared_world_probe_slot();
    let (dir, mut driver, identity, owner, _) = super::world_rule_fixture();
    let create = game_task_tests::create(&identity, &owner, "scope-task");
    commit(&mut driver, 3, create.clone());
    assert_cas_scope(&dir, 3, &identity);
    drop(driver);
    let bootstrap = crate::execution_bridge::local_bootstrap::derive_service_execution_bootstrap(
        &dir.join("world"),
        &dir.join("records"),
        &dir.join("store"),
        "w1",
    )
    .unwrap();
    let mut restarted = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
        &oasis7_proto::storage_profile::StorageProfileConfig::default(),
        bootstrap,
    )
    .unwrap();
    pin_world_rule_resource_identity(&mut restarted, &identity);
    commit(&mut restarted, 4, create);
    assert_cas_scope(&dir, 4, &identity);
    drop(restarted);
    fs::remove_dir_all(dir).unwrap();
}
