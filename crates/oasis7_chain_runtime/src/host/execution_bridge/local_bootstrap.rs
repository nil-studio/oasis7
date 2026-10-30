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
    derive_service_execution_bootstrap_with_context(
        world_dir,
        records_dir,
        storage_dir,
        world_id,
        &super::world_service_read::RestoreContext::default(),
    )
}

/// Trusted running-host context must be supplied before committed CAS restoration.
pub(crate) fn derive_service_execution_bootstrap_with_context(
    world_dir: &std::path::Path,
    records_dir: &std::path::Path,
    storage_dir: &std::path::Path,
    world_id: &str,
    context: &super::world_service_read::RestoreContext,
) -> Result<NodeExecutionBootstrap, String> {
    let identity = super::world_service_read::identity(world_dir, world_id)?;
    let pinned = super::world_service_read::pin_with_context(
        records_dir,
        storage_dir,
        &identity,
        None,
        context,
    )?;
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
    /// Service recovery cannot silently skip an invalid higher numbered head.
    /// The generic legacy highest-valid reader deliberately remains unchanged.
    fn strict_service_bootstrap_head(
        &self,
        world_id: &str,
    ) -> Result<Option<super::ExecutionBridgeRecord>, String> {
        let mut highest = None;
        match std::fs::read_dir(&self.records_dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|_| "service record inventory unavailable")?;
                    let name = entry.file_name();
                    let Some(name) = name.to_str() else {
                        continue;
                    };
                    let Some(stem) = name.strip_suffix(".json") else {
                        continue;
                    };
                    if stem.is_empty() || !stem.bytes().all(|byte| byte.is_ascii_digit()) {
                        continue;
                    }
                    if stem.len() != 20
                        || !entry
                            .file_type()
                            .map_err(|_| "service record carrier unavailable")?
                            .is_file()
                    {
                        return Err("service numeric record carrier or filename invalid".into());
                    }
                    let height = stem
                        .parse::<u64>()
                        .map_err(|_| "service numeric record height invalid")?;
                    if height == 0 {
                        return Err("service numeric record height invalid".into());
                    }
                    highest = Some(highest.map_or(height, |previous: u64| previous.max(height)));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("service record inventory unavailable".into()),
        }
        let latest_bytes = super::historical_material::read_regular(
            &self.records_dir.join("latest.json"),
            1024 * 1024,
        )?;
        let Some(height) = highest else {
            if latest_bytes.is_some() {
                return Err("service latest pointer lacks its numbered record".into());
            }
            return Ok(None);
        };
        let record = super::historical_material::load_record(&self.records_dir, height)?
            .ok_or("service highest numeric record unavailable")?;
        if record.height != height || record.world_id != world_id {
            return Err("service highest numeric record identity invalid".into());
        }
        if let Some(bytes) = latest_bytes {
            let pointer: super::ExecutionBridgeRecord =
                serde_json::from_slice(&bytes).map_err(|_| "service latest pointer invalid")?;
            if pointer.height > height || pointer.height == 0 || pointer.world_id != world_id {
                return Err("service latest pointer exceeds or conflicts with numeric head".into());
            }
            let retained =
                super::historical_material::load_record(&self.records_dir, pointer.height)?
                    .ok_or("service latest pointer retained record unavailable")?;
            // A lagging cache may predate ref compaction. Its immutable commit
            // identity must still match; a same-height pointer must be exact.
            let same_identity = pointer.height == retained.height
                && pointer.world_id == retained.world_id
                && pointer.node_block_hash == retained.node_block_hash
                && pointer.prev_node_block_hash == retained.prev_node_block_hash
                && pointer.proposer_id == retained.proposer_id
                && pointer.action_root == retained.action_root
                && pointer.execution_block_hash == retained.execution_block_hash
                && pointer.execution_state_root == retained.execution_state_root
                && pointer.journal_len == retained.journal_len
                && pointer.timestamp_ms == retained.timestamp_ms;
            if !same_identity || (pointer.height == height && pointer != retained) {
                return Err(
                    "service latest pointer conflicts with retained original record".into(),
                );
            }
        }
        Ok(Some(record))
    }

    /// A genuine durable service successor can precede the mutable World/state
    /// cache publication. Validate the old marker as a retained prefix, then
    /// restore the already qualified original CAS before applying time guards.
    /// This is not permission to overwrite a foreign/future cache marker.
    pub(super) fn recover_committed_service_successor_before_bootstrap(
        &mut self,
        baseline: &NodeExecutionBootstrap,
    ) -> Result<(), String> {
        if !self.world_dir.join("world-service-identity.json").exists() {
            if self
                .records_dir
                .join("local-execution-bootstrap.json")
                .exists()
            {
                return Err("service bootstrap identity is missing".into());
            }
            // Preserve the existing non-service local bootstrap contract.
            return Ok(());
        }
        let binding = self
            .execution_world
            .current_cognition_runtime_binding()
            .map_err(|_| "service cached Runtime binding unavailable")?;
        let identity = super::world_service_read::identity(&self.world_dir, &binding.world_id)?;
        let Some(latest) = self.strict_service_bootstrap_head(&identity.world_id)? else {
            return Ok(());
        };
        if latest.height != baseline.height
            || latest.execution_block_hash != baseline.execution_block_hash
            || latest.execution_state_root != baseline.execution_state_root
            || latest.node_block_hash.as_deref() != Some(baseline.consensus_block_hash.as_str())
        {
            return Err("service canonical record conflicts with exact bootstrap boundary".into());
        }
        let cached_height = self.state.last_applied_committed_height;
        if cached_height > baseline.height {
            return Err(
                "service cached execution marker exceeds canonical bootstrap boundary".into(),
            );
        }
        if cached_height == baseline.height {
            // The existing same-height path validates marker hashes and CAS
            // before repairing an independently selected cache generation.
            return Ok(());
        }
        if latest.world_id != identity.world_id {
            return Err("service canonical record world differs from cached binding".into());
        }
        if cached_height == 0 {
            if self.state.last_execution_block_hash.is_some()
                || self.state.last_execution_state_root.is_some()
                || self.state.last_node_block_hash.is_some()
            {
                return Err("empty service cache marker contains conflicting identity".into());
            }
        } else {
            let prefix = super::checkpoint::load_execution_bridge_record(
                &super::checkpoint::execution_bridge_record_path(&self.records_dir, cached_height),
            )?;
            if prefix.height != cached_height
                || prefix.world_id != identity.world_id
                || self.state.last_execution_block_hash.as_deref()
                    != Some(prefix.execution_block_hash.as_str())
                || self.state.last_execution_state_root.as_deref()
                    != Some(prefix.execution_state_root.as_str())
                || self.state.last_node_block_hash != prefix.node_block_hash
            {
                return Err(
                    "service cached execution marker is not the retained original prefix".into(),
                );
            }
            let mut previous_execution = prefix.execution_block_hash;
            let mut previous_node = prefix.node_block_hash;
            for height in cached_height
                .checked_add(1)
                .ok_or("service prefix height overflow")?
                ..=baseline.height
            {
                let record = super::checkpoint::load_execution_bridge_record(
                    &super::checkpoint::execution_bridge_record_path(&self.records_dir, height),
                )?;
                if record.world_id != identity.world_id
                    || record.height != height
                    || record.prev_node_block_hash != previous_node
                {
                    return Err("service successor is not the retained cached prefix chain".into());
                }
                let expected = blake3_hex(&super::to_cbor(ExecutionHashPayload {
                    world_id: &identity.world_id,
                    height,
                    prev_execution_block_hash: &previous_execution,
                    execution_state_root: &record.execution_state_root,
                    journal_len: record.journal_len,
                })?);
                if expected != record.execution_block_hash {
                    return Err(
                        "service successor execution chain conflicts with cached prefix".into(),
                    );
                }
                previous_execution = record.execution_block_hash;
                previous_node = record.node_block_hash;
            }
        }
        // Both admission and actual restoration use the original Host context;
        // snapshot fields never select policy or a local participant authority.
        let context = super::world_service_read::RestoreContext::new(
            self.execution_world.release_security_policy().clone(),
            self.devlocal_participant.authority.clone(),
        )?;
        let pinned = super::world_service_read::pin_at_height_with_context(
            &self.records_dir,
            self.execution_store.root(),
            &identity,
            baseline.height,
            &context,
        )?;
        if pinned.record != latest
            || pinned.commit.position != baseline.height
            || pinned.commit.execution_block_hash != baseline.execution_block_hash
            || pinned.commit.state_root_ref != baseline.execution_state_root
        {
            return Err("verified service cut differs from exact bootstrap before recovery".into());
        }
        if !self.restore_execution_head_from_record(&identity.world_id, baseline.height)? {
            return Err("service exact canonical record disappeared during recovery".into());
        }
        let restored_record = self
            .strict_service_bootstrap_head(&identity.world_id)?
            .ok_or("service exact canonical record disappeared after recovery")?;
        if restored_record != latest
            || self.state.last_applied_committed_height != baseline.height
            || self.state.last_execution_block_hash.as_deref()
                != Some(baseline.execution_block_hash.as_str())
            || self.state.last_execution_state_root.as_deref()
                != Some(baseline.execution_state_root.as_str())
            || self.state.last_node_block_hash.as_deref()
                != Some(baseline.consensus_block_hash.as_str())
        {
            return Err(
                "restored service boundary differs from exact admitted canonical cut".into(),
            );
        }
        Ok(())
    }

    pub(crate) fn new_with_local_bootstrap(
        state_path: std::path::PathBuf,
        world_dir: std::path::PathBuf,
        records_dir: std::path::PathBuf,
        storage_root: std::path::PathBuf,
        storage_profile: &StorageProfileConfig,
        local_execution_bootstrap: NodeExecutionBootstrap,
    ) -> Result<Self, String> {
        Self::new_with_config_and_local_bootstrap(
            state_path,
            world_dir,
            records_dir,
            storage_root,
            crate::execution_config::ExecutionDriverConfig::from(storage_profile),
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
        let restore_context = super::world_service_read::RestoreContext::new(
            self.execution_world.release_security_policy().clone(),
            self.devlocal_participant.authority.clone(),
        )?;
        let pinned = super::world_service_read::pin_with_context(
            &self.records_dir,
            self.execution_store.root(),
            &identity,
            None,
            &restore_context,
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
            let restore_context = super::world_service_read::RestoreContext::new(
                self.execution_world.release_security_policy().clone(),
                self.devlocal_participant.authority.clone(),
            )?;
            let pinned = super::world_service_read::pin_with_context(
                &self.records_dir,
                self.execution_store.root(),
                &identity,
                None,
                &restore_context,
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
