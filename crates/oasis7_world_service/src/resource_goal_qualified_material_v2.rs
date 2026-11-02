//! Actual Writer-owned publication decoding. A wire DTO cannot provide either
//! opaque anchor. This carries facts only, never Owner/Current/fee authority.
use super::*;
use oasis7_formal_protocol::controlled_authority::{
    formal::business_index::{BusinessIndexNodeReader, BusinessIndexNodeWriteV1},
    formal::business_material::FormalBusinessResultEnvelopeV1,
    replicated_protocol::{ArtifactRole, ClosedRecord},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

/// The exact existing three-field private Writer publication envelope. Its
/// candidate carrier is the shared original Protocol type, not a second codec.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredResourcePublicationV2 {
    pub(super) schema_version: u32,
    pub(super) candidate: FormalCandidateMaterialV1,
    pub(super) proof: FormalQualifiedCommitProofV1,
}
/// Raw owned handoff only, not a verified cut. There is no wire constructor,
/// Clone, Deserialize, Debug, caller Current or accepted/payed boolean.
pub struct CapturedResourcePublicationV2 {
    before: Option<VerifiedFormalReadAnchorV1>,
    after: VerifiedFormalReadAnchorV1,
    commit: CommitRef,
    original: Arc<Vec<u8>>,
}
impl LocalQualifiedPublicationV1 {
    /// Cheap capture from this actual validated cut, before any publication
    /// mutex. All parsing, hashes, Runtime restore and storage happen elsewhere.
    pub fn capture_original_material(
        &self,
        before: Option<VerifiedFormalReadAnchorV1>,
        after: VerifiedFormalReadAnchorV1,
        original: Arc<Vec<u8>>,
    ) -> FormalReadResult<CapturedResourcePublicationV2> {
        if after.head().head() != self.anchor.head().head()
            || after
                .authority()
                .authority_id()
                .map_err(FormalReadError::from)?
                != self
                    .anchor
                    .authority()
                    .authority_id()
                    .map_err(FormalReadError::from)?
            || after.head().record() != self.anchor.head().record()
        {
            return Err(FormalReadError::Integrity(
                "owned material differs from actual publication".into(),
            ));
        }
        Ok(CapturedResourcePublicationV2 {
            before,
            after,
            commit: self.commit.clone(),
            original,
        })
    }
}
pub(super) struct VerifiedResourcePublicationV2 {
    pub(super) before: Option<VerifiedFormalReadAnchorV1>,
    pub(super) cut: LocalQualifiedPublicationV1,
    pub(super) original: Arc<Vec<u8>>,
}
impl CapturedResourcePublicationV2 {
    // Cold path only: both anchors have just been cryptographically rebuilt
    // from configured origin and the complete preceding proof sequence.
    pub(super) fn from_proof_chain(
        before: Option<VerifiedFormalReadAnchorV1>,
        after: VerifiedFormalReadAnchorV1,
        commit: CommitRef,
        original: Arc<Vec<u8>>,
    ) -> Self {
        Self {
            before,
            after,
            commit,
            original,
        }
    }
    pub fn original_len(&self) -> usize {
        self.original.len()
    }
    pub(super) fn verify(self) -> FormalReadResult<VerifiedResourcePublicationV2> {
        if self.original.is_empty() || self.original.len() > 64 * 1024 * 1024 {
            return Err(FormalReadError::Integrity(
                "owned publication byte bound".into(),
            ));
        }
        let stored: StoredResourcePublicationV2 = serde_cbor::from_slice(&self.original)
            .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
        if serde_cbor::to_vec(&stored).map_err(|e| FormalReadError::Integrity(e.to_string()))?
            != *self.original
        {
            return Err(FormalReadError::Integrity(
                "owned publication is not exact closed codec".into(),
            ));
        }
        let record = self.after.head().record();
        if stored.schema_version != 1
            || &stored.candidate.candidate.record != record
            || stored.proof.decision.record != *record
            || stored.candidate.branch != stored.proof.decision.branch
            || stored.candidate.frozen != stored.proof.decision.frozen
        {
            return Err(FormalReadError::Integrity(
                "owned publication canonical identity".into(),
            ));
        }
        // Recovery/governance proof bundles require their actual additional
        // verified source. Do not infer them from a current checkpoint.
        if stored.proof.decision.branch != FormalCompletionBranchV1::Normal {
            return Err(FormalReadError::HistoryUnavailable(
                "complete owned recovery lineage unavailable".into(),
            ));
        }
        let authority = if let Some(before) = &self.before {
            if before.head().record().world != record.world
                || before.head().head().record_id != record.parent_record_id
                || before.head().record().position.checked_add(1) != Some(record.position)
                || before.head().record().after_state_root != record.before_state_root
                || before.authority().definition() != self.after.authority().definition()
                || before.authority().policy() != self.after.authority().policy()
                || before.authority().activation_ref() != self.after.authority().activation_ref()
            {
                return Err(FormalReadError::Integrity(
                    "owned publication exact adjacent authority/cut".into(),
                ));
            }
            before.authority()
        } else {
            if record.kind != FormalRecordKindV1::InitialActivation {
                return Err(FormalReadError::HistoryUnavailable(
                    "owned stream has no captured initial lineage".into(),
                ));
            }
            self.after.authority()
        };
        stored
            .candidate
            .candidate
            .verify_original(authority)
            .map_err(FormalReadError::from)?;
        let verified = verify_formal_qualified(
            &stored.proof,
            authority,
            self.before.as_ref().map(|v| v.head()),
            None,
        )
        .map_err(FormalReadError::from)?;
        if verified.head() != self.after.head().head()
            || verified.finalizing_authority_id() != self.after.head().finalizing_authority_id()
        {
            return Err(FormalReadError::Integrity(
                "owned publication verified result differs from installed head".into(),
            ));
        }
        verify_record_material(record, &stored.candidate.material)
            .map_err(FormalReadError::from)?;
        match (&stored.candidate.frozen, &stored.candidate.frozen_material) {
            (Some(certificate), Some(material)) => {
                verify_frozen_material(&certificate.reservation.body, material)
                    .map_err(FormalReadError::from)?
            }
            (None, None) if record.kind == FormalRecordKindV1::InitialActivation => {}
            _ => {
                return Err(FormalReadError::Integrity(
                    "owned publication missing exact Frozen material".into(),
                ));
            }
        }
        let snapshot_bytes = stored
            .candidate
            .material
            .snapshot_bytes()
            .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
        if blake3_hex(&snapshot_bytes) != record.after_state_root {
            return Err(FormalReadError::Integrity("owned Snapshot root".into()));
        }
        let journal_bytes = role(&stored.candidate.material, ArtifactRole::Journal)?;
        if record.artifact_roots.get(&ArtifactRole::Journal) != Some(&blake3_hex(journal_bytes)) {
            return Err(FormalReadError::Integrity("owned Journal root".into()));
        }
        let snapshot = oasis7_world_state::codec::snapshot_from_cbor(&snapshot_bytes)
            .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
        let journal = oasis7_world_state::codec::journal_from_cbor(journal_bytes)
            .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
        let world = if self.after.authority().definition().profile_version == 2 {
            let parent = oasis7_runtime::VerifiedFormalRetainedHistoryParentV1::verify(
                &self.after,
                &snapshot_bytes,
            )
            .map_err(FormalReadError::Integrity)?;
            let reader = Arc::new(LocalObjects::from_material(&stored.candidate.material)?);
            World::from_verified_formal_snapshot(snapshot, journal, parent, reader)
        } else {
            World::from_snapshot(snapshot, journal)
        }
        .map_err(|e| FormalReadError::Integrity(format!("{e:?}")))?;
        if self.after.authority().definition().profile_version == 2 {
            let envelope = FormalBusinessResultEnvelopeV1::decode(role(
                &stored.candidate.material,
                ArtifactRole::Result,
            )?)
            .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
            let tree = crate::formal_result::FormalBusinessResultTreeV1::decode_cbor(
                &envelope.tree_descriptor_cbor,
            )
            .map_err(FormalReadError::Integrity)?;
            tree.validate_world_boundary(&world, record)
                .map_err(FormalReadError::Integrity)?;
            if envelope
                .delta
                .new
                .canonical_result_root()
                .map_err(|e| FormalReadError::Integrity(e.to_string()))?
                != tree.root().map_err(FormalReadError::Integrity)?
            {
                return Err(FormalReadError::Integrity(
                    "owned publication business descriptor root".into(),
                ));
            }
        }
        let publication = FormalReadPublication::new(self.after, world)?;
        let cut = LocalQualifiedPublicationV1::from_publication(publication)?;
        if cut.commit != self.commit {
            return Err(FormalReadError::Integrity(
                "owned material CommitRef differs from publication".into(),
            ));
        }
        Ok(VerifiedResourcePublicationV2 {
            before: self.before,
            cut,
            original: self.original,
        })
    }
}
fn role(material: &ClosedRecord, role: ArtifactRole) -> FormalReadResult<&[u8]> {
    let hash = material
        .roots
        .get(&role)
        .ok_or_else(|| FormalReadError::Integrity("owned material role missing".into()))?;
    material
        .objects
        .iter()
        .find(|o| &o.content_hash == hash)
        .map(|o| o.bytes.as_slice())
        .ok_or_else(|| FormalReadError::Integrity("owned material role object missing".into()))
}
struct LocalObjects(BTreeMap<String, Vec<u8>>);
impl LocalObjects {
    fn from_material(material: &ClosedRecord) -> FormalReadResult<Self> {
        let mut values = BTreeMap::new();
        for object in &material.objects {
            values.insert(object.content_hash.clone(), object.bytes.clone());
        }
        if let Ok(envelope) =
            FormalBusinessResultEnvelopeV1::decode(role(material, ArtifactRole::Result)?)
        {
            for node in envelope.delta.node_writes.into_iter().chain(
                envelope
                    .retained_history_delta
                    .into_iter()
                    .flat_map(|d| d.node_writes),
            ) {
                install_node(&mut values, node)?;
            }
        }
        Ok(Self(values))
    }
}
fn install_node(
    values: &mut BTreeMap<String, Vec<u8>>,
    node: BusinessIndexNodeWriteV1,
) -> FormalReadResult<()> {
    if blake3_hex(&node.bytes) != node.digest {
        return Err(FormalReadError::Integrity(
            "owned local history node hash".into(),
        ));
    }
    if let Some(old) = values.insert(node.digest, node.bytes.clone())
        && old != node.bytes
    {
        return Err(FormalReadError::Integrity(
            "owned local history node collision".into(),
        ));
    }
    Ok(())
}
impl BusinessIndexNodeReader for LocalObjects {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(digest).cloned())
    }
    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.read_node(digest)
    }
}
