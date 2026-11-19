//! Healthy replacement seals only an already Qualified transition boundary.
//! Generic provider/seal wire remains shared; no successor installation here.
use super::*;
use crate::controlled_authority::replicated_protocol::HeadAnchor;
impl FileFormalEndpoint {
    pub(crate) fn verify_sealed_archive_grant_scope(
        &mut self,
        grant: &transport::SignedArchiveReadGrantV1,
        minimum: &FormalHeadRefV1,
    ) -> Result<(), FormalError> {
        let _guard = self.guard()?;
        let Some(seal) = self.store.body.seal.as_ref() else {
            return Err(invalid("archive delegation requires actual protected seal"));
        };
        if minimum != &grant.body.sealed_cut
            || self.store.body.head != grant.body.sealed_cut
            || seal.recovery_operation != grant.body.transition_digest
            || seal.next_epoch_floor != grant.body.next_epoch_floor
            || seal.highest_complete.position != grant.body.sealed_cut.position
            || seal.highest_complete.decision_hash != grant.body.sealed_cut.record_id
            || !seal.highest_complete.qualified
            || seal.tail != fencing::SealedTailV1::empty()
        {
            return Err(invalid("archive delegation exact protected sealed scope"));
        }
        Ok(())
    }
    /// Resolve H and H-1 from the provider-authenticated native checkpoint.
    /// Wire callers supply only the writer-signed intent, never opaque commits.
    pub fn verify_durable_set_transition_at_head(
        &mut self,
        signed: &SignedNormalDurableSetTransitionV1,
    ) -> Result<VerifiedNormalDurableSetTransitionV1, FormalError> {
        let _guard = self.guard()?;
        self.reload_parent(&_guard.evidence().map_err(fencing_error)?)?;
        let boundary = self
            .parent
            .as_ref()
            .ok_or_else(|| invalid("healthy durable set requires Qualified execution boundary"))?;
        let node: HistoryNode =
            self.store
                .get(self.store.body.head_node.as_ref().ok_or_else(|| {
                    invalid("healthy durable set requires native boundary history")
                })?)?;
        if node.head != boundary.head() {
            return Err(invalid("healthy durable set native boundary binding"));
        }
        let predecessor = if let Some(link) = node.ancestors.first() {
            let previous: HistoryNode = self.store.get(&link.node)?;
            if previous.head != link.head
                || link.head.position.checked_add(1) != Some(node.head.position)
            {
                return Err(invalid("healthy durable set immediate predecessor binding"));
            }
            let FormalRequestStateV1::Qualified {
                candidate, proof, ..
            } = self.store.get(&previous.entry)?
            else {
                return Err(invalid("healthy durable set predecessor qualification"));
            };
            verify_record_material(&proof.decision.record, &candidate.material)?;
            if proof.decision.record != candidate.candidate.record
                || proof.decision.record.record_id()? != link.head.record_id
                || proof.decision.primary.body.finalizing_authority_id
                    != proof.decision.replica.body.finalizing_authority_id
            {
                return Err(invalid(
                    "healthy durable set predecessor canonical identity",
                ));
            }
            let parent = if let Some(ancestor) = previous.ancestors.first() {
                let ancestor_node: HistoryNode = self.store.get(&ancestor.node)?;
                if ancestor_node.head != ancestor.head
                    || ancestor.head.position.checked_add(1) != Some(previous.head.position)
                {
                    return Err(invalid("healthy predecessor parent binding"));
                }
                let FormalRequestStateV1::Qualified { proof, .. } =
                    self.store.get(&ancestor_node.entry)?
                else {
                    return Err(invalid("healthy predecessor parent qualification"));
                };
                if proof.decision.record.record_id()? != ancestor.head.record_id {
                    return Err(invalid("healthy predecessor parent canonical identity"));
                }
                Some(CheckpointParent {
                    record: proof.decision.record,
                    finalizing_authority_id: proof.decision.primary.body.finalizing_authority_id,
                })
            } else {
                self.store.body.base_parent.clone()
            };
            let bytes = self.store.checkpoint_bytes()?;
            let instance = match self.role {
                EndpointRole::Primary => &self.authority.definition().primary.instance,
                EndpointRole::Replica => &self.authority.definition().replica.instance,
            };
            let verdict = NativeCheckpointVerdictV1::verify(
                &bytes,
                &self.authority,
                &instance.endpoint_public_key,
                &self.store.checkpoint,
                &_guard.evidence().map_err(fencing_error)?,
                &previous.head,
                &previous.head,
                &self.authority.authority_id()?,
                |id| {
                    self.store
                        .raw_object(id)?
                        .ok_or_else(|| invalid("healthy predecessor object missing"))
                },
            )?;
            if verdict.proof() != &proof
                || verdict.verified_parent_record() != parent.as_ref().map(|p| &p.record)
                || verdict.verified_parent_finalizing_authority_id()
                    != parent.as_ref().map(|p| p.finalizing_authority_id.as_str())
            {
                return Err(invalid("healthy checked checkpoint exact parent/proof"));
            }
            restore_formal_commit_from_checkpoint(verdict.proof(), &self.authority, None, &verdict)?
        } else {
            return Err(invalid(
                "healthy durable set complete predecessor history required",
            ));
        };
        verify_normal_durable_set_transition(signed, &self.authority, &predecessor, boundary)
    }
    fn check_durable_set_boundary(
        &mut self,
        transition: &VerifiedNormalDurableSetTransitionV1,
        provider: &fencing::MonotonicProviderEvidenceV1,
    ) -> Result<(), FormalError> {
        self.reload_parent(provider)?;
        let boundary = transition.boundary();
        if self.authority.context_digest()?
            != transition.predecessor_authority().context_digest()?
            || self.store.body.head != boundary.head()
            || self.store.body.pending.is_some()
            || self.store.body.decision_head.is_some()
            || self.store.checkpoint.head.position != boundary.head().position
            || self.store.checkpoint.head.decision_hash != boundary.head().record_id
            || !self.store.checkpoint.head.qualified
            || self.parent.as_ref().is_none_or(|local| {
                local.record() != boundary.record()
                    || local.finalizing_authority_id() != boundary.finalizing_authority_id()
            })
        {
            return Err(FormalError::Conflict(
                "healthy durable set requires exact Qualified H and empty slot".into(),
            ));
        }
        Ok(())
    }
    fn check_durable_set_seal(
        &self,
        body: &fencing::EpochSealBodyV1,
        transition: &VerifiedNormalDurableSetTransitionV1,
    ) -> Result<(), FormalError> {
        if &body.instance != self.instance()
            || body.recovery_operation != transition.digest()
            || body.next_epoch_floor != transition.body().successor.epoch
            || body.highest_complete.position != transition.boundary().head().position
            || body.highest_complete.decision_hash != transition.boundary().head().record_id
            || !body.highest_complete.qualified
            || body.tail != fencing::SealedTailV1::empty()
            || body.checkpoint_before.head != body.highest_complete
        {
            return Err(FormalError::Conflict(
                "healthy durable set original seal identity/boundary".into(),
            ));
        }
        Ok(())
    }
    /// Query-first retries return the original seal even with the pre-seal
    /// expected checkpoint. Different transitions can never replace that seal.
    pub fn seal_durable_set(
        &mut self,
        transition: &VerifiedNormalDurableSetTransitionV1,
        expected: &fencing::EndpointCheckpointBindingV1,
    ) -> Result<fencing::EpochSealReceiptV1, FormalError> {
        let mut guard = self.guard()?;
        self.check_durable_set_boundary(transition, &guard.evidence().map_err(fencing_error)?)?;
        if let Some(body) = self.store.body.seal.clone() {
            self.check_durable_set_seal(&body, transition)?;
            return self.signed_seal(body, guard.as_mut());
        }
        if expected != &self.store.checkpoint {
            return Err(FormalError::Conflict(
                "healthy durable set stale checkpoint".into(),
            ));
        }
        let body = fencing::EpochSealBodyV1 {
            schema_version: 1,
            instance: self.instance().clone(),
            recovery_operation: transition.digest().into(),
            highest_complete: HeadAnchor {
                position: transition.boundary().head().position,
                decision_hash: transition.boundary().head().record_id,
                qualified: true,
            },
            checkpoint_before: self.store.checkpoint.clone(),
            tail: fencing::SealedTailV1::empty(),
            next_epoch_floor: transition.body().successor.epoch,
        };
        body.validate().map_err(fencing_error)?;
        let mut next = self.store.body.clone();
        next.seal = Some(body.clone());
        if let Err(error) = self.publish_native(next, guard.as_mut()) {
            self.poisoned = true;
            return Err(error);
        }
        let receipt = self.signed_seal(body, guard.as_mut());
        if receipt.is_err() {
            self.poisoned = true;
        }
        receipt
    }
    /// Read-only authenticated reconciliation; absence never triggers a seal.
    pub fn query_durable_set(
        &mut self,
        transition: &VerifiedNormalDurableSetTransitionV1,
    ) -> Result<Option<fencing::EpochSealReceiptV1>, FormalError> {
        let mut guard = self.guard()?;
        self.check_durable_set_boundary(transition, &guard.evidence().map_err(fencing_error)?)?;
        let Some(body) = self.store.body.seal.clone() else {
            return Ok(None);
        };
        self.check_durable_set_seal(&body, transition)?;
        self.signed_seal(body, guard.as_mut()).map(Some)
    }
}
