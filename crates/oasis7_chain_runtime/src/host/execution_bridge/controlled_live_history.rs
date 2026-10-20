//! Read authority derives from the externally trusted activation and complete
//! immutable qualified prefix. A mutable latest cache is never authoritative.
use super::{
    ExecutionBridgeRecord, controlled_bootstrap_anchor::verify_bootstrap_anchor,
    controlled_capture as capture, controlled_live_package as package, to_cbor,
};
use crate::controlled_live_config::GuardedAuthority;
use oasis7::world_service::{CommitRef, ExecutionBinding, WorldIdentity, authority};
use oasis7_distfs::controlled_authority::{
    LocalRequestIdentity,
    replicated_protocol::{ArtifactRole, DurabilityEvidence, HeadAnchor, verify_evidence},
};
use oasis7_node::NodeLocalExecutionContinuation;
use oasis7_runtime::{Journal, Snapshot, World, blake3_hex};
use std::path::{Path, PathBuf};

pub(super) const MAX_HISTORY_RECORDS: u64 = 128;
pub(super) fn evidence_path(root: &Path, position: u64) -> PathBuf {
    root.join(format!("controlled-qualified-{position:020}.json"))
}
pub(super) fn request(
    continuation: &NodeLocalExecutionContinuation,
) -> Result<LocalRequestIdentity, String> {
    Ok(LocalRequestIdentity {
        verified_subject: continuation.context.node_id.clone(),
        operation_domain: package::SCOPE.into(),
        nonce_scope: continuation.context.world_id.clone(),
        request_id: blake3_hex(&to_cbor(continuation)?),
    })
}
/// Constructed only after full external trust, receipt and typed closure checks.
pub(super) struct VerifiedRuntimeHead {
    pub(super) head: HeadAnchor,
    pub(super) record: ExecutionBridgeRecord,
    pub(super) snapshot: Snapshot,
    pub(super) journal: Journal,
    pub(super) continuation: Option<NodeLocalExecutionContinuation>,
}
impl VerifiedRuntimeHead {
    pub(super) fn world(
        &self,
        policy: &oasis7_runtime::ReleaseSecurityPolicy,
    ) -> Result<World, String> {
        oasis7::runtime_host::world::from_snapshot(self.snapshot.clone(), self.journal.clone())
            .map(|w| w.with_release_security_policy(policy.clone()))
            .map_err(|e| format!("verified controlled world restore: {e:?}"))
    }
    pub(super) fn commit(&self, authority_config: &GuardedAuthority) -> Result<CommitRef, String> {
        let world = self.world(&authority_config.release_policy)?;
        let binding = world
            .current_cognition_runtime_binding()
            .map_err(|e| format!("controlled runtime binding unavailable: {e:?}"))?;
        let trust = &authority_config.policy.trust;
        if binding.world_id != trust.world_id {
            return Err("controlled provider world mismatch".into());
        }
        let commit = CommitRef {
            world: WorldIdentity {
                world_id: trust.world_id.clone(),
                genesis_digest: trust.genesis_digest.clone(),
            },
            binding: ExecutionBinding {
                provider_world_id: binding.world_id,
                branch_id: binding.branch_id,
                finality_ref: authority::request_digest(
                    "finality",
                    &(
                        binding.finality_epoch,
                        binding.finality_status,
                        binding.finality_block_hash,
                    ),
                )?,
                reorg_generation: binding.reorg_epoch,
                governing_manifest_ref: binding.runtime_manifest_hash.to_string(),
                authority_generation: world
                    .capability_revocation_state()
                    .authority_records
                    .values()
                    .map(|r| r.governance_epoch)
                    .max()
                    .unwrap_or(0),
                permission_generation: world.capability_revocation_state().epoch,
            },
            position: self.record.height,
            execution_block_hash: self.record.execution_block_hash.clone(),
            state_root_ref: self.record.execution_state_root.clone(),
        };
        commit.validate().map_err(|e| e.to_string())?;
        Ok(commit)
    }
}
fn original_anchor(a: &GuardedAuthority) -> Result<VerifiedRuntimeHead, String> {
    let bytes = crate::controlled_history_cli::read_bounded(
        &a.config.activation_evidence,
        64 * 1024 * 1024,
    )?;
    let evidence: DurabilityEvidence = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let original = &evidence.proposal.body.record;
    // Also rejects mutation between initial configuration load and this read.
    let anchor = verify_bootstrap_anchor(a.anchor.activation(), original, &a.release_policy)?;
    let record = capture::decode_role(original, ArtifactRole::Result)?;
    Ok(VerifiedRuntimeHead {
        head: anchor.activation().qualified_head().clone(),
        record,
        snapshot: anchor.snapshot().clone(),
        journal: anchor.journal().clone(),
        continuation: None,
    })
}

pub(super) fn verify_successor(
    a: &GuardedAuthority,
    predecessor: &VerifiedRuntimeHead,
    evidence: &DurabilityEvidence,
) -> Result<VerifiedRuntimeHead, String> {
    let head =
        verify_evidence(evidence, &a.policy.trust, &predecessor.head).map_err(|e| e.to_string())?;
    let closed = &evidence.proposal.body.record;
    capture::validate_package_budget(&to_cbor(closed)?)?;
    let manifest: package::LiveManifest =
        capture::decode_role(closed, ArtifactRole::ExecutionManifest)?;
    let c = &manifest.continuation;
    if c.context.node_id != a.config.node_id
        || c.predecessor_committed_height != predecessor.record.height
        || c.predecessor_execution_height != predecessor.record.height
        || c.predecessor_node_block_hash != predecessor.record.node_block_hash
        || c.predecessor_execution_block_hash.as_deref()
            != Some(predecessor.record.execution_block_hash.as_str())
        || c.predecessor_execution_state_root.as_deref()
            != Some(predecessor.record.execution_state_root.as_str())
        || closed.before_state_root != predecessor.record.execution_state_root
        || capture::object(closed, &manifest.before_snapshot_ref)?
            != to_cbor(&predecessor.snapshot)?
        || capture::object(closed, &manifest.before_journal_ref)? != to_cbor(&predecessor.journal)?
        || evidence.proposal.body.request != request(c)?
    {
        return Err("controlled original request/execution predecessor mismatch".into());
    }
    package::verify_package(
        closed,
        &a.policy.trust,
        c,
        &predecessor.record.execution_block_hash,
        &a.release_policy,
    )?;
    Ok(VerifiedRuntimeHead {
        head,
        record: capture::decode_role(closed, ArtifactRole::Result)?,
        snapshot: capture::decode_role(closed, ArtifactRole::Snapshot)?,
        journal: capture::decode_role(closed, ArtifactRole::Journal)?,
        continuation: Some(c.clone()),
    })
}

pub(super) fn load_head(
    a: &GuardedAuthority,
    records: &Path,
    fixed: Option<&CommitRef>,
) -> Result<VerifiedRuntimeHead, String> {
    load_selected(a, records, fixed, fixed.map(|c| c.position))
}
pub(super) fn load_at_height(
    a: &GuardedAuthority,
    records: &Path,
    height: u64,
) -> Result<VerifiedRuntimeHead, String> {
    load_selected(a, records, None, Some(height))
}
fn load_selected(
    a: &GuardedAuthority,
    records: &Path,
    fixed: Option<&CommitRef>,
    height: Option<u64>,
) -> Result<VerifiedRuntimeHead, String> {
    let mut current = original_anchor(a)?;
    let mut selected = if height.is_some_and(|h| h == current.record.height) {
        Some(original_anchor(a)?)
    } else {
        None
    };
    let initial_position = current.head.position;
    let mut minimum_seen = current.head == a.config.minimum_runtime_head;
    for offset in 1..=MAX_HISTORY_RECORDS {
        let position = initial_position
            .checked_add(offset)
            .ok_or("controlled history position exhausted")?;
        let path = evidence_path(records, position);
        let bytes = match crate::controlled_history_cli::read_bounded(&path, 64 * 1024 * 1024) {
            Ok(bytes) => bytes,
            Err(error) => {
                if std::fs::symlink_metadata(&path)
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                {
                    break;
                }
                return Err(error);
            }
        };
        let evidence: DurabilityEvidence = serde_json::from_slice(&bytes)
            .map_err(|e| format!("controlled qualified evidence: {e}"))?;
        current = verify_successor(a, &current, &evidence)?;
        if current.head.position == a.config.minimum_runtime_head.position {
            if current.head != a.config.minimum_runtime_head {
                return Err("controlled caller minimum hash mismatch".into());
            }
            minimum_seen = true;
        }
        if height.is_some_and(|h| h == current.record.height) {
            selected = Some(VerifiedRuntimeHead {
                head: current.head.clone(),
                record: current.record.clone(),
                snapshot: current.snapshot.clone(),
                journal: current.journal.clone(),
                continuation: current.continuation.clone(),
            });
        }
    }
    let beyond = initial_position
        .checked_add(MAX_HISTORY_RECORDS)
        .and_then(|p| p.checked_add(1))
        .ok_or("controlled history budget position overflow")?;
    if std::fs::symlink_metadata(evidence_path(records, beyond)).is_ok() {
        return Err("controlled engineering history budget exhausted".into());
    }
    let minimum = &a.config.minimum_runtime_head;
    if current.head.position < minimum.position
        || (current.head.position == minimum.position && &current.head != minimum)
    {
        return Err("controlled history is below or conflicts with caller minimum".into());
    }
    if !minimum_seen {
        return Err("controlled caller minimum is absent from verified prefix".into());
    }
    match std::fs::read_dir(records) {
        Ok(entries) => {
            for (index, entry) in entries.enumerate() {
                if index >= 512 {
                    return Err("controlled directory budget exhausted".into());
                }
                let name = entry.map_err(|e| e.to_string())?.file_name();
                let name = name.to_string_lossy();
                if let Some(number) = name
                    .strip_prefix("controlled-qualified-")
                    .and_then(|n| n.strip_suffix(".json"))
                {
                    let position: u64 = number
                        .parse()
                        .map_err(|_| "invalid controlled evidence file name")?;
                    if position > current.head.position || position <= initial_position {
                        return Err(
                            "controlled evidence directory contains a gap or alien prefix".into(),
                        );
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let selected = if height.is_some() {
        selected.ok_or("controlled fixed commit unavailable")?
    } else {
        current
    };
    if fixed.is_some_and(|c| selected.commit(a).as_ref() != Ok(c)) {
        return Err("controlled fixed commit conflict".into());
    }
    Ok(selected)
}
