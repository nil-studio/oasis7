use super::checkpoint::{
    list_execution_bridge_record_heights, load_highest_valid_execution_bridge_record,
};
use super::driver::{NodeRuntimeExecutionDriver, persist_execution_bridge_state};
use super::execution_hash::ExecutionHashPayload;
use super::external_effect::execution_world_snapshot_root;
use oasis7_node::NodeExecutionBootstrap;
use oasis7_proto::storage_profile::StorageProfileConfig;
use oasis7_runtime::ReleaseSecurityPolicy;
use oasis7_runtime::blake3_hex;

/// Restore the consensus boundary from one verified service commit. Commit
/// height and gameplay tick remain independent; all CAS material is required.
pub(crate) fn derive_service_execution_bootstrap(
    world_dir: &std::path::Path,
    records_dir: &std::path::Path,
    storage_dir: &std::path::Path,
    world_id: &str,
) -> Result<NodeExecutionBootstrap, String> {
    let identity = super::world_service_read::identity(world_dir, world_id)?;
    let pinned = super::world_service_read::pin(records_dir, storage_dir, &identity, None)?;
    Ok(NodeExecutionBootstrap {
        height: pinned.record.height,
        consensus_block_hash: pinned
            .record
            .node_block_hash
            .ok_or("service bootstrap node hash unavailable")?,
        execution_block_hash: pinned.record.execution_block_hash,
        execution_state_root: pinned.record.execution_state_root,
    })
}

/// Derive the explicit local execution boundary from the persisted world.
/// The boundary carries the real world snapshot root and journal length; it
/// does not create a per-height consensus record or bypass the committed tick
/// validator. The first consensus commit must use its successor height.
pub(crate) fn derive_local_execution_bootstrap(
    world_dir: &std::path::Path,
    records_dir: &std::path::Path,
    world_id: &str,
    height: u64,
    consensus_block_hash: &str,
    release_security_policy: &ReleaseSecurityPolicy,
) -> Result<NodeExecutionBootstrap, String> {
    let execution_world = super::driver::load_execution_world_with_policy(
        world_dir,
        release_security_policy.clone(),
    )?;
    if height == 0 {
        return Err("local execution bootstrap height must be > 0".to_string());
    }
    if execution_world.state().time != height {
        return Err(format!(
            "local execution bootstrap world time must equal boundary height: world_time={} height={}",
            execution_world.state().time,
            height
        ));
    }
    if consensus_block_hash.trim().is_empty() {
        return Err("local execution bootstrap consensus block hash must not be empty".to_string());
    }
    if let Some(record) = load_highest_valid_execution_bridge_record(records_dir)? {
        if record.height != height {
            return Err(format!(
                "local execution bootstrap durable head does not match world boundary: durable_height={} world_height={}",
                record.height, height
            ));
        }
        if record.world_id != world_id {
            return Err(format!(
                "local execution bootstrap durable head world mismatch: durable_world={} expected_world={}",
                record.world_id, world_id
            ));
        }
        if record.journal_len != execution_world.journal().len() {
            return Err(format!(
                "local execution bootstrap durable head journal length mismatch: durable={} world={}",
                record.journal_len,
                execution_world.journal().len()
            ));
        }
        let consensus_block_hash = record.node_block_hash.ok_or_else(|| {
            format!(
                "local execution bootstrap durable head is missing node block hash at height {}",
                height
            )
        })?;
        if consensus_block_hash.trim().is_empty() || record.execution_block_hash.trim().is_empty() {
            return Err(format!(
                "local execution bootstrap durable head has incomplete hashes at height {}",
                height
            ));
        }
        return Ok(NodeExecutionBootstrap {
            height,
            consensus_block_hash,
            execution_block_hash: record.execution_block_hash,
            execution_state_root: record.execution_state_root,
        });
    }
    let execution_state_root = execution_world_snapshot_root(&execution_world)?;
    let hash_payload = ExecutionHashPayload {
        world_id,
        height,
        prev_execution_block_hash: "genesis",
        execution_state_root: execution_state_root.as_str(),
        journal_len: execution_world.journal().len(),
    };
    let execution_block_hash = blake3_hex(super::to_cbor(hash_payload)?.as_slice());
    Ok(NodeExecutionBootstrap {
        height,
        consensus_block_hash: consensus_block_hash.to_string(),
        execution_block_hash,
        execution_state_root,
    })
}

impl NodeRuntimeExecutionDriver {
    pub(crate) fn new_with_local_bootstrap(
        state_path: std::path::PathBuf,
        world_dir: std::path::PathBuf,
        records_dir: std::path::PathBuf,
        storage_root: std::path::PathBuf,
        storage_profile: &StorageProfileConfig,
        local_execution_bootstrap: NodeExecutionBootstrap,
    ) -> Result<Self, String> {
        Self::new_with_storage_profile_and_local_bootstrap(
            state_path,
            world_dir,
            records_dir,
            storage_root,
            storage_profile,
            Some(local_execution_bootstrap),
        )
    }

    pub(super) fn is_unmaterialized_local_execution_bootstrap(&self) -> Result<bool, String> {
        let Some(baseline) = self.local_execution_bootstrap.as_ref() else {
            return Ok(false);
        };
        Ok(self.state.last_applied_committed_height == baseline.height
            && list_execution_bridge_record_heights(self.records_dir.as_path())?.is_empty()
            && !self.records_dir.join("latest.json").exists())
    }

    /// Mutable JSON/distfs generations are caches. A same-height driver marker
    /// cannot prove that the loader selected the committed generation.
    fn restore_same_height_service_cache(
        &mut self,
        baseline: &NodeExecutionBootstrap,
    ) -> Result<(), String> {
        if self.state.last_execution_block_hash.as_deref()
            != Some(baseline.execution_block_hash.as_str())
            || self.state.last_execution_state_root.as_deref()
                != Some(baseline.execution_state_root.as_str())
            || self.state.last_node_block_hash.as_deref()
                != Some(baseline.consensus_block_hash.as_str())
        {
            return Err(
                "persisted service execution state conflicts with bootstrap boundary".into(),
            );
        }
        let binding = self
            .execution_world
            .current_cognition_runtime_binding()
            .map_err(|e| format!("{e:?}"))?;
        let identity = super::world_service_read::identity(&self.world_dir, &binding.world_id)?;
        let pinned = super::world_service_read::pin(
            &self.records_dir,
            self.execution_store.root(),
            &identity,
            None,
        )?;
        if pinned.record.height != baseline.height
            || pinned.record.node_block_hash.as_deref()
                != Some(baseline.consensus_block_hash.as_str())
            || pinned.record.execution_block_hash != baseline.execution_block_hash
            || pinned.record.execution_state_root != baseline.execution_state_root
            || pinned.world.journal().len() != pinned.record.journal_len
            || execution_world_snapshot_root(&pinned.world)? != baseline.execution_state_root
        {
            return Err("verified service CAS bootstrap boundary mismatch".into());
        }
        if self.execution_world.journal().len() == pinned.record.journal_len
            && execution_world_snapshot_root(&self.execution_world)?
                == baseline.execution_state_root
        {
            return Ok(());
        }
        let restored = pinned
            .world
            .with_release_security_policy(self.execution_world.release_security_policy().clone());
        let original = std::mem::replace(&mut self.execution_world, restored);
        if let Err(error) =
            super::driver::persist_execution_world(&self.world_dir, &self.execution_world)
        {
            self.execution_world = original;
            return Err(format!(
                "persist verified same-height service cache failed: {error}"
            ));
        }
        #[cfg(any(test, feature = "test_tier_required"))]
        eprintln!(
            "pre2_verified_same_height_service_cache_restored height={} journal_len={}",
            baseline.height,
            self.execution_world.journal().len()
        );
        Ok(())
    }

    pub(super) fn apply_local_execution_bootstrap(
        &mut self,
        baseline: &NodeExecutionBootstrap,
    ) -> Result<(), String> {
        if baseline.height == 0
            || baseline.consensus_block_hash.trim().is_empty()
            || baseline.execution_block_hash.trim().is_empty()
            || baseline.execution_state_root.trim().is_empty()
        {
            return Err("local execution bootstrap boundary is incomplete".to_string());
        }
        if self.state.last_applied_committed_height > baseline.height {
            if self.world_dir.join("world-service-identity.json").exists() {
                return Err("service bootstrap is behind the persisted execution head".into());
            }
            return Ok(());
        }
        if self.state.last_applied_committed_height == baseline.height
            && self.world_dir.join("world-service-identity.json").exists()
        {
            self.restore_same_height_service_cache(baseline)?;
        }
        let verified_service_boundary = if self.execution_world.state().time != baseline.height
            && self.world_dir.join("world-service-identity.json").exists()
        {
            let binding = self
                .execution_world
                .current_cognition_runtime_binding()
                .map_err(|error| format!("{error:?}"))?;
            let identity = super::world_service_read::identity(&self.world_dir, &binding.world_id)?;
            let pinned = super::world_service_read::pin(
                &self.records_dir,
                self.execution_store.root(),
                &identity,
                None,
            )?;
            #[cfg(any(test, feature = "test_tier_required"))]
            if std::env::var_os("PRE2_SERVICE_BOOTSTRAP_PRIVATE_TRACE").is_some() {
                eprintln!(
                    "pre2_private_bootstrap_boundary height_equal={} node_hash_equal={} execution_hash_equal={} state_root_equal={} journal_equal={} world_snapshot_equal={} world_tick={} committed_height={}",
                    pinned.record.height == baseline.height,
                    pinned.record.node_block_hash.as_deref()
                        == Some(baseline.consensus_block_hash.as_str()),
                    pinned.record.execution_block_hash == baseline.execution_block_hash,
                    pinned.record.execution_state_root == baseline.execution_state_root,
                    pinned.record.journal_len == self.execution_world.journal().len(),
                    execution_world_snapshot_root(&self.execution_world)?
                        == baseline.execution_state_root,
                    self.execution_world.state().time,
                    baseline.height
                );
                let local = serde_json::to_value(self.execution_world.snapshot())
                    .map_err(|e| e.to_string())?;
                let canonical =
                    serde_json::to_value(pinned.world.snapshot()).map_err(|e| e.to_string())?;
                let differences = local
                    .as_object()
                    .ok_or("private bootstrap snapshot shape invalid")?
                    .iter()
                    .filter(|(key, value)| canonical.get(*key) != Some(*value))
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>();
                let local_state = local
                    .get("state")
                    .and_then(serde_json::Value::as_object)
                    .ok_or("private bootstrap state missing")?;
                let canonical_state = canonical
                    .get("state")
                    .ok_or("private canonical state missing")?;
                let state_differences = local_state
                    .iter()
                    .filter(|(key, value)| canonical_state.get(*key) != Some(*value))
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>();
                eprintln!(
                    "pre2_private_bootstrap_reconstruction local_journal_len={} canonical_journal_len={} pinned_world_snapshot_equal={} differing_fields={:?} differing_state_fields={:?}",
                    self.execution_world.journal().len(),
                    pinned.world.journal().len(),
                    execution_world_snapshot_root(&pinned.world)? == baseline.execution_state_root,
                    differences,
                    state_differences
                );
            }
            pinned.record.height == baseline.height
                && pinned.record.node_block_hash.as_deref()
                    == Some(baseline.consensus_block_hash.as_str())
                && pinned.record.execution_block_hash == baseline.execution_block_hash
                && pinned.record.execution_state_root == baseline.execution_state_root
                && pinned.record.journal_len == self.execution_world.journal().len()
                && execution_world_snapshot_root(&self.execution_world)?
                    == baseline.execution_state_root
        } else {
            false
        };
        if self.execution_world.state().time != baseline.height && !verified_service_boundary {
            return Err(format!(
                "local execution bootstrap world time must equal boundary height: world_time={} height={}",
                self.execution_world.state().time,
                baseline.height
            ));
        }
        let durable_boundary_matches = load_highest_valid_execution_bridge_record(
            self.records_dir.as_path(),
        )?
        .is_some_and(|record| {
            record.height == baseline.height
                && record.execution_block_hash == baseline.execution_block_hash
                && record.execution_state_root == baseline.execution_state_root
                && record.node_block_hash.as_deref() == Some(baseline.consensus_block_hash.as_str())
                && record.journal_len == self.execution_world.journal().len()
        });
        if !durable_boundary_matches {
            let actual_state_root = execution_world_snapshot_root(&self.execution_world)?;
            if actual_state_root != baseline.execution_state_root {
                return Err(format!(
                    "local execution bootstrap snapshot root mismatch: expected={} actual={}",
                    baseline.execution_state_root, actual_state_root
                ));
            }
        }
        if self.state.last_applied_committed_height == 0 {
            super::service_bootstrap_boundary::persist_initial(self, baseline)?;
            self.state.last_applied_committed_height = baseline.height;
            self.state.last_execution_block_hash = Some(baseline.execution_block_hash.clone());
            self.state.last_execution_state_root = Some(baseline.execution_state_root.clone());
            self.state.last_node_block_hash = Some(baseline.consensus_block_hash.clone());
            persist_execution_bridge_state(self.state_path.as_path(), &self.state)?;
            return Ok(());
        }
        if self.state.last_applied_committed_height == baseline.height
            && (self.state.last_execution_block_hash.as_deref()
                != Some(baseline.execution_block_hash.as_str())
                || self.state.last_execution_state_root.as_deref()
                    != Some(baseline.execution_state_root.as_str())
                || self.state.last_node_block_hash.as_deref()
                    != Some(baseline.consensus_block_hash.as_str()))
        {
            return Err(format!(
                "persisted execution bridge state does not match local bootstrap boundary at height {}",
                baseline.height
            ));
        }
        if self.state.last_applied_committed_height < baseline.height {
            return Err(format!(
                "persisted execution bridge state is below local bootstrap boundary: state={} boundary={}",
                self.state.last_applied_committed_height, baseline.height
            ));
        }
        Ok(())
    }
}
