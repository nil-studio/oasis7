//! Independent first-window admission parent reconstruction.
use super::super::super::super::super::super::storage as common_storage;
use super::super::super::super::formal_ledger::OriginContract;
use super::super::super::super::formal_ledger::RecordContract;
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAuthenticatedAdmissionParentV1 as AdmissionParent,
    BftPrivateRetentionVerificationContextV1 as Current, BftSignerBindingV3 as Binding,
};

/// No serialization or public constructor: metadata cannot manufacture this owner.
pub(in super::super) struct VerifiedActiveAdmissionOwnerV1 {
    pub(in super::super) base: Arc<VerifiedPrivateSuccessorBaseV1>,
    pub(in super::super) execution: Box<FormalBftHeaderExecutionV3>,
    pub(in super::super) origin: super::super::window_storage::WindowOrigin,
    pub(in super::super) parent_state_digest: String,
    retained_baseline_refs: BTreeSet<String>,
    private_directory: PathBuf,
}
impl VerifiedActiveAdmissionOwnerV1 {
    pub(in super::super) fn private_directory(&self) -> &Path {
        &self.private_directory
    }
    pub(in super::super) fn retained_baseline_refs(&self) -> &BTreeSet<String> {
        &self.retained_baseline_refs
    }
}
impl ActualBftSuccessorWindowDriverVerifierV1 {
    #[inline(never)]
    pub(in super::super) fn recover_active_admission_parent(
        &self,
        binding: &Binding,
        context: &Current<'_>,
        parent: &AdmissionParent<'_>,
        deadline: Instant,
    ) -> Result<VerifiedActiveAdmissionOwnerV1, String> {
        live(deadline)?;
        // This is the original configured flag; a caller-provided role is never authority.
        if !context.attestor().allow_authenticated_successor_origins {
            return Err("fixed original role denies fresh Active admission".into());
        }
        if parent.live_signer() != context.signer() {
            return Err("Active parent differs from exact current backend fence".into());
        }
        let active = context.active_parent().map_err(err)?;
        let original = active.original_request();
        let bytes = self.load(&original.original_public_state, MAX_CLOSURE, deadline)?;
        let historical = context
            .authenticated_original_c0(binding, &bytes)
            .map_err(err)?;
        if historical.original_request() != original {
            return Err("Active C0 original request differs".into());
        }
        let base: Arc<VerifiedPrivateSuccessorBaseV1> = self
            .verify_original_closure_boxed_access(
                original,
                deadline,
                &ProtectedInputAccessV1::HistoricalC0(&historical),
            )?
            .into();
        if &base.public_state != historical.original_state()
            || active.base_schedule() != &base.execution.schedule
        {
            return Err("Active C0 actual replay differs from authenticated original".into());
        }
        base.verify_retention_continuity()?;
        self.recover_active_suffix(
            binding,
            context,
            parent,
            deadline,
            &active,
            &historical,
            bytes,
            base,
        )
    }

    // Suffix owned temporaries must not occupy the native stack while C0 is
    // independently replayed above. Authority inputs remain authenticated borrows;
    // the owned byte buffer is released at its original post-reconstruction point.
    #[inline(never)]
    fn recover_active_suffix(
        &self,
        binding: &Binding,
        context: &Current<'_>,
        parent: &AdmissionParent<'_>,
        deadline: Instant,
        active: &oasis7_distfs::protected_bft_v3::VerifiedBftSignerActiveWindowV1,
        historical: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionHistoricalOwnedC0V1<'_>,
        bytes: Vec<u8>,
        base: Arc<VerifiedPrivateSuccessorBaseV1>,
    ) -> Result<VerifiedActiveAdmissionOwnerV1, String> {
        let original = active.original_request();
        let installed = context
            .reconstruct_installed_original(binding, original, &bytes)
            .map_err(err)?;
        drop(bytes);
        let state = installed.installed_state();
        let immutable = state
            .successor_window
            .as_ref()
            .and_then(|w| w.request(&original.request.intent.body.checkpoint.request_id))
            .ok_or("Active historical installed original absent")?;
        let evidence = original
            .request
            .adopted_evidence
            .as_ref()
            .ok_or("Active adoption absent")?;
        let root = self
            .installation_root
            .as_ref()
            .ok_or("Active installation root absent")?;
        self.verify_adopted_readback(root, &base, immutable, state, evidence, deadline)?;
        let (origin, reference) = root.read_window_origin(&base, deadline)?;
        if &reference != active.active_segment_origin() {
            return Err("Active actual Origin differs from signed ancestry".into());
        }
        let private_directory =
            super::super::window_execution::layout::segment_path(root.path(), &base)?
                .join(super::super::window_storage::DIRECTORY)
                .join(storage::NODE_PRIVATE_DIRECTORY);
        let retained_baseline_refs = base.retained_baseline_refs()?;
        let records = root.read_active_prefix(&base, &origin, parent.history(), deadline)?;
        let store = self
            .active_private_store
            .as_ref()
            .ok_or("Active CAS absent")?;
        let mut references = BTreeSet::new();
        let mut recovered_nodes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut recovered_bytes = 0usize;
        let mut prepin = BTreeSet::new();
        let mut fresh_prepin = BTreeSet::new();
        let pin_shards = store
            .list_pin_scope_shards(super::super::window_storage::DIRECTORY)
            .map_err(err)?;
        for (record, reserved) in &records {
            let shard = format!(
                "origin-{}-height-{:020}",
                record.origin_digest(),
                record.height()
            );
            if *reserved && !pin_shards.contains(&shard) {
                let header_id = record.input().certificate.header.block_id()?;
                if context.original_inputs().any(|input| {
                    input.key.ledger_origin_digest == reference.digest
                        && input.key.height == record.height()
                        && input.key.header_id == header_id
                }) {
                    // Locate only. Authorization and candidate overlay remain scoped
                    // to this record after its actual parent has been replayed.
                    prepin.insert(record.height());
                    fresh_prepin.insert(record.height());
                    continue;
                }

                // The live union only locates candidates. Exact C0 witness + owned
                // original ref below supplies authority; union membership never does.
                let mut exact = None;
                for candidate in context.original_inputs().filter(|input| {
                    input.key.height == record.height() && input.key.header_id == header_id
                }) {
                    let Some(entry) = context.original_entry(&candidate.key).map_err(err)? else {
                        continue;
                    };
                    let reference = oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputRefV1::from_entry(
                        &context.attestor().source, entry).map_err(err)?;
                    if base.verify_original_carry_reference(&reference).is_err() {
                        continue;
                    }
                    let original = historical.owned_original(&reference).map_err(err)?;
                    if exact.is_some() {
                        return Err("pre-pin recovery has ambiguous authorized original".into());
                    }
                    exact = Some(original);
                }
                let original =
                    exact.ok_or("pre-pin suffix has no exact authenticated original Input")?;
                let candidate = node_execution::source_admission_candidate(original)?;
                if candidate.public_value.header != record.input().certificate.header
                    || candidate.public_value.execution_commitment
                        != record.input().certificate.execution_commitment
                    || candidate.actions_cbor != record.input().actions_cbor
                {
                    return Err("pre-pin original Input differs from signed record".into());
                }
                for object in original.objects.iter().filter(|object|
                    object.kind == oasis7_distfs::protected_bft_v3::BftPrivateRetentionInputObjectKindV1::Dependency) {
                    object.reference.validate_bytes(&object.bytes).map_err(err)?;
                    if object.bytes.len() > formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES {
                        return Err("pre-pin original dependency node capacity".into());
                    }
                    if let Some(old) = recovered_nodes.get(&object.reference.digest) {
                        if old != &object.bytes { return Err("pre-pin original dependency conflicts".into()); }
                    } else {
                        recovered_bytes = recovered_bytes.checked_add(object.bytes.len())
                            .filter(|n| *n <= formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES)
                            .ok_or("pre-pin dependency byte capacity")?;
                        if recovered_nodes.len() >= formal_dependencies::MAX_RETAINED_INDEX_NODES {
                            return Err("pre-pin dependency object capacity".into());
                        }
                        recovered_nodes.insert(object.reference.digest.clone(), object.bytes.clone());
                    }
                }
                prepin.insert(record.height());
            } else {
                store
                    .verify_pin_scope_shard(
                        super::super::window_storage::DIRECTORY,
                        &shard,
                        &record.references(),
                    )
                    .map_err(err)?;
                references.extend(record.business_index_refs().iter().cloned());
            }
        }
        for digest in references
            .iter()
            .filter(|digest| recovered_nodes.contains_key(*digest))
        {
            let expected = &recovered_nodes[digest];
            if common_storage::read_cas_bounded(
                store,
                digest,
                formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            )? != *expected
            {
                return Err("pre-pin original and retained dependency conflict".into());
            }
        }
        references.retain(|digest| !recovered_nodes.contains_key(digest));
        if references
            .len()
            .checked_add(recovered_nodes.len())
            .is_none_or(|n| n > formal_dependencies::MAX_RETAINED_INDEX_NODES)
        {
            return Err("pre-pin dependency object capacity".into());
        }
        let remaining = formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES
            .checked_sub(recovered_bytes)
            .ok_or("pre-pin dependency byte capacity")?;
        let mut nodes = formal_storage::read_business_index_nodes_bounded(
            &references,
            formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            remaining,
            |reference, limit| common_storage::read_cas_bounded(store, reference, limit),
        )?;
        nodes.append(&mut recovered_nodes);
        let total = nodes.values().try_fold(0usize, |total, bytes| {
            total
                .checked_add(bytes.len())
                .ok_or("pre-pin dependency byte overflow")
        })?;
        if nodes.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES
            || total > formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES
        {
            return Err("pre-pin dependency inventory exceeds existing capacity".into());
        }
        let mut execution = base
            .execution
            .fork_strict_with_seed(nodes.into_iter().collect())?;
        // This execution owns the strict reader installed by the fork above.
        // Only the authenticated pre-pin overlay below changes its layer mode.
        let mut derived_current_layer = false;
        let mut previous = origin.digest()?;
        let mut prefix_history = Vec::new();
        for (record, _) in records {
            let prefix_owner = VerifiedActiveAdmissionOwnerV1 {
                base: base.clone(),
                execution: Box::new(execution.fork_live_closed()?),
                origin: origin.clone(),
                parent_state_digest: String::new(),
                retained_baseline_refs: retained_baseline_refs.clone(),
                private_directory: private_directory.clone(),
            };
            live(deadline)?;
            let input = record.input();
            if fresh_prepin.contains(&record.height()) {
                let header_id = input.certificate.header.block_id()?;
                let key = context
                    .original_inputs()
                    .find(|original| {
                        original.key.ledger_origin_digest == reference.digest
                            && original.key.height == record.height()
                            && original.key.header_id == header_id
                    })
                    .map(|original| original.key.clone())
                    .ok_or("Fresh pre-pin exact key missing")?;
                let original_context = context.original_ready(binding, &key).map_err(err)?;
                let entry = original_context
                    .original_entry(&key)
                    .map_err(err)?
                    .ok_or("Fresh pre-pin original Ready missing")?;
                let original_ref = oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputRefV1::from_entry(
                    &context.attestor().source, entry).map_err(err)?;
                let original = original_context
                    .original_input(&original_ref.admission_digest)
                    .ok_or("Fresh pre-pin original Input missing")?;
                original_ref.validate_input(original).map_err(err)?;
                let seed = original.objects.iter().filter(|object|
                    object.kind == oasis7_distfs::protected_bft_v3::BftPrivateRetentionInputObjectKindV1::Dependency)
                    .map(|object| (object.reference.digest.clone(),object.bytes.clone())).collect();
                // Exact Input is only a candidate access overlay. The independently
                // authenticated C0/H and already replayed prefix remain the parent.
                execution = execution.fork_replay_current(seed)?.fork_live_closed()?;
                derived_current_layer = true;
            }
            let executed = execution.replay_next(&input.certificate, &input.actions_cbor)?;
            let observed = execution.retained_nodes.accessed_nodes()?;
            if observed
                .keys()
                .any(|key| !record.business_index_refs().contains(key))
            {
                return Err("Active replay consumed unretained dependency".into());
            }
            let reproduced = super::super::window_storage::WindowRecord::from_actual(
                &origin,
                &previous,
                &input,
                &executed,
                record.business_index_refs().to_vec(),
            )?
            .with_publication_reservation(
                record.publication_reservation_ref().map(str::to_owned),
            )?;
            if reproduced != record {
                return Err("Active record failed actual replay".into());
            }
            for (reference, expected, max) in [
                (
                    record.snapshot_ref(),
                    &executed.snapshot_bytes,
                    common_storage::MAX_BFT_SNAPSHOT_BYTES,
                ),
                (
                    record.journal_ref(),
                    &executed.journal_bytes,
                    common_storage::MAX_BFT_JOURNAL_BYTES,
                ),
            ] {
                if !prepin.contains(&record.height())
                    && common_storage::read_cas_bounded(store, reference, max)? != *expected
                {
                    return Err("Active replay Snapshot/Journal bytes differ".into());
                }
            }
            drop(observed);
            let header_id = input.certificate.header.block_id()?;
            let active_key = context
                .original_inputs()
                .find(|original| {
                    original.key.ledger_origin_digest == reference.digest
                        && original.key.height == record.height()
                        && original.key.header_id == header_id
                })
                .map(|original| original.key.clone());
            if let Some(key) = active_key {
                let historical_parent = context
                    .authenticated_original_admission_parent(binding, &key)
                    .map_err(err)?;
                if historical_parent.history() != prefix_history.as_slice()
                    || historical_parent.schedule() != &prefix_owner.execution.schedule
                {
                    return Err(
                        "Active suffix original parent differs from actual signed prefix".into(),
                    );
                }
                let historical_input = context.original_ready(binding, &key).map_err(err)?;
                let entry = historical_input
                    .original_entry(&key)
                    .map_err(err)?
                    .ok_or("Active suffix original entry missing")?;
                let reference = oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputRefV1::from_entry(
                    &context.attestor().source, entry).map_err(err)?;
                let original = historical_input
                    .original_input(&reference.admission_digest)
                    .ok_or("Active suffix original Input missing")?;
                reference.validate_input(original).map_err(err)?;
                let mut prefix_owner = prefix_owner;
                prefix_owner.parent_state_digest = historical_parent.state().state_digest.clone();
                let grant = node_execution::verify_active_admission_original(
                    original,
                    &prefix_owner,
                    &historical_parent,
                )?;
                if record.publication_reservation_ref() != Some(grant.publication_metadata_ref()) {
                    return Err(
                        "Active suffix publication differs from independently verified original"
                            .into(),
                    );
                }
            } else {
                record
                    .publication_reservation_ref()
                    .ok_or("protected Active suffix lacks original publication")?;
                self.validate_record_publication(
                    &base.publications,
                    record.publication_reservation_ref(),
                    &input,
                    &executed,
                )?;
            }
            prefix_history.push(input.certificate.clone());
            previous = record.digest()?;
        }
        if &execution.schedule != parent.schedule() {
            return Err("Active actual parent schedule differs from signed prefix".into());
        }
        if derived_current_layer {
            execution.close_verified_replay_inventory()?;
        } else {
            // No derived writes were admitted: actual replay already ran over
            // this execution's strict, immutable dependency owner.
            execution.retained_nodes.require_strict_inventory()?;
        }
        if root.read_window_origin(&base, deadline)?.1 != reference {
            return Err("Active Origin changed during replay".into());
        }
        Ok(VerifiedActiveAdmissionOwnerV1 {
            base,
            execution: Box::new(execution),
            origin,
            parent_state_digest: parent.state().state_digest.clone(),
            retained_baseline_refs,
            private_directory,
        })
    }
}
