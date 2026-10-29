//! Immutable private closure exported from actual durable wire-3 execution.
//! It is replay input, never a replacement for the protected minimum.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftSuccessorWindowArchiveEntryV1, BftSuccessorWindowArchiveKindV1,
    BftSuccessorWindowArchiveManifestV1, BftSuccessorWindowArchiveRefV1,
    BftSuccessorWindowArchiveUsageV1, BftSuccessorWindowCutV1, BftSuccessorWindowSourceV1,
    LocalBftSuccessorArchiveStoreV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Instant;

pub(super) const MAX_METADATA: usize = 1024 * 1024;
pub(super) const MAX_CLOSURE: usize = 512 * 1024 * 1024;
pub(super) const MAX_REFS: usize = 4096;
const DOMAIN: &str = "oasis7.formal.bft.successor-private-closure.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrivateClosureV1 {
    pub domain: String,
    pub source: BftSuccessorWindowSourceV1,
    pub private_instance_id: String,
    pub material: BftSuccessorWindowArchiveRefV1,
    pub origin: BftSuccessorWindowArchiveRefV1,
    pub public_state: BftSuccessorWindowArchiveRefV1,
    pub current_journal: Option<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub snapshots: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub journals: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub manifests: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub records: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub nodes: Vec<BftSuccessorWindowArchiveRefV1>,
    pub cut: BftSuccessorWindowCutV1,
}
fn bounded_refs<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BftSuccessorWindowArchiveRefV1>, D::Error> {
    struct Refs;
    impl<'de> serde::de::Visitor<'de> for Refs {
        type Value = Vec<BftSuccessorWindowArchiveRefV1>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded successor private closure references")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_REFS) {
                return Err(serde::de::Error::custom(
                    "successor private reference bound",
                ));
            }
            let mut refs = Vec::new();
            while let Some(reference) = seq.next_element()? {
                if refs.len() == MAX_REFS {
                    return Err(serde::de::Error::custom(
                        "successor private reference bound",
                    ));
                }
                refs.push(reference);
            }
            Ok(refs)
        }
    }
    deserializer.deserialize_seq(Refs)
}
impl PrivateClosureV1 {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.source.validate().map_err(err)?;
        self.cut.validate().map_err(err)?;
        if self.domain != DOMAIN
            || self.private_instance_id.is_empty()
            || self.private_instance_id.len() > 256
            || self.records.is_empty()
            || self.records.len() > 64
            || self.nodes.len() > MAX_REFS
            || self.snapshots.len() != self.records.len()
            || self.journals.len() != self.records.len()
            || self.manifests.len() != self.records.len()
        {
            return Err("successor private closure bounds/domain".into());
        }
        let mut total = 0u64;
        for reference in std::iter::once(&self.material)
            .chain(std::iter::once(&self.origin))
            .chain(std::iter::once(&self.public_state))
            .chain(self.current_journal.iter())
            .chain(&self.records)
            .chain(&self.nodes)
            .chain(&self.snapshots)
            .chain(&self.journals)
            .chain(&self.manifests)
        {
            reference.validate().map_err(err)?;
            total = total
                .checked_add(reference.bytes)
                .filter(|n| *n <= MAX_CLOSURE as u64)
                .ok_or("successor private cumulative capacity")?;
        }
        if !self.nodes.windows(2).all(|p| p[0].digest < p[1].digest) {
            return Err("successor private index references not sorted unique".into());
        }
        Ok(())
    }
}
/// Fields are private so only an actual durable exporter can mint this receipt.
pub(crate) struct ExportedBftSuccessorPrivateClosureV1 {
    manifest: BftSuccessorWindowArchiveManifestV1,
    manifest_ref: BftSuccessorWindowArchiveRefV1,
    metadata_ref: BftSuccessorWindowArchiveRefV1,
    public_state_ref: BftSuccessorWindowArchiveRefV1,
    cut: BftSuccessorWindowCutV1,
}
impl ExportedBftSuccessorPrivateClosureV1 {
    pub(crate) fn manifest(&self) -> &BftSuccessorWindowArchiveManifestV1 {
        &self.manifest
    }
    pub(crate) fn execution_manifest_ref(&self) -> &BftSuccessorWindowArchiveRefV1 {
        &self.manifest_ref
    }
    pub(crate) fn metadata_ref(&self) -> &BftSuccessorWindowArchiveRefV1 {
        &self.metadata_ref
    }
    pub(crate) fn public_state_ref(&self) -> &BftSuccessorWindowArchiveRefV1 {
        &self.public_state_ref
    }
    pub(crate) fn cut(&self) -> &BftSuccessorWindowCutV1 {
        &self.cut
    }
}
pub(super) fn metadata_identity() -> String {
    blake3_hex(DOMAIN.as_bytes())
}
pub(super) fn err(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}
pub(super) fn live(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("successor private closure deadline expired".into())
    } else {
        Ok(())
    }
}
pub(super) fn encode(value: &impl Serialize, max: usize) -> Result<Vec<u8>, String> {
    let bytes = serde_cbor::to_vec(value).map_err(err)?;
    if bytes.is_empty() || bytes.len() > max {
        return Err("successor private encoding bound".into());
    }
    Ok(bytes)
}
pub(super) fn encode_protocol(value: &impl Serialize, max: usize) -> Result<Vec<u8>, String> {
    let bytes = oasis7_cas::to_canonical_cbor(value).map_err(err)?;
    if bytes.is_empty() || bytes.len() > max {
        return Err("successor protocol encoding bound".into());
    }
    Ok(bytes)
}
pub(super) fn decode<T: serde::de::DeserializeOwned + Serialize>(
    bytes: &[u8],
    max: usize,
) -> Result<T, String> {
    if bytes.is_empty() || bytes.len() > max {
        return Err("successor private decode bound".into());
    }
    let value: T = serde_cbor::from_slice(bytes).map_err(err)?;
    if encode(&value, max)? != bytes {
        return Err("successor private noncanonical encoding".into());
    }
    Ok(value)
}
pub(super) fn install_digest(
    material: &str,
    record: &str,
    certificate: &FormalBftCertifiedHeaderV3,
) -> Result<String, String> {
    let c = &certificate.execution_commitment;
    Ok(blake3_hex(
        &serde_cbor::to_vec(&(
            "oasis7.formal.bft.node_install.v3",
            material,
            record,
            certificate.header.block_id()?,
            &certificate.header.execution_commitment_hash,
            &c.execution_state_root,
            &c.state_root,
            &c.post_manifest_hash,
        ))
        .map_err(err)?,
    ))
}

impl FormalBftHeaderLedgerExecutionV3 {
    pub(super) fn export_successor_private_closure(
        &mut self,
        archive: &LocalBftSuccessorArchiveStoreV1,
        source: &BftSuccessorWindowSourceV1,
        height: u64,
        header_id: &str,
        sequence: u64,
        predecessor_root: Option<String>,
        private_instance_id: &str,
        public_state: &oasis7_distfs::protected_bft_v3::BftSignerStateV3,
        previous_usage: BftSuccessorWindowArchiveUsageV1,
        deadline: Instant,
    ) -> Result<ExportedBftSuccessorPrivateClosureV1, String> {
        live(deadline)?;
        let cut = self.verified_cut(height, header_id)?;
        live(deadline)?;
        source.validate().map_err(err)?;
        if public_state.binding_digest != source.binding_digest
            || public_state.pending_driver_apply.is_some()
            || public_state.history.last() != Some(cut.proof().certificate().certified_header())
            || public_state
                .journal
                .as_ref()
                .is_some_and(|j| j.pending_commit().is_some())
        {
            return Err("successor private original S differs from installed cut".into());
        }
        let h = &self.origin.handoff;
        if source.scope != h.target().scope
            || source.handoff_position != h.handoff_position()
            || source.target_digest != h.target_digest()
            || source.activation_digest != h.activation_digest()
            || source.handoff_snapshot_root != h.raw_snapshot_root()
        {
            return Err("successor private source differs from original H".into());
        }
        if sequence
            != previous_usage
                .segments
                .checked_add(1)
                .ok_or("successor segment overflow")?
        {
            return Err("successor private sequence/usage mismatch".into());
        }
        previous_usage.validate().map_err(err)?;
        if previous_usage
            != public_state
                .successor_window
                .as_ref()
                .map(|s| s.usage)
                .unwrap_or_default()
        {
            return Err("successor export cannot reset original protected archive usage".into());
        }
        let public_bytes = encode_protocol(public_state, MAX_CLOSURE)?;
        let mut objects = BTreeMap::<String, BftSuccessorWindowArchiveRefV1>::new();
        let mut total = 0u64;
        let mut put =
            |bytes: &[u8], max: usize| -> Result<BftSuccessorWindowArchiveRefV1, String> {
                live(deadline)?;
                let digest = blake3_hex(bytes);
                if let Some(reference) = objects.get(&digest) {
                    return Ok(reference.clone());
                }
                if objects.len() >= MAX_REFS {
                    return Err("successor private object count".into());
                }
                total = total
                    .checked_add(bytes.len() as u64)
                    .filter(|n| *n <= MAX_CLOSURE as u64)
                    .ok_or("successor private archive capacity")?;
                let reference = archive.persist_object(bytes, max).map_err(err)?;
                if archive.load_object(&reference, max).map_err(err)? != bytes {
                    return Err("successor private fresh read differs".into());
                }
                live(deadline)?;
                objects.insert(digest, reference.clone());
                Ok(reference)
            };
        let current_journal = public_state
            .journal
            .as_ref()
            .map(|journal| put(&encode_protocol(journal, MAX_CLOSURE)?, MAX_CLOSURE))
            .transpose()?;
        let public_state = put(&public_bytes, MAX_CLOSURE)?;
        let material = put(&encoded_h_material(&self.inputs.material)?, MAX_H_BYTES)?;
        let origin = put(&encode(self.ledger.origin()?, MAX_METADATA)?, MAX_METADATA)?;
        let records = self
            .ledger
            .records_through(height)?
            .map(|r| {
                put(
                    &encode(r, super::super::super::formal_ledger::MAX_RECORD)?,
                    super::super::super::formal_ledger::MAX_RECORD,
                )
            })
            .collect::<Result<Vec<_>, String>>()?;
        let nodes = self
            .origin
            .retained_nodes
            .accessed_nodes()?
            .into_values()
            .map(|bytes| {
                put(
                    &bytes,
                    super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                )
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut nodes = nodes;
        nodes.sort_by(|a, b| a.digest.cmp(&b.digest));
        let seed = self
            .origin
            .retained_nodes
            .accessed_nodes()?
            .into_iter()
            .collect();
        let mut replay = FormalBftHeaderExecutionV3::from_handoff(
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
            &self.inputs.material,
            self.inputs.source.clone(),
            seed,
            true,
        )?;
        let mut snapshots = Vec::new();
        let mut journals = Vec::new();
        let mut manifests = Vec::new();
        for record in self.ledger.records_through(height)? {
            live(deadline)?;
            let entry = record.input();
            let executed = replay.replay_next(&entry.certificate, &entry.actions_cbor)?;
            let snapshot = put(&executed.snapshot_bytes, MAX_CLOSURE)?;
            let journal = put(&executed.journal_bytes, MAX_CLOSURE)?;
            if snapshot.digest != record.snapshot_ref() || journal.digest != record.journal_ref() {
                return Err("successor reexecution differs from original Snapshot/Journal".into());
            }
            manifests.push(put(
                &encode(executed.world.manifest(), MAX_CLOSURE)?,
                MAX_CLOSURE,
            )?);
            snapshots.push(snapshot);
            journals.push(journal);
        }
        let snapshot_ref = snapshots.last().ok_or("successor empty cut")?.clone();
        let journal_ref = journals.last().ok_or("successor empty cut")?.clone();
        let certificate = cut.proof().certificate().certified_header();
        let c = &certificate.execution_commitment;
        let cut_body = BftSuccessorWindowCutV1 {
            height,
            header_id: header_id.into(),
            certificate_digest: blake3_hex(&certificate.canonical_bytes()?),
            execution_commitment_digest: c.digest()?,
            snapshot_ref,
            journal_ref,
            semantic_state_root: c.state_root.clone(),
            post_manifest_hash: c.post_manifest_hash.clone(),
            business_result_root: c.result_root.clone(),
            driver_record_digest: cut.durable_record_digest().into(),
            driver_install_digest: install_digest(
                &source.material_identity,
                cut.durable_record_digest(),
                certificate,
            )?,
        };
        if cut_body.snapshot_ref.digest != c.execution_state_root {
            return Err("successor private Snapshot differs from committed original".into());
        }
        let metadata = PrivateClosureV1 {
            domain: DOMAIN.into(),
            source: source.clone(),
            private_instance_id: private_instance_id.into(),
            material,
            origin,
            public_state: public_state.clone(),
            current_journal,
            snapshots,
            journals,
            manifests,
            records,
            nodes,
            cut: cut_body.clone(),
        };
        metadata.validate()?;
        let metadata_ref = put(&encode(&metadata, MAX_METADATA)?, MAX_METADATA)?;
        drop(put);
        let segment_bytes = total
            .checked_add(MAX_METADATA as u64)
            .filter(|n| *n <= MAX_CLOSURE as u64)
            .ok_or("successor private segment capacity including public manifest reserve")?;
        let base_bytes = previous_usage
            .total_bytes
            .checked_add(segment_bytes)
            // Persistently charge one bounded public manifest too. This is a
            // conservative reserve, not an exact physical-byte measurement.
            .ok_or("successor archive usage overflow")?;
        let object_count = previous_usage
            .objects
            .checked_add(objects.len() as u64)
            .and_then(|n| n.checked_add(2)) // execution manifest + reserved public manifest
            .ok_or("successor object usage overflow")?;
        let mut manifest = BftSuccessorWindowArchiveManifestV1 {
            schema_version: 1,
            source: source.clone(),
            kind: BftSuccessorWindowArchiveKindV1::Execution,
            segment_sequence: sequence,
            predecessor_root,
            first_height: self.ledger.origin()?.first_height(),
            last_height: height,
            entries: objects
                .into_values()
                .map(|object| BftSuccessorWindowArchiveEntryV1 {
                    identity_digest: if object == metadata_ref {
                        metadata_identity()
                    } else {
                        object.digest.clone()
                    },
                    object,
                })
                .collect(),
            usage: BftSuccessorWindowArchiveUsageV1 {
                segments: sequence,
                objects: object_count,
                total_bytes: base_bytes,
            },
        };
        manifest
            .entries
            .sort_by(|a, b| a.identity_digest.cmp(&b.identity_digest));
        // Only usage.total_bytes depends on the manifest's own encoded length.
        // CBOR integer lengths grow monotonically; require an actual fixed point.
        let mut stable = false;
        for _ in 0..16 {
            let length = encode_protocol(&manifest, MAX_METADATA)?.len() as u64;
            let bytes = base_bytes
                .checked_add(length)
                .ok_or("successor manifest usage overflow")?;
            if bytes == manifest.usage.total_bytes {
                stable = true;
                break;
            }
            manifest.usage.total_bytes = bytes;
        }
        if !stable {
            return Err("successor manifest accounting did not converge".into());
        }
        manifest.validate().map_err(err)?;
        let bytes = encode_protocol(&manifest, MAX_METADATA)?;
        if segment_bytes
            .checked_add(bytes.len() as u64)
            .is_none_or(|n| n > MAX_CLOSURE as u64)
        {
            return Err("successor private segment exceeds capacity with both manifests".into());
        }
        let manifest_ref = archive.persist_object(&bytes, MAX_METADATA).map_err(err)?;
        if archive
            .load_object(&manifest_ref, MAX_METADATA)
            .map_err(err)?
            != bytes
        {
            return Err("successor manifest fresh read differs".into());
        }
        live(deadline)?;
        Ok(ExportedBftSuccessorPrivateClosureV1 {
            manifest,
            manifest_ref,
            metadata_ref,
            public_state_ref: public_state,
            cut: cut_body,
        })
    }
}
