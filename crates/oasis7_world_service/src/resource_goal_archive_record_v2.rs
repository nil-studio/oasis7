//! Full proof-chain historical activation records. No serialized checkpoint
//! digest is promoted to trust; cold verification starts at configured origin.
use super::qualified_material_v2::{CapturedResourcePublicationV2, StoredResourcePublicationV2};
use super::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const MAX_PUBLICATIONS: usize = 4096;
const MAX_ORIGINAL_BYTES: usize = 64 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicationEvidenceV2 {
    pub(super) commit: CommitRef,
    pub(super) original: OriginalBytesV2,
}
pub(super) struct OriginalBytesV2(pub(super) Vec<u8>);
impl Serialize for OriginalBytesV2 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.0)
    }
}
impl<'de> Deserialize<'de> for OriginalBytesV2 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Bytes;
        impl<'de> serde::de::Visitor<'de> for Bytes {
            type Value = OriginalBytesV2;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("bounded original publication bytes")
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                if v.is_empty() || v.len() > MAX_ORIGINAL_BYTES {
                    return Err(E::custom("publication byte bound"));
                }
                Ok(OriginalBytesV2(v.to_vec()))
            }
            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
                if v.is_empty() || v.len() > MAX_ORIGINAL_BYTES {
                    return Err(E::custom("publication byte bound"));
                }
                Ok(OriginalBytesV2(v))
            }
        }
        d.deserialize_byte_buf(Bytes)
    }
}
fn publications<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<PublicationEvidenceV2>, D::Error> {
    oasis7_formal_protocol::controlled_authority::replicated_protocol::package::bounded_vec::<
        D,
        PublicationEvidenceV2,
        MAX_PUBLICATIONS,
    >(d)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceGoalArchiveRecordV2 {
    api_version: u8,
    initial_certificate: SignedInitialActivationV1,
    #[serde(deserialize_with = "publications")]
    publications: Vec<PublicationEvidenceV2>,
    activation_commit: CommitRef,
    canonical_results: std::collections::BTreeMap<
        String,
        oasis7_client_api::legacy_world_service::CanonicalIntentResultV1,
    >,
}
/// Historical result only. Caller must independently authorize actual Current,
/// exact requested original and paid transport before using private goal data.
pub struct VerifiedResourceGoalArchiveV2 {
    activation_commit: CommitRef,
    canonical_results: std::collections::BTreeMap<
        String,
        oasis7_client_api::legacy_world_service::CanonicalIntentResultV1,
    >,
}
impl ResourceGoalArchiveRecordV2 {
    pub fn decode_strict(bytes: &[u8], maximum_bytes: usize) -> FormalReadResult<Self> {
        if bytes.is_empty() || bytes.len() > maximum_bytes {
            return Err(FormalReadError::Integrity(
                "activation record byte bound".into(),
            ));
        }
        let record: Self =
            serde_cbor::from_slice(bytes).map_err(|e| FormalReadError::Integrity(e.to_string()))?;
        if serde_cbor::to_vec(&record).map_err(|e| FormalReadError::Integrity(e.to_string()))?
            != bytes
        {
            return Err(FormalReadError::Integrity(
                "activation record noncanonical/unknown fields".into(),
            ));
        }
        Ok(record)
    }
    pub fn verify(
        self,
        trust: &InitialActivationTrustV1,
    ) -> FormalReadResult<VerifiedResourceGoalArchiveV2> {
        if self.api_version != 2 || self.publications.len() < 2 {
            return Err(FormalReadError::Integrity(
                "activation complete lineage/version".into(),
            ));
        }
        let mut previous: Option<LocalQualifiedPublicationV1> = None;
        let mut previous_anchor: Option<VerifiedFormalReadAnchorV1> = None;
        let mut final_transition = None;
        for (position, evidence) in self.publications.into_iter().enumerate() {
            let stored: StoredResourcePublicationV2 = serde_cbor::from_slice(&evidence.original.0)
                .map_err(|e| FormalReadError::Integrity(e.to_string()))?;
            let after = if position == 0 {
                if stored.proof.decision.record.kind != FormalRecordKindV1::InitialActivation {
                    return Err(FormalReadError::Integrity(
                        "activation proof chain lacks true initial publication".into(),
                    ));
                }
                VerifiedFormalReadAnchorV1::from_initial(
                    &self.initial_certificate,
                    trust,
                    &stored.proof,
                )
                .map_err(FormalReadError::from)?
            } else {
                let before = previous_anchor.as_ref().ok_or_else(|| {
                    FormalReadError::Integrity("activation predecessor missing".into())
                })?;
                if stored.proof.decision.branch != FormalCompletionBranchV1::Normal {
                    return Err(FormalReadError::HistoryUnavailable(
                        "complete activation governance/recovery chain unavailable".into(),
                    ));
                }
                let verified = verify_formal_qualified(
                    &stored.proof,
                    before.authority(),
                    Some(before.head()),
                    None,
                )
                .map_err(FormalReadError::from)?;
                before
                    .after_verified_commit(before.authority().clone(), verified)
                    .map_err(FormalReadError::from)?
            };
            let captured = CapturedResourcePublicationV2::from_proof_chain(
                previous_anchor.clone(),
                after.clone(),
                evidence.commit,
                Arc::new(evidence.original.0),
            );
            let next = captured.verify()?;
            if let Some(before) = previous.take() {
                final_transition = Some(PreparedGoalActivationArchiveV1::from_adjacent(
                    &before, &next.cut,
                )?);
            }
            previous_anchor = Some(after);
            previous = Some(next.cut);
        }
        let prepared = final_transition
            .ok_or_else(|| FormalReadError::Integrity("activation transition missing".into()))?;
        let (commit, results) = prepared.into_exact_facts();
        if results.is_empty()
            || commit != self.activation_commit
            || results != self.canonical_results
        {
            return Err(FormalReadError::Integrity(
                "activation record differs from actual final transition".into(),
            ));
        }
        Ok(VerifiedResourceGoalArchiveV2 {
            activation_commit: commit,
            canonical_results: results,
        })
    }
}
impl VerifiedResourceGoalArchiveV2 {
    pub fn activation_commit(&self) -> &CommitRef {
        &self.activation_commit
    }
    pub fn exact_result(
        &self,
        key: &str,
    ) -> Option<&oasis7_client_api::legacy_world_service::CanonicalIntentResultV1> {
        self.canonical_results.get(key)
    }
}

/// Bounded live stream accumulator. A gap or failure cannot seed a new origin
/// from a later current checkpoint. The persisted record retains real proof
/// bytes, not an assertion that somebody once verified them.
pub(super) struct ResourceArchiveCollectorV2 {
    previous: Option<LocalQualifiedPublicationV1>,
    initial: Option<SignedInitialActivationV1>,
    evidence: Vec<PublicationEvidenceV2>,
    original_bytes: u64,
    maximum_bytes: u64,
    maximum_publications: usize,
    healthy: bool,
}
impl ResourceArchiveCollectorV2 {
    pub(super) fn new(maximum_bytes: u64, maximum_publications: usize) -> FormalReadResult<Self> {
        if maximum_bytes == 0 || maximum_publications < 2 || maximum_publications > MAX_PUBLICATIONS
        {
            return Err(FormalReadError::Integrity(
                "archive proof stream bound".into(),
            ));
        }
        Ok(Self {
            previous: None,
            initial: None,
            evidence: Vec::new(),
            original_bytes: 0,
            maximum_bytes,
            maximum_publications,
            healthy: true,
        })
    }
    pub(super) fn accept(
        &mut self,
        input: CapturedResourcePublicationV2,
    ) -> FormalReadResult<Option<ResourceGoalArchiveRecordV2>> {
        if !self.healthy {
            return Err(FormalReadError::HistoryUnavailable(
                "archive stream requires complete lineage recovery".into(),
            ));
        }
        let result = self.accept_one(input);
        if result.is_err() {
            self.healthy = false;
        }
        result
    }
    fn accept_one(
        &mut self,
        input: CapturedResourcePublicationV2,
    ) -> FormalReadResult<Option<ResourceGoalArchiveRecordV2>> {
        let verified = input.verify()?;
        if let Some(before) = &verified.before {
            let previous = self.previous.as_ref().ok_or_else(|| {
                FormalReadError::HistoryUnavailable(
                    "archive true initial stream unavailable".into(),
                )
            })?;
            if before.head().head() != previous.anchor.head().head()
                || before
                    .authority()
                    .authority_id()
                    .map_err(FormalReadError::from)?
                    != previous
                        .anchor
                        .authority()
                        .authority_id()
                        .map_err(FormalReadError::from)?
                || before.initial_certificate() != previous.anchor.initial_certificate()
            {
                return Err(FormalReadError::Integrity(
                    "archive publication gap/fork".into(),
                ));
            }
        } else {
            if self.previous.is_some()
                || !self.evidence.is_empty()
                || verified.cut.anchor.head().record().kind != FormalRecordKindV1::InitialActivation
            {
                return Err(FormalReadError::Integrity(
                    "archive cannot replace captured initial origin".into(),
                ));
            }
            self.initial = Some(verified.cut.anchor.initial_certificate().clone());
        }
        let next = self
            .original_bytes
            .checked_add(
                u64::try_from(verified.original.len())
                    .map_err(|_| FormalReadError::Integrity("archive source size".into()))?,
            )
            .ok_or_else(|| FormalReadError::Integrity("archive source byte overflow".into()))?;
        if next > self.maximum_bytes || self.evidence.len() >= self.maximum_publications {
            return Err(FormalReadError::HistoryUnavailable(
                "archive complete lineage storage bound exhausted".into(),
            ));
        }
        let prepared = if let Some(before) = self.previous.as_ref() {
            Some(PreparedGoalActivationArchiveV1::from_adjacent(
                before,
                &verified.cut,
            )?)
        } else {
            None
        };
        self.evidence.push(PublicationEvidenceV2 {
            commit: verified.cut.commit.clone(),
            original: OriginalBytesV2((*verified.original).clone()),
        });
        self.original_bytes = next;
        self.previous = Some(verified.cut);
        let Some(prepared) = prepared else {
            return Ok(None);
        };
        let (commit, results) = prepared.into_exact_facts();
        if results.is_empty() {
            return Ok(None);
        }
        // This component's temporary ownership is bounded by maximum_bytes;
        // production persistence must share immutable proof files rather than
        // claim that repeatedly copying a prefix has no CPU/storage cost.
        let publications = self
            .evidence
            .iter()
            .map(|e| PublicationEvidenceV2 {
                commit: e.commit.clone(),
                original: OriginalBytesV2(e.original.0.clone()),
            })
            .collect();
        Ok(Some(ResourceGoalArchiveRecordV2 {
            api_version: 2,
            initial_certificate: self
                .initial
                .as_ref()
                .ok_or_else(|| {
                    FormalReadError::Integrity("archive initial certificate missing".into())
                })?
                .clone(),
            publications,
            activation_commit: commit,
            canonical_results: results,
        }))
    }
}
