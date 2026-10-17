use std::collections::BTreeSet;
use std::path::Path;

use oasis7::runtime::{BlobStore, LocalCasStore, blake3_hex};
use oasis7_proto::distributed::{BlobRef, WIRE_ENCODING_CBOR, WorldBlock, WorldHeadAnnounce};
use serde::{Deserialize, Serialize};

pub(crate) mod controlled_live;
mod controlled_live_history;
mod controlled_live_package;
mod driver_startup_recovery;
mod execution_hash;
#[cfg(unix)]
mod formal_attempt_lifecycle;
mod formal_business_package;
#[cfg(all(test, unix))]
pub(crate) mod formal_crash_test_boundary;
#[cfg(unix)]
pub(crate) mod formal_durable_set_arm;
mod formal_execution_timing;
#[cfg(unix)]
mod formal_executor;
pub(crate) mod formal_initial;
pub(crate) mod formal_native_environment;
#[cfg(unix)]
pub(crate) mod formal_node_hook;
pub(crate) mod formal_package;
pub(crate) mod formal_snapshot;
#[cfg(unix)]
pub(crate) mod formal_writer;
mod product_validation_intent;
mod provider_bootstrap;
mod world_service_execution;
pub(crate) mod world_service_read;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ExecutionBridgeState {
    pub last_applied_committed_height: u64,
    pub last_execution_block_hash: Option<String>,
    pub last_execution_state_root: Option<String>,
    pub last_node_block_hash: Option<String>,
}

pub(crate) const EXECUTION_BRIDGE_RECORD_SCHEMA_V1: u32 = 1;
pub(crate) const EXECUTION_BRIDGE_RECORD_SCHEMA_V2: u32 = 2;
pub(crate) const EXECUTION_BRIDGE_RECORD_SCHEMA_V3: u32 = 3;
pub(super) const EXECUTION_BRIDGE_DEFAULT_HOT_WINDOW_HEIGHTS: u64 = 32;
pub(super) const EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS: u64 = 32;
pub(super) const EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_KEEP_LATEST: usize = 4;
pub(super) const WORLD_HEAD_PROOF_V1_SCHEMA: u16 = 1;
pub(super) const WORLD_HEAD_PROOF_HASH_DOMAIN_V1: &str = "oasis7.world_head_proof.v1";
pub(super) const WORLD_HEAD_PROOF_CLAIM_BOUNDARY_V1: &str =
    "head_execution_checkpoint_evidence_only_not_light_client_or_mainnet_readiness";
fn execution_bridge_record_schema_v1() -> u32 {
    EXECUTION_BRIDGE_RECORD_SCHEMA_V1
}

fn world_head_proof_v1_schema() -> u16 {
    WORLD_HEAD_PROOF_V1_SCHEMA
}

fn world_head_proof_claim_boundary_v1() -> String {
    WORLD_HEAD_PROOF_CLAIM_BOUNDARY_V1.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct HeadConsensusEvidenceV1 {
    pub consensus_status: String,
    pub proposer_id: String,
    #[serde(default)]
    pub quorum_threshold: u64,
    #[serde(default)]
    pub validator_count: u64,
    #[serde(default)]
    pub vote_count: u64,
    #[serde(default)]
    pub approver_ids: Vec<String>,
    #[serde(default)]
    pub evidence_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExecutionBindingEvidenceV1 {
    pub execution_height: u64,
    pub node_block_hash: String,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub action_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CheckpointClosureEvidenceV1 {
    pub checkpoint_height: u64,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub manifest_ref: String,
    #[serde(default)]
    pub manifest_hash: String,
    #[serde(default)]
    pub pinned_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct WorldHeadProofV1 {
    #[serde(default = "world_head_proof_v1_schema")]
    pub schema_version: u16,
    pub world_id: String,
    pub height: u64,
    pub timestamp_ms: i64,
    pub head: WorldHeadAnnounce,
    pub block: WorldBlock,
    pub snapshot_manifest_ref: BlobRef,
    pub journal_segments_ref: BlobRef,
    pub consensus: HeadConsensusEvidenceV1,
    pub execution: ExecutionBindingEvidenceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<CheckpointClosureEvidenceV1>,
    #[serde(default = "world_head_proof_claim_boundary_v1")]
    pub claim_boundary: String,
}

impl WorldHeadProofV1 {
    pub(super) fn validate_contract(&self) -> Result<(), String> {
        if self.schema_version != WORLD_HEAD_PROOF_V1_SCHEMA {
            return Err(format!(
                "unsupported world head proof schema: {}",
                self.schema_version
            ));
        }
        if self.claim_boundary != WORLD_HEAD_PROOF_CLAIM_BOUNDARY_V1 {
            return Err(format!(
                "unexpected world head proof claim boundary: {}",
                self.claim_boundary
            ));
        }
        if self.world_id != self.head.world_id || self.world_id != self.block.world_id {
            return Err(format!(
                "world_id mismatch: proof={} head={} block={}",
                self.world_id, self.head.world_id, self.block.world_id
            ));
        }
        if self.height != self.head.height || self.height != self.block.height {
            return Err(format!(
                "height mismatch: proof={} head={} block={}",
                self.height, self.head.height, self.block.height
            ));
        }
        if self.timestamp_ms != self.head.timestamp_ms
            || self.timestamp_ms != self.block.timestamp_ms
        {
            return Err(format!(
                "timestamp mismatch: proof={} head={} block={}",
                self.timestamp_ms, self.head.timestamp_ms, self.block.timestamp_ms
            ));
        }
        let computed_block_hash = blake3_hex(to_cbor(&self.block)?.as_slice());
        if self.head.block_hash != computed_block_hash {
            return Err(format!(
                "head block hash mismatch: head={} block={}",
                self.head.block_hash, computed_block_hash
            ));
        }
        if self.head.state_root != self.block.state_root {
            return Err(format!(
                "head state_root mismatch: head={} block={}",
                self.head.state_root, self.block.state_root
            ));
        }
        if self.block.snapshot_ref != self.snapshot_manifest_ref.content_hash {
            return Err(format!(
                "snapshot_ref mismatch: block={} proof={}",
                self.block.snapshot_ref, self.snapshot_manifest_ref.content_hash
            ));
        }
        if self.block.journal_ref != self.journal_segments_ref.content_hash {
            return Err(format!(
                "journal_ref mismatch: block={} proof={}",
                self.block.journal_ref, self.journal_segments_ref.content_hash
            ));
        }
        if self.execution.execution_height != self.height {
            return Err(format!(
                "execution height mismatch: proof={} execution={}",
                self.height, self.execution.execution_height
            ));
        }
        if self.execution.node_block_hash.trim().is_empty() {
            return Err("execution node block hash must not be empty".to_string());
        }
        if self.execution.execution_state_root != self.block.state_root {
            return Err(format!(
                "execution state_root mismatch: execution={} block={}",
                self.execution.execution_state_root, self.block.state_root
            ));
        }
        if self.execution.action_root != self.block.action_root {
            return Err(format!(
                "execution action_root mismatch: execution={} block={}",
                self.execution.action_root, self.block.action_root
            ));
        }
        if self.consensus.consensus_status != "committed" {
            return Err(format!(
                "world head proof requires committed consensus status, got {}",
                self.consensus.consensus_status
            ));
        }
        if self.consensus.proposer_id != self.block.proposer_id {
            return Err(format!(
                "consensus proposer mismatch: consensus={} block={}",
                self.consensus.proposer_id, self.block.proposer_id
            ));
        }
        if let Some(checkpoint) = &self.checkpoint {
            if checkpoint.manifest_ref.is_empty() {
                return Err("checkpoint manifest_ref must not be empty".to_string());
            }
            if !checkpoint
                .pinned_refs
                .iter()
                .any(|reference| reference == &self.snapshot_manifest_ref.content_hash)
            {
                return Err(format!(
                    "checkpoint pinned refs missing snapshot manifest ref: {}",
                    self.snapshot_manifest_ref.content_hash
                ));
            }
            if !checkpoint
                .pinned_refs
                .iter()
                .any(|reference| reference == &self.journal_segments_ref.content_hash)
            {
                return Err(format!(
                    "checkpoint pinned refs missing journal segments ref: {}",
                    self.journal_segments_ref.content_hash
                ));
            }
            if checkpoint.checkpoint_height != self.height {
                return Err(format!(
                    "checkpoint height mismatch: proof={} checkpoint={}",
                    self.height, checkpoint.checkpoint_height
                ));
            }
            if checkpoint.execution_block_hash != self.execution.execution_block_hash {
                return Err(format!(
                    "checkpoint execution block mismatch: checkpoint={} execution={}",
                    checkpoint.execution_block_hash, self.execution.execution_block_hash
                ));
            }
            if checkpoint.execution_state_root != self.execution.execution_state_root {
                return Err(format!(
                    "checkpoint execution state mismatch: checkpoint={} execution={}",
                    checkpoint.execution_state_root, self.execution.execution_state_root
                ));
            }
        }
        Ok(())
    }

    pub(super) fn proof_hash(&self) -> Result<String, String> {
        self.validate_contract()?;
        Ok(blake3_hex(
            to_cbor((WORLD_HEAD_PROOF_HASH_DOMAIN_V1, self))?.as_slice(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ExecutionBridgeRecordWire")]
pub(crate) struct ExecutionBridgeRecord {
    pub schema_version: u32,
    pub world_id: String,
    pub height: u64,
    pub node_block_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_node_block_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_root: Option<String>,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub journal_len: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_state_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_log_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_effect_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_head_proof_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_head_proof_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controlled_capture_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulator_mirror: Option<ExecutionSimulatorMirrorRecord>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ExecutionBridgeRecordWire {
    #[serde(default = "execution_bridge_record_schema_v1")]
    pub schema_version: u32,
    pub world_id: String,
    pub height: u64,
    #[serde(default)]
    pub node_block_hash: Option<String>,
    #[serde(default)]
    pub prev_node_block_hash: Option<String>,
    #[serde(default)]
    pub proposer_id: Option<String>,
    #[serde(default)]
    pub action_root: Option<String>,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub journal_len: usize,
    #[serde(default)]
    pub latest_state_ref: Option<String>,
    #[serde(default)]
    pub snapshot_ref: Option<String>,
    #[serde(default)]
    pub journal_ref: Option<String>,
    #[serde(default)]
    pub commit_log_ref: Option<String>,
    #[serde(default)]
    pub checkpoint_ref: Option<String>,
    #[serde(default)]
    pub external_effect_ref: Option<String>,
    #[serde(default)]
    pub world_head_proof_ref: Option<String>,
    #[serde(default)]
    pub world_head_proof_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controlled_capture_ref: Option<String>,
    #[serde(default)]
    pub simulator_mirror: Option<ExecutionSimulatorMirrorRecord>,
    pub timestamp_ms: i64,
}

impl From<ExecutionBridgeRecordWire> for ExecutionBridgeRecord {
    fn from(record: ExecutionBridgeRecordWire) -> Self {
        let snapshot_ref = record.snapshot_ref;
        let latest_state_ref = record.latest_state_ref.or_else(|| snapshot_ref.clone());
        Self {
            schema_version: record.schema_version.max(EXECUTION_BRIDGE_RECORD_SCHEMA_V1),
            world_id: record.world_id,
            height: record.height,
            node_block_hash: record.node_block_hash,
            prev_node_block_hash: record.prev_node_block_hash,
            proposer_id: record.proposer_id,
            action_root: record.action_root,
            execution_block_hash: record.execution_block_hash,
            execution_state_root: record.execution_state_root,
            journal_len: record.journal_len,
            latest_state_ref,
            snapshot_ref,
            journal_ref: record.journal_ref,
            commit_log_ref: record.commit_log_ref,
            checkpoint_ref: record.checkpoint_ref,
            external_effect_ref: record.external_effect_ref,
            world_head_proof_ref: record.world_head_proof_ref,
            world_head_proof_hash: record.world_head_proof_hash,
            controlled_capture_ref: record.controlled_capture_ref,
            simulator_mirror: record.simulator_mirror,
            timestamp_ms: record.timestamp_ms,
        }
    }
}

impl ExecutionBridgeRecord {
    pub(super) fn recovery_snapshot_ref(&self) -> Option<&str> {
        self.latest_state_ref
            .as_deref()
            .or(self.snapshot_ref.as_deref())
            .or_else(|| {
                if self.latest_state_ref.is_none()
                    && self.snapshot_ref.is_none()
                    && !self.execution_state_root.trim().is_empty()
                {
                    Some(self.execution_state_root.as_str())
                } else {
                    None
                }
            })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
    )]
    pub(super) fn new_v2(
        world_id: String,
        height: u64,
        node_block_hash: Option<String>,
        execution_block_hash: String,
        execution_state_root: String,
        journal_len: usize,
        snapshot_ref: String,
        journal_ref: String,
        external_effect_ref: Option<String>,
        simulator_mirror: Option<ExecutionSimulatorMirrorRecord>,
        timestamp_ms: i64,
    ) -> Self {
        Self {
            schema_version: EXECUTION_BRIDGE_RECORD_SCHEMA_V2,
            world_id,
            height,
            node_block_hash,
            prev_node_block_hash: None,
            proposer_id: None,
            action_root: None,
            execution_block_hash,
            execution_state_root,
            journal_len,
            latest_state_ref: Some(snapshot_ref.clone()),
            snapshot_ref: Some(snapshot_ref),
            journal_ref: Some(journal_ref),
            commit_log_ref: None,
            checkpoint_ref: None,
            external_effect_ref,
            world_head_proof_ref: None,
            world_head_proof_hash: None,
            controlled_capture_ref: None,
            simulator_mirror,
            timestamp_ms,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
    )]
    pub(super) fn new_v3(
        world_id: String,
        height: u64,
        node_block_hash: Option<String>,
        prev_node_block_hash: Option<String>,
        proposer_id: String,
        action_root: String,
        execution_block_hash: String,
        execution_state_root: String,
        journal_len: usize,
        snapshot_ref: String,
        journal_ref: String,
        external_effect_ref: Option<String>,
        simulator_mirror: Option<ExecutionSimulatorMirrorRecord>,
        timestamp_ms: i64,
    ) -> Self {
        Self {
            schema_version: EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
            world_id,
            height,
            node_block_hash,
            prev_node_block_hash,
            proposer_id: Some(proposer_id),
            action_root: Some(action_root),
            execution_block_hash,
            execution_state_root,
            journal_len,
            latest_state_ref: Some(snapshot_ref.clone()),
            snapshot_ref: Some(snapshot_ref),
            journal_ref: Some(journal_ref),
            commit_log_ref: None,
            checkpoint_ref: None,
            external_effect_ref,
            world_head_proof_ref: None,
            world_head_proof_hash: None,
            controlled_capture_ref: None,
            simulator_mirror,
            timestamp_ms,
        }
    }

    pub(super) fn world_head_proof_v1(
        &self,
        checkpoint: Option<&ExecutionCheckpointManifest>,
    ) -> Result<WorldHeadProofV1, String> {
        let node_block_hash = self
            .node_block_hash
            .as_deref()
            .filter(|hash| !hash.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "execution bridge record height {} missing node_block_hash for world head proof",
                    self.height
                )
            })?;
        let proposer_id = self
            .proposer_id
            .as_deref()
            .filter(|proposer_id| !proposer_id.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "execution bridge record height {} missing proposer_id for world head proof",
                    self.height
                )
            })?;
        let action_root = self
            .action_root
            .as_deref()
            .filter(|action_root| !action_root.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "execution bridge record height {} missing action_root for world head proof",
                    self.height
                )
            })?;
        let prev_block_hash = self
            .prev_node_block_hash
            .as_deref()
            .filter(|prev_node_block_hash| !prev_node_block_hash.trim().is_empty())
            .unwrap_or("genesis");
        let snapshot_ref = self
            .snapshot_ref
            .as_deref()
            .or(self.latest_state_ref.as_deref())
            .filter(|reference| !reference.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "execution bridge record height {} missing snapshot ref for world head proof",
                    self.height
                )
            })?;
        let journal_ref = self
            .journal_ref
            .as_deref()
            .filter(|reference| !reference.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "execution bridge record height {} missing journal ref for world head proof",
                    self.height
                )
            })?;

        let block = WorldBlock {
            world_id: self.world_id.clone(),
            height: self.height,
            prev_block_hash: prev_block_hash.to_string(),
            action_root: action_root.to_string(),
            event_root: self.external_effect_ref.clone().unwrap_or_default(),
            state_root: self.execution_state_root.clone(),
            journal_ref: journal_ref.to_string(),
            snapshot_ref: snapshot_ref.to_string(),
            receipts_root: self.execution_block_hash.clone(),
            proposer_id: proposer_id.to_string(),
            timestamp_ms: self.timestamp_ms,
            signature: "runtime_bridge_evidence_only_v1".to_string(),
        };
        let block_hash = blake3_hex(to_cbor(&block)?.as_slice());
        let checkpoint = checkpoint
            .map(|manifest| {
                if manifest.height != self.height
                    || manifest.execution_block_hash != self.execution_block_hash
                    || manifest.execution_state_root != self.execution_state_root
                {
                    return Err(format!(
                        "execution checkpoint manifest does not match bridge record height={}",
                        self.height
                    ));
                }
                Ok(CheckpointClosureEvidenceV1 {
                    checkpoint_height: manifest.height,
                    execution_block_hash: manifest.execution_block_hash.clone(),
                    execution_state_root: manifest.execution_state_root.clone(),
                    manifest_ref: self.checkpoint_ref.clone().unwrap_or_default(),
                    manifest_hash: manifest.manifest_hash.clone(),
                    pinned_refs: manifest.pinned_refs.clone(),
                })
            })
            .transpose()?;
        let proof = WorldHeadProofV1 {
            schema_version: WORLD_HEAD_PROOF_V1_SCHEMA,
            world_id: self.world_id.clone(),
            height: self.height,
            timestamp_ms: self.timestamp_ms,
            head: WorldHeadAnnounce {
                world_id: self.world_id.clone(),
                height: self.height,
                block_hash,
                state_root: self.execution_state_root.clone(),
                timestamp_ms: self.timestamp_ms,
                signature: "runtime_bridge_evidence_only_v1".to_string(),
            },
            block,
            snapshot_manifest_ref: BlobRef {
                content_hash: snapshot_ref.to_string(),
                size_bytes: 0,
                codec: WIRE_ENCODING_CBOR.to_string(),
                links: Vec::new(),
            },
            journal_segments_ref: BlobRef {
                content_hash: journal_ref.to_string(),
                size_bytes: 0,
                codec: WIRE_ENCODING_CBOR.to_string(),
                links: Vec::new(),
            },
            consensus: HeadConsensusEvidenceV1 {
                consensus_status: "committed".to_string(),
                proposer_id: proposer_id.to_string(),
                quorum_threshold: 0,
                validator_count: 0,
                vote_count: 0,
                approver_ids: Vec::new(),
                evidence_hash: node_block_hash.to_string(),
            },
            execution: ExecutionBindingEvidenceV1 {
                execution_height: self.height,
                node_block_hash: node_block_hash.to_string(),
                execution_block_hash: self.execution_block_hash.clone(),
                execution_state_root: self.execution_state_root.clone(),
                action_root: action_root.to_string(),
            },
            checkpoint,
            claim_boundary: WORLD_HEAD_PROOF_CLAIM_BOUNDARY_V1.to_string(),
        };
        proof.validate_contract()?;
        Ok(proof)
    }
}

pub(super) fn persist_world_head_proof_for_record(
    execution_store: &LocalCasStore,
    record: &mut ExecutionBridgeRecord,
    checkpoint: Option<&ExecutionCheckpointManifest>,
) -> Result<(), String> {
    let proof = record.world_head_proof_v1(checkpoint)?;
    let proof_hash = proof.proof_hash()?;
    let proof_bytes = to_cbor(&proof)?;
    let proof_ref = execution_store
        .put_bytes(proof_bytes.as_slice())
        .map_err(|err| format!("persist world head proof failed: {:?}", err))?;
    record.world_head_proof_ref = Some(proof_ref);
    record.world_head_proof_hash = Some(proof_hash);
    Ok(())
}

pub(crate) const EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1: u32 = 1;
pub(crate) const EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2: u32 = 2;

fn execution_checkpoint_manifest_schema_v1() -> u32 {
    EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExecutionCheckpointManifest {
    #[serde(default = "execution_checkpoint_manifest_schema_v1")]
    pub schema_version: u32,
    pub checkpoint_id: String,
    pub world_id: String,
    pub height: u64,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_execution_block_hash: Option<String>,
    pub latest_state_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_refs: Vec<String>,
    pub manifest_hash: String,
    pub created_at_ms: i64,
}

/// Bounded checkpoint identity exposed to the governed rebuild proof endpoint.
/// This intentionally carries the execution binding fields needed to compare
/// the retained boundary with the live runtime head without exposing the full
/// manifest or CAS references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionCheckpointStatusEvidence {
    pub schema_version: u32,
    pub checkpoint_id: String,
    pub world_id: String,
    pub height: u64,
    pub execution_block_hash: String,
    pub execution_state_root: String,
    pub manifest_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExecutionCheckpointLatestPointer {
    #[serde(default = "execution_checkpoint_manifest_schema_v1")]
    pub schema_version: u32,
    pub checkpoint_id: String,
    pub height: u64,
    pub manifest_hash: String,
    pub manifest_rel_path: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct ExecutionCheckpointManifestHashPayload<'a> {
    pub schema_version: u32,
    pub checkpoint_id: &'a str,
    pub world_id: &'a str,
    pub height: u64,
    pub execution_block_hash: &'a str,
    pub execution_state_root: &'a str,
    pub latest_state_ref: &'a str,
    pub snapshot_ref: Option<&'a str>,
    pub journal_ref: Option<&'a str>,
    pub pinned_refs: &'a [String],
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct ExecutionCheckpointManifestHashPayloadV2<'a> {
    pub schema_version: u32,
    pub checkpoint_id: &'a str,
    pub world_id: &'a str,
    pub height: u64,
    pub execution_block_hash: &'a str,
    pub execution_state_root: &'a str,
    pub predecessor_execution_block_hash: Option<&'a str>,
    pub latest_state_ref: &'a str,
    pub snapshot_ref: Option<&'a str>,
    pub journal_ref: Option<&'a str>,
    pub pinned_refs: &'a [String],
    pub created_at_ms: i64,
}

pub(super) const EXECUTION_EXTERNAL_EFFECT_SCHEMA_V1: u32 = 1;
pub(super) const EXECUTION_EXTERNAL_EFFECT_CONTRACT_CLOSED_WORLD_V1: &str = "closed_world_v1";

fn execution_external_effect_schema_v1() -> u32 {
    EXECUTION_EXTERNAL_EFFECT_SCHEMA_V1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExecutionExternalEffectMaterialization {
    #[serde(default = "execution_external_effect_schema_v1")]
    pub schema_version: u32,
    pub contract: String,
    pub world_id: String,
    pub node_id: String,
    pub height: u64,
    pub slot: u64,
    pub epoch: u64,
    pub node_block_hash: String,
    pub action_root: String,
    pub committed_at_unix_ms: i64,
    pub pre_step_execution_state_root: String,
    pub world_manifest_hash: String,
    pub active_modules_hash: String,
    pub committed_actions_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_modules: Vec<ExecutionModuleResolutionAnchor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub committed_actions: Vec<ExecutionCommittedActionAnchor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved_inputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExecutionModuleResolutionAnchor {
    pub instance_id: String,
    pub module_id: String,
    pub module_version: String,
    pub wasm_hash: String,
    pub install_target: oasis7_agent_api::world_action::ModuleInstallTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExecutionCommittedActionAnchor {
    pub action_id: u64,
    pub submitter_player_id: String,
    pub payload_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExecutionReplayRecordInput {
    pub record: ExecutionBridgeRecord,
    pub external_effect: Option<ExecutionExternalEffectMaterialization>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExecutionReplayPlan {
    pub target_height: u64,
    pub start_height: u64,
    pub checkpoint: Option<ExecutionCheckpointManifest>,
    pub records: Vec<ExecutionReplayRecordInput>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ExecutionBridgePinSet {
    pub latest_height: Option<u64>,
    pub hot_window_start_height: Option<u64>,
    pub archive_window_start_height: Option<u64>,
    pub checkpoint_heights: BTreeSet<u64>,
    pub pinned_refs: BTreeSet<String>,
    pub best_effort_pinned_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ExecutionSimulatorMirrorRecord {
    pub action_count: usize,
    pub rejected_action_count: usize,
    pub journal_len: usize,
    pub snapshot_ref: String,
    pub journal_ref: String,
    pub state_root: String,
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    super::write_bytes_atomic(path, bytes)
}

fn to_cbor<T: Serialize>(value: T) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(&value).map_err(|err| format!("serialize to cbor failed: {}", err))
}

mod checkpoint;
mod checkpoint_manifest;

pub(crate) fn load_latest_execution_checkpoint_status_evidence(
    execution_records_dir: &Path,
) -> Result<Option<ExecutionCheckpointStatusEvidence>, String> {
    checkpoint::load_latest_execution_checkpoint_manifest(execution_records_dir).map(|manifest| {
        manifest.map(|manifest| ExecutionCheckpointStatusEvidence {
            schema_version: manifest.schema_version,
            checkpoint_id: manifest.checkpoint_id,
            world_id: manifest.world_id,
            height: manifest.height,
            execution_block_hash: manifest.execution_block_hash,
            execution_state_root: manifest.execution_state_root,
            manifest_hash: manifest.manifest_hash,
        })
    })
}
mod driver;
mod driver_checkpoint_install;
mod driver_committed_heights;
pub(crate) mod driver_observability;
mod driver_persistence;
mod driver_replay_validation;
mod driver_replicated_input;
mod durable_transaction;
mod external_effect;
mod local_bootstrap;
mod service_bootstrap_boundary;
mod simulator_mirror;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use self::driver::ExecutionResourceIdentity;
#[allow(unused_imports)]
pub(crate) use self::driver::NodeRuntimeExecutionDriver;
#[cfg(not(test))]
pub(super) use self::driver::derive_local_execution_bootstrap;
#[allow(unused_imports)]
pub(crate) use self::driver::{load_execution_world, load_execution_world_with_policy};
#[allow(unused_imports)]
pub(super) use self::driver_committed_heights::bridge_committed_heights;
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use self::driver_observability::reset_execution_bridge_commit_timing_for_tests;
#[allow(unused_imports)]
pub(crate) use self::driver_observability::{
    ExecutionBridgeCommitTimingSnapshot, record_execution_bridge_module_tick_routing_metrics,
    snapshot_execution_bridge_commit_timing, snapshot_execution_bridge_module_tick_routing_metrics,
};
pub(crate) use self::execution_hash::legacy_migration_execution_hash;
#[cfg(test)]
pub(crate) use self::local_bootstrap::derive_local_execution_bootstrap;
#[cfg(not(test))]
pub(super) use self::provider_bootstrap::publish_provider_backed_bootstrap_from_paths;
pub(crate) use self::simulator_mirror::simulator_world_dir_from_execution_world_dir;

#[cfg(test)]
pub(crate) use self::local_bootstrap::derive_service_execution_bootstrap;
#[cfg(not(test))]
pub(crate) use self::local_bootstrap::derive_service_execution_bootstrap;

pub(crate) mod controlled_bootstrap_anchor;
mod controlled_capture;

pub(crate) mod controlled_history;
