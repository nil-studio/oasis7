//! Native instance/progress gate. Controlled providers never prove formal fencing.
use super::*;
use crate::controlled_authority::fencing::EndpointInstanceBindingDirectoryExt;
impl FileEndpoint {
    pub(super) fn check_gate_write(
        gate: &Option<Box<dyn MonotonicEndpointGuard>>,
    ) -> Result<(), ProtocolError> {
        if gate.as_ref().is_some_and(|g| g.seal().is_some()) {
            return Err(invalid("EPOCH_SEALED"));
        }
        Ok(())
    }
    pub(super) fn acquire_fencing_guard(
        &mut self,
    ) -> Result<Option<Box<dyn MonotonicEndpointGuard>>, ProtocolError> {
        let Some(fencing) = self.fencing.clone() else {
            return Ok(None);
        };
        let result = (|| {
            fencing
                .instance
                .verify_directory(&self.path)
                .map_err(fencing_error)?;
            let initial = archive::Store::checkpoint_binding_at(&self.path)?;
            let mut guard = fencing
                .provider
                .acquire(&fencing.instance, &initial)
                .map_err(fencing_error)?;
            archive::Store::reconcile_guard(
                &self.path,
                self.trust(),
                self.role(),
                &self.head,
                guard.as_mut(),
            )?;
            let archive = self.archive.refresh(&self.head)?;
            if archive.instance_binding()
                != Some(fencing.instance.digest().map_err(fencing_error)?.as_str())
                || archive.seal() != guard.seal()
            {
                return Err(ProtocolError::Poisoned);
            }
            let (state, base) = archive.hot()?;
            self.head = archive.head();
            self.state = state;
            self.base = base;
            self.archive = archive;
            Ok(Some(guard))
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn checkpoint_binding(&mut self) -> Result<EndpointCheckpointBindingV1, ProtocolError> {
        let _gate = self.acquire_fencing_guard()?;
        archive::Store::checkpoint_binding_at(&self.path)
    }
    pub fn seal_epoch(
        &mut self,
        recovery_operation: String,
        next_epoch_floor: u64,
        expected: &EndpointCheckpointBindingV1,
    ) -> Result<EpochSealReceiptV1, ProtocolError> {
        self.ensure_writable()?;
        let mut gate = self
            .acquire_fencing_guard()?
            .ok_or_else(|| invalid("FENCING_UNPROVEN: external monotonic provider required"))?;
        if let Some(sealed) = gate.seal() {
            if sealed.recovery_operation != recovery_operation
                || sealed.next_epoch_floor != next_epoch_floor
            {
                return Err(invalid(
                    "EPOCH_SEALED: different irreversible recovery operation",
                ));
            }
            return EpochSealReceiptV1::sign(
                sealed.clone(),
                gate.evidence().map_err(fencing_error)?,
                &self.key,
            )
            .map_err(fencing_error);
        }
        if gate.checkpoint() != expected {
            return Err(invalid("seal expected checkpoint changed"));
        }
        let instance = self.fencing.as_ref().expect("guarded").instance.clone();
        let mut tail = SealedTailV1::empty();
        let highest_complete = if self.head.qualified || self.head.position == 0 {
            self.head.clone()
        } else {
            self.base.clone()
        };
        if let Some(entry) = self.state.decisions.last().filter(|e| !e.published) {
            tail.kind = SealedTailKindV1::Decision;
            tail.request = Some(entry.proposal.body.request.clone());
            tail.position = Some(entry.proposal.body.position);
            tail.proposal_digest = Some(crypto::verify_proposal(&entry.proposal, self.trust())?);
            tail.replica_prepare_digest = Some(
                crate::controlled_authority::fencing::hash_value(&entry.replica_prepare)
                    .map_err(fencing_error)?,
            );
            tail.primary_receipt_digest = entry
                .primary_receipt
                .as_ref()
                .map(crate::controlled_authority::fencing::hash_value)
                .transpose()
                .map_err(fencing_error)?;
            tail.replica_receipt_digest = entry
                .replica_receipt
                .as_ref()
                .map(crate::controlled_authority::fencing::hash_value)
                .transpose()
                .map_err(fencing_error)?;
        } else if let Some(prepared) = self.state.prepared.first() {
            tail.kind = SealedTailKindV1::Prepared;
            tail.request = Some(prepared.proposal.body.request.clone());
            tail.position = Some(prepared.proposal.body.position);
            tail.proposal_digest = Some(crypto::verify_proposal(&prepared.proposal, self.trust())?);
        }
        if let Some(frozen) = &self.state.frozen {
            if tail.kind == SealedTailKindV1::Empty {
                tail.kind = SealedTailKindV1::Frozen;
                tail.request = Some(frozen.input.body.request.clone());
                tail.position = Some(frozen.input.body.next_position);
            }
            tail.frozen_input_digest = Some(frozen.input.verify(self.trust())?);
            tail.frozen_certificate_digest = frozen
                .certificate
                .as_ref()
                .map(crate::controlled_authority::fencing::hash_value)
                .transpose()
                .map_err(fencing_error)?;
        }
        let body = EpochSealBodyV1 {
            recovery_plan: None,
            writer_seal: None,
            schema_version: 1,
            instance,
            recovery_operation,
            highest_complete,
            checkpoint_before: expected.clone(),
            tail,
            next_epoch_floor,
        };
        body.validate().map_err(fencing_error)?;
        self.archive.stage_seal(body.clone());
        let result = self
            .archive
            .publish_guarded(&self.key, Fault::None, Some(gate.as_mut()));
        if result.is_err() {
            self.poisoned = true;
        }
        result?;
        EpochSealReceiptV1::sign(body, gate.evidence().map_err(fencing_error)?, &self.key)
            .map_err(fencing_error)
    }
}
