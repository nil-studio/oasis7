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
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

pub(crate) struct ActualBftSuccessorWindowDriverVerifierV1 {
    anchor: VerifiedFormalReadAnchorV1,
    target: BftProfileTargetV2,
    archive: Arc<LocalBftSuccessorArchiveStoreV1>,
    installation_root: Option<super::successor_carriers::InstallationRootV1>,
    active_private_store: Option<LocalCasStore>,
    original_binding: Option<oasis7_distfs::protected_bft_v3::BftSignerBindingV3>,
    original_retention: Option<(
        oasis7_distfs::protected_bft_v3::BftPrivateRetentionAttestorV1,
        Arc<dyn oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReaderV1>,
    )>,
}
/// Constructed only after actual H/archive replay and original pending reproduction.
/// It grants private execution continuity, not provider signing authority.
pub(crate) struct VerifiedPrivateSuccessorBaseV1 {
    protected_continuity: Option<Box<retention_continuity::VerifiedRetentionContinuityV1>>,
    verified_initializer: Option<(u64, u64)>,
    pub(super) origin: storage::Origin,
    pub(super) anchor: VerifiedFormalReadAnchorV1,
    pub(super) target: BftProfileTargetV2,
    pub(super) execution: Box<FormalBftHeaderExecutionV3>,
    pub(super) metadata: PrivateClosureV1,
    pub(super) original: Option<BftSuccessorWindowStoredRequestV1>,
    pub(super) public_state: BftSignerStateV3,
    pub(super) publications: Vec<node_execution::VerifiedPublicationOriginalV1>,
    pub(super) pending: Vec<(
        BTreeSet<PendingRoleV1>,
        oasis7_node::NodeBftPrivateCandidatePackageV3,
    )>,
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
            installation_root: None,
            active_private_store: None,
            original_retention: None,
            original_binding: None,
        })
    }
    pub(crate) fn with_original_retention_binding(
        mut self,
        binding: oasis7_distfs::protected_bft_v3::BftSignerBindingV3,
    ) -> Result<Self, String> {
        let digest = binding.digest().map_err(err)?;
        if self
            .original_retention
            .as_ref()
            .is_some_and(|(role, _)| role.source.binding_digest != digest)
        {
            return Err("archive original binding differs from fixed role".into());
        }
        self.original_binding = Some(binding);
        Ok(self)
    }
    pub(crate) fn with_original_retention_reader(
        mut self,
        role: oasis7_distfs::protected_bft_v3::BftPrivateRetentionAttestorV1,
        reader: Arc<dyn oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReaderV1>,
    ) -> Result<Self, String> {
        role.digest().map_err(err)?;
        self.original_retention = Some((role, reader));
        Ok(self)
    }
    pub(crate) fn with_installation_root(mut self, records: PathBuf) -> Result<Self, String> {
        let root = canonical_storage_path(&records)?;
        crate::controlled_live_config::require_private_owned(&root, true)?;
        let archive = canonical_storage_path(self.archive.root())?;
        if root.starts_with(&archive) || archive.starts_with(&root) {
            return Err("successor install/archive paths overlap".into());
        }
        self.installation_root = Some(super::successor_carriers::InstallationRootV1::new(root)?);
        Ok(self)
    }
    pub(crate) fn recover_private_base(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        verified_public: &oasis7_distfs::protected_bft_v3::ValidatedBftSuccessorOriginalStateV1,
        deadline: Instant,
    ) -> Result<VerifiedPrivateSuccessorBaseV1, String> {
        let base = self.verify_original_closure_with_origin(original, deadline)?;
        if &base.public_state != verified_public.original_state() {
            return Err(
                "private active base differs from fully authenticated original public State".into(),
            );
        }
        Ok(base)
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
        self.verify_closure_with_origin(checkpoint, public_state, expected_public_ref, deadline)
            .map(|_| ())
    }
    fn verify_one_closure_with_access(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        public_state: &BftSignerStateV3,
        expected_public_ref: &BftSuccessorWindowArchiveRefV1,
        deadline: Instant,
        predecessor: Option<&VerifiedPrivateSuccessorBaseV1>,
        access: &ProtectedInputAccessV1<'_, '_>,
    ) -> Result<Box<VerifiedPrivateSuccessorBaseV1>, String> {
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
        let protected = self.load_protected_history(&metadata, public_state, access, deadline)?;
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
            .chain(metadata.publications.iter().flat_map(|p| p.references()))
            .chain(metadata.protected_retention.iter())
            .chain(protected.iter().flat_map(|proof| proof.objects.values()))
            .chain(std::iter::once(&metadata_entry.object))
        {
            if expected_objects
                .insert(reference.digest.clone(), reference.clone())
                .is_some_and(|old| old != *reference)
            {
                return Err("successor private duplicate digest with conflicting length".into());
            }
        }
        if let Some(base) = &metadata.active_base {
            expected_objects.insert(
                base.window_origin.digest.clone(),
                base.window_origin.clone(),
            );
        }
        for candidate in &metadata.pending_candidates {
            for reference in std::iter::once(&candidate.actions).chain(&candidate.dependencies) {
                if expected_objects
                    .insert(reference.digest.clone(), reference.clone())
                    .is_some_and(|old| old != *reference)
                {
                    return Err("successor pending object digest/length conflict".into());
                }
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
        let (init_bytes, init_objects) = window_init_reserve(&metadata.pending_candidates)?;
        let (wal_bytes, wal_objects) = publication_layout_reserve(metadata.publication_layout)?;
        let init_bytes = init_bytes
            .checked_add(wal_bytes)
            .ok_or("publication initializer reserve overflow")?;
        let init_objects = init_objects
            .checked_add(wal_objects)
            .ok_or("publication initializer object reserve overflow")?;
        let charged_bytes = previous_usage
            .total_bytes
            .checked_add(total)
            .and_then(|n| n.checked_add(init_bytes))
            .and_then(|n| n.checked_add(MAX_METADATA as u64))
            .ok_or("successor private cumulative accounting overflow")?;
        let charged_objects = previous_usage
            .objects
            .checked_add(manifest.entries.len() as u64)
            .and_then(|n| n.checked_add(2 + init_objects))
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
        let replay::ReplayedPrivateSegmentV1 {
            origin,
            mut execution,
            previous,
            segment_start,
            window_origin,
            publications,
            h_base,
        } = self.replay_segment(
            checkpoint,
            public_state,
            &manifest,
            &metadata,
            deadline,
            predecessor,
            protected.as_ref(),
            access,
        )?;
        let source = &metadata.source;
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
        validate_original_pending_commit(
            public_state,
            &execution.schedule,
            cut.height,
            &cut.header_id,
            &cut.driver_install_digest,
        )?;
        let pending = self.verify_pending_packages(
            &metadata,
            public_state,
            protected.as_ref(),
            &segment_start,
            h_base.as_ref(),
            window_origin.is_some(),
            &publications,
            access,
            deadline,
        )?;
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
                {
                    return Err("successor changed original T+1 journal".into());
                }
            }
            _ => return Err("successor synthesized or removed T+1 journal".into()),
        }
        live(deadline)?;
        // Actual original replay has completed under each exact read scope.
        // Metadata alone cannot establish the cost of a different same-size carry.
        let initializer = carry_initialization::from_verified(&metadata, &publications)?;
        let old_initializer = window_init_reserve(&metadata.pending_candidates)?;
        carry_initialization::validate_committed(
            (charged_bytes, charged_objects),
            old_initializer,
            initializer,
            (manifest.usage.total_bytes, manifest.usage.objects),
        )?;
        let verified_initializer = (metadata.publication_layout == Some(2)).then_some(initializer);
        let protected_continuity = match protected {
            Some(proof) => {
                let (binding, role) = self.protected_source(access, public_state)?;
                Some(
                    retention_continuity::VerifiedRetentionContinuityV1::from_verified(
                        binding.clone(),
                        role.clone(),
                        proof,
                        h_base.ok_or("protected continuity H parent absent")?,
                        &metadata,
                        public_state,
                    )?,
                )
            }
            None => None,
        };
        if metadata.publication_layout == Some(2) {
            // Every original/parent/pending actual replay has succeeded. Only
            // now may this derived result become an immutable Active parent.
            execution.close_verified_replay_inventory()?;
        }
        let base = Box::new(VerifiedPrivateSuccessorBaseV1 {
            protected_continuity,
            verified_initializer,
            origin,
            anchor: self.anchor.clone(),
            target: self.target.clone(),
            execution,
            metadata,
            original: None,
            public_state: public_state.clone(),
            publications,
            pending,
        });
        base.verify_retention_continuity()?;
        Ok(base)
    }
}
#[path = "driver_bft_v3_successor_retention_continuity.rs"]
mod retention_continuity;
impl ActualBftSuccessorWindowDriverVerifierV1 {
    // Keep pending candidate execution temporaries out of the earlier full
    // history replay stack. This changes neither scope nor validation order.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn verify_pending_packages(
        &self,
        metadata: &PrivateClosureV1,
        public_state: &BftSignerStateV3,
        protected: Option<
            &super::successor_archive::protected_retention::ValidatedProtectedRetentionArchiveV1,
        >,
        segment_start: &FormalBftHeaderExecutionV3,
        h_base: Option<&replay::ProtectedHReplayV1>,
        has_window_origin: bool,
        publications: &[node_execution::VerifiedPublicationOriginalV1],
        access: &ProtectedInputAccessV1<'_, '_>,
        deadline: Instant,
    ) -> Result<
        Vec<(
            BTreeSet<PendingRoleV1>,
            oasis7_node::NodeBftPrivateCandidatePackageV3,
        )>,
        String,
    > {
        let mut pending = Vec::new();
        let mut required = required_pending(public_state.journal.as_ref())?;
        for candidate in &metadata.pending_candidates {
            let mut journal_roles = candidate.roles.clone();
            journal_roles.remove(&PendingRoleV1::RetainedLocal);
            if protected.is_some() {
                journal_roles.remove(&PendingRoleV1::ProtectedOriginal);
            }
            let expected_roles = required.remove(&candidate.header_id).unwrap_or_default();
            if expected_roles != journal_roles || (candidate.roles.is_empty()) {
                return Err(
                    "successor pending package role/header differs from original journal".into(),
                );
            }
            validate_pending_commit_value(public_state, &candidate.roles, &candidate.value)?;
            let original = protected
                .as_ref()
                .map(|protected| {
                    protected
                        .originals
                        .values()
                        .find(|original| {
                            original.entry.key.height == candidate.height
                                && original.entry.key.header_id == candidate.header_id
                        })
                        .ok_or("protected pending original exact key missing")
                })
                .transpose()?;
            // Reacquire the exact-purpose scoped read before any pending payload
            // is loaded again. The earlier historical verification is not a grant.
            let protected_parent = protected
                .as_ref()
                .map(|proof| {
                    self.replay_protected_parent_prefix(
                        &metadata,
                        proof,
                        &public_state.history,
                        h_base
                            .as_ref()
                            .ok_or("protected pending H baseline absent")?,
                        access,
                        &publications,
                        deadline,
                    )
                })
                .transpose()?;
            let package = self.verify_scoped_pending_package(
                metadata,
                candidate,
                original,
                protected_parent,
                segment_start,
                h_base,
                has_window_origin,
                protected.is_some(),
                publications,
                access,
                deadline,
            )?;
            pending.push((candidate.roles.clone(), package));
        }
        if !required.is_empty() {
            return Err("successor original journal is missing required private candidates".into());
        }
        Ok(pending)
    }
}

impl ActualBftSuccessorWindowDriverVerifierV1 {
    // Earlier parent replay finishes before this phase allocates its current
    // candidate execution temporaries. Its exact Input hold covers all reads.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn verify_scoped_pending_package(
        &self,
        metadata: &PrivateClosureV1,
        candidate: &PendingCandidateV1,
        original: Option<&super::successor_archive::protected_retention::OriginalInputWitnessV1>,
        protected_parent: Option<Box<FormalBftHeaderExecutionV3>>,
        segment_start: &FormalBftHeaderExecutionV3,
        h_base: Option<&replay::ProtectedHReplayV1>,
        has_window_origin: bool,
        is_protected: bool,
        publications: &[node_execution::VerifiedPublicationOriginalV1],
        access: &ProtectedInputAccessV1<'_, '_>,
        deadline: Instant,
    ) -> Result<oasis7_node::NodeBftPrivateCandidatePackageV3, String> {
        let scoped = original
            .map(|original| self.scoped_original(&original.reference, access, deadline))
            .transpose()?;
        let input_candidate = scoped
            .as_ref()
            .map(|read| node_execution::source_admission_candidate(read.input()))
            .transpose()?;
        let mut detached = if let Some(read) = &scoped {
            let input = read.input();
            let supplied = input.objects.iter().filter(|object| object.kind == oasis7_distfs::protected_bft_v3::BftPrivateRetentionInputObjectKindV1::Dependency);
            if supplied.clone().count() != candidate.dependencies.len()
                || supplied
                    .clone()
                    .zip(&candidate.dependencies)
                    .any(|(object, reference)| &object.reference != reference)
                || input_candidate
                    .as_ref()
                    .is_none_or(|value| value.public_value != candidate.value)
            {
                return Err(
                    "protected pending payload refs/value differ from authorized exact Input"
                        .into(),
                );
            }
            for (object, reference) in supplied.zip(&candidate.dependencies) {
                if self.load(
                    reference,
                    super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                    deadline,
                )? != object.bytes
                {
                    return Err(
                        "protected pending archived dependency is missing or changed".into(),
                    );
                }
            }
            self.fork_protected_current(
                protected_parent
                    .as_ref()
                    .ok_or("protected pending actual parent absent")?,
                h_base
                    .as_ref()
                    .ok_or("protected pending H baseline unavailable")?,
                input,
            )?
        } else {
            let mut seed = Vec::new();
            let mut total = 0usize;
            for reference in &candidate.dependencies {
                let bytes = self.load(
                    reference,
                    super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                    deadline,
                )?;
                total = total.checked_add(bytes.len()).filter(|n| *n <= super::super::super::formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES)
                        .ok_or("successor pending dependency capacity")?;
                seed.push((reference.digest.clone(), bytes));
            }
            segment_start.fork_replay_current(seed)?
        };
        drop(protected_parent);
        // Every alternative begins from the same actual H and replays the
        // same canonical prefix to T. No candidate advances the live parent.
        if !is_protected {
            for reference in &metadata.records {
                live(deadline)?;
                let bytes = self.load(
                    reference,
                    super::super::super::formal_ledger::MAX_RECORD,
                    deadline,
                )?;
                let entry = if has_window_origin {
                    decode::<super::window_storage::WindowRecord>(
                        &bytes,
                        super::super::super::formal_ledger::MAX_RECORD,
                    )?
                    .input()
                } else {
                    decode::<storage::Record>(
                        &bytes,
                        super::super::super::formal_ledger::MAX_RECORD,
                    )?
                    .input()
                };
                detached.replay_next(&entry.certificate, &entry.actions_cbor)?;
            }
        }
        let actions = self.load(
            &candidate.actions,
            oasis7_proto::distributed_bft::BFT_MAX_ACTIONS_CBOR_BYTES_V1,
            deadline,
        )?;
        if input_candidate
            .as_ref()
            .is_some_and(|value| value.actions_cbor != actions)
        {
            return Err(
                "protected pending archived actions differ from authorized original".into(),
            );
        }
        let executed = detached.execute(&actions)?;
        let reproduced = &executed.candidate;
        let exact_dependencies = detached.retained_nodes.with_accessed_nodes(|observed| {
            Ok(observed.keys().map(String::as_str).eq(candidate
                .dependencies
                .iter()
                .map(|reference| reference.digest.as_str())))
        })?;
        if reproduced.header != candidate.value.header
            || reproduced.commitment != candidate.value.execution_commitment
            || !exact_dependencies
            || detached.schedule.height() != candidate.height
            || detached
                .parent_world
                .execution_state_identity()
                .map_err(err)?
                .state_root_hash
                != metadata.cut.semantic_state_root
        {
            return Err("successor original T+1 actions/value/dependencies did not reproduce at the same cut".into());
        }
        if let Some(grant) = publications
            .iter()
            .find(|grant| grant.header_id().as_deref() == Ok(candidate.header_id.as_str()))
        {
            grant.validate_execution(&actions, &executed)?;
            grant.validate_pending_dependencies(
                candidate.dependencies.iter().map(|r| r.digest.clone()),
            )?;
        }
        // The overlay owns the second input-sized allocation. Release it
        // before loading the bounded verified package output, while keeping
        // the exact Input read hold alive through that readback as well.
        drop(executed);
        drop(detached);
        drop(input_candidate);
        let package = oasis7_node::NodeBftPrivateCandidatePackageV3 {
            candidate: oasis7_node::NodeBftPrivateCandidateV3 {
                public_value: candidate.value.clone(),
                actions_cbor: actions,
            },
            dependency_nodes: candidate
                .dependencies
                .iter()
                .map(|reference| {
                    Ok((
                        reference.digest.clone(),
                        self.load(
                            reference,
                            super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                            deadline,
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
        };
        drop(scoped);
        Ok(package)
    }
}

impl BftSuccessorWindowDriverVerifierV1 for ActualBftSuccessorWindowDriverVerifierV1 {
    fn verify_prepared_with_retention_context(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        public_state: &BftSignerStateV3,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionSealVerificationContextV1<'_>,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        self.verify_prepared_access(
            original,
            public_state,
            deadline,
            &ProtectedInputAccessV1::Owned(context),
        )
    }
    fn verify_adopted_with_retention_context(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        installed: &BftSignerStateV3,
        evidence: &BftSuccessorWindowAdoptedEvidenceV1,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionSealVerificationContextV1<'_>,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        self.verify_adopted_access(
            original,
            installed,
            evidence,
            deadline,
            &ProtectedInputAccessV1::Owned(context),
        )
    }

    fn verify_prepared(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        public_state: &BftSignerStateV3,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        if public_state.private_retention.is_some() {
            return Err(BftSignerErrorV3(
                "covered preparation requires same-backend protected owner context".into(),
            ));
        }
        self.verify_prepared_access(
            original,
            public_state,
            deadline,
            &ProtectedInputAccessV1::External,
        )
    }
    fn verify_adopted(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        installed: &BftSignerStateV3,
        evidence: &BftSuccessorWindowAdoptedEvidenceV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        if installed.private_retention.is_some() {
            return Err(BftSignerErrorV3(
                "covered adoption requires same-backend protected owner context".into(),
            ));
        }
        self.verify_adopted_access(
            original,
            installed,
            evidence,
            deadline,
            &ProtectedInputAccessV1::External,
        )
    }
}

#[path = "driver_bft_v3_window_replay.rs"]
mod replay;

#[path = "driver_bft_v3_window_chain.rs"]
mod chain;

#[path = "driver_bft_v3_successor_publication_replay.rs"]
mod publication_replay;

#[path = "driver_bft_v3_protected_retention_consumer.rs"]
mod protected_consumer;
use protected_consumer::ProtectedInputAccessV1;

impl ActualBftSuccessorWindowDriverVerifierV1 {
    fn verify_prepared_access(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        public_state: &BftSignerStateV3,
        deadline: Instant,
        access: &ProtectedInputAccessV1<'_, '_>,
    ) -> Result<(), BftSignerErrorV3> {
        let body = &original.request.intent.body;
        if public_state.revision != body.expected_revision
            || public_state.state_digest != body.expected_state_digest
        {
            return Err(BftSignerErrorV3(
                "successor private original protected revision/digest differs".into(),
            ));
        }
        super::successor_install::preflight_pointer(original).map_err(BftSignerErrorV3)?;
        self.verify_closure_with_origin_access(
            &body.checkpoint,
            public_state,
            &original.original_public_state,
            deadline,
            access,
        )
        .map(|_| ())
        .map_err(BftSignerErrorV3)
    }
    fn verify_adopted_access(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        installed: &BftSignerStateV3,
        evidence: &BftSuccessorWindowAdoptedEvidenceV1,
        deadline: Instant,
        access: &ProtectedInputAccessV1<'_, '_>,
    ) -> Result<(), BftSignerErrorV3> {
        self.verify_adopted_replay_then_readback(original, installed, evidence, deadline, access)
            .map_err(BftSignerErrorV3)
    }
    #[inline(never)]
    fn verify_adopted_replay_then_readback(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        installed: &BftSignerStateV3,
        evidence: &BftSuccessorWindowAdoptedEvidenceV1,
        deadline: Instant,
        access: &ProtectedInputAccessV1<'_, '_>,
    ) -> Result<(), String> {
        let root = self
            .installation_root
            .as_ref()
            .ok_or("successor trusted installation root is not configured")?;
        let origin = self.verify_original_closure_boxed_access(original, deadline, access)?;
        self.verify_adopted_readback(root, &origin, original, installed, evidence, deadline)
    }
    // Pointer/Ready decoding temporaries do not exist during actual World replay.
    #[inline(never)]
    fn verify_adopted_readback(
        &self,
        root: &super::successor_carriers::InstallationRootV1,
        origin: &VerifiedPrivateSuccessorBaseV1,
        original: &BftSuccessorWindowStoredRequestV1,
        installed: &BftSignerStateV3,
        evidence: &BftSuccessorWindowAdoptedEvidenceV1,
        deadline: Instant,
    ) -> Result<(), String> {
        let pointer = root.read_installed(&origin.origin, original, installed, deadline)?;
        let expected_origin = evidence
            .active_segment_origin
            .as_ref()
            .ok_or("successor active Origin commitment missing")?;
        let store = self
            .active_private_store
            .as_ref()
            .ok_or("successor trusted active private CAS is not configured")?;
        let (actual_origin, actual_ref) = root.read_window_origin(origin, deadline)?;
        if &actual_ref != expected_origin {
            return Err("successor active Origin differs from adoption commitment".into());
        }
        let records = root.path();
        super::node_execution::WindowPrivateStore::verify_ready(
            super::window_execution::layout::segment_path(records, origin)?
                .join(super::window_storage::DIRECTORY)
                .join(storage::NODE_PRIVATE_DIRECTORY),
            records,
            store.clone(),
            origin,
            &actual_origin,
            true,
        )?;
        if root.read_window_origin(origin, deadline)?.1 != actual_ref {
            return Err("successor active Origin changed during independent ready readback".into());
        }
        let mut expected = pointer.evidence();
        expected.active_segment_origin = Some(actual_ref);
        if expected != *evidence {
            return Err("successor installed readback evidence differs".into());
        }
        live(deadline)
    }
}

impl VerifiedPrivateSuccessorBaseV1 {
    pub(super) fn verify_carry_initializer_before_write(
        &self,
        records: &Path,
        store: &LocalCasStore,
    ) -> Result<(), String> {
        self.verify_retention_continuity()?;
        if self.metadata.publication_layout != Some(2) {
            return Ok(());
        }
        if self.verified_initializer
            != Some(carry_initialization::from_verified(
                &self.metadata,
                &self.publications,
            )?)
        {
            return Err("protected carry initializer differs from authenticated C0".into());
        }
        let auxiliary = node_execution::carry_auxiliary_usage(
            records,
            store,
            &self.metadata.source.material_identity,
            &self.metadata.private_instance_id,
        )?;
        let mut local = super::window_execution::layout::base_charge(self)?
            .checked_add(node_execution::carry_origin_usage(
                &self.metadata.source.material_identity,
                &self.metadata.private_instance_id,
                self.metadata.cut.height,
            )?)
            .and_then(|n| n.checked_add(auxiliary))
            .ok_or("carry complete local initialization overflow")?;
        let mut dependencies = BTreeMap::new();
        for candidate in &self.metadata.pending_candidates {
            let grant = self
                .publications
                .iter()
                .find(|grant| grant.header_id().as_deref() == Ok(candidate.header_id.as_str()))
                .ok_or("carry local original absent")?;
            local = local
                .checked_add(grant.carry_local_initial_usage(
                    candidate.roles.contains(&PendingRoleV1::RetainedLocal),
                )?)
                .ok_or("carry local initialization overflow")?;
            for reference in &candidate.dependencies {
                dependencies.insert(reference.digest.clone(), reference.bytes);
            }
        }
        let dependency_bytes = dependencies.values().try_fold(0u64, |sum, n| {
            sum.checked_add(*n).ok_or("carry dependency overflow")
        })?;
        if dependencies.len() > super::super::super::formal_dependencies::MAX_RETAINED_INDEX_NODES
            || dependency_bytes
                > super::super::super::formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES as u64
            || (local as u64)
                .checked_add(dependency_bytes)
                .ok_or("carry local capacity overflow")?
                > MAX_CLOSURE as u64
        {
            return Err("carry complete local initialization capacity".into());
        }
        // This is the original source owner's target CAS, not a repair source.
        for grant in &self.publications {
            if self
                .pending
                .iter()
                .any(|(_, package)| grant.matches_candidate(&package.candidate))
            {
                grant.require_carry_originals(store)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
impl VerifiedPrivateSuccessorBaseV1 {
    pub(super) fn verify_carry_initializer_mutations_for_test(
        &mut self,
        store: &LocalCasStore,
    ) -> Result<(), String> {
        if self.metadata.publication_layout != Some(2) {
            return Err("test requires protected initializer".into());
        }
        let price = carry_initialization::from_verified(&self.metadata, &self.publications)?;
        let roles = self.metadata.pending_candidates[0].roles.clone();
        let role = [
            PendingRoleV1::Proposal,
            PendingRoleV1::BlockPrevote,
            PendingRoleV1::PendingVote,
            PendingRoleV1::PendingProposal,
            PendingRoleV1::Equivocation,
        ]
        .into_iter()
        .find(|role| !roles.contains(role))
        .ok_or("test role set exhausted")?;
        self.metadata.pending_candidates[0].roles.insert(role);
        assert_eq!(
            carry_initialization::from_verified(&self.metadata, &self.publications)?,
            price,
            "role substitution has exactly the same price"
        );
        assert!(
            self.verify_retention_continuity().is_err(),
            "same-price foreign role set accepted"
        );
        self.metadata.pending_candidates[0].roles = roles;
        let original_ref = std::mem::replace(
            &mut self.metadata.publications[0].data.digest,
            "ab".repeat(32),
        );
        assert!(
            self.verify_retention_continuity().is_err(),
            "same-size original substitution accepted"
        );
        self.metadata.publications[0].data.digest = original_ref;
        for grant in &self.publications {
            if self
                .pending
                .iter()
                .any(|(_, package)| grant.matches_candidate(&package.candidate))
            {
                grant.require_carry_originals(store)?;
                grant.verify_carry_alias_negatives_for_test()?;
                grant.verify_alias_only_cold_target_for_test(store.root())?;
            }
        }
        self.verify_retention_continuity()
    }
}

#[path = "driver_bft_v3_actual_carry_parent.rs"]
mod actual_carry_parent;

#[path = "driver_bft_v3_active_admission_parent.rs"]
mod active_admission_parent;

pub(super) use active_admission_parent::VerifiedActiveAdmissionOwnerV1;
