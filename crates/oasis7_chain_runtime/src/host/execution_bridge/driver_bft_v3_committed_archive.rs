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

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ReplayedCommittedPrefixV1 {
    pub(crate) header_id: String,
    pub(crate) height: u64,
    pub(crate) durable_record_digest: String,
    pub(crate) world_tick: u64,
    pub(crate) semantic_state_root: String,
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

fn check_archive_capacity(object_bytes: u64, manifest_bytes: u64) -> Result<(), String> {
    if object_bytes.checked_add(manifest_bytes)
        .is_none_or(|total| total > MAX_COMMITTED_ARCHIVE_BYTES)
    {
        return Err("committed archive capacity including manifest".into());
    }
    Ok(())
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
    ) -> Result<(CommittedPrefixArchiveRefV1, ReplayedCommittedPrefixV1), String> {
        source_at_h(source, &self.origin.handoff)?;
        let cut = self.verified_cut(height, header_id)?;
        let original = ReplayedCommittedPrefixV1 {
            header_id: header_id.into(),
            height,
            durable_record_digest: cut.durable_record_digest().into(),
            world_tick: cut.world().state().time,
            semantic_state_root: cut.proof().commitment().state_root.clone(),
            result_root: cut.result_tree().root()?,
        };
        drop(cut);
        let prior = self.ledger.origin()?;
        let prefix = PrivateHeaderPrefixV3::from_entries(
            self.ledger.records_through(height)?.map(storage::Record::input).collect(),
        )?;
        let mut objects = vec![
            (blake3_hex(H_MATERIAL), encoded_h_material(&self.inputs.material)?, MAX_H_BYTES),
            (blake3_hex(LEDGER_ORIGIN), encode_origin(prior)?, MAX_ORIGIN_BYTES),
            (
                blake3_hex(PRIVATE_PREFIX),
                prefix.canonical_private_bytes()?,
                PRIVATE_HEADER_PREFIX_MAX_BYTES_V3,
            ),
        ];
        let nodes = self.origin.retained_nodes.accessed_nodes()?;
        if nodes.len() + objects.len()
            > oasis7_distfs::protected_bft_v3::BFT_SUCCESSOR_WINDOW_MAX_REFS_V1
        {
            return Err("committed archive dependency count capacity".into());
        }
        for (digest, bytes) in nodes {
            if digest != blake3_hex(&bytes) {
                return Err("committed archive dependency differs from original hash".into());
            }
            objects.push((digest, bytes, BUSINESS_INDEX_MAX_NODE_BYTES));
        }
        objects.sort_by(|left, right| left.0.cmp(&right.0));
        let entries = objects.iter().map(|(identity_digest, bytes, maximum)| {
            if bytes.is_empty() || bytes.len() > *maximum {
                return Err("committed archive object length outside its role bound".into());
            }
            Ok(BftSuccessorWindowArchiveEntryV1 {
                identity_digest: identity_digest.clone(),
                object: BftSuccessorWindowArchiveRefV1 {
                    digest: blake3_hex(bytes),
                    bytes: bytes.len() as u64,
                },
            })
        }).collect::<Result<Vec<_>, String>>()?;
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
        check_archive_capacity(
            manifest.usage.total_bytes,
            manifest.encode_canonical().map_err(error)?.len() as u64,
        )?;
        let root = manifest.digest().map_err(error)?;
        // Compute the complete manifest and all role/cumulative bounds before
        // creating staged objects; publish the reference only after every readback.
        for ((_, bytes, maximum), entry) in objects.iter().zip(&manifest.entries) {
            if archive.persist_object(bytes, *maximum).map_err(error)? != entry.object {
                return Err("committed archive published object differs from its manifest".into());
            }
        }
        let manifest = archive.persist_manifest(&manifest).map_err(error)?;
        Ok((CommittedPrefixArchiveRefV1 { manifest, root }, original))
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
        check_archive_capacity(manifest.usage.total_bytes, reference.manifest.bytes)?;
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
            semantic_state_root: proof.commitment().state_root.clone(),
            result_root: execution.parent_result_tree.root()?,
        })
    }
}
