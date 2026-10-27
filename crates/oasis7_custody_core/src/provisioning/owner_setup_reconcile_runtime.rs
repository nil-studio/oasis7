use super::super::super::participant::validation::{
    admission_for_descriptor, policy_for_descriptor,
};
use super::*;
impl AccountProvisionApprovalState {
    pub fn owner_setup_reconcile_nonces(
        &self,
        id: &str,
        attempt: &str,
    ) -> Result<OwnerSetupAccessNoncesV1> {
        let a = self
            .owner_setup_reconcile_attempt(id, attempt)
            .ok_or_else(invalid)?;
        Ok(OwnerSetupAccessNoncesV1 {
            policy: reconcile_nonce(id, attempt, &a.nonce, "policy")?,
            policy_session: reconcile_nonce(id, attempt, &a.nonce, "policy-session")?,
            authorize_session: reconcile_nonce(id, attempt, &a.nonce, "authorize-session")?,
        })
    }
    pub fn reserve_owner_setup_reconcile_read(
        &mut self,
        id: &str,
        attempt: &str,
        purpose: OwnerSetupReadPurposeV1,
        role: Role,
        now: u64,
    ) -> Result<OwnerSetupReadTicket> {
        self.reconcile_local(id, attempt, now)?;
        let a = self
            .owner_setup_reconcile_attempt(id, attempt)
            .ok_or_else(invalid)?;
        let stage = self
            .owner_setup_reconciles
            .get(id)
            .ok_or_else(invalid)?
            .plan
            .stage;
        let read = OwnerSetupReadTicket {
            stage,
            ticket: a.oracle_reads.checked_add(1).ok_or_else(invalid)?,
            purpose,
        };
        read.validate_role(role)?;
        if read.ticket > 5
            || read.ticket == 3 && a.access_claimed_at.is_none()
            || read.ticket >= 4 && a.access.is_none()
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        next.owner_setup_reconciles
            .get_mut(id)
            .and_then(|v| v.attempts.get_mut(attempt))
            .ok_or_else(invalid)?
            .oracle_reads = read.ticket;
        next.validate_persistence()?;
        *self = next;
        Ok(read)
    }
    pub fn claim_owner_setup_reconcile_access(
        &mut self,
        id: &str,
        attempt: &str,
        now: u64,
    ) -> Result<()> {
        self.reconcile_local(id, attempt, now)?;
        let mut next = self.clone();
        let a = next
            .owner_setup_reconciles
            .get_mut(id)
            .and_then(|v| v.attempts.get_mut(attempt))
            .ok_or_else(invalid)?;
        if a.oracle_reads != 2 || a.access_claimed_at.is_some() {
            return Err(invalid());
        }
        a.access_claimed_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub(super) fn reconcile_local(&self, id: &str, attempt: &str, now: u64) -> Result<()> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let a = v.attempts.get(attempt).ok_or_else(invalid)?;
        let r = v
            .book
            .original(id)
            .filter(|r| r.consumed_at().is_some())
            .ok_or_else(invalid)?;
        if self.frozen()
            || self.cancellation_operation_id.is_some()
            || now < a.reserved_at.0
            || now >= v.plan.deadline.0
            || self.provision.security_epoch() != r.input().request.context.security_version
            || a.completed_at.is_some()
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn reconcile_session(
        &self,
        id: &str,
        nonce: &str,
        s: &ParticipantSessionEvidenceV1,
        now: u64,
        epoch: DecimalU64,
    ) -> Result<()> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let session = v.session.as_ref().ok_or_else(invalid)?;
        let d = &self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?
            .descriptor;
        s.status
            .status
            .validate_for(
                &d.native_binding,
                nonce,
                session.native_binding_generation,
                epoch,
                now,
            )
            .map_err(|_| invalid())?;
        let status = &s.status.status;
        if !status.active
            || status.frozen
            || status.pending
            || status.session_ref != session.session_ref
            || s.private_sid_generation != session.private_sid_generation
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn owner_setup_reconcile_access_checks(
        &self,
        id: &str,
        attempt: &str,
        e: &OwnerSetupAccessEvidenceV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.owner_setup_reconcile_checks(id, attempt, current.provision, now)?;
        let a = self
            .owner_setup_reconcile_attempt(id, attempt)
            .ok_or_else(invalid)?;
        let claimed = a.access_claimed_at.ok_or_else(invalid)?.0;
        if a.oracle_reads < 3 || a.access.as_ref().is_some_and(|old| old != e) {
            return Err(invalid());
        }
        let d = &self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?
            .descriptor;
        let n = self.owner_setup_reconcile_nonces(id, attempt)?;
        policy_for_descriptor(d, &n.policy, &e.policy)?;
        admission_for_descriptor(d, &e.policy, &e.admission)?;
        let mut checks = append_participant_signature_checks(
            checks,
            ParticipantSignatureFact::Policy(&e.policy, &d.world.service_public_key),
        )?;
        checks = append_participant_signature_checks(
            checks,
            ParticipantSignatureFact::Admission(
                &e.admission,
                e.policy.response.payload["policy"]["issuer_public_key"]
                    .as_str()
                    .ok_or_else(invalid)?,
            ),
        )?;
        for (nonce, status) in [
            (&n.policy_session, &e.policy_session),
            (&n.authorize_session, &e.authorize_session),
        ] {
            if status.status.status.observed_at.0 < claimed {
                return Err(invalid());
            }
            self.reconcile_session(
                id,
                nonce,
                status,
                now,
                current.provision.approval.authority.service_epoch,
            )?;
            checks = append_native_history_checks(
                checks,
                current.provision.approval,
                NativeFact::Status(&status.status),
                current.native_scope.native_root_key_id,
                current.native_scope.native_root_public_key,
                &digest(&(id, attempt, nonce))?,
            )?;
        }
        Ok(checks)
    }
    pub fn record_owner_setup_reconcile_access(
        &mut self,
        id: &str,
        attempt: &str,
        e: OwnerSetupAccessEvidenceV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier
            .verify(&self.owner_setup_reconcile_access_checks(id, attempt, &e, current, now)?)?;
        let mut next = self.clone();
        let a = next
            .owner_setup_reconciles
            .get_mut(id)
            .and_then(|v| v.attempts.get_mut(attempt))
            .ok_or_else(invalid)?;
        if let Some(old) = &a.access {
            return if old == &e {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        a.access = Some(e);
        a.access_recorded_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub(super) fn owner_setup_reconcile_view(
        &self,
        id: &str,
        attempt: &str,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<OwnerSetupReconcilePermitV1> {
        self.owner_setup_reconcile_checks(id, attempt, current.provision, now)?;
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let a = v.attempts.get(attempt).ok_or_else(invalid)?;
        let p = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let mut participant = p.descriptor.clone();
        participant.session = v.session.clone().ok_or_else(invalid)?;
        let mut access = self.owner_setup_participant_access(&v.plan.setup_id)?;
        if let Some(e) = &a.access {
            access.admission = e.admission.clone();
        }
        let mut observation = participant.owner_read.clone();
        observation.observation_nonce = Some(reconcile_nonce(id, attempt, &a.nonce, "preflight")?);
        observation.deadline_unix_ms = Some(v.plan.deadline.0);
        observation.fixed_commit = None;
        observation.formal_anchor = None;
        observation.min_commit = None;
        let final_goal = if v.plan.stage == OwnerSetupStageV1::FinalObservation {
            match self.owner_setup_expected(&v.plan.setup_id, v.plan.stage)? {
                OwnerSetupSigningRequestV1::FinalObservation { goal, .. } => Some(goal),
                _ => return Err(invalid()),
            }
        } else {
            None
        };
        let observed = a
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification
            .observed_at
            .0;
        Ok(OwnerSetupReconcilePermitV1 {
            reconcile_id: id.into(),
            setup_id: v.plan.setup_id.clone(),
            provision_id: v.plan.provision_id.clone(),
            attempt_id: attempt.into(),
            kind: OwnerSetupReconcilePermitKindV1::Session,
            read_purpose: OwnerSetupReadPurposeV1::Session,
            read_ticket: a.oracle_reads,
            target_security_epoch: current.provision.approval.authority.security_version,
            source_revision: digest(&a.source)?,
            stage: v.plan.stage,
            participant,
            participant_access: access,
            observation,
            original_lookup_bytes: a.original_lookup_bytes.clone(),
            final_goal,
            nonces: self.owner_setup_reconcile_nonces(id, attempt)?,
            preflight_session_nonce: reconcile_nonce(id, attempt, &a.nonce, "preflight")?,
            lookup_session_nonce: reconcile_nonce(id, attempt, &a.nonce, "lookup")?,
            poststate_session_nonce: reconcile_nonce(id, attempt, &a.nonce, "poststate")?,
            authority_revision: digest(&(
                "oasis7.custody.setup-reconcile-authority.v1",
                v.plan.digest()?,
                &a.source,
                current.provision.approval.authority.security_version,
            ))?,
            service_epoch: current.provision.approval.authority.service_epoch,
            observed_at: DecimalU64(now),
            expires_at: DecimalU64(
                observed
                    .checked_add(5000)
                    .ok_or_else(invalid)?
                    .min(a.source.expires_at.0)
                    .min(v.plan.deadline.0),
            ),
        })
    }
    pub fn authorize_owner_setup_reconcile_read(
        &self,
        id: &str,
        reservation: OwnerSetupReconcileRead<'_>,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupReconcilePermitV1> {
        let OwnerSetupReconcileRead {
            attempt_id: attempt,
            read,
            role,
        } = reservation;
        read.validate_role(role)?;
        let a = self
            .owner_setup_reconcile_attempt(id, attempt)
            .ok_or_else(invalid)?;
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        if read.stage != v.plan.stage || read.ticket == 0 || read.ticket > a.oracle_reads {
            return Err(invalid());
        }
        let checks = if read.purpose == OwnerSetupReadPurposeV1::Session {
            self.owner_setup_reconcile_checks(id, attempt, current.provision, now)?
        } else {
            self.owner_setup_reconcile_access_checks(
                id,
                attempt,
                a.access.as_ref().ok_or_else(invalid)?,
                current,
                now,
            )?
        };
        verifier.verify(&checks)?;
        let mut view = self.owner_setup_reconcile_view(id, attempt, current, now)?;
        view.kind = if read.purpose == OwnerSetupReadPurposeV1::Session {
            OwnerSetupReconcilePermitKindV1::Session
        } else {
            OwnerSetupReconcilePermitKindV1::Sign
        };
        view.read_purpose = read.purpose;
        view.read_ticket = read.ticket;
        Ok(view)
    }
}
pub(super) fn reconcile_nonce(
    id: &str,
    attempt: &str,
    nonce: &str,
    purpose: &str,
) -> Result<String> {
    digest(&(
        "oasis7.custody.setup-reconcile-nonce.v1",
        id,
        attempt,
        nonce,
        purpose,
    ))
}
