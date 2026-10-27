use super::runtime::reconcile_nonce;
use super::*;

impl AccountProvisionApprovalState {
    pub fn owner_setup_reconcile_publish_checks(
        &self,
        id: &str,
        attempt: &str,
        candidate: &OwnerSetupReconcileReadCandidateV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let a = self
            .owner_setup_reconcile_attempt(id, attempt)
            .ok_or_else(invalid)?;
        let mut checks = self.owner_setup_reconcile_access_checks(
            id,
            attempt,
            a.access.as_ref().ok_or_else(invalid)?,
            current,
            now,
        )?;
        let view = self.owner_setup_reconcile_view(id, attempt, current, now)?;
        let o = &candidate.observation;
        if a.oracle_reads != 5
            || o.owner != view.participant_access.owner
            || o.admission != view.participant_access.admission
            || o.read.request != view.observation
            || o.read.subject_public_key != view.participant.owner_key.public_key
            || o.response_profile
                != Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1)
            || candidate.goal.as_ref().map(|s| &s.request) != view.final_goal.as_ref()
            || a.observation.as_ref().is_some_and(|old| old != o)
            || a.goal_request
                .as_ref()
                .is_some_and(|old| Some(old) != candidate.goal.as_ref())
        {
            return Err(invalid());
        }
        checks = append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::Read(o, &view.participant.owner_key.public_key),
        )?;
        if let Some(goal) = &candidate.goal {
            if goal.subject_public_key != view.participant.owner_key.public_key {
                return Err(invalid());
            }
            checks = append_owner_setup_checks(
                checks,
                OwnerSetupSignatureFact::GoalRead(goal, &view.participant.owner_key.public_key),
            )?;
        }
        Ok(checks)
    }
    pub fn publish_owner_setup_reconcile_reads(
        &mut self,
        id: &str,
        attempt: &str,
        candidate: OwnerSetupReconcileReadCandidateV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupReconcilePermitV1> {
        verifier.verify(
            &self.owner_setup_reconcile_publish_checks(id, attempt, &candidate, current, now)?,
        )?;
        let mut next = self.clone();
        let a = next
            .owner_setup_reconciles
            .get_mut(id)
            .and_then(|v| v.attempts.get_mut(attempt))
            .ok_or_else(invalid)?;
        if a.observation.is_none() {
            a.observation = Some(candidate.observation);
            a.goal_request = candidate.goal;
            a.published_at = Some(DecimalU64(now));
        }
        next.validate_persistence()?;
        let mut result = next.owner_setup_reconcile_view(id, attempt, current, now)?;
        result.kind = OwnerSetupReconcilePermitKindV1::Preflight;
        *self = next;
        Ok(result)
    }
    pub fn reserve_owner_setup_reconcile_callback(
        &mut self,
        id: &str,
        attempt: &str,
        kind: OwnerSetupReconcileCallbackV1,
        now: u64,
    ) -> Result<()> {
        self.reconcile_local(id, attempt, now)?;
        let mut next = self.clone();
        let a = next
            .owner_setup_reconciles
            .get_mut(id)
            .and_then(|v| v.attempts.get_mut(attempt))
            .ok_or_else(invalid)?;
        let field = match kind {
            OwnerSetupReconcileCallbackV1::Admission => {
                if a.observation.is_none() {
                    return Err(invalid());
                }
                &mut a.admission_reserved_at
            }
            OwnerSetupReconcileCallbackV1::Lookup => {
                if a.admission.is_none() || a.original_lookup_bytes.is_none() {
                    return Err(invalid());
                }
                &mut a.lookup_reserved_at
            }
            OwnerSetupReconcileCallbackV1::Result => {
                if a.admission.is_none() {
                    return Err(invalid());
                }
                &mut a.completion_reserved_at
            }
        };
        if field.is_some() {
            return Err(invalid());
        }
        *field = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn owner_setup_reconcile_admission_checks(
        &self,
        id: &str,
        attempt: &str,
        e: &OwnerSetupAdmissionEvidenceV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let a = v.attempts.get(attempt).ok_or_else(invalid)?;
        let mut checks = self.owner_setup_reconcile_access_checks(
            id,
            attempt,
            a.access.as_ref().ok_or_else(invalid)?,
            current,
            now,
        )?;
        self.validate_reconcile_world(id, attempt, &e.world)?;
        if a.admission_reserved_at.is_none_or(|t| t.0 > now)
            || a.published_at
                .is_none_or(|t| e.session.status.status.observed_at.0 < t.0)
        {
            return Err(invalid());
        }
        self.reconcile_session(
            id,
            &reconcile_nonce(id, attempt, &a.nonce, "preflight")?,
            &e.session,
            now,
            current.provision.approval.authority.service_epoch,
        )?;
        checks = append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::World(&e.world, &v.plan.world_identity.service_public_key),
        )?;
        Ok(append_native_history_checks(
            checks,
            current.provision.approval,
            NativeFact::Status(&e.session.status),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &digest(&(id, attempt, "preflight"))?,
        )?)
    }
    pub fn record_owner_setup_reconcile_admission(
        &mut self,
        id: &str,
        attempt: &str,
        e: OwnerSetupAdmissionEvidenceV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupReconcilePermitV1> {
        verifier
            .verify(&self.owner_setup_reconcile_admission_checks(id, attempt, &e, current, now)?)?;
        let mut next = self.clone();
        let a = next
            .owner_setup_reconciles
            .get_mut(id)
            .and_then(|v| v.attempts.get_mut(attempt))
            .ok_or_else(invalid)?;
        if let Some(old) = &a.admission {
            if old != &e {
                return Err(ProvisionApprovalError::Conflict);
            }
        } else {
            a.admission = Some(e);
            a.admitted_at = Some(DecimalU64(now));
        }
        next.validate_persistence()?;
        let mut result = next.owner_setup_reconcile_view(id, attempt, current, now)?;
        result.kind = OwnerSetupReconcilePermitKindV1::LookupReady;
        *self = next;
        Ok(result)
    }
    pub fn claim_owner_setup_reconcile_lookup(
        &mut self,
        id: &str,
        attempt: &str,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<String> {
        let a = self
            .owner_setup_reconcile_attempt(id, attempt)
            .ok_or_else(invalid)?;
        if a.lookup_reserved_at.is_none_or(|t| t.0 > now)
            || a.lookup_claimed_at.is_some()
            || a.admitted_at.is_none_or(|t| t.0 > now || now - t.0 > 5000)
        {
            return Err(invalid());
        }
        verifier.verify(&self.owner_setup_reconcile_admission_checks(
            id,
            attempt,
            a.admission.as_ref().ok_or_else(invalid)?,
            current,
            now,
        )?)?;
        let bytes = a.original_lookup_bytes.clone().ok_or_else(invalid)?;
        let mut next = self.clone();
        next.owner_setup_reconciles
            .get_mut(id)
            .and_then(|v| v.attempts.get_mut(attempt))
            .ok_or_else(invalid)?
            .lookup_claimed_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(bytes)
    }
    pub(super) fn validate_reconcile_world(
        &self,
        id: &str,
        attempt: &str,
        w: &OwnerSetupWorldEvidenceV1,
    ) -> Result<()> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let a = v.attempts.get(attempt).ok_or_else(invalid)?;
        if a.observation.as_ref() != Some(&w.request) {
            return Err(invalid());
        }
        self.validate_owner_setup_world_access(
            &v.plan.setup_id,
            w,
            &a.access.as_ref().ok_or_else(invalid)?.admission,
        )?;
        let policy = &a.access.as_ref().ok_or_else(invalid)?.admission.request;
        let tick = w.response.payload.control.logical_tick;
        if tick < policy.valid_from_tick.0 || tick > policy.valid_until_tick.0 {
            return Err(invalid());
        }
        Ok(())
    }
}

impl AccountProvisionApprovalState {
    /// Same-transaction view after once-claim; no new read credit or remote I/O.
    pub fn owner_setup_reconcile_lookup_permit(
        &self,
        id: &str,
        attempt: &str,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupReconcilePermitV1> {
        let a = self
            .owner_setup_reconcile_attempt(id, attempt)
            .ok_or_else(invalid)?;
        if a.lookup_claimed_at
            .is_none_or(|t| t.0 > now || now - t.0 > 5000)
        {
            return Err(invalid());
        }
        verifier.verify(&self.owner_setup_reconcile_admission_checks(
            id,
            attempt,
            a.admission.as_ref().ok_or_else(invalid)?,
            current,
            now,
        )?)?;
        let mut view = self.owner_setup_reconcile_view(id, attempt, current, now)?;
        view.kind = OwnerSetupReconcilePermitKindV1::Lookup;
        Ok(view)
    }
}
impl AccountProvisionApprovalState {
    /// Called only after Result local once-reservation and current I/O, inside
    /// the same SQL transition; this grants the fixed post-state read only.
    pub fn owner_setup_reconcile_poststate_permit(
        &self,
        id: &str,
        attempt: &str,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupReconcilePermitV1> {
        let a = self
            .owner_setup_reconcile_attempt(id, attempt)
            .ok_or_else(invalid)?;
        let plan = &self
            .owner_setup_reconciles
            .get(id)
            .ok_or_else(invalid)?
            .plan;
        if a.completion_reserved_at.is_none_or(|t| t.0 > now)
            || a.completed_at.is_some()
            || a.admitted_at.is_none_or(|t| t.0 > now || now - t.0 > 5000)
            || !matches!(
                plan.stage,
                OwnerSetupStageV1::PrepareGoal | OwnerSetupStageV1::FinalObservation
            ) && a.lookup_claimed_at.is_none()
        {
            return Err(invalid());
        }
        verifier.verify(&self.owner_setup_reconcile_admission_checks(
            id,
            attempt,
            a.admission.as_ref().ok_or_else(invalid)?,
            current,
            now,
        )?)?;
        let mut view = self.owner_setup_reconcile_view(id, attempt, current, now)?;
        view.kind = OwnerSetupReconcilePermitKindV1::Poststate;
        Ok(view)
    }
}
