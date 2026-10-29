//! Independent immutable-archive replay. No live driver/ledger mutex is held or
//! reopened here; caller DTOs never manufacture a World or schedule capability.
use super::successor_archive::*;
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftSignerErrorV3, BftSignerStateV3, BftSuccessorWindowAdoptedEvidenceV1,
    BftSuccessorWindowArchiveKindV1, BftSuccessorWindowArchiveManifestV1,
    BftSuccessorWindowArchiveRefV1, BftSuccessorWindowCheckpointBodyV1,
    BftSuccessorWindowDriverVerifierV1, BftSuccessorWindowStoredRequestV1,
    LocalBftSuccessorArchiveStoreV1,
};
use std::collections::BTreeMap;
use std::time::Instant;

pub(crate) struct ActualBftSuccessorWindowDriverVerifierV1 {
    anchor: VerifiedFormalReadAnchorV1,
    target: BftProfileTargetV2,
    archive: Arc<LocalBftSuccessorArchiveStoreV1>,
}
/// Strict replay does not consult an external/live business-index reader.
struct NoExternalIndex;
impl BusinessIndexNodeReader for NoExternalIndex {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Err("successor replay attempted an external dependency read".into())
    }
}
impl ActualBftSuccessorWindowDriverVerifierV1 {
    pub(crate) fn new(
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        archive: Arc<LocalBftSuccessorArchiveStoreV1>,
    ) -> Result<Self, String> {
        let root = canonical_storage_path(archive.root())?;
        crate::controlled_live_config::require_private_owned(&root, true)?;
        verify_formal_bft_handoff_v2(&anchor, target.clone())?;
        Ok(Self {
            anchor,
            target,
            archive,
        })
    }
    fn load(
        &self,
        reference: &BftSuccessorWindowArchiveRefV1,
        max: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>, String> {
        live(deadline)?;
        let bytes = self.archive.load_object(reference, max).map_err(err)?;
        reference.validate_bytes(&bytes).map_err(err)?;
        live(deadline)?;
        Ok(bytes)
    }
    pub(crate) fn verify_closure(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        public_state: &BftSignerStateV3,
        expected_public_ref: &BftSuccessorWindowArchiveRefV1,
        deadline: Instant,
    ) -> Result<(), String> {
        live(deadline)?;
        checkpoint.validate().map_err(err)?;
        let manifest_bytes =
            self.load(&checkpoint.execution_manifest_ref, MAX_METADATA, deadline)?;
        let manifest =
            BftSuccessorWindowArchiveManifestV1::decode_canonical(&manifest_bytes).map_err(err)?;
        manifest.validate_for_checkpoint(checkpoint).map_err(err)?;
        if manifest.kind != BftSuccessorWindowArchiveKindV1::Execution {
            return Err("successor private manifest has wrong role".into());
        }
        let metadata_entry = manifest
            .entries
            .iter()
            .find(|entry| entry.identity_digest == metadata_identity())
            .ok_or("successor private metadata entry missing")?;
        if !checkpoint.evidence_refs.contains(&metadata_entry.object)
            || checkpoint
                .evidence_refs
                .iter()
                .any(|reference| !manifest.entries.iter().any(|e| &e.object == reference))
        {
            return Err("successor private evidence refs missing from execution manifest".into());
        }
        let metadata: PrivateClosureV1 = decode(
            &self.load(&metadata_entry.object, MAX_METADATA, deadline)?,
            MAX_METADATA,
        )?;
        metadata.validate()?;
        if metadata.source != checkpoint.source
            || metadata.cut != checkpoint.cut
            || metadata.public_state != *expected_public_ref
        {
            return Err("successor private closure differs from original request".into());
        }
        let archived_public = self.load(&metadata.public_state, MAX_CLOSURE, deadline)?;
        if archived_public != encode_protocol(public_state, MAX_CLOSURE)?
            || public_state.binding_digest != metadata.source.binding_digest
            || public_state.pending_driver_apply.is_some()
        {
            return Err("successor archived public state differs from protected original".into());
        }
        let mut expected_objects = BTreeMap::new();
        for reference in std::iter::once(&metadata.material)
            .chain(std::iter::once(&metadata.origin))
            .chain(std::iter::once(&metadata.public_state))
            .chain(metadata.current_journal.iter())
            .chain(&metadata.records)
            .chain(&metadata.nodes)
            .chain(&metadata.snapshots)
            .chain(&metadata.journals)
            .chain(&metadata.manifests)
            .chain(std::iter::once(&metadata_entry.object))
        {
            if expected_objects
                .insert(reference.digest.clone(), reference.clone())
                .is_some_and(|old| old != *reference)
            {
                return Err("successor private duplicate digest with conflicting length".into());
            }
        }
        if expected_objects.len() != manifest.entries.len() {
            return Err("successor execution manifest omits or adds closure material".into());
        }
        let mut total = manifest_bytes.len() as u64;
        for entry in &manifest.entries {
            let identity = if entry.object == metadata_entry.object {
                metadata_identity()
            } else {
                entry.object.digest.clone()
            };
            if identity != entry.identity_digest
                || expected_objects.get(&entry.object.digest) != Some(&entry.object)
            {
                return Err("successor execution manifest identity differs".into());
            }
            total = total
                .checked_add(entry.object.bytes)
                .filter(|n| *n <= MAX_CLOSURE as u64)
                .ok_or("successor execution material capacity")?;
        }
        // Usage is cumulative across both archives and prior segments. The
        // provider enforces prior monotonicity; this verifier charges this full
        // private closure (including its manifest and copied original S).
        let previous_usage = public_state
            .successor_window
            .as_ref()
            .map(|s| s.usage)
            .unwrap_or_default();
        let charged_bytes = previous_usage
            .total_bytes
            .checked_add(total)
            .and_then(|n| n.checked_add(MAX_METADATA as u64))
            .ok_or("successor private cumulative accounting overflow")?;
        let charged_objects = previous_usage
            .objects
            .checked_add(manifest.entries.len() as u64)
            .and_then(|n| n.checked_add(2))
            .ok_or("successor private cumulative object overflow")?;
        if manifest.segment_sequence
            != previous_usage
                .segments
                .checked_add(1)
                .ok_or("successor private segment overflow")?
            || charged_bytes > manifest.usage.total_bytes
            || charged_objects > manifest.usage.objects
            || total > manifest.usage.total_bytes
            || manifest.entries.len() as u64 + 1 > manifest.usage.objects
        {
            return Err("successor private archive undercharged usage".into());
        }
        let material_bytes = self.load(&metadata.material, MAX_H_BYTES, deadline)?;
        let material: ClosedRecord = serde_cbor::from_slice(&material_bytes).map_err(err)?;
        if encoded_h_material(&material)? != material_bytes {
            return Err("successor original H material not canonical".into());
        }
        let origin: storage::Origin = decode(
            &self.load(&metadata.origin, MAX_METADATA, deadline)?,
            MAX_METADATA,
        )?;
        origin.validate()?;
        let handoff = verify_formal_bft_handoff_v2(&self.anchor, self.target.clone())?;
        let source = &metadata.source;
        if source.scope != handoff.target().scope
            || source.handoff_position != handoff.handoff_position()
            || source.handoff_record_id != handoff.handoff_head().record_id
            || source.handoff_snapshot_root != handoff.raw_snapshot_root()
            || source.target_digest != handoff.target_digest()
            || source.activation_digest != handoff.activation_digest()
            || origin.first_height() != handoff.activation_height()?
            || manifest.first_height != origin.first_height()
        {
            return Err("successor private closure differs from independently trusted H".into());
        }
        let mut seed = Vec::new();
        for reference in &metadata.nodes {
            seed.push((
                reference.digest.clone(),
                self.load(
                    reference,
                    super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                    deadline,
                )?,
            ));
        }
        let mut execution = FormalBftHeaderExecutionV3::from_handoff(
            self.anchor.clone(),
            self.target.clone(),
            &material,
            Arc::new(NoExternalIndex),
            seed,
            true,
        )?;
        if material_pin_digest(&execution, &material)? != origin.material_pin_digest() {
            return Err("successor H material differs from original ledger pin".into());
        }
        let material_identity = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                origin.digest()?,
                &metadata.private_instance_id,
                origin.material_pin_digest(),
            ))
            .map_err(err)?,
        );
        if material_identity != source.material_identity {
            return Err("successor private original Node identity changed".into());
        }
        let mut previous = origin.digest()?;
        if public_state.history.len() != metadata.records.len() {
            return Err("successor original history length differs".into());
        }
        for (index, reference) in metadata.records.iter().enumerate() {
            live(deadline)?;
            let record: storage::Record = decode(
                &self.load(
                    reference,
                    super::super::super::formal_ledger::MAX_RECORD,
                    deadline,
                )?,
                super::super::super::formal_ledger::MAX_RECORD,
            )?;
            record.validate(&origin)?;
            let entry = record.input();
            if entry.certificate != public_state.history[index] {
                return Err("successor original certificate changed".into());
            }
            let executed = execution.replay_next(&entry.certificate, &entry.actions_cbor)?;
            let nodes = execution.retained_nodes.accessed_nodes()?;
            let reproduced = storage::Record::from_actual(
                &origin,
                &previous,
                &entry,
                &executed,
                nodes.keys().cloned().collect(),
            )?;
            if reproduced != record {
                return Err(
                    "successor canonical Record/actions/index dependencies did not reproduce"
                        .into(),
                );
            }
            if self.load(&metadata.snapshots[index], MAX_CLOSURE, deadline)?
                != executed.snapshot_bytes
                || self.load(&metadata.journals[index], MAX_CLOSURE, deadline)?
                    != executed.journal_bytes
                || metadata.snapshots[index].digest != record.snapshot_ref()
                || metadata.journals[index].digest != record.journal_ref()
            {
                return Err("successor original Snapshot/Journal did not reproduce".into());
            }
            if self.load(&metadata.manifests[index], MAX_CLOSURE, deadline)?
                != encode(executed.world.manifest(), MAX_CLOSURE)?
                || executed.world.current_manifest_hash().map_err(err)?
                    != entry.certificate.execution_commitment.post_manifest_hash
            {
                return Err("successor original execution manifest did not reproduce".into());
            }
            previous = record.digest()?;
        }
        let observed = execution.retained_nodes.accessed_nodes()?;
        if observed
            .keys()
            .map(String::as_str)
            .ne(metadata.nodes.iter().map(|r| r.digest.as_str()))
        {
            return Err(
                "successor private closure contains unobserved or missing index dependencies"
                    .into(),
            );
        }
        let tip = execution
            .tip
            .as_ref()
            .ok_or("successor private empty replay")?;
        let certificate = tip.certified_header();
        let c = &certificate.execution_commitment;
        let cut = &metadata.cut;
        if cut.height != c.height
            || cut.header_id != tip.block_id()
            || cut.certificate_digest != blake3_hex(&certificate.canonical_bytes()?)
            || cut.execution_commitment_digest != c.digest()?
            || cut.snapshot_ref != *metadata.snapshots.last().ok_or("successor no Snapshot")?
            || cut.journal_ref != *metadata.journals.last().ok_or("successor no Journal")?
            || cut.semantic_state_root != c.state_root
            || cut.post_manifest_hash != c.post_manifest_hash
            || cut.business_result_root != execution.parent_result_tree.root()?
            || cut.driver_record_digest != previous
            || cut.driver_install_digest
                != install_digest(&source.material_identity, &previous, certificate)?
            || checkpoint.successor_schedule_cbor
                != encode_protocol(&execution.schedule, MAX_METADATA)?
            || checkpoint.successor_schedule_digest != execution.schedule.digest()?
        {
            return Err("successor exact executed cut or schedule differs".into());
        }
        match (
            &public_state.journal,
            &metadata.current_journal,
            &checkpoint.current_journal_ref,
            &checkpoint.current_journal_digest,
        ) {
            (None, None, None, None) => {}
            (Some(journal), Some(reference), Some(expected), Some(digest)) => {
                if reference != expected
                    || self.load(reference, MAX_CLOSURE, deadline)?
                        != encode_protocol(journal, MAX_CLOSURE)?
                    || journal.digest()? != *digest
                    || journal.height() != execution.schedule.height()
                    || journal.parent_commit_ref() != cut.header_id
                    || journal.pending_commit().is_some()
                {
                    return Err("successor changed original T+1 journal".into());
                }
            }
            _ => return Err("successor synthesized or removed T+1 journal".into()),
        }
        live(deadline)
    }
}
impl BftSuccessorWindowDriverVerifierV1 for ActualBftSuccessorWindowDriverVerifierV1 {
    fn verify_prepared(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        public_state: &BftSignerStateV3,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        let body = &original.request.intent.body;
        if public_state.revision != body.expected_revision
            || public_state.state_digest != body.expected_state_digest
        {
            return Err(BftSignerErrorV3(
                "successor private original protected revision/digest differs".into(),
            ));
        }
        self.verify_closure(
            &body.checkpoint,
            public_state,
            &original.original_public_state,
            deadline,
        )
        .map_err(BftSignerErrorV3)
    }
    fn verify_adopted(
        &self,
        _: &BftSuccessorWindowStoredRequestV1,
        _: &BftSignerStateV3,
        _: &BftSuccessorWindowAdoptedEvidenceV1,
        _: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(BftSignerErrorV3(
            "successor independent adopted pointer/WAL verification is not implemented".into(),
        ))
    }
}
