//! In-process continuity retained only after complete authorized actual replay.
//! This is neither a wire DTO nor a reader/grant. Cold installation rebuilds it
//! from the original full archive; base.cbor never reconstructs this authority.
use super::super::successor_archive::protected_retention::ValidatedProtectedRetentionArchiveV1;
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputRefV1 as InputRef, BftPrivateRetentionAttestorV1 as Role,
    BftSignerBindingV3 as Binding,
};

pub(super) struct VerifiedRetentionContinuityV1 {
    binding: Binding,
    role: Role,
    // Move the complete metadata witness and share its authenticated closed H.
    // No original payload, extra H inventory, or duplicate receipt chain exists.
    protected: ValidatedProtectedRetentionArchiveV1,
    h_parent: replay::ProtectedHReplayV1,
    metadata_digest: String,
    revision: u64,
    state_digest: String,
}
impl VerifiedRetentionContinuityV1 {
    #[inline(never)]
    pub(super) fn from_verified(
        binding: Binding,
        role: Role,
        protected: ValidatedProtectedRetentionArchiveV1,
        h_parent: replay::ProtectedHReplayV1,
        metadata: &PrivateClosureV1,
        public_state: &BftSignerStateV3,
    ) -> Result<Box<Self>, String> {
        // Only verify_one constructs this after every original and every pending
        // alternative completed its actual execution under its exact read scope.
        let metadata_digest = blake3_hex(&encode(metadata, MAX_METADATA)?);
        // Keep the complete metadata witness out of every caller's by-value
        // base/result temporaries while earlier World replay is still active.
        Ok(Box::new(Self {
            binding,
            role,
            protected,
            h_parent,
            metadata_digest,
            revision: public_state.revision,
            state_digest: public_state.state_digest.clone(),
        }))
    }
    fn verify_base(&self, base: &VerifiedPrivateSuccessorBaseV1) -> Result<(), String> {
        let metadata = &base.metadata;
        let map = &self.protected.final_ledger.map;
        let head = &self.protected.final_ledger.receipt.body.head;
        let binding_digest = self.binding.digest().map_err(err)?;
        if metadata.publication_layout != Some(2)
            || metadata.protected_retention.is_none()
            || metadata.active_base.is_some()
            || blake3_hex(&encode(metadata, MAX_METADATA)?) != self.metadata_digest
            || metadata.source.binding_digest != binding_digest
            || self.role.source.binding_digest != binding_digest
            || map.source != self.role.source
            || head.source != self.role.source
            || head.capability_digest != self.role.digest().map_err(err)?
            || base.public_state.revision != self.revision
            || base.public_state.state_digest != self.state_digest
            || base.public_state.private_retention.as_ref() != Some(head)
            || map.coverage_start_height
                != metadata
                    .cut
                    .height
                    .checked_add(1)
                    .ok_or("protected continuity cut height overflow")?
        {
            return Err("protected continuity fixed binding/source/cut/head differs".into());
        }
        self.h_parent.nodes.require_strict_inventory()?;
        let h_refs: BTreeSet<_> = self
            .protected
            .index
            .h_nodes
            .iter()
            .map(|reference| reference.digest.clone())
            .collect();
        if &h_refs != self.protected.activation.h_refs()
            || self.protected.activation.first().map.source != map.source
        {
            return Err("protected continuity original H namespace differs".into());
        }
        // h_parent retains the same authenticated material Arc, without a new
        // object read or an installation-supplied dependency inventory.
        let mut expected = BTreeSet::new();
        for entry in &map.entries {
            let original = self
                .protected
                .originals
                .get(&entry.key)
                .ok_or("protected continuity carry original missing")?;
            if &original.entry != entry
                || original.reference != InputRef::from_entry(&map.source, entry).map_err(err)?
                || entry.key.height != map.coverage_start_height
            {
                return Err("protected continuity carry fact/ref/namespace differs".into());
            }
            expected.insert((entry.key.height, entry.key.header_id.as_str()));
        }
        let mut actual = BTreeSet::new();
        for (roles, package) in &base.pending {
            let id = package.candidate.public_value.block_id()?;
            let descriptor = metadata
                .pending_candidates
                .iter()
                .find(|candidate| candidate.header_id == id)
                .ok_or("protected continuity carry descriptor absent")?;
            let selected = self
                .protected
                .final_ledger
                .selection_index
                .as_ref()
                .is_some_and(|index| {
                    index.selections.iter().any(|selection| {
                        selection.key.height == descriptor.height
                            && selection.key.header_id == descriptor.header_id
                    })
                });
            if descriptor.roles.contains(&PendingRoleV1::RetainedLocal) != selected {
                return Err("protected continuity current selection differs".into());
            }
            if descriptor.roles != *roles
                || descriptor.value != package.candidate.public_value
                || descriptor.dependencies.len() != package.dependency_nodes.len()
                || !base
                    .publications
                    .iter()
                    .any(|publication| publication.matches_candidate(&package.candidate))
            {
                return Err("protected continuity carry candidate differs".into());
            }
            descriptor
                .actions
                .validate_bytes(&package.candidate.actions_cbor)
                .map_err(err)?;
            for (reference, (digest, bytes)) in descriptor
                .dependencies
                .iter()
                .zip(&package.dependency_nodes)
            {
                if &reference.digest != digest {
                    return Err("protected continuity carry dependency order differs".into());
                }
                reference.validate_bytes(bytes).map_err(err)?;
            }
            if !roles.contains(&PendingRoleV1::ProtectedOriginal)
                || !actual.insert((
                    package.candidate.public_value.execution_commitment.height,
                    package.candidate.public_value.block_id()?,
                ))
            {
                return Err("protected continuity carry roles/duplicates differ".into());
            }
        }
        if actual.len() != expected.len()
            || expected
                .iter()
                .any(|(height, id)| !actual.contains(&(*height, (*id).to_owned())))
        {
            return Err("protected continuity carry package set differs".into());
        }
        if let Some(original) = &base.original {
            let checkpoint = &original.request.intent.body.checkpoint;
            if checkpoint.source != metadata.source
                || checkpoint.cut != metadata.cut
                || original.original_public_state != metadata.public_state
            {
                return Err("protected continuity installed original differs".into());
            }
        }
        Ok(())
    }
}
impl VerifiedPrivateSuccessorBaseV1 {
    pub(in super::super) fn verify_retention_continuity(&self) -> Result<(), String> {
        match (&self.protected_continuity, self.metadata.publication_layout) {
            (Some(witness), Some(2)) => witness.verify_base(self),
            (None, Some(2)) | (Some(_), _) => {
                Err("protected continuity witness absent or unexpected".into())
            }
            (None, _) => Ok(()),
        }
    }
}

#[cfg(test)]
impl ActualBftSuccessorWindowDriverVerifierV1 {
    pub(crate) fn verify_retention_continuity_mutations_for_test(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        state: &BftSignerStateV3,
        public_ref: &BftSuccessorWindowArchiveRefV1,
        target_store: &LocalCasStore,
        deadline: Instant,
    ) -> Result<(), String> {
        let mut base = self.verify_closure_with_origin(checkpoint, state, public_ref, deadline)?;
        base.verify_retention_continuity()?;
        base.verify_carry_initializer_mutations_for_test(target_store)?;
        let original_height = base.metadata.cut.height;
        base.metadata.cut.height = original_height
            .checked_add(1)
            .ok_or("test height overflow")?;
        assert!(
            base.verify_retention_continuity().is_err(),
            "changed verified cut accepted"
        );
        base.metadata.cut.height = original_height;
        let original_material = std::mem::replace(
            &mut base.metadata.source.material_identity,
            "different-original-material".to_owned(),
        );
        assert!(
            base.verify_retention_continuity().is_err(),
            "changed source accepted"
        );
        base.metadata.source.material_identity = original_material;
        assert!(
            !base.pending.is_empty(),
            "test must retain an original carry candidate"
        );
        let pending = std::mem::take(&mut base.pending);
        assert!(
            base.verify_retention_continuity().is_err(),
            "omitted carry accepted"
        );
        base.pending = pending;
        base.verify_retention_continuity()
    }
}

impl VerifiedPrivateSuccessorBaseV1 {
    pub(in super::super) fn verify_original_carry_reference(
        &self,
        reference: &InputRef,
    ) -> Result<(), String> {
        self.verify_retention_continuity()?;
        let witness = self
            .protected_continuity
            .as_ref()
            .ok_or("Active Source carry lacks actual protected continuity")?;
        let original = witness
            .protected
            .originals
            .get(&reference.key)
            .ok_or("original Source input is absent from complete authenticated C0 carry")?;
        if &original.reference != reference
            || InputRef::from_entry(&witness.protected.final_ledger.map.source, &original.entry)
                .map_err(err)?
                != *reference
        {
            return Err("authenticated C0 carry original ref/fact differs".into());
        }
        Ok(())
    }
}

impl VerifiedPrivateSuccessorBaseV1 {
    pub(in super::super) fn verify_local_carry_selection(
        &self,
        reference: &InputRef,
        current: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionSelectionIndexV1,
        identity: &oasis7_distfs::protected_bft_v3::VerifiedBftPrivateRetentionSelectionIdentityV1,
    ) -> Result<(), String> {
        self.verify_original_carry_reference(reference)?;
        let witness = self
            .protected_continuity
            .as_ref()
            .ok_or("Local carry continuity missing")?;
        let index = witness
            .protected
            .final_ledger
            .selection_index
            .as_ref()
            .ok_or("authenticated C0 Local choice index absent")?;
        if index != current {
            return Err(
                "Local carry current choice index/sequence differs from authenticated C0".into(),
            );
        }
        let selected = index
            .selections
            .iter()
            .find(|entry| entry.key == reference.key)
            .ok_or("original carry is retained but not selected for Local proposal")?;
        if selected.reference != *reference
            || selected.source != reference.source
            || selected.kind != identity.kind()
            || selected.local_identity_digest != identity.digest()
            || selected.local_identity_bytes != identity.bytes()
            || selected.operation.digest().map_err(err)? != selected.operation_digest
        {
            return Err("Local carry original selection identity/typed operation differs".into());
        }
        let local = self.pending.iter().any(|(roles, package)| {
            roles.contains(&PendingRoleV1::RetainedLocal)
                && package.candidate.public_value.execution_commitment.height
                    == reference.key.height
                && package.candidate.public_value.block_id().ok().as_deref()
                    == Some(reference.key.header_id.as_str())
        });
        if !local {
            return Err(
                "Local carry selection lacks exact independently verified RetainedLocal role"
                    .into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
impl ActualBftSuccessorWindowDriverVerifierV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn verify_carry_purpose_matrix_for_test(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        state: &BftSignerStateV3,
        public_ref: &BftSuccessorWindowArchiveRefV1,
        backend: &mut oasis7_distfs::protected_bft_v3::BftPrivateRetentionMemoryBackendV1,
        binding: &str,
        signatures: &std::sync::atomic::AtomicUsize,
        deadline: Instant,
    ) -> Result<(), String> {
        use oasis7_distfs::protected_bft_v3::BftSignerBackendV3;
        // This performs complete actual external Noise/ref-scoped replay first.
        // Its legitimate egress fees precede the pure per-item mutation baseline.
        let base = self.verify_closure_with_origin(checkpoint, state, public_ref, deadline)?;
        base.verify_retention_continuity()?;
        let before = backend
            .load_private_retention_current(binding)
            .map_err(err)?;
        let count = signatures.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(count, 0);
        let ledger = before
            .retention
            .as_ref()
            .ok_or("purpose matrix original ledger absent")?;
        let index = ledger
            .selection_index
            .as_ref()
            .ok_or("purpose matrix original index absent")?;
        let carry: Vec<_> = ledger
            .map
            .entries
            .iter()
            .filter(|entry| entry.key.height == checkpoint.cut.height + 1)
            .collect();
        assert_eq!(
            carry.len(),
            2,
            "two genuinely retained alternatives required"
        );
        let chosen = carry
            .iter()
            .find(|entry| {
                index
                    .selections
                    .iter()
                    .any(|choice| choice.key == entry.key)
            })
            .ok_or("purpose matrix original selected choice missing")?;
        let alternative = carry
            .iter()
            .find(|entry| {
                !index
                    .selections
                    .iter()
                    .any(|choice| choice.key == entry.key)
            })
            .ok_or("purpose matrix unselected original missing")?;
        let chosen_ref = InputRef::from_entry(&ledger.map.source, chosen).map_err(err)?;
        let alternative_ref = InputRef::from_entry(&ledger.map.source, alternative).map_err(err)?;
        let chosen_input = before
            .inputs
            .get(&chosen_ref.admission_digest)
            .ok_or("chosen original missing")?;
        let alternative_input = before
            .inputs
            .get(&alternative_ref.admission_digest)
            .ok_or("alternative original missing")?;
        chosen_ref.validate_input(chosen_input).map_err(err)?;
        alternative_ref
            .validate_input(alternative_input)
            .map_err(err)?;
        let chosen_identity =
            super::super::node_execution::source_selection_identity(chosen_input)?;
        let alternative_identity =
            super::super::node_execution::source_selection_identity(alternative_input)?;
        base.verify_original_carry_reference(&chosen_ref)?;
        base.verify_local_carry_selection(&chosen_ref, index, &chosen_identity)?;
        // Block material validity is independently established by full actual
        // C0 replay (including this Input), then exact carry-ref membership.
        base.verify_original_carry_reference(&alternative_ref)?;
        assert_eq!(
            base.verify_local_carry_selection(&alternative_ref, index, &alternative_identity)
                .unwrap_err(),
            "original carry is retained but not selected for Local proposal",
        );
        let after = backend
            .load_private_retention_current(binding)
            .map_err(err)?;
        assert!(
            before == after,
            "per-item purpose checks mutated original backend/fees"
        );
        for (digest, input) in &before.inputs {
            assert!(Arc::ptr_eq(input, after.inputs.get(digest).unwrap()));
        }
        assert_eq!(signatures.load(std::sync::atomic::Ordering::SeqCst), count);
        Ok(())
    }
}

impl VerifiedPrivateSuccessorBaseV1 {
    /// Producer preflight reads only the immutable independently verified role.
    /// It does not construct a provider admission owner or grant material access.
    pub(in super::super) fn require_fresh_active_capability(&self) -> Result<(), String> {
        self.verify_retention_continuity()?;
        let continuity = self
            .protected_continuity
            .as_ref()
            .ok_or("fresh Active requires independently verified protected continuity")?;
        if !continuity.role.allow_authenticated_successor_origins {
            return Err("fixed original role denies fresh Active admission".into());
        }
        Ok(())
    }
}

impl VerifiedPrivateSuccessorBaseV1 {
    pub(in super::super) fn verified_c0_retention_baseline(
        &self,
    ) -> Result<(BTreeSet<String>, Vec<InputRef>), String> {
        self.verify_retention_continuity()?;
        let continuity = self
            .protected_continuity
            .as_ref()
            .ok_or("Active baseline requires protected C0 continuity")?;
        Ok((
            continuity.protected.activation.h_refs().clone(),
            continuity
                .protected
                .originals
                .values()
                .map(|original| original.reference.clone())
                .collect(),
        ))
    }
}

impl VerifiedPrivateSuccessorBaseV1 {
    pub(in super::super) fn original_retention_attestor(&self) -> Result<&Role, String> {
        self.verify_retention_continuity()?;
        self.protected_continuity
            .as_ref()
            .map(|continuity| &continuity.role)
            .ok_or_else(|| "Active original fixed attestor unavailable".into())
    }
}

impl VerifiedPrivateSuccessorBaseV1 {
    /// Only independently executed exact originals supply these dependency refs.
    pub(in super::super) fn retained_baseline_refs(&self) -> Result<BTreeSet<String>, String> {
        self.verify_retention_continuity()?;
        let continuity = self
            .protected_continuity
            .as_ref()
            .ok_or("Active baseline continuity absent")?;
        let mut refs = continuity.protected.activation.h_refs().clone();
        for publication in &self.publications {
            refs.extend(publication.verified_dependency_refs().iter().cloned());
        }
        Ok(refs)
    }
}
