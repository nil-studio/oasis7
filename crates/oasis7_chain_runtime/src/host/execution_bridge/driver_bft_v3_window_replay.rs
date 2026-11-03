//! Actual per-segment replay, chained to the independently verified predecessor archive.
use super::super::super::super::formal_dependencies::RecordingBusinessIndexReaderV1;
use super::*;
pub(super) struct ProtectedHReplayV1 {
    pub(super) material: Arc<ClosedRecord>,
    pub(super) nodes: Arc<RecordingBusinessIndexReaderV1>,
}
/// Only nodes requested by authenticated H restoration may be loaded here.
/// Post-H originals require their separate exact-input scope.
struct RequestedHArchiveNodesV1 {
    archive: Arc<LocalBftSuccessorArchiveStoreV1>,
    references: Vec<BftSuccessorWindowArchiveRefV1>,
    deadline: Instant,
}
impl BusinessIndexNodeReader for RequestedHArchiveNodesV1 {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        live(self.deadline)?;
        let Ok(index) = self
            .references
            .binary_search_by(|r| r.digest.as_str().cmp(digest))
        else {
            return Ok(None);
        };
        let reference = &self.references[index];
        let bytes = self
            .archive
            .load_object(
                reference,
                super::super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            )
            .map_err(err)?;
        reference.validate_bytes(&bytes).map_err(err)?;
        live(self.deadline)?;
        Ok(Some(bytes))
    }
}
pub(super) struct ReplayedPrivateSegmentV1 {
    pub(super) origin: storage::Origin,
    pub(super) execution: Box<FormalBftHeaderExecutionV3>,
    pub(super) previous: String,
    pub(super) segment_start: Box<FormalBftHeaderExecutionV3>,
    pub(super) window_origin: Option<super::super::window_storage::WindowOrigin>,
    pub(super) publications: Vec<node_execution::VerifiedPublicationOriginalV1>,
    pub(super) h_base: Option<ProtectedHReplayV1>,
}
struct PublicationReplayContextV1<'a> {
    origin: &'a storage::Origin,
    window_origin: Option<&'a super::super::window_storage::WindowOrigin>,
    publications: &'a [node_execution::VerifiedPublicationOriginalV1],
    previous: &'a str,
}
struct ScopedRecordReplayV1<'a, 'b, 'c> {
    origin: &'a storage::Origin,
    window_origin: Option<&'a super::super::window_storage::WindowOrigin>,
    publications: &'a [node_execution::VerifiedPublicationOriginalV1],
    metadata: &'a PrivateClosureV1,
    protected: Option<&'a super::super::successor_archive::protected_retention::ValidatedProtectedRetentionArchiveV1>,
    h_base: Option<&'a ProtectedHReplayV1>,
    access: &'a ProtectedInputAccessV1<'b, 'c>,
    deadline: Instant,
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
        protected: Option<&super::super::successor_archive::protected_retention::ValidatedProtectedRetentionArchiveV1>,
        access: &ProtectedInputAccessV1<'_, '_>,
    ) -> Result<ReplayedPrivateSegmentV1, String> {
        let ReplayedPrivateSegmentV1 {
            origin,
            mut execution,
            mut previous,
            segment_start,
            window_origin,
            publications: _,
            h_base,
        } = self.restore_segment(
            checkpoint,
            public_state,
            manifest,
            metadata,
            deadline,
            predecessor,
            protected,
        )?;
        let owner_origin = match &window_origin {
            Some(value) => value.digest()?,
            None => origin.digest()?,
        };
        let publications = match protected {
            Some(protected) => self.verify_protected_inputs(
                metadata,
                public_state,
                protected,
                &segment_start,
                h_base
                    .as_ref()
                    .ok_or("covered segment lacks verified H base")?,
                access,
                deadline,
            )?,
            None => self.load_publications(
                metadata,
                &owner_origin,
                predecessor,
                public_state,
                deadline,
            )?,
        };
        let scope = ScopedRecordReplayV1 {
            origin: &origin,
            window_origin: window_origin.as_ref(),
            publications: &publications,
            metadata,
            protected,
            h_base: h_base.as_ref(),
            access,
            deadline,
        };
        for (index, certificate) in public_state.history.iter().enumerate() {
            self.replay_scoped_record(&mut execution, index, certificate, &mut previous, &scope)?;
        }
        execution.retained_nodes.with_accessed_nodes(|observed| {
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
            Ok(())
        })?;
        Ok(ReplayedPrivateSegmentV1 {
            origin,
            execution,
            previous,
            segment_start,
            window_origin,
            publications,
            h_base,
        })
    }
    fn replay_scoped_record(
        &self,
        execution: &mut Box<FormalBftHeaderExecutionV3>,
        index: usize,
        certificate: &FormalBftCertifiedHeaderV3,
        previous: &mut String,
        scope: &ScopedRecordReplayV1<'_, '_, '_>,
    ) -> Result<(), String> {
        let metadata = scope.metadata;
        let origin = scope.origin;
        let window_origin = scope.window_origin;
        let publications = scope.publications;
        let protected = scope.protected;
        let h_base = scope.h_base;
        let access = scope.access;
        let deadline = scope.deadline;
        let reference = metadata
            .records
            .get(index)
            .ok_or("scoped Record index outside archive")?;
        live(deadline)?;
        // The exact original scope spans Data loading, World replay and all
        // Snapshot/Journal/manifest comparisons for this selected Some.
        let header_id = certificate.header.block_id()?;
        let original = protected.and_then(|history| {
            history.originals.values().find(|original| {
                original.entry.key.height == certificate.execution_commitment.height
                    && original.entry.key.header_id == header_id
            })
        });
        let scoped = original
            .map(|original| self.scoped_original(&original.reference, access, deadline))
            .transpose()?;
        if let (Some(original), Some(scoped)) = (original, scoped.as_ref()) {
            original
                .reference
                .validate_input(scoped.input())
                .map_err(err)?;
        }
        let bytes = self.load(
            reference,
            super::super::super::super::formal_ledger::MAX_RECORD,
            deadline,
        )?;
        if let Some(scoped) = scoped.as_ref() {
            if window_origin.is_some() {
                return Err(
                    "covered active Record replay is not supported by this source layout".into(),
                );
            }
            {
                // Candidate Data and a committed Record are distinct objects.
                // Bind their actions/value before executing the derived Record.
                let candidate = node_execution::source_admission_candidate(scoped.input())?;
                let record: storage::Record = decode(
                    &bytes,
                    super::super::super::super::formal_ledger::MAX_RECORD,
                )?;
                let recorded = record.input();
                if recorded.actions_cbor != candidate.actions_cbor
                    || recorded.certificate.header != candidate.public_value.header
                    || recorded.certificate.execution_commitment
                        != candidate.public_value.execution_commitment
                {
                    return Err(
                        "selected Record actions/value differ from authorized original candidate"
                            .into(),
                    );
                }
            }
            *execution = Box::new(
                self.fork_protected_current(
                    execution.as_ref(),
                    h_base
                        .as_ref()
                        .ok_or("selected protected Record H base absent")?,
                    scoped.input(),
                )?,
            );
        }
        let (snapshot, journal, record_digest) = self.replay_record(
            execution.as_mut(),
            &bytes,
            certificate,
            PublicationReplayContextV1 {
                origin,
                window_origin,
                publications,
                previous: previous.as_str(),
            },
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
                != certificate.execution_commitment.post_manifest_hash
        {
            return Err("successor original execution manifest did not reproduce".into());
        }
        *previous = record_digest;
        drop(scoped);
        Ok(())
    }

    /// Rebuild the exact original Reserve prefix before acquiring the next
    /// candidate input. No two full input receiving permits are nested.
    pub(super) fn replay_protected_parent_prefix(
        &self,
        metadata: &PrivateClosureV1,
        protected: &super::super::successor_archive::protected_retention::ValidatedProtectedRetentionArchiveV1,
        parent_history: &[FormalBftCertifiedHeaderV3],
        h_base: &ProtectedHReplayV1,
        access: &ProtectedInputAccessV1<'_, '_>,
        publications: &[node_execution::VerifiedPublicationOriginalV1],
        deadline: Instant,
    ) -> Result<Box<FormalBftHeaderExecutionV3>, String> {
        live(deadline)?;
        if metadata.active_base.is_some() || parent_history.len() > metadata.records.len() {
            return Err("protected source parent prefix range/layout differs".into());
        }
        let origin: storage::Origin = decode(
            &self.load(&metadata.origin, MAX_METADATA, deadline)?,
            MAX_METADATA,
        )?;
        origin.validate()?;
        let reader = RecordingBusinessIndexReaderV1::new_live_closed_layer(
            h_base.nodes.clone(),
            Vec::new(),
        )?;
        let mut execution = Box::new(FormalBftHeaderExecutionV3::from_live_closed_handoff(
            self.anchor.clone(),
            self.target.clone(),
            &h_base.material,
            reader,
        )?);
        let mut previous = origin.digest()?;
        let scope = ScopedRecordReplayV1 {
            origin: &origin,
            window_origin: None,
            publications,
            metadata,
            protected: Some(protected),
            h_base: Some(h_base),
            access,
            deadline,
        };
        for (index, certificate) in parent_history.iter().enumerate() {
            self.replay_scoped_record(&mut execution, index, certificate, &mut previous, &scope)?;
        }
        Ok(execution)
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
        protected: Option<&super::super::successor_archive::protected_retention::ValidatedProtectedRetentionArchiveV1>,
    ) -> Result<ReplayedPrivateSegmentV1, String> {
        let material_bytes = self.load(&metadata.material, MAX_H_BYTES, deadline)?;
        let material: Arc<ClosedRecord> =
            Arc::new(serde_cbor::from_slice(&material_bytes).map_err(err)?);
        if encoded_h_material(&material)? != material_bytes {
            return Err("successor original H material not canonical".into());
        }
        drop(material_bytes);
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
        if protected.is_none() {
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
        }
        let mut h_base = None;
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
            let execution = if let Some(protected) = protected {
                let open = FormalBftHeaderExecutionV3::from_handoff(
                    self.anchor.clone(),
                    self.target.clone(),
                    &material,
                    Arc::new(RequestedHArchiveNodesV1 {
                        archive: self.archive.clone(),
                        references: protected.index.h_nodes.clone(),
                        deadline,
                    }),
                    Vec::new(),
                    false,
                )?;
                node_execution::capture_h_index(&open)?;
                let refs = open.retained_nodes.with_accessed_nodes(|nodes| {
                    Ok(nodes.keys().cloned().collect::<BTreeSet<_>>())
                })?;
                if &refs != protected.activation.h_refs() {
                    return Err(
                        "covered original H inventory differs from authenticated roots".into(),
                    );
                }
                let nodes = open.retained_nodes.take_closed_inventory()?;
                drop(open);
                let reader = RecordingBusinessIndexReaderV1::new_live_closed_layer(
                    nodes.clone(),
                    Vec::new(),
                )?;
                let execution = FormalBftHeaderExecutionV3::from_live_closed_handoff(
                    self.anchor.clone(),
                    self.target.clone(),
                    &material,
                    reader,
                )?;
                h_base = Some(ProtectedHReplayV1 {
                    material: material.clone(),
                    nodes,
                });
                execution
            } else {
                FormalBftHeaderExecutionV3::from_handoff(
                    self.anchor.clone(),
                    self.target.clone(),
                    &material,
                    Arc::new(NoExternalIndex),
                    seed,
                    true,
                )?
            };
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
        let segment_start = if h_base.is_some() {
            // The shared H parent supplies immutable bytes. This detached World
            // does not duplicate that complete inventory or inherit its accessed set.
            Box::new(execution.fork_replay_current(Vec::new())?)
        } else {
            Box::new(execution.fork_replay_current(execution.retained_nodes.recorded_nodes()?)?)
        };
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
            publications: Vec::new(),
            h_base,
        })
    }

    /// Only one record's owned execution result is live in this frame.
    #[inline(never)]
    fn replay_record(
        &self,
        execution: &mut FormalBftHeaderExecutionV3,
        bytes: &[u8],
        certificate: &FormalBftCertifiedHeaderV3,
        context: PublicationReplayContextV1<'_>,
    ) -> Result<(Vec<u8>, Vec<u8>, String), String> {
        let PublicationReplayContextV1 {
            origin,
            window_origin,
            publications,
            previous,
        } = context;
        if let Some(window) = window_origin {
            let record: super::super::window_storage::WindowRecord =
                decode(bytes, super::super::super::super::formal_ledger::MAX_RECORD)?;
            record.validate(window)?;
            let input = record.input();
            if input.certificate != *certificate {
                return Err("successor original certificate changed".into());
            }
            let executed = execution.replay_next(&input.certificate, &input.actions_cbor)?;
            let nodes = execution
                .retained_nodes
                .with_accessed_nodes(|nodes| Ok(nodes.keys().cloned().collect::<Vec<_>>()))?;
            let reproduced = super::super::window_storage::WindowRecord::from_actual(
                window, previous, &input, &executed, nodes,
            )?;
            self.validate_record_publication(
                publications,
                record.publication_reservation_ref(),
                &input,
                &executed,
            )?;
            let reproduced = reproduced.with_publication_reservation(
                record.publication_reservation_ref().map(str::to_owned),
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
            let nodes = execution
                .retained_nodes
                .with_accessed_nodes(|nodes| Ok(nodes.keys().cloned().collect::<Vec<_>>()))?;
            let reproduced =
                storage::Record::from_actual(origin, previous, &input, &executed, nodes)?;
            self.validate_record_publication(
                publications,
                record.publication_reservation_ref(),
                &input,
                &executed,
            )?;
            let reproduced = reproduced.with_publication_reservation(
                record.publication_reservation_ref().map(str::to_owned),
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
