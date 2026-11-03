//! Reuse full independent C0 replay under same-backend original ownership.
use super::*;
impl ActualBftSuccessorWindowDriverVerifierV1 {
    pub(in super::super) fn verify_active_admission_configuration(
        &self,
        records: &Path,
        store: &LocalCasStore,
    ) -> Result<(), String> {
        let root = self
            .installation_root
            .as_ref()
            .ok_or("Active carry requires fixed installation root")?;
        let configured = self
            .active_private_store
            .as_ref()
            .ok_or("Active carry requires fixed private CAS")?;
        if canonical_storage_path(root.path())? != canonical_storage_path(records)?
            || canonical_storage_path(configured.root())? != canonical_storage_path(store.root())?
        {
            return Err("Active carry verifier records/CAS differs from configured source".into());
        }
        Ok(())
    }
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn verify_inherited_source_carry_parent(
        &self,
        binding: &oasis7_distfs::protected_bft_v3::BftSignerBindingV3,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        active: &oasis7_distfs::protected_bft_v3::VerifiedBftSignerActiveWindowV1,
        reference: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputRefV1,
        parent: &super::super::source_creation::VerifiedSourceInputParentV1,
        local_selection: Option<(
            &oasis7_distfs::protected_bft_v3::BftPrivateRetentionSelectionIndexV1,
            &oasis7_distfs::protected_bft_v3::VerifiedBftPrivateRetentionSelectionIdentityV1,
        )>,
        deadline: Instant,
    ) -> Result<(), String> {
        let owned = context.original_replay_context(binding).map_err(err)?;
        let original = active.original_request();
        original
            .validate(binding, &context.attestor().provider_public_key)
            .map_err(err)?;
        // Boxed replay preserves the bounded default-service-stack phases.
        let base = self.verify_original_closure_boxed_access(
            original,
            deadline,
            &ProtectedInputAccessV1::Owned(&owned),
        )?;
        base.verify_retention_continuity()?;
        let cut = parent
            .cut
            .as_ref()
            .ok_or("Active Source carry lacks actual original parent cut")?;
        if cut != &base.metadata.cut
            || parent.schedule_digest != base.execution.schedule.digest()?
            || active.base_schedule() != &base.execution.schedule
            || cut.height.checked_add(1) != Some(reference.key.height)
        {
            return Err("Active C0 differs from independently verified Source carry parent".into());
        }
        base.verify_original_carry_reference(reference)?;
        if let Some((index, identity)) = local_selection {
            base.verify_local_carry_selection(reference, index, identity)?;
        }
        // The bridge validates these complete original bytes against the exact
        // authenticated original ref and request before historical reconstruction.
        // It never uses latest Active State or truncates/re-hashes a State.
        let bytes = encode_protocol(&base.public_state, MAX_CLOSURE)?;
        let installed = context
            .reconstruct_installed_original(binding, original, &bytes)
            .map_err(err)?;
        drop(bytes);
        let evidence = original
            .request
            .adopted_evidence
            .as_ref()
            .ok_or("Active original lacks Adopted evidence")?;
        if evidence.active_segment_origin.as_ref() != Some(active.active_segment_origin()) {
            return Err("Active Source carry Origin differs from authenticated adoption".into());
        }
        let root = self
            .installation_root
            .as_ref()
            .ok_or("Active carry installation root missing")?;
        // The current opaque Active record is Adopted. The readback pointer
        // instead commits the exact historical InstalledPending record/state.
        // Borrow that immutable record from the validated reconstruction; never
        // rewrite a caller's status or pass latest State as historical State.
        let historical = installed.installed_state();
        let immutable = historical
            .successor_window
            .as_ref()
            .and_then(|window| window.request(&original.request.intent.body.checkpoint.request_id))
            .ok_or("Active carry historical installed original missing")?;
        if immutable.request.status
            != oasis7_distfs::protected_bft_v3::BftSuccessorWindowStatusV1::InstalledPending
            || original.request.status
                != oasis7_distfs::protected_bft_v3::BftSuccessorWindowStatusV1::Adopted
            || immutable.request.intent != original.request.intent
            || immutable.request.receipt != original.request.receipt
            || immutable.original_public_state != original.original_public_state
            || immutable.manifest != original.manifest
            || immutable.prepared_revision != original.prepared_revision
            || immutable.request.adopted_evidence.is_some()
            || immutable.request.superseded_reason.is_some()
        {
            return Err(
                "Active carry historical original differs from authenticated adoption".into(),
            );
        }
        self.verify_adopted_readback(root, &base, immutable, historical, evidence, deadline)
    }
}
