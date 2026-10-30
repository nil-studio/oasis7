//! Install the next original cut without reopening or resetting the original H ledger.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftSignerBindingV3, BftSignerLeaseV3, BftSuccessorWindowAdoptedEvidenceV1,
    LocalBftSuccessorArchiveStoreV1,
};
impl FormalBftHeaderActiveExecutionV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn install_successor_window(
        &mut self,
        binding: &BftSignerBindingV3,
        pin: &str,
        lease: &mut dyn BftSignerLeaseV3,
        archive: Arc<LocalBftSuccessorArchiveStoreV1>,
        request_id: &str,
        checkpoint_digest: &str,
        deadline: Instant,
    ) -> Result<BftSuccessorWindowAdoptedEvidenceV1, String> {
        self.reset(deadline)?;
        let (original, current) = lease
            .get_successor_window(request_id, checkpoint_digest)
            .map_err(err)?;
        let original = original.ok_or("next active original request unavailable")?;
        original.validate(binding, pin).map_err(err)?;
        let body = &original.request.intent.body.checkpoint;
        let tip = self
            .execution
            .tip
            .as_ref()
            .ok_or("next active cut absent")?;
        let record = self
            .ledger
            .records()
            .last()
            .ok_or("next active private record absent")?;
        if body.source != self.base.metadata.source
            || body.cut.height != tip.certified_header().execution_commitment.height
            || body.cut.header_id != tip.block_id()
            || body.cut.driver_record_digest != record.digest()?
            || body.successor_schedule_cbor
                != successor_archive::encode_protocol(&self.execution.schedule, MAX_METADATA)?
            || body.cut.semantic_state_root
                != self
                    .execution
                    .parent_world
                    .execution_state_identity()
                    .map_err(err)?
                    .state_root_hash
        {
            return Err("next active install differs from actual archive-rooted cut".into());
        }
        let public = lease
            .read_successor_original_state(request_id, checkpoint_digest, deadline)
            .map_err(err)?;
        let historical = public
            .reconstruct_installed_pending(binding, pin, &original)
            .map_err(err)?;
        if historical.installed_state() != &current.state {
            return Err("next active original InstalledPending changed".into());
        }
        let verifier = successor_verifier::ActualBftSuccessorWindowDriverVerifierV1::new(
            self.base.anchor.clone(),
            self.base.target.clone(),
            archive,
        )?;
        let base = verifier.recover_private_base(&original, &public, deadline)?;
        let reference = layout::prepare(&self.records_root, self.store.clone(), &base, deadline)?;
        layout::verify_ready(
            &self.records_root,
            self.store.clone(),
            &base,
            &reference,
            true,
            deadline,
        )?;
        let installed = successor_install::publish_installed(
            &self.records_root,
            &original,
            &current.state,
            deadline,
        )?;
        self.verify_base(deadline)?;
        let mut evidence = installed.evidence();
        evidence.active_segment_origin = Some(reference);
        Ok(evidence)
    }
}
