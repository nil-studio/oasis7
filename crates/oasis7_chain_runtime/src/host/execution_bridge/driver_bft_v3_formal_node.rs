//! Actual qualified-H Node driver. Legacy execution remains unavailable here.
use super::super::super::super::super::storage as common_storage;
use super::super::super::formal_dependencies;
use super::super::super::formal_prefix_files as files;
use super::*;
use oasis7_consensus::tendermint_v3::BftHeaderValueV3;
use oasis7_distfs::protected_bft_v3::BftSignerBindingV3;
use oasis7_node::{
    NodeBftBuildContextV3, NodeBftExecutionCommitResultV3, NodeBftExecutionContextV3,
    NodeBftExecutionHeadV3, NodeBftPrivateCandidatePackageV3, NodeBftPrivateCandidateV3,
    NodeExecutionHook,
};

#[path = "driver_bft_v3_node_private_store.rs"]
mod private_store;
use private_store::NodePrivateStore;
#[path = "driver_bft_v3_parent_memo.rs"]
mod parent_memo;
use parent_memo::{ParentMemo, ParentMemoKey};
#[path = "driver_bft_v3_window_private.rs"]
mod window_private;
pub(super) use window_private::WindowPrivateStore;

#[path = "driver_bft_v3_node_auto_prepare.rs"]
mod auto_prepare;

#[cfg(test)]
#[path = "driver_bft_v3_node_publication_test_support.rs"]
mod publication_test_support;

/// Read-only original identity/bootstrap; it never returns a current execution head.
pub(crate) struct OriginalHeaderIdentityV1 {
    execution: FormalBftHeaderLedgerExecutionV3,
    private: NodePrivateStore,
    material_identity: String,
}
impl OriginalHeaderIdentityV1 {
    pub(crate) fn open(
        records: &Path,
        store: LocalCasStore,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        material: ClosedRecord,
        source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<Self, String> {
        let (private, retained) = NodePrivateStore::open(records, store.clone(), source)?;
        private.require_completed_origin()?;
        let execution = FormalBftHeaderLedgerExecutionV3::open_read_only_identity(
            records, store, anchor, target, material, retained,
        )?;
        let origin = execution.ledger.origin_digest()?;
        let material_identity = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                &origin,
                private.instance_id(),
                execution.ledger.material_pin_digest()?,
            ))
            .map_err(|_| "encode read-only original Node identity")?,
        );
        private.verify_original_binding(
            &origin,
            &material_identity,
            execution.origin.handoff.handoff_position(),
        )?;
        Ok(Self {
            execution,
            private,
            material_identity,
        })
    }
    #[cfg(test)]
    pub(crate) fn persist_auto_prepare_for_test(
        &self,
        records: &Path,
        binding: &BftSignerBindingV3,
        pin: &str,
        body: oasis7_distfs::protected_bft_v3::BftSuccessorWindowInstallIntentBodyV1,
        manifest: oasis7_distfs::protected_bft_v3::BftSuccessorWindowArchiveManifestV1,
    ) -> Result<(), String> {
        let owner = auto_prepare::AutoPrepareOwnerV1::from_identity(self, pin)?;
        let intent = auto_prepare::AutoPrepareIntentV1::new(owner, binding, body, manifest)?;
        self.verify_unchanged()?;
        intent.persist(records)?;
        self.verify_unchanged()
    }

    pub(crate) fn handoff(&self) -> &VerifiedBftHandoffTargetV2 {
        &self.execution.origin.handoff
    }
    pub(crate) fn initial_schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.execution.origin.schedule
    }
    pub(crate) fn material_identity(&self) -> &str {
        &self.material_identity
    }
    pub(crate) fn verify_unchanged(&self) -> Result<(), String> {
        self.execution.ledger.require_completed()?;
        self.private.verify_original_binding(
            &self.execution.ledger.origin_digest()?,
            &self.material_identity,
            self.handoff().handoff_position(),
        )
    }
}

pub(crate) struct FormalBftHeaderNodeDriverV3 {
    records_root: PathBuf,
    execution: FormalBftHeaderLedgerExecutionV3,
    private: NodePrivateStore,
    initial_schedule: BftHeaderHeightScheduleV3,
    material_identity: String,
    configured_binding: Option<String>,
    cursor_height: u64,
    installed_world: RuntimeWorld,
    peer_network: bool,
    membership_enabled: bool,
    parent_memo: Option<ParentMemo>,
}
impl FormalBftHeaderNodeDriverV3 {
    pub(crate) fn open_from_handoff(
        records: &Path,
        store: LocalCasStore,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        material: ClosedRecord,
        source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<Self, String> {
        Self::open_with_network_mode(records, store, anchor, target, material, source, false)
    }
    /// Multi-validator admission is explicit and must be paired with the
    /// runtime's authenticated public and separately authorized private paths.
    pub(crate) fn open_from_handoff_with_peer_network(
        records: &Path,
        store: LocalCasStore,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        material: ClosedRecord,
        source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<Self, String> {
        Self::open_with_network_mode(records, store, anchor, target, material, source, true)
    }
    fn open_with_network_mode(
        records: &Path,
        store: LocalCasStore,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        material: ClosedRecord,
        source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        peer_network: bool,
    ) -> Result<Self, String> {
        validate_storage_paths(records, store.root())?;
        let (mut private, retained_source) =
            NodePrivateStore::open(records, store.clone(), source)?;
        let mut execution = FormalBftHeaderLedgerExecutionV3::open_for_node(
            records,
            store,
            anchor,
            target,
            material,
            retained_source,
            true,
        )?;
        let initial_schedule = execution.origin.schedule.clone();
        if (!peer_network && initial_schedule.active_set().validators().len() != 1)
            || initial_schedule.pending_transition().is_some()
        {
            return Err(
                "native header Node requires explicit peer networking for multiple validators and does not yet admit pending transitions".into(),
            );
        }
        let cursor_height = execution.origin.handoff.handoff_position();
        let ledger_origin = execution.ledger.origin_digest()?;
        let material_identity = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                &ledger_origin,
                private.instance_id(),
                execution.ledger.material_pin_digest()?,
            ))
            .map_err(|_| "encode original Node material identity")?,
        );
        capture_h_index(&execution.origin)?;
        let nodes = execution.origin.retained_nodes.accessed_nodes()?;
        private.bind_origin(
            &ledger_origin,
            &material_identity,
            cursor_height,
            &nodes,
            &mut |bytes, future| {
                execution
                    .ledger
                    .reserve_private_material(bytes, future.clone(), &execution.store)
            },
        )?;
        execution.ledger.reserve_private_material(
            private.usage()?,
            private.publication_promises()?,
            &execution.store,
        )?;
        let installed_world = execution.origin.parent_world.clone();
        Ok(Self {
            records_root: canonical_storage_path(records)?,
            peer_network,
            membership_enabled: false,
            parent_memo: None,
            installed_world,
            execution,
            private,
            initial_schedule,
            material_identity,
            configured_binding: None,
            cursor_height,
        })
    }
    /// Export only after actual durable H-to-cut replay; this does not adopt a
    /// successor minimum or permit a new signing window.
    pub(crate) fn export_successor_private_closure(
        &mut self,
        request: super::successor_archive::SuccessorPrivateExportRequestV1<'_>,
    ) -> Result<ExportedBftSuccessorPrivateClosureV1, String> {
        let super::successor_archive::SuccessorPrivateExportRequestV1 {
            archive,
            source,
            public_state,
            previous_usage,
            height,
            header_id,
            sequence,
            predecessor,
            deadline,
        } = request;
        self.require_configured()?;
        let archive_root = canonical_storage_path(archive.root())?;
        crate::controlled_live_config::require_private_owned(&archive_root, true)?;
        for live in [
            &self.records_root,
            &canonical_storage_path(self.execution.store.root())?,
        ] {
            if archive_root.starts_with(live) || live.starts_with(&archive_root) {
                return Err("successor immutable archive overlaps live private storage".into());
            }
        }
        if source.material_identity != self.material_identity
            || source.binding_digest
                != *self
                    .configured_binding
                    .as_ref()
                    .ok_or("Node binding absent")?
            || self.cursor_height != height
        {
            return Err("successor export differs from installed original Node".into());
        }
        let mut pending_packages = Vec::new();
        let next_height = height
            .checked_add(1)
            .ok_or("successor pending height overflow")?;
        let mut required =
            super::successor_archive::required_pending(public_state.journal.as_ref())?;
        if let Some(local) = self.private.candidate(next_height)? {
            required
                .entry(local.public_value.block_id()?)
                .or_default()
                .insert(super::successor_archive::PendingRoleV1::RetainedLocal);
        }
        for (id, roles) in required {
            let value = self
                .private
                .public_value(next_height, &id)?
                .ok_or("original T+1 private candidate unavailable")?;
            pending_packages.push((roles, self.private.export(&value)?));
        }
        self.execution.export_successor_private_closure(
            super::successor_archive::SuccessorPrivateExportRequestV1 {
                archive,
                source,
                public_state,
                previous_usage,
                height,
                header_id,
                sequence,
                predecessor,
                deadline,
            },
            self.private.instance_id(),
            &pending_packages,
        )
    }

    pub(crate) fn install_successor_window(
        &mut self,
        request: super::successor_install::SuccessorPrivateInstallRequestV1<'_>,
    ) -> Result<oasis7_distfs::protected_bft_v3::BftSuccessorWindowAdoptedEvidenceV1, String> {
        let binding = request.binding;
        let archive = &request.archive;
        self.require_configured()?;
        if self.check_binding(binding)?
            != *self
                .configured_binding
                .as_ref()
                .ok_or("Node binding absent")?
        {
            return Err("successor install binding changed".into());
        }
        let archive_root = canonical_storage_path(archive.root())?;
        for live in [
            &self.records_root,
            &canonical_storage_path(self.execution.store.root())?,
        ] {
            if archive_root.starts_with(live) || live.starts_with(&archive_root) {
                return Err("successor installation archive overlaps live storage".into());
            }
        }
        self.execution.install_successor_window(
            &self.records_root,
            &self.material_identity,
            self.cursor_height,
            request,
        )
    }

    #[cfg(test)]
    pub(crate) fn inject_publication_failure(
        &mut self,
        height: u64,
        phase: &str,
    ) -> Result<(), String> {
        self.execution
            .inject_publication_failure(height, phase, false)
    }
    #[cfg(test)]
    pub(crate) fn clear_publication_failure(&mut self) {
        self.execution.ledger.clear_publication_failure();
    }
    pub(crate) fn handoff(&self) -> &VerifiedBftHandoffTargetV2 {
        &self.execution.origin.handoff
    }
    pub(crate) fn initial_schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.initial_schedule
    }
    pub(crate) fn material_identity(&self) -> &str {
        &self.material_identity
    }
    pub(crate) fn world(&self) -> &RuntimeWorld {
        &self.installed_world
    }

    fn check_binding(&self, binding: &BftSignerBindingV3) -> Result<String, String> {
        binding
            .validate()
            .map_err(|_| "Node signer binding rejected")?;
        if binding.initial_schedule() != &self.initial_schedule
            || binding.material_identity != self.material_identity
            || binding.handoff().target_digest() != self.handoff().target_digest()
            || binding.handoff().activation_digest() != self.handoff().activation_digest()
            || (!self.peer_network && self.initial_schedule.active_set().validators().len() != 1)
            || self.initial_schedule.pending_transition().is_some()
        {
            return Err(
                "Node signer binding differs from original actual H or supported membership".into(),
            );
        }
        let digest = binding
            .digest()
            .map_err(|_| "Node signer binding digest rejected")?;
        if self
            .configured_binding
            .as_ref()
            .is_some_and(|expected| expected != &digest)
        {
            return Err("Node restart changed its configured signer binding".into());
        }
        Ok(digest)
    }
    fn require_configured(&self) -> Result<(), String> {
        if self.configured_binding.is_none() {
            return Err("formal header Node driver is not configured".into());
        }
        Ok(())
    }
    /// Rebuild the exact parent before every preview. Failed or alternative
    /// previews cannot contaminate retained dependency observations.
    fn reset_parent(&mut self) -> Result<(), String> {
        self.execution.ledger.reload()?;
        self.execution.ledger.reserve_private_material(
            self.private.usage()?,
            self.private.publication_promises()?,
            &self.execution.store,
        )?;
        // Cached execution never replaces durable material/capacity checks.
        self.execution.ensure_origin_material()?;
        self.execution
            .ledger
            .preload(&self.execution.store, Some(self.cursor_height))?;
        let origin_nodes = self.private.origin_nodes()?;
        let key = ParentMemoKey::new(
            &self.material_identity,
            &self.execution.ledger.origin_digest()?,
            self.execution.ledger.material_pin_digest()?,
            self.cursor_height,
            self.execution
                .ledger
                .records()
                .filter(|record| record.height() <= self.cursor_height)
                .map(|record| record.digest())
                .collect::<Result<Vec<_>, _>>()?,
            &origin_nodes,
        )?;
        if let Some(memo) = &self.parent_memo
            && memo.key == key
        {
            self.execution.origin = memo.parent.fork_for_preview()?;
            return Ok(());
        }
        // A failed replacement must never revive a previous parent's memo.
        self.parent_memo = None;
        let h = self.handoff().handoff_position();
        if self.cursor_height == h {
            let seed = self.private.origin_nodes()?;
            self.execution.origin = FormalBftHeaderExecutionV3::from_handoff(
                self.execution.inputs.anchor.clone(),
                self.execution.inputs.target.clone(),
                &self.execution.inputs.material,
                self.execution.inputs.source.clone(),
                seed,
                true,
            )?;
        } else {
            let record = self
                .execution
                .ledger
                .record_at(self.cursor_height)
                .ok_or("Node replay cursor lost its original canonical record")?;
            let id = record.input().certificate.header.block_id()?;
            self.execution.verified_cut(self.cursor_height, &id)?;
        }
        // Historical replay is read-only: it must reject an unrecorded node.
        // Live execution may derive new nodes, but all parent reads must still
        // resolve solely through this verified strict overlay.
        let verified_parent = self.execution.origin.retained_nodes.clone();
        let mut parent_nodes: std::collections::BTreeMap<_, _> =
            self.private.origin_nodes()?.into_iter().collect();
        for (digest, bytes) in verified_parent.recorded_nodes()? {
            if parent_nodes
                .get(&digest)
                .is_some_and(|original| original != &bytes)
            {
                return Err("verified Node parent dependency bytes conflict".into());
            }
            parent_nodes.insert(digest, bytes);
        }
        let seed: Vec<_> = parent_nodes.into_iter().collect();
        let strict_source = formal_dependencies::RecordingBusinessIndexReaderV1::new_strict(
            verified_parent,
            seed.clone(),
        )?;
        let mut live = FormalBftHeaderExecutionV3::from_handoff(
            self.execution.inputs.anchor.clone(),
            self.execution.inputs.target.clone(),
            &self.execution.inputs.material,
            strict_source,
            seed,
            false,
        )?;
        for record in self
            .execution
            .ledger
            .records()
            .filter(|record| record.height() <= self.cursor_height)
        {
            let entry = record.input();
            live.replay_next(&entry.certificate, &entry.actions_cbor)?;
        }
        let parent = live.fork_for_preview()?;
        self.execution.origin = live;
        self.parent_memo = Some(ParentMemo { key, parent });
        Ok(())
    }
    fn expected(&self) -> NodeBftExecutionContextV3 {
        NodeBftExecutionContextV3::from_schedule(&self.execution.origin.schedule)
    }
    fn exact_parent(&mut self, expected: &NodeBftExecutionContextV3) -> Result<(), String> {
        self.require_configured()?;
        self.reset_parent()?;
        if &self.expected() != expected {
            return Err("Node candidate uses a stale or foreign actual parent".into());
        }
        Ok(())
    }
    fn validate_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        self.exact_parent(expected)?;
        let actual = self
            .execution
            .origin
            .build_candidate(&candidate.actions_cbor)
            .map_err(|_| "Node private candidate actual execution rejected")?;
        if actual.header != candidate.public_value.header
            || actual.commitment != candidate.public_value.execution_commitment
            || (!self.membership_enabled
                && (actual.commitment.transition.is_some()
                    || actual.commitment.transition_ref.is_some()))
        {
            return Err(
                "Node candidate differs from actual execution or supported membership".into(),
            );
        }
        Ok(())
    }
    /// Test-only detached World from the original qualified-parent execution.
    /// It does not retain a candidate, append a QC or install a canonical head.
    #[cfg(test)]
    pub(crate) fn preview_candidate_world_for_test(
        &mut self,
        context: NodeBftBuildContextV3,
    ) -> Result<RuntimeWorld, String> {
        self.exact_parent(&context.execution)?;
        if self.private.candidate(context.execution.height)?.is_some() {
            return Err("Node height has an original private candidate to resume".into());
        }
        if !self.membership_enabled {
            return Err("test World membership preview requires explicit opt-in".into());
        }
        let actions_cbor =
            serde_cbor::to_vec(&context.actions).map_err(|_| "encode private Node actions")?;
        self.execution
            .origin
            .preview_candidate_world_for_test(&actions_cbor)
    }

    fn private_candidate(
        &self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        value
            .validate_binding()
            .map_err(|_| "Node public candidate binding rejected")?;
        let height = value.execution_commitment.height;
        let candidate = if let Some(candidate) = self.private.candidate_for_value(value)? {
            candidate
        } else {
            let input = self
                .execution
                .ledger
                .record_at(height)
                .ok_or("original Node private candidate is unavailable")?
                .input();
            NodeBftPrivateCandidateV3 {
                public_value: BftHeaderValueV3 {
                    header: input.certificate.header,
                    execution_commitment: input.certificate.execution_commitment,
                },
                actions_cbor: input.actions_cbor,
            }
        };
        if &candidate.public_value != value {
            return Err("Node requested different private bytes or public candidate".into());
        }
        Ok(candidate)
    }
}

impl NodeExecutionHook for FormalBftHeaderNodeDriverV3 {
    fn enable_bft_v3_membership_execution(&mut self) -> Result<(), String> {
        if self.membership_enabled {
            return Ok(());
        }
        if self.configured_binding.is_some() {
            return Err("enable header Node membership before configuring execution".into());
        }
        self.membership_enabled = true;
        Ok(())
    }
    fn on_commit(
        &mut self,
        _: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        Err("qualified-H header driver refuses legacy execution".into())
    }
    fn configure_bft_v3_execution(
        &mut self,
        binding: &BftSignerBindingV3,
    ) -> Result<Option<NodeBftExecutionHeadV3>, String> {
        let digest = self.check_binding(binding)?;
        self.configured_binding = Some(digest);
        self.cursor_height = self.handoff().handoff_position();
        self.reset_parent()?;
        // Structural disk history cannot itself authorize a Node head.
        Ok(None)
    }
    fn current_bft_v3_execution_context(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
    ) -> Result<NodeBftExecutionContextV3, String> {
        self.require_configured()?;
        self.reset_parent()?;
        if schedule != &self.execution.origin.schedule {
            return Err("Node protocol and actual execution cursors differ".into());
        }
        Ok(self.expected())
    }
    fn pending_bft_v3_private_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        self.exact_parent(expected)?;
        self.private.candidate(expected.height)
    }
    fn build_bft_v3_candidate(
        &mut self,
        context: NodeBftBuildContextV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.exact_parent(&context.execution)?;
        if self.private.candidate(context.execution.height)?.is_some() {
            return Err("Node height has an original private candidate to resume".into());
        }
        let actions_cbor =
            serde_cbor::to_vec(&context.actions).map_err(|_| "encode private Node actions")?;
        let candidate = self
            .execution
            .origin
            .build_candidate(&actions_cbor)
            .map_err(|_| "Node action execution rejected")?;
        if !self.membership_enabled
            && (candidate.commitment.transition.is_some()
                || candidate.commitment.transition_ref.is_some())
        {
            return Err("Node membership transitions are not installed".into());
        }
        Ok(NodeBftPrivateCandidateV3 {
            public_value: BftHeaderValueV3 {
                header: candidate.header,
                execution_commitment: candidate.commitment,
            },
            actions_cbor,
        })
    }
    fn preview_bft_v3_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        self.validate_candidate(expected, candidate)
    }
    fn retain_bft_v3_reserved_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        self.validate_candidate(expected, candidate)?;
        let executed = self.execution.origin.execute(&candidate.actions_cbor)?;
        let nodes = self.execution.origin.retained_nodes.accessed_nodes()?;
        let prefix = self
            .execution
            .ledger
            .records()
            .map(|record| record.input().certificate)
            .collect::<Vec<_>>();
        let plan = super::super::publication_plan::PublicationPlanV1::from_actual(
            &self.execution.origin.schedule,
            &executed,
            &candidate.actions_cbor,
            &nodes,
            &prefix,
            storage::RECORD_DOMAIN,
        )?;
        self.private
            .retain_planned(candidate, &plan, &nodes, &mut |bytes, future| {
                self.execution.ledger.reserve_private_material(
                    bytes,
                    future.clone(),
                    &self.execution.store,
                )
            })
    }

    fn retain_bft_v3_private_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        self.validate_candidate(expected, candidate)?;
        let nodes = self.execution.origin.retained_nodes.accessed_nodes()?;
        let future = self.private.publication_promises()?;
        self.private.retain(candidate, &nodes, &mut |bytes| {
            self.execution.ledger.reserve_private_material(
                bytes,
                future.clone(),
                &self.execution.store,
            )
        })
    }
    fn export_bft_v3_private_candidate(
        &mut self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidatePackageV3, String> {
        self.require_configured()?;
        self.private.export(value)
    }
    fn retain_received_bft_v3_private_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV3,
        package: &NodeBftPrivateCandidatePackageV3,
    ) -> Result<String, String> {
        if package.dependency_nodes.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES
            || !package.dependency_nodes.windows(2).all(|w| w[0].0 < w[1].0)
        {
            return Err("received Node dependency order/count differs".into());
        }
        let mut total = 0usize;
        for (digest, bytes) in &package.dependency_nodes {
            formal_dependencies::validate_node(digest, bytes)?;
            total = total
                .checked_add(bytes.len())
                .filter(|n| *n <= formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES)
                .ok_or("received Node dependency byte capacity")?;
        }
        let supplied: std::collections::BTreeMap<_, _> =
            package.dependency_nodes.iter().cloned().collect();
        formal_dependencies::validate_nodes(&supplied)?;
        // Never seed the strict parent with untrusted network material. The
        // actual transition must independently reproduce this exact closure.
        self.validate_candidate(expected, &package.candidate)?;
        let nodes = self.execution.origin.retained_nodes.accessed_nodes()?;
        if nodes != supplied {
            return Err("received Node dependencies differ from actual original transition".into());
        }
        let future = self.private.publication_promises()?;
        self.private
            .retain_received(&package.candidate, &nodes, &mut |bytes| {
                self.execution.ledger.reserve_private_material(
                    bytes,
                    future.clone(),
                    &self.execution.store,
                )
            })
    }
    fn load_bft_v3_private_candidate(
        &mut self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.require_configured()?;
        self.private_candidate(value)
    }
    fn lookup_bft_v3_public_value(
        &mut self,
        height: u64,
        header_id: &str,
    ) -> Result<Option<BftHeaderValueV3>, String> {
        self.require_configured()?;
        if header_id.len() != 64
            || !header_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Node public lookup header identity rejected".into());
        }
        if let Some(value) = self.private.public_value(height, header_id)? {
            return Ok(Some(value));
        }
        self.execution.ledger.reload()?;
        let Some(record) = self.execution.ledger.record_at(height) else {
            return Ok(None);
        };
        let input = record.input();
        let value = BftHeaderValueV3 {
            header: input.certificate.header,
            execution_commitment: input.certificate.execution_commitment,
        };
        if value.block_id()? != header_id {
            return Ok(None);
        }
        Ok(Some(value))
    }
    fn validate_bft_v3_static_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        let envelope =
            oasis7::consensus_action_payload::decode_consensus_action_payload_envelope(payload)
                .map_err(|_| "Node action payload encoding rejected")?;
        match envelope.body {
            oasis7::consensus_action_payload::ConsensusActionPayloadBody::RuntimeAction {
                ..
            } => Ok(()),
            _ => Err("formal header Node requires a closed RuntimeAction payload".into()),
        }
    }
    fn validate_bft_v3_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        self.require_configured()?;
        // Actual batch execution still checks the original signed payload and
        // immutable parent policy before retention or signing.
        let envelope =
            oasis7::consensus_action_payload::decode_consensus_action_payload_envelope(payload)
                .map_err(|_| "Node action payload encoding rejected")?;
        match envelope.body {
            oasis7::consensus_action_payload::ConsensusActionPayloadBody::RuntimeAction {
                ..
            } => Ok(()),
            _ => Err("formal header Node requires a closed RuntimeAction payload".into()),
        }
    }
    fn validate_bft_v3_action_batch(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
        actions: &[oasis7_node::NodeConsensusAction],
    ) -> Result<(), String> {
        let context = self.current_bft_v3_execution_context(schedule)?;
        let bytes = serde_cbor::to_vec(&actions).map_err(|_| "encode Node batch admission")?;
        let actual = self
            .execution
            .origin
            .build_candidate(&bytes)
            .map_err(|_| "Node immutable parent-policy admission rejected")?;
        if actual.commitment.height != context.height
            || (!self.membership_enabled
                && (actual.commitment.transition.is_some()
                    || actual.commitment.transition_ref.is_some()))
        {
            return Err("Node batch changes unsupported membership".into());
        }
        Ok(())
    }
    fn load_bft_v3_commit_history_page(
        &mut self,
        binding: &BftSignerBindingV3,
        after: u64,
        max: usize,
    ) -> Result<Vec<FormalBftCertifiedHeaderV3>, String> {
        self.check_binding(binding)?;
        if max == 0 || max > 64 {
            return Err("Node history page capacity".into());
        }
        self.execution.ledger.reload()?;
        Ok(self
            .execution
            .ledger
            .records()
            .filter(|record| record.height() > after)
            .take(max)
            .map(|record| record.input().certificate)
            .collect())
    }
    fn preflight_bft_v3_capacity(
        &mut self,
        context: &NodeBftBuildContextV3,
    ) -> Result<oasis7_node::NodeBftCapacityAdmissionV3, String> {
        use formal_dependencies::ContentNodeCapacityV1;
        use oasis7_node::NodeBftCapacityAdmissionV3 as Admission;
        self.exact_parent(&context.execution)?;
        let actions =
            serde_cbor::to_vec(&context.actions).map_err(|_| "encode capacity actions")?;
        let current = self.execution.origin.plan_candidate(&actions)?.1;
        if matches!(current, ContentNodeCapacityV1::Fits { .. })
            && self.execution.ledger.records().count() < 64
        {
            return Ok(Admission::Fits);
        }
        // Empty segments cannot improve the operation's capacity, and cannot
        // provide an applied cut. No speculative checkpoint loop is permitted.
        if self.cursor_height == self.handoff().handoff_position() {
            return Ok(Admission::Blocked);
        }
        // reset_parent exposes a live preview fork (legacy has no closed_parent).
        // Obtain the opaque strict durable replay again before selecting a
        // new layer; speculative reads cannot authorize that parent inventory.
        let record = self
            .execution
            .ledger
            .record_at(self.cursor_height)
            .ok_or("capacity cut lost original canonical record")?;
        let id = record.input().certificate.header.block_id()?;
        self.execution.verified_cut(self.cursor_height, &id)?;
        Ok(
            if matches!(
                self.execution.origin.capacity_in_new_layer(&actions)?,
                ContentNodeCapacityV1::Fits { .. }
            ) {
                Admission::NeedsRoll
            } else {
                Admission::Blocked
            },
        )
    }

    fn commit_bft_v3_certificate(
        &mut self,
        token: VerifiedBftHeaderCertificateV3,
    ) -> Result<NodeBftExecutionCommitResultV3, String> {
        #[cfg(test)]
        let mut diagnostic_stage = "entry";
        let result: Result<NodeBftExecutionCommitResultV3, String> = (|| {
            #[cfg(test)]
            {
                diagnostic_stage = "configure";
            }
            self.require_configured()?;
            #[cfg(test)]
            {
                diagnostic_stage = "reset_parent";
            }
            self.reset_parent()?;
            if token.schedule() != &self.execution.origin.schedule {
                return Err("Node commit differs from actual parent schedule".into());
            }
            let certificate = token.certified_header().clone();
            let value = BftHeaderValueV3 {
                header: certificate.header.clone(),
                execution_commitment: certificate.execution_commitment.clone(),
            };
            #[cfg(test)]
            {
                diagnostic_stage = "private_candidate";
            }
            let private = self.private_candidate(&value)?;
            #[cfg(test)]
            {
                diagnostic_stage = "execute";
            }
            let executed = self
                .execution
                .origin
                .replay_next(&certificate, &private.actions_cbor)
                .map_err(|reason| {
                    #[cfg(test)]
                    eprintln!(
                        "wire3_private_commit_execution_failure category={}",
                        commit_failure_category(&reason)
                    );
                    #[cfg(not(test))]
                    let _ = reason;
                    "Node canonical private execution rejected"
                })?;
            #[cfg(test)]
            {
                diagnostic_stage = "dependencies";
            }
            let nodes = self.execution.origin.retained_nodes.accessed_nodes()?;
            let height = certificate.execution_commitment.height;
            let previous = if height == self.initial_schedule.height() {
                self.execution.ledger.origin_digest()?
            } else {
                self.execution
                    .ledger
                    .record_at(height - 1)
                    .ok_or("Node canonical predecessor unavailable")?
                    .digest()?
            };
            let entry = PrivateHeaderEntryV3 {
                certificate: certificate.clone(),
                actions_cbor: private.actions_cbor.clone(),
            };
            #[cfg(test)]
            {
                diagnostic_stage = "record";
            }
            let record = storage::Record::from_actual(
                self.execution.ledger.origin()?,
                &previous,
                &entry,
                &executed,
                nodes.keys().cloned().collect(),
            )?;
            #[cfg(test)]
            {
                diagnostic_stage = "reserve";
            }
            self.execution.ledger.reserve_private_material(
                self.private.usage()?,
                self.private.publication_promises()?,
                &self.execution.store,
            )?;
            let accounting = self
                .execution
                .ledger
                .publication_accounting(&self.execution.store)?;
            let claim =
                self.private
                    .publication_claim_for_actual(&private, &executed, &accounting)?;
            let record = record.with_publication_reservation(
                claim.as_ref().map(|retained| retained.reference.clone()),
            )?;
            let mut entries = self
                .execution
                .ledger
                .records()
                .filter(|record| record.height() < height)
                .map(|record| record.input().certificate)
                .collect::<Vec<_>>();
            entries.push(certificate.clone());
            let public_prefix = FormalBftCertifiedHeaderPrefixV3 {
                schema_version: 1,
                entries,
            };
            let proof_bytes = public_prefix.canonical_bytes()?.len();
            #[cfg(test)]
            {
                diagnostic_stage = "publish";
            }
            if let Some(retained) = claim {
                self.execution.ledger.accept_prepared_with_publication(
                    record,
                    super::super::super::formal_ledger::PreparedRecordMaterialV1 {
                        snapshot_bytes: &executed.snapshot_bytes,
                        journal_bytes: &executed.journal_bytes,
                        nodes: &nodes,
                        store: &self.execution.store,
                    },
                    retained.claim.as_ref(),
                    Some(proof_bytes),
                )?;
            } else {
                self.execution.ledger.charge_proof(proof_bytes)?;
                self.execution.ledger.accept_prepared(
                    record,
                    &executed.snapshot_bytes,
                    &executed.journal_bytes,
                    &nodes,
                    &self.execution.store,
                    true,
                )?;
            }
            // Obtain the installed World only from a fresh strict durable replay.
            #[cfg(test)]
            {
                diagnostic_stage = "durable_replay";
            }
            let cut = self.execution.verified_cut(height, token.block_id())?;
            let c = cut.proof().commitment();
            let install = blake3_hex(
                &serde_cbor::to_vec(&(
                    "oasis7.formal.bft.node_install.v3",
                    &self.material_identity,
                    cut.durable_record_digest(),
                    token.block_id(),
                    &certificate.header.execution_commitment_hash,
                    &c.execution_state_root,
                    &c.state_root,
                    &c.post_manifest_hash,
                ))
                .map_err(|_| "encode exact Node canonical install")?,
            );
            self.installed_world = cut.world().clone();
            self.cursor_height = height;
            let installed_semantic_root = self
                .world()
                .execution_state_identity()
                .map_err(|_| "installed Node World identity unavailable")?
                .state_root_hash;
            if installed_semantic_root != c.state_root {
                return Err("installed Node World differs from durable execution receipt".into());
            }
            Ok(NodeBftExecutionCommitResultV3 {
                height,
                header_id: token.block_id().into(),
                execution_commitment_hash: certificate.header.execution_commitment_hash,
                execution_state_root: c.execution_state_root.clone(),
                state_root: installed_semantic_root,
                post_manifest_hash: c.post_manifest_hash.clone(),
                driver_install_digest: install,
            })
        })();
        #[cfg(test)]
        if let Err(reason) = &result {
            let category = commit_failure_category(reason);
            eprintln!(
                "wire3_private_commit_failure stage={} category={} cursor={}",
                diagnostic_stage, category, self.cursor_height
            );
        }
        result
    }
}

/// Retain the bounded complete H index before the first signing lease: future
/// actions can touch branches no earlier proposal happened to observe.
fn capture_h_index(origin: &FormalBftHeaderExecutionV3) -> Result<(), String> {
    use oasis7_distfs::controlled_authority::formal::business_index::{
        BUSINESS_INDEX_DEPTH, BusinessIndexNodeKindV1, BusinessIndexNodeV1, empty_subtree_hashes,
    };
    let empty = empty_subtree_hashes()?;
    let mut stack = vec![(
        origin.parent_result_tree.index_root().node_root.clone(),
        0usize,
    )];
    let mut visited = std::collections::BTreeSet::new();
    while let Some((digest, depth)) = stack.pop() {
        if depth > BUSINESS_INDEX_DEPTH {
            return Err("Node H index depth exceeded".into());
        }
        if digest == empty[depth] || !visited.insert((digest.clone(), depth)) {
            continue;
        }
        if visited.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES {
            return Err("Node H index count capacity".into());
        }
        let bytes = origin
            .retained_nodes
            .read_node(&digest)?
            .ok_or("original Node H index node missing")?;
        match BusinessIndexNodeV1::decode(&bytes)?.kind {
            BusinessIndexNodeKindV1::Branch {
                depth: stored,
                left_hash,
                right_hash,
            } if usize::from(stored) == depth && depth < BUSINESS_INDEX_DEPTH => {
                stack.push((right_hash, depth + 1));
                stack.push((left_hash, depth + 1));
            }
            BusinessIndexNodeKindV1::Leaf(_) if depth == BUSINESS_INDEX_DEPTH => {}
            _ => return Err("Node H index node depth differs".into()),
        }
    }
    Ok(())
}

#[path = "driver_bft_v3_node_deferred.rs"]
mod deferred;

#[cfg(test)]
fn commit_failure_category(reason: &str) -> &'static str {
    if reason.contains("retained index node count exceeds bound") {
        "index_node_capacity"
    } else if reason.contains("retained index bytes")
        || reason.contains("retained dependency bytes exceed")
    {
        "index_byte_capacity"
    } else if reason.contains("unrecorded index node") {
        "unrecorded_node"
    } else if reason.contains("unretained dependency") {
        "unretained_dependency"
    } else if reason.contains("capacity") || reason.contains("exceeds bound") {
        "other_capacity"
    } else if reason.contains("conflict") || reason.contains("differs") {
        "identity_conflict"
    } else {
        "other"
    }
}
