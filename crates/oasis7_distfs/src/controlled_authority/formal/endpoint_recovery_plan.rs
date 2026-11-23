//! Plan publication shares the native checkpoint CAS and protected signer lease.
use super::*;

impl FileFormalEndpoint {
    pub fn prepare_recovery_plan(
        &mut self,
        plan: SignedRecoveryPlanV1,
        pending: RecoveryPendingHandoffV1,
        writer_ack: SignedWriterRecoveryPlanAckV1,
        expected: &fencing::EndpointCheckpointBindingV1,
    ) -> Result<(), FormalError> {
        let mut guard = self.guard()?;
        plan.verify(&self.authority)?;
        writer_ack.verify(&plan, &self.authority)?;
        if plan.body.pending_handoff_root != pending.root()? {
            return Err(invalid(
                "recovery plan requires the complete original pending collection",
            ));
        }
        if let Some(original) = &self.store.body.recovery_plan {
            let retained: RecoveryPendingHandoffV1 = self.store.get(
                self.store
                    .body
                    .recovery_handoff
                    .as_deref()
                    .ok_or_else(|| invalid("persisted recovery pending collection missing"))?,
            )?;
            if original != &plan
                || retained != pending
                || self.store.body.writer_plan_ack.as_ref() != Some(&writer_ack)
            {
                return Err(FormalError::Conflict(
                    "original recovery plan or private pending collection changed".into(),
                ));
            }
            return Ok(());
        }
        if expected != &self.store.checkpoint || self.store.body.seal.is_some() {
            return Err(FormalError::Conflict(
                "recovery plan stale or already sealed checkpoint".into(),
            ));
        }
        let cut = self
            .parent
            .as_ref()
            .ok_or_else(|| invalid("recovery plan requires a qualified cut"))?;
        if cut.record().handoff.is_some() || cut.head() != self.store.body.head {
            return Err(invalid(
                "recovery plan cannot reopen a profile handoff or a shorter cut",
            ));
        }
        let witness = self.recovery_plan_tail()?;
        if plan.body.intent.retained_context.is_some() {
            let previous = self
                .recovery
                .as_ref()
                .ok_or_else(|| invalid("recovery plan prior permission missing"))?;
            let context = VerifiedActiveRecoveryContextV2::from_previous(
                previous,
                &self.original,
                cut,
                &witness,
            )?;
            VerifiedRecoveryPlanV1::verify_retained(plan.body.clone(), &context, &pending)?;
        } else if plan.body.original_qualified_proof_digest.is_some() {
            let previous = self.recovery.as_ref().ok_or_else(|| {
                invalid("legacy plan cut requires its original verified recovery")
            })?;
            let legacy = VerifiedRecoveryPlanLegacyCutV1::verify(
                &self.original_qualified_proof()?,
                previous,
            )?;
            VerifiedRecoveryPlanV1::verify_after_legacy_completion(
                plan.body.clone(),
                &self.authority,
                cut,
                &witness,
                &pending,
                &legacy,
            )?;
        } else {
            VerifiedRecoveryPlanV1::verify(
                plan.body.clone(),
                &self.authority,
                cut,
                &witness,
                &pending,
            )?;
        }
        let pending_object = self.store.put(&pending)?;
        let mut next = self.store.body.clone();
        if matches!(witness, RecoveryTailWitnessV1::Empty) {
            let drained = RecoveryDrainedProofV1 {
                schema_version: 1,
                plan_digest: plan.digest()?,
                fixed: witness,
                qualified: self.original_qualified_proof()?,
            };
            drained.verify(&plan, &self.authority)?;
            next.recovery_drained = Some(drained);
        } else {
            let progress = self.planned_progress(witness)?;
            progress.verify(&plan, &self.authority)?;
            next.recovery_progress = Some(progress);
        }
        next.recovery_plan = Some(plan);
        next.writer_plan_ack = Some(writer_ack);
        next.recovery_handoff = Some(pending_object);
        if let Err(error) = self.publish_native(next, guard.as_mut()) {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }

    /// The caller retains the original authenticated bytes and must re-admit
    /// them through the current Runtime. This read grants no business authority.
    pub fn recovery_pending_handoff(&mut self) -> Result<RecoveryPendingHandoffV1, FormalError> {
        let _guard = self.guard()?;
        let id = self
            .store
            .body
            .recovery_handoff
            .as_deref()
            .ok_or_else(|| invalid("native recovery pending handoff not installed"))?;
        let pending: RecoveryPendingHandoffV1 = self.store.get(id)?;
        pending.validate()?;
        Ok(pending)
    }

    pub(super) fn recovery_plan_tail(&self) -> Result<RecoveryTailWitnessV1, FormalError> {
        let Some(id) = &self.store.body.pending else {
            return Ok(RecoveryTailWitnessV1::Empty);
        };
        match self.store.get::<FormalRequestStateV1>(id)? {
            FormalRequestStateV1::Frozen {
                certificate: Some(frozen),
                ..
            } => Ok(RecoveryTailWitnessV1::Frozen(frozen)),
            FormalRequestStateV1::Prepared { candidate, .. } => {
                Ok(RecoveryTailWitnessV1::Prepared {
                    frozen: candidate
                        .frozen
                        .ok_or_else(|| invalid("planned original frozen certificate missing"))?,
                    candidate: candidate.candidate,
                })
            }
            FormalRequestStateV1::Decided { candidate, .. }
            | FormalRequestStateV1::Ready { candidate, .. } => {
                Ok(RecoveryTailWitnessV1::Decision {
                    frozen: candidate
                        .frozen
                        .ok_or_else(|| invalid("planned original frozen certificate missing"))?,
                    candidate: candidate.candidate,
                })
            }
            _ => Err(invalid(
                "recovery plan requires complete original fixed tail material",
            )),
        }
    }
}
