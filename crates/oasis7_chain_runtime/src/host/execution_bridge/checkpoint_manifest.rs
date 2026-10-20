use super::{
    EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1, EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2,
    ExecutionCheckpointManifest, ExecutionCheckpointManifestHashPayload,
    ExecutionCheckpointManifestHashPayloadV2,
};

impl ExecutionCheckpointManifest {
    #[expect(
        clippy::too_many_arguments,
        reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
    )]
    pub(super) fn new(
        world_id: String,
        height: u64,
        execution_block_hash: String,
        execution_state_root: String,
        latest_state_ref: String,
        snapshot_ref: Option<String>,
        journal_ref: Option<String>,
        created_at_ms: i64,
    ) -> Result<Self, String> {
        let checkpoint_id = execution_checkpoint_id(height, execution_block_hash.as_str());
        let mut pinned_refs = vec![latest_state_ref.clone()];
        if let Some(snapshot_ref) = snapshot_ref.as_ref() {
            pinned_refs.push(snapshot_ref.clone());
        }
        if let Some(journal_ref) = journal_ref.as_ref() {
            pinned_refs.push(journal_ref.clone());
        }
        pinned_refs.sort();
        pinned_refs.dedup();

        let mut manifest = Self {
            schema_version: EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1,
            checkpoint_id,
            world_id,
            height,
            execution_block_hash,
            execution_state_root,
            predecessor_execution_block_hash: None,
            latest_state_ref,
            snapshot_ref,
            journal_ref,
            pinned_refs,
            manifest_hash: String::new(),
            created_at_ms,
        };
        manifest.manifest_hash = manifest.compute_manifest_hash()?;
        Ok(manifest)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
    )]
    pub(super) fn new_with_predecessor_execution_block_hash(
        world_id: String,
        height: u64,
        execution_block_hash: String,
        execution_state_root: String,
        predecessor_execution_block_hash: String,
        latest_state_ref: String,
        snapshot_ref: Option<String>,
        journal_ref: Option<String>,
        created_at_ms: i64,
    ) -> Result<Self, String> {
        if predecessor_execution_block_hash.is_empty() {
            return Err(format!(
                "execution checkpoint height {} missing predecessor execution block hash",
                height
            ));
        }
        let checkpoint_id = execution_checkpoint_id(height, execution_block_hash.as_str());
        let mut pinned_refs = vec![latest_state_ref.clone()];
        if let Some(snapshot_ref) = snapshot_ref.as_ref() {
            pinned_refs.push(snapshot_ref.clone());
        }
        if let Some(journal_ref) = journal_ref.as_ref() {
            pinned_refs.push(journal_ref.clone());
        }
        pinned_refs.sort();
        pinned_refs.dedup();

        let mut manifest = Self {
            schema_version: EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2,
            checkpoint_id,
            world_id,
            height,
            execution_block_hash,
            execution_state_root,
            predecessor_execution_block_hash: Some(predecessor_execution_block_hash),
            latest_state_ref,
            snapshot_ref,
            journal_ref,
            pinned_refs,
            manifest_hash: String::new(),
            created_at_ms,
        };
        manifest.manifest_hash = manifest.compute_manifest_hash()?;
        Ok(manifest)
    }

    fn compute_manifest_hash(&self) -> Result<String, String> {
        let payload = match self.schema_version {
            EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1 => {
                super::to_cbor(ExecutionCheckpointManifestHashPayload {
                    schema_version: self.schema_version,
                    checkpoint_id: self.checkpoint_id.as_str(),
                    world_id: self.world_id.as_str(),
                    height: self.height,
                    execution_block_hash: self.execution_block_hash.as_str(),
                    execution_state_root: self.execution_state_root.as_str(),
                    latest_state_ref: self.latest_state_ref.as_str(),
                    snapshot_ref: self.snapshot_ref.as_deref(),
                    journal_ref: self.journal_ref.as_deref(),
                    pinned_refs: self.pinned_refs.as_slice(),
                    created_at_ms: self.created_at_ms,
                })?
            }
            EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2 => {
                super::to_cbor(ExecutionCheckpointManifestHashPayloadV2 {
                    schema_version: self.schema_version,
                    checkpoint_id: self.checkpoint_id.as_str(),
                    world_id: self.world_id.as_str(),
                    height: self.height,
                    execution_block_hash: self.execution_block_hash.as_str(),
                    execution_state_root: self.execution_state_root.as_str(),
                    predecessor_execution_block_hash: self
                        .predecessor_execution_block_hash
                        .as_deref(),
                    latest_state_ref: self.latest_state_ref.as_str(),
                    snapshot_ref: self.snapshot_ref.as_deref(),
                    journal_ref: self.journal_ref.as_deref(),
                    pinned_refs: self.pinned_refs.as_slice(),
                    created_at_ms: self.created_at_ms,
                })?
            }
            schema_version => {
                return Err(format!(
                    "execution checkpoint manifest {} has unsupported schema_version={}",
                    self.checkpoint_id, schema_version
                ));
            }
        };
        Ok(oasis7_runtime::blake3_hex(payload.as_slice()))
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.schema_version,
            EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1 | EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2
        ) {
            return Err(format!(
                "execution checkpoint manifest {} has invalid schema_version={}",
                self.checkpoint_id, self.schema_version
            ));
        }
        if self.height == 0 {
            return Err(format!(
                "execution checkpoint manifest {} has invalid height=0",
                self.checkpoint_id
            ));
        }
        if self.schema_version >= EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2
            && self
                .predecessor_execution_block_hash
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return Err(format!(
                "execution checkpoint manifest {} missing predecessor execution block hash",
                self.checkpoint_id
            ));
        }
        if self.latest_state_ref.is_empty() {
            return Err(format!(
                "execution checkpoint manifest {} missing latest_state_ref",
                self.checkpoint_id
            ));
        }
        let mut expected_pins = vec![self.latest_state_ref.clone()];
        if let Some(snapshot_ref) = self.snapshot_ref.as_ref() {
            expected_pins.push(snapshot_ref.clone());
        }
        if let Some(journal_ref) = self.journal_ref.as_ref() {
            expected_pins.push(journal_ref.clone());
        }
        expected_pins.sort();
        expected_pins.dedup();
        if expected_pins != self.pinned_refs {
            return Err(format!(
                "execution checkpoint manifest {} pin-set mismatch expected={:?} actual={:?}",
                self.checkpoint_id, expected_pins, self.pinned_refs
            ));
        }
        let expected_hash = self.compute_manifest_hash()?;
        if self.manifest_hash != expected_hash {
            return Err(format!(
                "execution checkpoint manifest {} hash mismatch expected={} actual={}",
                self.checkpoint_id, expected_hash, self.manifest_hash
            ));
        }
        Ok(())
    }
}

fn execution_checkpoint_id(height: u64, execution_block_hash: &str) -> String {
    let short_hash: String = execution_block_hash.chars().take(16).collect();
    format!("checkpoint-{:020}-{short_hash}", height)
}
