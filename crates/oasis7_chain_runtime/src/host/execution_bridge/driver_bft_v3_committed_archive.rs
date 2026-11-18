//! Local transport of the committed part of the original successor closure.
//! The retained pre-signing candidates and their authority have a separate path.
use super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES;
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftSuccessorWindowArchiveEntryV1, BftSuccessorWindowArchiveKindV1,
    BftSuccessorWindowArchiveManifestV1, BftSuccessorWindowArchiveRefV1,
    BftSuccessorWindowArchiveUsageV1, BftSuccessorWindowSourceV1, LocalBftSuccessorArchiveStoreV1,
};

const MAX_ORIGIN_BYTES: usize = 1024 * 1024;
const MAX_COMMITTED_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const H_MATERIAL: &[u8] = b"oasis7.formal.bft.committed-prefix-archive.v1.h-material";
const LEDGER_ORIGIN: &[u8] = b"oasis7.formal.bft.committed-prefix-archive.v1.ledger-origin";
const PRIVATE_PREFIX: &[u8] = b"oasis7.formal.bft.committed-prefix-archive.v1.private-prefix";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommittedPrefixArchiveRefV1 {
    pub(crate) manifest: BftSuccessorWindowArchiveRefV1,
    pub(crate) root: String,
}

#[derive(Serialize)]
pub(crate) struct ReplayedCommittedPrefixV1 {
    pub(crate) header_id: String,
    pub(crate) height: u64,
    pub(crate) durable_record_digest: String,
    pub(crate) world_tick: u64,
    pub(crate) result_root: String,
}

struct ArchiveOnlyReader;

impl BusinessIndexNodeReader for ArchiveOnlyReader {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Err("committed prefix dependency is absent from the archive".into())
    }
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn source_at_h(
    source: &BftSuccessorWindowSourceV1,
    handoff: &VerifiedBftHandoffTargetV2,
) -> Result<(), String> {
    source.validate()?;
    if source.scope != handoff.target().scope
        || source.handoff_position != handoff.handoff_position()
        || source.handoff_record_id != handoff.handoff_head().record_id
        || source.handoff_snapshot_root != handoff.raw_snapshot_root()
        || source.target_digest != handoff.target_digest()
        || source.activation_digest != handoff.activation_digest()
    {
        return Err("committed prefix source differs from independently qualified H".into());
    }
    Ok(())
}

fn encode_origin(origin: &storage::Origin) -> Result<Vec<u8>, String> {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(origin).map_err(error)?;
    if bytes.len() > MAX_ORIGIN_BYTES {
        return Err("committed archive origin byte capacity".into());
    }
    Ok(bytes)
}

fn charge(entries: &[BftSuccessorWindowArchiveEntryV1]) -> Result<BftSuccessorWindowArchiveUsageV1, String> {
    let mut unique = std::collections::BTreeMap::new();
    let mut total_bytes = 0u64;
    for entry in entries {
        if let Some(bytes) = unique.insert(entry.object.digest.as_str(), entry.object.bytes) {
            if bytes != entry.object.bytes {
                return Err("committed archive object has conflicting sizes".into());
            }
        } else {
            total_bytes = total_bytes
                .checked_add(entry.object.bytes)
                .filter(|size| *size <= MAX_COMMITTED_ARCHIVE_BYTES)
                .ok_or("committed archive object byte capacity")?;
        }
    }
    Ok(BftSuccessorWindowArchiveUsageV1 {
        segments: 1,
        objects: unique.len() as u64,
        total_bytes,
    })
}

impl FormalBftHeaderLedgerExecutionV3 {
    /// Export committed records from an actual durable H-to-cut replay.
    /// The explicitly pinned source describes this local archive, not signer authority.
    pub(crate) fn export_committed_prefix(
        &mut self,
        archive: &LocalBftSuccessorArchiveStoreV1,
        source: &BftSuccessorWindowSourceV1,
        height: u64,
        header_id: &str,
    ) -> Result<CommittedPrefixArchiveRefV1, String> {
        source_at_h(source, &self.origin.handoff)?;
        self.verified_cut(height, header_id)?;
        let prior = self.ledger.origin()?;
        let prefix = PrivateHeaderPrefixV3::from_entries(
            self.ledger.records_through(height)?.map(storage::Record::input).collect(),
        )?;
        let mut entries = Vec::new();
        for (domain, bytes, max) in [
            (H_MATERIAL, encoded_h_material(&self.inputs.material)?, MAX_H_BYTES),
            (LEDGER_ORIGIN, encode_origin(prior)?, MAX_ORIGIN_BYTES),
            (
                PRIVATE_PREFIX,
                prefix.canonical_private_bytes()?,
                PRIVATE_HEADER_PREFIX_MAX_BYTES_V3,
            ),
        ] {
            let object = archive.persist_object(&bytes, max).map_err(error)?;
            entries.push(BftSuccessorWindowArchiveEntryV1 {
                identity_digest: blake3_hex(domain),
                object,
            });
        }
        let nodes = self.origin.retained_nodes.accessed_nodes()?;
        if nodes.len() + entries.len()
            > oasis7_distfs::protected_bft_v3::BFT_SUCCESSOR_WINDOW_MAX_REFS_V1
        {
            return Err("committed archive dependency count capacity".into());
        }
        for (digest, bytes) in nodes {
            let object = archive
                .persist_object(&bytes, BUSINESS_INDEX_MAX_NODE_BYTES)
                .map_err(error)?;
            if digest != object.digest {
                return Err("committed archive dependency differs from original hash".into());
            }
            entries.push(BftSuccessorWindowArchiveEntryV1 {
                identity_digest: digest,
                object,
            });
        }
        entries.sort_by(|left, right| left.identity_digest.cmp(&right.identity_digest));
        let manifest = BftSuccessorWindowArchiveManifestV1 {
            public_index: None,
            schema_version: 1,
            source: source.clone(),
            kind: BftSuccessorWindowArchiveKindV1::Execution,
            segment_sequence: 1,
            predecessor_root: None,
            first_height: prior.first_height(),
            last_height: height,
            usage: charge(&entries)?,
            entries,
        };
        let root = manifest.digest().map_err(error)?;
        let manifest = archive.persist_manifest(&manifest).map_err(error)?;
        Ok(CommittedPrefixArchiveRefV1 { manifest, root })
    }

    pub(crate) fn replay_committed_prefix(
        archive: &LocalBftSuccessorArchiveStoreV1,
        reference: &CommittedPrefixArchiveRefV1,
        source: &BftSuccessorWindowSourceV1,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        height: u64,
        header_id: &str,
    ) -> Result<ReplayedCommittedPrefixV1, String> {
        let handoff = verify_formal_bft_handoff_v2(&anchor, target.clone())?;
        source_at_h(source, &handoff)?;
        let manifest = archive
            .load_manifest(&reference.manifest, &reference.root)
            .map_err(error)?;
        if manifest.source != *source
            || manifest.kind != BftSuccessorWindowArchiveKindV1::Execution
            || manifest.first_height != handoff.activation_height()?
            || manifest.last_height != height
            || manifest.segment_sequence != 1
            || manifest.usage != charge(&manifest.entries)?
        {
            return Err("committed archive manifest differs from its pinned source or cut".into());
        }
        let mut entries = manifest
            .entries
            .into_iter()
            .map(|entry| (entry.identity_digest, entry.object))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut load = |domain: &[u8], max| -> Result<Vec<u8>, String> {
            let object = entries.remove(&blake3_hex(domain)).ok_or("committed archive role absent")?;
            archive.load_object(&object, max).map_err(error)
        };
        let material_bytes = load(H_MATERIAL, MAX_H_BYTES)?;
        let material: ClosedRecord = serde_cbor::from_slice(&material_bytes).map_err(error)?;
        if encoded_h_material(&material)? != material_bytes {
            return Err("committed archive original H material is not canonical".into());
        }
        let origin_bytes = load(LEDGER_ORIGIN, MAX_ORIGIN_BYTES)?;
        let origin: storage::Origin = serde_cbor::from_slice(&origin_bytes).map_err(error)?;
        origin.validate()?;
        if encode_origin(&origin)? != origin_bytes || origin.first_height() != manifest.first_height {
            return Err("committed archive original ledger origin differs".into());
        }
        let prefix = PrivateHeaderPrefixV3::decode_private_canonical(&load(
            PRIVATE_PREFIX,
            PRIVATE_HEADER_PREFIX_MAX_BYTES_V3,
        )?)?;
        let proof = prefix.public_prefix()?.verify(&handoff)?;
        check_target(&proof, height, header_id)?;
        let seeds = entries
            .iter()
            .map(|(digest, object)| {
                if digest != &object.digest {
                    return Err("committed archive index dependency hash differs".into());
                }
                archive
                    .load_object(object, BUSINESS_INDEX_MAX_NODE_BYTES)
                    .map(|bytes| (digest.clone(), bytes))
                    .map_err(error)
            })
            .collect::<Result<_, String>>()?;
        let mut execution = FormalBftHeaderExecutionV3::from_handoff(
            anchor,
            target,
            &material,
            Arc::new(ArchiveOnlyReader),
            seeds,
            true,
        )?;
        if material_pin_digest(&execution, &material)? != origin.material_pin_digest() {
            return Err("committed archive H material differs from the original ledger pin".into());
        }
        let mut previous = origin.digest()?;
        for entry in prefix.entries() {
            let executed = execution.replay_next(&entry.certificate, &entry.actions_cbor)?;
            let dependencies = execution.retained_nodes.accessed_nodes()?;
            let record = storage::Record::from_actual(
                &origin,
                &previous,
                entry,
                &executed,
                dependencies.keys().cloned().collect(),
            )?;
            previous = record.digest()?;
        }
        if execution
            .retained_nodes
            .accessed_nodes()?
            .keys()
            .ne(entries.keys())
        {
            return Err("committed archive contains missing or unobserved dependencies".into());
        }
        Ok(ReplayedCommittedPrefixV1 {
            header_id: header_id.into(),
            height,
            durable_record_digest: previous,
            world_tick: execution.parent_world.state().time,
            result_root: execution.parent_result_tree.root()?,
        })
    }
}
