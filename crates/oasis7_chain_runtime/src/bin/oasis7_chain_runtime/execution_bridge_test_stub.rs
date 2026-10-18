pub(crate) use super::execution_bridge_real_tests::real_execution_bridge::controlled_live;
pub(crate) use super::execution_bridge_real_tests::real_execution_bridge::formal_package;
pub(crate) use super::execution_bridge_real_tests::real_execution_bridge::world_service_read;
#[cfg(unix)]
pub(crate) use super::execution_bridge_real_tests::real_execution_bridge::{
    formal_durable_set_arm, formal_node_hook, formal_writer,
};
pub(crate) use super::execution_bridge_real_tests::real_execution_bridge::{
    legacy_migration_execution_hash, simulator_world_dir_from_execution_world_dir,
};
use std::path::Path;

use oasis7_node::{NodeExecutionCommitContext, NodeExecutionCommitResult, NodeExecutionHook};
use oasis7_proto::storage_profile::StorageProfileConfig;
use oasis7_runtime::{ReleaseSecurityPolicy, World as RuntimeWorld};

pub(crate) use super::execution_bridge_real_tests::real_execution_bridge::ExecutionCheckpointStatusEvidence;
pub(crate) use super::execution_bridge_real_tests::real_execution_bridge::driver_observability::{
    ExecutionBridgeCommitTimingSnapshot, record_execution_bridge_module_tick_routing_metrics,
    reset_execution_bridge_commit_timing_for_tests, snapshot_execution_bridge_commit_timing,
    snapshot_execution_bridge_module_tick_routing_metrics,
};

pub(crate) fn load_latest_execution_checkpoint_status_evidence(
    execution_records_dir: &Path,
) -> Result<Option<ExecutionCheckpointStatusEvidence>, String> {
    super::execution_bridge_real_tests::real_execution_bridge::load_latest_execution_checkpoint_status_evidence(
        execution_records_dir,
    )
}

#[derive(Debug)]
pub(super) struct NodeRuntimeExecutionDriver;

#[allow(dead_code)]
impl NodeRuntimeExecutionDriver {
    pub(super) fn set_controlled_capture_enabled(&mut self, enabled: bool) -> Result<(), String> {
        if enabled {
            Err("test stub cannot produce historical capture".into())
        } else {
            Ok(())
        }
    }
    pub(super) fn new(
        _state_path: std::path::PathBuf,
        _world_dir: std::path::PathBuf,
        _records_dir: std::path::PathBuf,
        _storage_root: std::path::PathBuf,
    ) -> Result<Self, String> {
        Ok(Self)
    }

    pub(super) fn new_with_storage_profile(
        _state_path: std::path::PathBuf,
        _world_dir: std::path::PathBuf,
        _records_dir: std::path::PathBuf,
        _storage_root: std::path::PathBuf,
        _storage_profile: &StorageProfileConfig,
    ) -> Result<Self, String> {
        Ok(Self)
    }
}

impl NodeExecutionHook for NodeRuntimeExecutionDriver {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: String::new(),
            execution_state_root: String::new(),
        })
    }
}

pub(super) fn load_execution_world(world_dir: &Path) -> Result<RuntimeWorld, String> {
    let snapshot_path = world_dir.join("snapshot.json");
    let journal_path = world_dir.join("journal.json");
    if !snapshot_path.exists() || !journal_path.exists() {
        return Ok(oasis7::runtime_host::world::new_production_hardened());
    }
    oasis7::runtime_host::world::load_from_dir(world_dir)
        .map_err(|err| {
            format!(
                "load execution world from {} failed: {:?}",
                world_dir.display(),
                err
            )
        })
        .map(|world| {
            world.with_release_security_policy(ReleaseSecurityPolicy::production_hardened())
        })
}
