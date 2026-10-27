use super::*;
impl AccountProvisionApprovalState {
    /// Called only inside the same SQL transition that persisted the immutable
    /// candidate. This returns a preflight-read scope, never an effect grant.
    pub fn owner_setup_published_permit(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupPermitViewV1> {
        let o = self.owner_setup_outbox(id, stage).ok_or_else(invalid)?;
        if o.original.is_none() || o.oracle_reads != OWNER_SETUP_MAX_ORACLE_READS {
            return Err(invalid());
        }
        let candidate = OwnerSetupCandidateV1 {
            original: o.original.clone().ok_or_else(invalid)?,
            observation: o.observation.clone().ok_or_else(invalid)?,
        };
        verifier.verify(&self.owner_setup_publish_checks(id, stage, &candidate, current, now)?)?;
        let mut view = self.owner_setup_view(
            id,
            OwnerSetupReadTicket {
                stage,
                ticket: OWNER_SETUP_MAX_ORACLE_READS,
                purpose: OwnerSetupReadPurposeV1::Sign,
            },
            current.provision,
            now,
        )?;
        view.kind = OwnerSetupPermitKindV1::Preflight;
        Ok(view)
    }
    /// Callback-only final view: no extra read/transport credit is minted.
    pub fn owner_setup_effect_permit(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupPermitViewV1> {
        let o = self.owner_setup_dispatched(id, stage)?;
        if o.result.is_some() || o.claimed_at.is_none() {
            return Err(invalid());
        }
        verifier.verify(&self.owner_setup_stored_admission_checks(id, stage, current, now)?)?;
        let mut view = self.owner_setup_view(
            id,
            OwnerSetupReadTicket {
                stage,
                ticket: OWNER_SETUP_MAX_ORACLE_READS,
                purpose: OwnerSetupReadPurposeV1::Sign,
            },
            current.provision,
            now,
        )?;
        view.kind = if o.lookup_claimed_at.is_some() {
            OwnerSetupPermitKindV1::Lookup
        } else {
            OwnerSetupPermitKindV1::Effect
        };
        Ok(view)
    }
}
