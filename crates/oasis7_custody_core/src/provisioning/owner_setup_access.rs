use super::super::participant::validation::{admission_for_descriptor, policy_for_descriptor};
use super::*;
impl AccountProvisionApprovalState {
    pub fn owner_setup_access_nonces(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
    ) -> Result<OwnerSetupAccessNoncesV1> {
        let o = self.owner_setup_outbox(id, stage).ok_or_else(invalid)?;
        Ok(OwnerSetupAccessNoncesV1 {
            policy: digest(&(
                "oasis7.custody.owner-setup-access-policy.v1",
                id,
                stage,
                &o.session_nonce,
            ))?,
            policy_session: digest(&(
                "oasis7.custody.owner-setup-access-policy-session.v1",
                id,
                stage,
                &o.session_nonce,
            ))?,
            authorize_session: digest(&(
                "oasis7.custody.owner-setup-access-authorize-session.v1",
                id,
                stage,
                &o.session_nonce,
            ))?,
        })
    }
    /// Called in the same SQL transition as authorized Identity session read 2.
    /// This only burns the one fixed policy/authorize pair before either I/O.
    pub fn claim_owner_setup_access(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        now: u64,
    ) -> Result<()> {
        self.owner_setup_local(id, now)?;
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?;
        if o.oracle_reads != 2 || o.access_claimed_at.is_some() || o.original.is_some() {
            return Err(invalid());
        }
        o.access_claimed_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn owner_setup_access_checks(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        evidence: &OwnerSetupAccessEvidenceV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.owner_setup_stage_checks(id, stage, current.provision, now)?;
        self.validate_owner_setup_access(
            id,
            stage,
            evidence,
            now,
            current.provision.approval.authority.service_epoch,
        )?;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let checks = append_participant_signature_checks(
            checks,
            ParticipantSignatureFact::Policy(
                &evidence.policy,
                &participant.descriptor.world.service_public_key,
            ),
        )?;
        let checks = append_participant_signature_checks(
            checks,
            ParticipantSignatureFact::Admission(
                &evidence.admission,
                evidence.policy.response.payload["policy"]["issuer_public_key"]
                    .as_str()
                    .ok_or_else(invalid)?,
            ),
        )?;
        let checks = append_native_history_checks(
            checks,
            current.provision.approval,
            NativeFact::Status(&evidence.policy_session.status),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &digest(&("owner-setup-access-policy", id, stage))?,
        )?;
        Ok(append_native_history_checks(
            checks,
            current.provision.approval,
            NativeFact::Status(&evidence.authorize_session.status),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &digest(&("owner-setup-access-authorize", id, stage))?,
        )?)
    }
    pub fn record_owner_setup_access(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        evidence: OwnerSetupAccessEvidenceV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.owner_setup_access_checks(id, stage, &evidence, current, now)?)?;
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?;
        if let Some(old) = &o.access {
            return if old == &evidence {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        o.access = Some(evidence);
        o.access_recorded_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn owner_setup_access_current_checks(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let evidence = self
            .owner_setup_outbox(id, stage)
            .and_then(|o| o.access.as_ref())
            .ok_or_else(invalid)?;
        self.owner_setup_access_checks(id, stage, evidence, current, now)
    }
    pub fn owner_setup_stage_access(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
    ) -> Result<ws::participant_discovery::ParticipantOwnerViewRequestV1> {
        let mut original = self.owner_setup_participant_access(id)?;
        original.admission = self
            .owner_setup_outbox(id, stage)
            .and_then(|o| o.access.as_ref())
            .ok_or_else(invalid)?
            .admission
            .clone();
        Ok(original)
    }
    pub(super) fn validate_owner_setup_access(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        evidence: &OwnerSetupAccessEvidenceV1,
        now: u64,
        service_epoch: DecimalU64,
    ) -> Result<()> {
        let o = self.owner_setup_outbox(id, stage).ok_or_else(invalid)?;
        let claimed = o.access_claimed_at.ok_or_else(invalid)?.0;
        if o.oracle_reads < 3
            || claimed > now
            || o.access.as_ref().is_some_and(|old| old != evidence)
        {
            return Err(invalid());
        }
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let nonces = self.owner_setup_access_nonces(id, stage)?;
        policy_for_descriptor(&participant.descriptor, &nonces.policy, &evidence.policy)?;
        admission_for_descriptor(
            &participant.descriptor,
            &evidence.policy,
            &evidence.admission,
        )?;
        for (nonce, status) in [
            (&nonces.policy_session, &evidence.policy_session),
            (&nonces.authorize_session, &evidence.authorize_session),
        ] {
            if status.status.status.observed_at.0 < claimed {
                return Err(invalid());
            }
            self.validate_owner_setup_session(id, nonce, status, now, service_epoch)?;
        }
        Ok(())
    }
}
