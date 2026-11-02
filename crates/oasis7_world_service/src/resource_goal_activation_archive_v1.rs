//! Actual adjacent-cut material for the original activation pin. No I/O,
//! Current, audience or paid authority is constructed by this archive producer.
use super::*;
use ed25519_dalek::{Signature, VerifyingKey};
use oasis7_agent_api::game::GAME_GOAL_PUBLISH_DOMAIN;
use oasis7_client_api::legacy_world_service::{CanonicalIntentResultV1, WorldServicePayloadV1};
use oasis7_formal_protocol::controlled_authority::replicated_protocol::ArtifactRole;
use serde::{Deserialize, Serialize};

/// Not Clone/Serde/Debug: only a verified adjacent producer creates this owned
/// material. Persistence must occur outside Node/Viewer/publication locks.
pub struct PreparedGoalActivationArchiveV1 {
    material: GoalActivationArchiveMaterialV1,
}
/// Persisted historical facts only. A consumer must independently obtain
/// verified checkpoint pins from its actual protected ledger before restoring
/// opaque publications; serialized checkpoints cannot qualify themselves.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalActivationArchiveMaterialV1 {
    api_version: u8,
    before_checkpoint: FormalReadCheckpointV1,
    after_checkpoint: FormalReadCheckpointV1,
    before_snapshot: Vec<u8>,
    after_snapshot: Vec<u8>,
    before_journal: Vec<u8>,
    after_journal: Vec<u8>,
    activation_commit: CommitRef,
    results: std::collections::BTreeMap<String, CanonicalIntentResultV1>,
}
impl PreparedGoalActivationArchiveV1 {
    pub fn from_adjacent(
        before: &LocalQualifiedPublicationV1,
        after: &LocalQualifiedPublicationV1,
    ) -> FormalReadResult<Self> {
        let old = before.anchor.head().record();
        let new = after.anchor.head().record();
        if new.world != old.world
            || new.parent_record_id != before.anchor.head().head().record_id
            || old.position.checked_add(1) != Some(new.position)
            || new.before_state_root != old.after_state_root
        {
            return Err(FormalReadError::Integrity(
                "activation archive requires adjacent qualified cuts".into(),
            ));
        }
        let prior = &before
            .world
            .capability_revocation_state()
            .world_service_results;
        let current = &after
            .world
            .capability_revocation_state()
            .world_service_results;
        let _history = game_task_history(&after.world)?;
        let mut results = std::collections::BTreeMap::new();
        for (key, value) in current {
            if let Some(older) = prior.get(key) {
                if older != value {
                    return Err(FormalReadError::Integrity(
                        "canonical activation result changed".into(),
                    ));
                }
                continue;
            }
            let result=oasis7_world_state::service_admission::correlation::validate_result_record_uncached(key,value)
                .map_err(FormalReadError::Integrity)?;
            let WorldServicePayloadV1::GameGoalPublish(signed) = &result.request.signed_payload
            else {
                continue;
            };
            if result.rejected.is_some() {
                continue;
            }
            if new.kind != FormalRecordKindV1::Execution
                || result.committed_height <= before.world.state().time
                || result.committed_height != after.world.state().time
            {
                return Err(FormalReadError::Integrity(
                    "activation result is not this exact Runtime transition".into(),
                ));
            }
            verify_owner(signed)?;
            // The actual reducer already checked all canonical signatures,
            // order and resulting tasks, not a caller-supplied success bit.
            results.insert(key.clone(), result);
        }
        let before_snapshot = oasis7_world_state::codec::snapshot_cbor(&before.world.snapshot())
            .map_err(|e| FormalReadError::Internal(e.to_string()))?;
        let after_snapshot = oasis7_world_state::codec::snapshot_cbor(&after.world.snapshot())
            .map_err(|e| FormalReadError::Internal(e.to_string()))?;
        if blake3_hex(&before_snapshot) != old.after_state_root
            || blake3_hex(&after_snapshot) != new.after_state_root
        {
            return Err(FormalReadError::Integrity(
                "activation material differs from qualified snapshot".into(),
            ));
        }
        let before_journal = actual_journal(before)?;
        let after_journal = actual_journal(after)?;
        Ok(Self {
            material: GoalActivationArchiveMaterialV1 {
                api_version: 1,
                before_checkpoint: before.anchor.checkpoint(),
                after_checkpoint: after.anchor.checkpoint(),
                before_snapshot,
                after_snapshot,
                before_journal,
                after_journal,
                activation_commit: after.commit.clone(),
                results,
            },
        })
    }
    /// Moving bytes to a durable local store does not assert that it committed.
    /// Only the real floor/once/archive ledger may publish restored availability.
    pub fn into_material(self) -> GoalActivationArchiveMaterialV1 {
        self.material
    }
    pub(super) fn into_exact_facts(
        self,
    ) -> (
        CommitRef,
        std::collections::BTreeMap<String, CanonicalIntentResultV1>,
    ) {
        (self.material.activation_commit, self.material.results)
    }
}
fn actual_journal(cut: &LocalQualifiedPublicationV1) -> FormalReadResult<Vec<u8>> {
    let bytes = oasis7_world_state::codec::journal_cbor(cut.world.journal())
        .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
    let root = cut
        .anchor
        .head()
        .record()
        .artifact_roots
        .get(&ArtifactRole::Journal)
        .ok_or_else(|| {
            FormalReadError::Integrity("actual qualified Journal root missing".into())
        })?;
    if blake3_hex(&bytes) != *root {
        return Err(FormalReadError::Integrity(
            "actual Journal differs from qualified artifact root".into(),
        ));
    }
    Ok(bytes)
}
fn game_task_history(
    world: &World,
) -> FormalReadResult<oasis7_runtime::service_admission::game_task::ValidatedRetentionHistory<'_>> {
    oasis7_runtime::service_admission::game_task::validate_retention_history(world)
        .map_err(FormalReadError::Integrity)
}
fn verify_owner(
    signed: &oasis7_client_api::world_service::SignedReadRequest<
        oasis7_agent_api::game::GameGoalPublishV1,
    >,
) -> FormalReadResult<()> {
    fn hex_bytes<const N: usize>(value: &str) -> FormalReadResult<[u8; N]> {
        let bytes = hex::decode(value)
            .map_err(|_| FormalReadError::Integrity("activation signature encoding".into()))?;
        if hex::encode(&bytes) != value {
            return Err(FormalReadError::Integrity(
                "activation signature encoding".into(),
            ));
        }
        bytes
            .try_into()
            .map_err(|_| FormalReadError::Integrity("activation signature length".into()))
    }
    let key = VerifyingKey::from_bytes(&hex_bytes::<32>(&signed.subject_public_key)?)
        .map_err(|_| FormalReadError::Integrity("activation Owner key".into()))?;
    let signature = Signature::from_bytes(&hex_bytes::<64>(&signed.signature_hex)?);
    let bytes =
        oasis7_client_api::world_service::signing_bytes(GAME_GOAL_PUBLISH_DOMAIN, &signed.request)
            .map_err(FormalReadError::Integrity)?;
    key.verify_strict(&bytes, &signature)
        .map_err(|_| FormalReadError::Integrity("activation Owner signature".into()))
}

impl GoalActivationArchiveMaterialV1 {
    /// These are candidate historical digests, not trusted pins. The durable
    /// ledger must acquire/retain them from the actual prepared producer.
    pub fn checkpoint_digests(&self) -> FormalReadResult<(String, String)> {
        Ok((
            self.before_checkpoint
                .digest()
                .map_err(|e| FormalReadError::Integrity(e.to_string()))?,
            self.after_checkpoint
                .digest()
                .map_err(|e| FormalReadError::Integrity(e.to_string()))?,
        ))
    }
    /// Strict cold consumer: the caller already restored independently pinned
    /// opaque publications. A JSON checkpoint or self-reported pin cannot be
    /// supplied in place of either publication.
    pub fn restore_against(
        self,
        before: &LocalQualifiedPublicationV1,
        after: &LocalQualifiedPublicationV1,
    ) -> FormalReadResult<PreparedGoalActivationArchiveV1> {
        let prepared = PreparedGoalActivationArchiveV1::from_adjacent(before, after)?;
        let original = oasis7_wasm_abi::encode_canonical_cbor(&self)
            .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
        let rebuilt = oasis7_wasm_abi::encode_canonical_cbor(&prepared.material)
            .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
        if original != rebuilt {
            return Err(FormalReadError::Integrity(
                "activation archive differs from actual verified cuts".into(),
            ));
        }
        Ok(prepared)
    }
}

/// Captured only from a successfully qualified publication. Cloning its bytes
/// is O(1); decoding and reducer validation belong to the archive worker.
pub struct GoalActivationArchiveCutV1 {
    pub(super) anchor: VerifiedFormalReadAnchorV1,
    pub(super) commit: CommitRef,
    pub(super) snapshot: std::sync::Arc<Vec<u8>>,
    // Supplied only by the upcoming actual Writer-owned qualified material.
    // Snapshot-only publication cannot manufacture the committed Journal.
    pub(super) committed_journal: Option<std::sync::Arc<Vec<u8>>>,
}
impl GoalActivationArchiveCutV1 {
    fn restore(self) -> FormalReadResult<LocalQualifiedPublicationV1> {
        let snapshot = oasis7_world_state::codec::snapshot_from_cbor(&self.snapshot)
            .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
        // Use strict actual Runtime restore, never a synthetic World or a
        // retained-history resolver that can silently introduce network I/O.
        let journal = self.committed_journal.ok_or_else(|| {
            FormalReadError::HistoryUnavailable(
                "actual qualified committed Journal material unavailable".into(),
            )
        })?;
        let expected = self
            .anchor
            .head()
            .record()
            .artifact_roots
            .get(&ArtifactRole::Journal)
            .ok_or_else(|| FormalReadError::Integrity("qualified Journal root missing".into()))?;
        if blake3_hex(&journal) != *expected {
            return Err(FormalReadError::Integrity(
                "qualified Journal bytes differ from actual artifact root".into(),
            ));
        }
        let journal: oasis7_world_state::Journal = serde_cbor::from_slice(&journal)
            .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
        let world = World::from_snapshot(snapshot, journal)
            .map_err(|e| FormalReadError::Integrity(format!("{e:?}")))?;
        let publication = FormalReadPublication::new(self.anchor, world)?;
        let cut = LocalQualifiedPublicationV1::from_publication(publication)?;
        if cut.commit != self.commit {
            return Err(FormalReadError::Integrity(
                "archive cut commit changed".into(),
            ));
        }
        Ok(cut)
    }
}

#[path = "resource_goal_activation_archive_queue_v1.rs"]
mod queue;
pub use queue::GoalActivationArchiveQueueV1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formal_read_fixtures as fixtures;

    #[test]
    fn activation_archive_captures_actual_qualified_bytes_without_read_grant() {
        let (world, _, anchor, _) = fixtures::actual_world_initial_fixture();
        let plain = LocalStopPublication::from_publication(
            FormalReadPublication::new(anchor.clone(), world.clone()).unwrap(),
        )
        .unwrap();
        assert!(plain.qualified_cut().archive_cut().is_err());
        let captured = LocalStopPublication::from_publication_with_archive(
            FormalReadPublication::new_with_archive(anchor.clone(), world).unwrap(),
        )
        .unwrap();
        let job = captured.qualified_cut().archive_cut().unwrap();
        assert_eq!(
            blake3_hex(&job.snapshot),
            anchor.head().record().after_state_root
        );
        assert_eq!(job.commit, *captured.commit());
        assert!(
            job.restore().is_err(),
            "snapshot-only capture cannot replace actual committed Journal"
        );
        assert!(
            PreparedGoalActivationArchiveV1::from_adjacent(
                plain.qualified_cut(),
                captured.qualified_cut()
            )
            .is_err()
        );
    }

    #[test]
    fn activation_archive_adjacent_actual_cut_cold_material_is_exact() {
        let (world, _, anchor, _) = fixtures::actual_world_initial_fixture();
        let before = LocalStopPublication::from_publication(
            FormalReadPublication::new(anchor.clone(), world.clone()).unwrap(),
        )
        .unwrap();
        let (next_world, next_anchor, _) = fixtures::actual_world_successor_fixture(&anchor, world);
        let after = LocalStopPublication::from_publication(
            FormalReadPublication::new(next_anchor, next_world).unwrap(),
        )
        .unwrap();
        let prepared = PreparedGoalActivationArchiveV1::from_adjacent(
            before.qualified_cut(),
            after.qualified_cut(),
        )
        .unwrap();
        // Empty actual transition must not manufacture an activation entry.
        assert!(prepared.material.results.is_empty());
        assert_eq!(&prepared.material.activation_commit, after.commit());
        let encoded = oasis7_wasm_abi::encode_canonical_cbor(&prepared.material).unwrap();
        let cold: GoalActivationArchiveMaterialV1 = serde_cbor::from_slice(&encoded).unwrap();
        assert!(
            cold.restore_against(before.qualified_cut(), after.qualified_cut())
                .is_ok()
        );
        let mut corrupt: GoalActivationArchiveMaterialV1 =
            serde_cbor::from_slice(&encoded).unwrap();
        corrupt.before_snapshot.push(0);
        assert!(
            corrupt
                .restore_against(before.qualified_cut(), after.qualified_cut())
                .is_err()
        );
    }
}
