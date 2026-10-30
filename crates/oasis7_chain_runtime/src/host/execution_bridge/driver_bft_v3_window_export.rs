//! Export only this bounded active suffix and its immutable predecessor dependency.
use super::super::successor_archive::*;
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftSignerStateV3, BftSuccessorWindowArchiveEntryV1, BftSuccessorWindowArchiveKindV1,
    BftSuccessorWindowArchiveManifestV1, BftSuccessorWindowArchiveRefV1,
    BftSuccessorWindowArchiveUsageV1, BftSuccessorWindowCutV1, BftSuccessorWindowSourceV1,
    LocalBftSuccessorArchiveStoreV1,
};
use std::collections::BTreeMap;

impl FormalBftHeaderActiveExecutionV1 {
    pub(crate) fn export_successor_private_closure(
        &mut self,
        archive: &LocalBftSuccessorArchiveStoreV1,
        source: &BftSuccessorWindowSourceV1,
        public_state: &BftSignerStateV3,
        deadline: Instant,
    ) -> Result<ExportedBftSuccessorPrivateClosureV1, String> {
        self.reset(deadline)?;
        if source != &self.base.metadata.source {
            return Err("active export original source changed".into());
        }
        let original = self
            .base
            .original
            .as_ref()
            .ok_or("active predecessor original absent")?;
        let predecessor_checkpoint = original
            .request
            .intent
            .body
            .checkpoint
            .digest()
            .map_err(err)?;
        let predecessor_root = Some(
            original
                .request
                .intent
                .body
                .checkpoint
                .execution_archive_root
                .clone(),
        );
        if public_state
            .active_window
            .as_ref()
            .map(|w| w.checkpoint_digest.as_str())
            != Some(predecessor_checkpoint.as_str())
        {
            return Err("active export public State differs from private base".into());
        }
        let previous_usage = public_state
            .successor_window
            .as_ref()
            .ok_or("active export usage absent")?
            .usage;
        previous_usage.validate().map_err(err)?;
        let sequence = previous_usage
            .segments
            .checked_add(1)
            .ok_or("active export sequence overflow")?;
        let private_instance_id = self.base.metadata.private_instance_id.clone();
        let private_instance_id = private_instance_id.as_str();
        let certificate = self
            .execution
            .tip
            .as_ref()
            .ok_or("active export has no new actual cut")?
            .certified_header()
            .clone();
        let height = certificate.execution_commitment.height;
        let header_id = certificate.header.block_id()?;
        if public_state.binding_digest != source.binding_digest
            || public_state.pending_driver_apply.is_some()
            || public_state.history.last() != Some(&certificate)
            || public_state.history.iter().ne(self
                .ledger
                .records()
                .map(|r| r.input().certificate)
                .collect::<Vec<_>>()
                .iter())
            || public_state
                .journal
                .as_ref()
                .is_some_and(|j| j.pending_commit().is_some())
        {
            return Err("active export original public history/cut differs".into());
        }
        let record_digest = self
            .ledger
            .records()
            .last()
            .ok_or("active export record absent")?
            .digest()?;
        let mut required = required_pending(public_state.journal.as_ref())?;
        if let Some(local) = self.private.candidate(self.execution.schedule.height())? {
            required
                .entry(local.public_value.block_id()?)
                .or_default()
                .insert(PendingRoleV1::RetainedLocal);
        }
        let pending_packages = required
            .into_iter()
            .map(|(id, roles)| {
                Ok((
                    roles,
                    self.private
                        .package_for_header(self.execution.schedule.height(), &id)?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let public_bytes = encode_protocol(public_state, MAX_CLOSURE)?;
        let mut objects = BTreeMap::<String, BftSuccessorWindowArchiveRefV1>::new();
        let mut total = 0u64;
        let (metadata, metadata_ref, public_state, cut_body) = {
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
            let material_bytes = archive
                .load_object(&self.base.metadata.material, MAX_H_BYTES)
                .map_err(err)?;
            let material = put(&material_bytes, MAX_H_BYTES)?;
            let origin = put(&encode(&self.base.origin, MAX_METADATA)?, MAX_METADATA)?;
            let window_origin = put(&encode(self.ledger.origin()?, MAX_METADATA)?, MAX_METADATA)?;
            let records = self
                .ledger
                .records_through(height)?
                .map(|r| {
                    put(
                        &encode(r, super::super::super::super::formal_ledger::MAX_RECORD)?,
                        super::super::super::super::formal_ledger::MAX_RECORD,
                    )
                })
                .collect::<Result<Vec<_>, String>>()?;
            let nodes = self
                .execution
                .retained_nodes
                .accessed_nodes()?
                .into_values()
                .map(|bytes| {
                    put(
                    &bytes,
                    super::super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                )
                })
                .collect::<Result<Vec<_>, String>>()?;
            let mut nodes = nodes;
            nodes.sort_by(|a, b| a.digest.cmp(&b.digest));
            let seed = self
                .execution
                .retained_nodes
                .accessed_nodes()?
                .into_iter()
                .collect();
            let mut replay = self.base.execution.fork_strict_with_seed(seed)?;
            let mut snapshots = Vec::new();
            let mut journals = Vec::new();
            let mut manifests = Vec::new();
            for record in self.ledger.records_through(height)? {
                live(deadline)?;
                let entry = record.input();
                let executed = replay.replay_next(&entry.certificate, &entry.actions_cbor)?;
                let snapshot = put(&executed.snapshot_bytes, MAX_CLOSURE)?;
                let journal = put(&executed.journal_bytes, MAX_CLOSURE)?;
                if snapshot.digest != record.snapshot_ref()
                    || journal.digest != record.journal_ref()
                {
                    return Err(
                        "successor reexecution differs from original Snapshot/Journal".into(),
                    );
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
            let certificate = &certificate;
            let c = &certificate.execution_commitment;
            let cut_body = BftSuccessorWindowCutV1 {
                height,
                header_id,
                certificate_digest: blake3_hex(&certificate.canonical_bytes()?),
                execution_commitment_digest: c.digest()?,
                snapshot_ref,
                journal_ref,
                semantic_state_root: c.state_root.clone(),
                post_manifest_hash: c.post_manifest_hash.clone(),
                business_result_root: c.result_root.clone(),
                driver_record_digest: record_digest.as_str().into(),
                driver_install_digest: install_digest(
                    &source.material_identity,
                    record_digest.as_str(),
                    certificate,
                )?,
            };
            if cut_body.snapshot_ref.digest != c.execution_state_root {
                return Err("successor private Snapshot differs from committed original".into());
            }
            let mut pending_candidates = Vec::new();
            for (roles, package) in &pending_packages {
                let mut dependencies = Vec::new();
                for (digest, bytes) in &package.dependency_nodes {
                    let reference = put(
                    bytes,
                    super::super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                )?;
                    if reference.digest != *digest {
                        return Err("successor pending index identity".into());
                    }
                    dependencies.push(reference);
                }
                pending_candidates.push(PendingCandidateV1 {
                    height: height
                        .checked_add(1)
                        .ok_or("successor pending height overflow")?,
                    header_id: package.candidate.public_value.block_id()?,
                    roles: roles.clone(),
                    value: package.candidate.public_value.clone(),
                    actions: put(
                        &package.candidate.actions_cbor,
                        oasis7_proto::distributed_bft::BFT_MAX_ACTIONS_CBOR_BYTES_V1,
                    )?,
                    dependencies,
                });
            }
            pending_candidates.sort_by(|a, b| a.header_id.cmp(&b.header_id));
            let metadata = PrivateClosureV1 {
                domain: WINDOW_DOMAIN.into(),
                active_base: Some(PrivateWindowBaseV1 {
                    checkpoint_digest: predecessor_checkpoint,
                    window_origin,
                }),
                source: source.clone(),
                private_instance_id: private_instance_id.into(),
                material,
                origin,
                public_state: public_state.clone(),
                current_journal,
                pending_candidates,
                snapshots,
                journals,
                manifests,
                records,
                nodes,
                cut: cut_body.clone(),
            };
            metadata.validate()?;
            let metadata_ref = put(&encode(&metadata, MAX_METADATA)?, MAX_METADATA)?;
            (metadata, metadata_ref, public_state, cut_body)
        };
        let (init_bytes, init_objects) = window_init_reserve(&metadata.pending_candidates)?;
        let segment_bytes = total
            .checked_add(init_bytes)
            .and_then(|n| n.checked_add(MAX_METADATA as u64))
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
            .and_then(|n| n.checked_add(2 + init_objects)) // manifests and initialization reserve
            .ok_or("successor object usage overflow")?;
        let mut manifest = BftSuccessorWindowArchiveManifestV1 {
            public_index: None,
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
            pending_actions: metadata
                .pending_candidates
                .iter()
                .map(|c| c.actions.clone())
                .collect(),
            cut: cut_body,
        })
    }
}
