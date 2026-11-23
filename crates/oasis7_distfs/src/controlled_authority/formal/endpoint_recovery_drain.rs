//! Existing native request stages perform old-epoch draining. This layer binds
//! their protected checkpoint metadata to the same immutable planned request.
use super::*;

impl FileFormalEndpoint {
    pub(super) fn validate_recovery_phase(&self) -> Result<(), FormalError> {
        let body = &self.store.body;
        if let Some(healthy) = &body.healthy_seal_authorization {
            healthy.verify_seal(
                body,
                &self.authority,
                body.seal
                    .as_ref()
                    .ok_or_else(|| invalid("native healthy authorization without sealed H"))?,
            )?;
        }
        let Some(plan) = &body.recovery_plan else {
            if body.writer_plan_ack.is_some()
                || body.recovery_progress.is_some()
                || body.recovery_drained.is_some()
            {
                return Err(invalid("native recovery phase without original plan"));
            }
            return Ok(());
        };
        plan.verify(&self.authority)?;
        let ack = body
            .writer_plan_ack
            .as_ref()
            .ok_or_else(|| invalid("native recovery writer acknowledgement missing"))?;
        ack.verify(plan, &self.authority)?;
        let pending: RecoveryPendingHandoffV1 = self.store.get(
            body.recovery_handoff
                .as_deref()
                .ok_or_else(|| invalid("native recovery original pending collection missing"))?,
        )?;
        if pending.root()? != plan.body.pending_handoff_root {
            return Err(invalid(
                "native recovery pending collection differs from the original plan",
            ));
        }
        let drained = if let Some(seal) = &body.seal {
            let writer = seal
                .writer_seal
                .as_ref()
                .ok_or_else(|| invalid("native recovery writer seal missing"))?;
            writer.verify_with_ack(plan, &self.authority, ack)?;
            if body.recovery_drained.is_some() || seal.recovery_plan.as_ref() != Some(plan) {
                return Err(invalid("sealed recovery phase or plan differs"));
            }
            Some(&writer.drained)
        } else {
            body.recovery_drained.as_ref()
        };
        if let Some(drained) = drained {
            let id = self
                .store
                .lookup(&drained.qualified.decision.record.request_digest)?
                .ok_or_else(|| invalid("drained original Qualified entry missing"))?;
            let entry: FormalRequestStateV1 = self.store.get(&id)?;
            if drained.verify(plan, &self.authority)? != body.head
                || body.pending.is_some()
                || body.recovery_progress.is_some()
                || body.decision_head.is_some()
                || !matches!(entry, FormalRequestStateV1::Qualified { proof, .. } if proof == drained.qualified)
            {
                return Err(invalid(
                    "drained proof differs from actual original Qualified state",
                ));
            }
        } else {
            let progress = body
                .recovery_progress
                .as_ref()
                .ok_or_else(|| invalid("native recovery progress missing"))?;
            progress.verify(plan, &self.authority)?;
            if body.head != plan.body.intent.sealed_cut
                || body.pending.is_none()
                || self.planned_progress(progress.fixed.clone())? != *progress
            {
                return Err(invalid(
                    "native recovery progress differs from actual original request",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn original_qualified_proof(
        &self,
    ) -> Result<FormalQualifiedCommitProofV1, FormalError> {
        let parent = self
            .parent
            .as_ref()
            .ok_or_else(|| invalid("recovery qualified parent missing"))?;
        let id = self
            .store
            .lookup(&parent.record().request_digest)?
            .ok_or_else(|| invalid("recovery original cut private index missing"))?;
        let FormalRequestStateV1::Qualified { proof, .. } = self.store.get(&id)? else {
            return Err(invalid("recovery original cut must be locally Qualified"));
        };
        if proof.decision.record != *parent.record() {
            return Err(invalid(
                "recovery original qualified cut differs from native parent",
            ));
        }
        Ok(proof)
    }

    pub(super) fn planned_progress(
        &self,
        fixed: RecoveryTailWitnessV1,
    ) -> Result<RecoveryDrainProgressV1, FormalError> {
        let entry = self
            .store
            .body
            .pending
            .as_ref()
            .map(|id| self.store.get::<FormalRequestStateV1>(id))
            .transpose()?;
        progress_from_entry(fixed, entry.as_ref())
    }

    pub(super) fn update_recovery_progress(
        &self,
        entry: &FormalRequestStateV1,
        next: &mut HotBody,
    ) -> Result<(), FormalError> {
        let Some(plan) = &self.store.body.recovery_plan else {
            return Ok(());
        };
        if self.store.body.recovery_drained.is_some() || self.store.body.seal.is_some() {
            return Err(invalid(
                "drained or sealed recovery refuses another old-epoch write",
            ));
        }
        let previous = self
            .store
            .body
            .recovery_progress
            .as_ref()
            .ok_or_else(|| invalid("planned recovery original fixed progress missing"))?;
        let progress = progress_from_entry(previous.fixed.clone(), Some(entry))?;
        progress.verify(plan, &self.authority)?;
        if let FormalRequestStateV1::Qualified { proof, .. } = entry {
            let drained = RecoveryDrainedProofV1 {
                schema_version: 1,
                plan_digest: plan.digest()?,
                fixed: previous.fixed.clone(),
                qualified: proof.clone(),
            };
            if drained.verify(plan, &self.authority)? != next.head || next.pending.is_some() {
                return Err(invalid(
                    "recovery qualified publication did not drain the original slot",
                ));
            }
            next.recovery_progress = None;
            next.recovery_drained = Some(drained);
        } else {
            next.recovery_progress = Some(progress);
        }
        Ok(())
    }

    pub fn recovery_drained_proof(&mut self) -> Result<RecoveryDrainedProofV1, FormalError> {
        let _guard = self.guard()?;
        let plan =
            self.store.body.recovery_plan.as_ref().ok_or_else(|| {
                invalid("recovery drained proof requires original persisted plan")
            })?;
        let drained = self
            .store
            .body
            .recovery_drained
            .as_ref()
            .or_else(|| {
                self.store
                    .body
                    .seal
                    .as_ref()
                    .and_then(|seal| seal.writer_seal.as_ref())
                    .map(|writer| &writer.drained)
            })
            .ok_or_else(|| invalid("recovery original epoch is not yet drained"))?;
        if drained.verify(plan, &self.authority)? != self.store.body.head
            || self.store.body.pending.is_some()
        {
            return Err(invalid(
                "recovery drained proof differs from actual native head/slot",
            ));
        }
        Ok(drained.clone())
    }
}

fn progress_from_entry(
    fixed: RecoveryTailWitnessV1,
    entry: Option<&FormalRequestStateV1>,
) -> Result<RecoveryDrainProgressV1, FormalError> {
    let candidate = entry.and_then(FormalRequestStateV1::candidate);
    let decision = match entry {
        Some(FormalRequestStateV1::Ready { decision, .. }) => Some(decision.clone()),
        Some(FormalRequestStateV1::Qualified { proof, .. }) => Some(proof.decision.clone()),
        _ => None,
    };
    Ok(RecoveryDrainProgressV1 {
        fixed,
        candidate: candidate.map(|candidate| candidate.candidate.clone()),
        candidate_material_digest: candidate
            .map(formal_candidate_material_digest)
            .transpose()?,
        decision,
    })
}
