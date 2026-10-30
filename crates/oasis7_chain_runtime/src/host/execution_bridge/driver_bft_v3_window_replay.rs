//! Actual per-segment replay, chained to the independently verified predecessor archive.
use super::*;
pub(super) struct ReplayedPrivateSegmentV1 {
    pub(super) origin: storage::Origin,
    pub(super) execution: Box<FormalBftHeaderExecutionV3>,
    pub(super) previous: String,
    pub(super) segment_start: Box<FormalBftHeaderExecutionV3>,
    pub(super) window_origin: Option<super::super::window_storage::WindowOrigin>,
}
/// Strict replay does not consult an external/live business-index reader.
struct NoExternalIndex;
impl BusinessIndexNodeReader for NoExternalIndex {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Err("successor replay attempted an external dependency read".into())
    }
}
impl ActualBftSuccessorWindowDriverVerifierV1 {
    pub(crate) fn with_active_private_store(
        mut self,
        store: LocalCasStore,
    ) -> Result<Self, String> {
        let path = canonical_storage_path(store.root())?;
        let archive = canonical_storage_path(self.archive.root())?;
        if path.starts_with(&archive) || archive.starts_with(&path) {
            return Err("active CAS/archive paths overlap".into());
        }
        self.active_private_store = Some(store);
        Ok(self)
    }
    pub(super) fn replay_segment(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        public_state: &BftSignerStateV3,
        manifest: &BftSuccessorWindowArchiveManifestV1,
        metadata: &PrivateClosureV1,
        deadline: Instant,
        predecessor: Option<&VerifiedPrivateSuccessorBaseV1>,
    ) -> Result<ReplayedPrivateSegmentV1, String> {
        let ReplayedPrivateSegmentV1 {
            origin,
            mut execution,
            mut previous,
            segment_start,
            window_origin,
        } = self.restore_segment(
            checkpoint,
            public_state,
            manifest,
            metadata,
            deadline,
            predecessor,
        )?;
        for (index, reference) in metadata.records.iter().enumerate() {
            live(deadline)?;
            let bytes = self.load(
                reference,
                super::super::super::super::formal_ledger::MAX_RECORD,
                deadline,
            )?;
            let (snapshot, journal, record_digest) = self.replay_record(
                &mut execution,
                &origin,
                window_origin.as_ref(),
                &bytes,
                &public_state.history[index],
                &previous,
            )?;
            if self.load(&metadata.snapshots[index], MAX_CLOSURE, deadline)? != snapshot
                || self.load(&metadata.journals[index], MAX_CLOSURE, deadline)? != journal
                || metadata.snapshots[index].digest != blake3_hex(&snapshot)
                || metadata.journals[index].digest != blake3_hex(&journal)
            {
                return Err("successor original Snapshot/Journal did not reproduce".into());
            }
            if self.load(&metadata.manifests[index], MAX_CLOSURE, deadline)?
                != encode(execution.parent_world.manifest(), MAX_CLOSURE)?
                || execution
                    .parent_world
                    .current_manifest_hash()
                    .map_err(err)?
                    != public_state.history[index]
                        .execution_commitment
                        .post_manifest_hash
            {
                return Err("successor original execution manifest did not reproduce".into());
            }
            previous = record_digest;
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
        Ok(ReplayedPrivateSegmentV1 {
            origin,
            execution,
            previous,
            segment_start,
            window_origin,
        })
    }
    /// Keep H restoration and predecessor-owned contexts out of the live World
    /// replay frame. Service threads use their normal bounded native stack.
    #[inline(never)]
    fn restore_segment(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        public_state: &BftSignerStateV3,
        manifest: &BftSuccessorWindowArchiveManifestV1,
        metadata: &PrivateClosureV1,
        deadline: Instant,
        predecessor: Option<&VerifiedPrivateSuccessorBaseV1>,
    ) -> Result<ReplayedPrivateSegmentV1, String> {
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
        {
            return Err("successor private closure differs from independently trusted H".into());
        }
        let mut seed = Vec::new();
        for reference in &metadata.nodes {
            seed.push((
                reference.digest.clone(),
                self.load(
                    reference,
                    super::super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                    deadline,
                )?,
            ));
        }
        let (execution, window_origin) = if let Some(base) = &metadata.active_base {
            if checkpoint.sequence > 64 {
                return Err("private archive predecessor chain capacity".into());
            }
            let previous = public_state
                .successor_window
                .as_ref()
                .and_then(|s| s.requests.last())
                .ok_or("private archive predecessor original missing")?;
            let previous_body = &previous.request.intent.body.checkpoint;
            if previous_body.digest().map_err(err)? != base.checkpoint_digest
                || previous_body.sequence.checked_add(1) != Some(checkpoint.sequence)
                || previous.request.status
                    != oasis7_distfs::protected_bft_v3::BftSuccessorWindowStatusV1::Adopted
                || manifest.predecessor_root.as_ref() != Some(&previous_body.execution_archive_root)
                || public_state
                    .active_window
                    .as_ref()
                    .map(|w| w.checkpoint_digest.as_str())
                    != Some(base.checkpoint_digest.as_str())
            {
                return Err(
                    "private archive predecessor continuity differs from protected original State"
                        .into(),
                );
            }
            let verified_previous = predecessor.ok_or("verified private predecessor absent")?;
            if verified_previous.original.as_ref() != Some(previous)
                || previous
                    .request
                    .adopted_evidence
                    .as_ref()
                    .and_then(|evidence| evidence.active_segment_origin.as_ref())
                    != Some(&base.window_origin)
            {
                return Err("private predecessor differs from protected original/Origin".into());
            }
            let previous = verified_previous;
            if previous.origin != origin
                || previous.metadata.material != metadata.material
                || previous.metadata.private_instance_id != metadata.private_instance_id
            {
                return Err(
                    "private archive predecessor changed original H/private identity".into(),
                );
            }
            let window: super::super::window_storage::WindowOrigin = decode(
                &self.load(&base.window_origin, MAX_METADATA, deadline)?,
                MAX_METADATA,
            )?;
            window.validate()?;
            if window.material_pin_digest()
                != blake3_hex(&super::super::window_execution::base_identity(previous)?)
                || window.first_height() != previous.execution.schedule.height()
            {
                return Err(
                    "private archive window origin differs from verified predecessor".into(),
                );
            }
            (
                Box::new(previous.execution.fork_strict_with_seed(seed)?),
                Some(window),
            )
        } else {
            if public_state.active_window.is_some() {
                return Err("private archive lost active predecessor".into());
            }
            let execution = FormalBftHeaderExecutionV3::from_handoff(
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
            (Box::new(execution), None)
        };
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
        let segment_start =
            Box::new(execution.fork_replay_current(execution.retained_nodes.recorded_nodes()?)?);
        let previous = window_origin
            .as_ref()
            .map(OriginContract::digest)
            .transpose()?
            .unwrap_or(origin.digest()?);
        let first_height = window_origin
            .as_ref()
            .map_or(origin.first_height(), OriginContract::first_height);
        if manifest.first_height != first_height
            || public_state.history.len() != metadata.records.len()
        {
            return Err("successor segment range/history differs".into());
        }
        Ok(ReplayedPrivateSegmentV1 {
            origin,
            execution,
            previous,
            segment_start,
            window_origin,
        })
    }

    /// Only one record's owned execution result is live in this frame.
    #[inline(never)]
    fn replay_record(
        &self,
        execution: &mut FormalBftHeaderExecutionV3,
        origin: &storage::Origin,
        window_origin: Option<&super::super::window_storage::WindowOrigin>,
        bytes: &[u8],
        certificate: &FormalBftCertifiedHeaderV3,
        previous: &str,
    ) -> Result<(Vec<u8>, Vec<u8>, String), String> {
        if let Some(window) = window_origin {
            let record: super::super::window_storage::WindowRecord =
                decode(bytes, super::super::super::super::formal_ledger::MAX_RECORD)?;
            record.validate(window)?;
            let input = record.input();
            if input.certificate != *certificate {
                return Err("successor original certificate changed".into());
            }
            let executed = execution.replay_next(&input.certificate, &input.actions_cbor)?;
            let nodes = execution.retained_nodes.accessed_nodes()?;
            let reproduced = super::super::window_storage::WindowRecord::from_actual(
                window,
                previous,
                &input,
                &executed,
                nodes.keys().cloned().collect(),
            )?;
            if reproduced != record {
                return Err("successor active Record/actions/index did not reproduce".into());
            }
            Ok((
                executed.snapshot_bytes,
                executed.journal_bytes,
                record.digest()?,
            ))
        } else {
            let record: storage::Record =
                decode(bytes, super::super::super::super::formal_ledger::MAX_RECORD)?;
            record.validate(origin)?;
            let input = record.input();
            if input.certificate != *certificate {
                return Err("successor original certificate changed".into());
            }
            let executed = execution.replay_next(&input.certificate, &input.actions_cbor)?;
            let nodes = execution.retained_nodes.accessed_nodes()?;
            let reproduced = storage::Record::from_actual(
                origin,
                previous,
                &input,
                &executed,
                nodes.keys().cloned().collect(),
            )?;
            if reproduced != record {
                return Err("successor canonical Record/actions/index did not reproduce".into());
            }
            Ok((
                executed.snapshot_bytes,
                executed.journal_bytes,
                record.digest()?,
            ))
        }
    }
}
