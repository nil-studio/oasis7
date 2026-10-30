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
}
/// Constructed only after actual H/archive replay and original pending reproduction.
/// It grants private execution continuity, not provider signing authority.
pub(crate) struct VerifiedPrivateSuccessorBaseV1 {
    pub(super) origin: storage::Origin,
    pub(super) anchor: VerifiedFormalReadAnchorV1,
    pub(super) target: BftProfileTargetV2,
    pub(super) execution: Box<FormalBftHeaderExecutionV3>,
    pub(super) metadata: PrivateClosureV1,
    pub(super) original: Option<BftSuccessorWindowStoredRequestV1>,
    pub(super) public_state: BftSignerStateV3,
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
        })
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
    fn verify_one_closure(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        public_state: &BftSignerStateV3,
        expected_public_ref: &BftSuccessorWindowArchiveRefV1,
        deadline: Instant,
        predecessor: Option<&VerifiedPrivateSuccessorBaseV1>,
    ) -> Result<VerifiedPrivateSuccessorBaseV1, String> {
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
            execution,
            previous,
            segment_start,
            window_origin,
        } = self.replay_segment(
            checkpoint,
            public_state,
            &manifest,
            &metadata,
            deadline,
            predecessor,
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
        let mut pending = Vec::new();
        let mut required = required_pending(public_state.journal.as_ref())?;
        for candidate in &metadata.pending_candidates {
            let mut journal_roles = candidate.roles.clone();
            journal_roles.remove(&PendingRoleV1::RetainedLocal);
            let expected_roles = required.remove(&candidate.header_id).unwrap_or_default();
            if expected_roles != journal_roles || (candidate.roles.is_empty()) {
                return Err(
                    "successor pending package role/header differs from original journal".into(),
                );
            }
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
            // Each alternative starts from the same independently replayed T.
            // Detached execution cannot advance the committed parent or sign.
            let mut detached = segment_start.fork_replay_current(seed)?;
            for reference in &metadata.records {
                live(deadline)?;
                let bytes = self.load(
                    reference,
                    super::super::super::formal_ledger::MAX_RECORD,
                    deadline,
                )?;
                let entry = if window_origin.is_some() {
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
            let actions = self.load(
                &candidate.actions,
                oasis7_proto::distributed_bft::BFT_MAX_ACTIONS_CBOR_BYTES_V1,
                deadline,
            )?;
            let reproduced = detached.build_candidate(&actions)?;
            let observed = detached.retained_nodes.accessed_nodes()?;
            if reproduced.header != candidate.value.header
                || reproduced.commitment != candidate.value.execution_commitment
                || observed
                    .keys()
                    .map(String::as_str)
                    .ne(candidate.dependencies.iter().map(|r| r.digest.as_str()))
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
            pending.push((candidate.roles.clone(), oasis7_node::NodeBftPrivateCandidatePackageV3 {
                candidate: oasis7_node::NodeBftPrivateCandidateV3 { public_value: candidate.value.clone(), actions_cbor: actions },
                dependency_nodes: candidate.dependencies.iter().map(|reference| {
                    Ok((reference.digest.clone(), self.load(reference, super::super::super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES, deadline)?))
                }).collect::<Result<Vec<_>, String>>()?,
            }));
        }
        if !required.is_empty() {
            return Err("successor original journal is missing required private candidates".into());
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
        live(deadline)?;
        Ok(VerifiedPrivateSuccessorBaseV1 {
            origin,
            anchor: self.anchor.clone(),
            target: self.target.clone(),
            execution,
            metadata,
            original: None,
            public_state: public_state.clone(),
            pending,
        })
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
        super::successor_install::preflight_pointer(original).map_err(BftSignerErrorV3)?;
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
        original: &BftSuccessorWindowStoredRequestV1,
        installed: &BftSignerStateV3,
        evidence: &BftSuccessorWindowAdoptedEvidenceV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        let result = (|| -> Result<(), String> {
            let root = self
                .installation_root
                .as_ref()
                .ok_or("successor trusted installation root is not configured")?;
            let origin = self.verify_original_closure_with_origin(original, deadline)?;
            let pointer = root.read_installed(&origin.origin, original, installed, deadline)?;
            let expected_origin = evidence
                .active_segment_origin
                .as_ref()
                .ok_or("successor active Origin commitment missing")?;
            let store = self
                .active_private_store
                .as_ref()
                .ok_or("successor trusted active private CAS is not configured")?;
            let (actual_origin, actual_ref) = root.read_window_origin(&origin, deadline)?;
            if &actual_ref != expected_origin {
                return Err("successor active Origin differs from adoption commitment".into());
            }
            let records = root.path();
            super::node_execution::WindowPrivateStore::verify_ready(
                super::window_execution::layout::segment_path(records, &origin)?
                    .join(super::window_storage::DIRECTORY)
                    .join(storage::NODE_PRIVATE_DIRECTORY),
                store.clone(),
                &origin,
                &actual_origin,
                true,
            )?;
            if root.read_window_origin(&origin, deadline)?.1 != actual_ref {
                return Err(
                    "successor active Origin changed during independent ready readback".into(),
                );
            }
            let mut expected = pointer.evidence();
            expected.active_segment_origin = Some(actual_ref);
            if expected != *evidence {
                return Err("successor installed readback evidence differs".into());
            }
            live(deadline)
        })();
        result.map_err(BftSignerErrorV3)
    }
}

#[path = "driver_bft_v3_window_replay.rs"]
mod replay;

#[path = "driver_bft_v3_window_chain.rs"]
mod chain;
