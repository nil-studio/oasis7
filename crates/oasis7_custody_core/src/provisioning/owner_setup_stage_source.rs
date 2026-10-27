use super::*;
impl AccountProvisionApprovalState {
    pub fn claim_owner_setup_stage_source(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<crate::approval::ApprovalVerifyRequestV1> {
        self.owner_setup_live(id, current, now)?;
        let mut next = self.clone();
        let setup = next.owner_setups.get_mut(id).ok_or_else(invalid)?;
        let original = setup.book.original(id).ok_or_else(invalid)?;
        let source = &mut setup.stages.get_mut(&stage).ok_or_else(invalid)?.source;
        if source.claimed_at.is_some() || now >= source.expires_at.0 {
            return Err(invalid());
        }
        let mut request = original.native_verify_request()?;
        request.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
        request.verify_nonce = source.verify_nonce.clone();
        source.claimed_at = Some(DecimalU64(now));
        source.request_bytes = Some(serde_json::to_string(&request).map_err(|_| invalid())?);
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
    pub fn owner_setup_stage_source_checks(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        self.owner_setup_live(id, current, now)?;
        let source = &self
            .owner_setup_outbox(id, stage)
            .ok_or_else(invalid)?
            .source;
        self.validate_owner_setup_inspection(id, source, signed, current.approval, now)
    }
    pub fn complete_owner_setup_stage_source(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        signed: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier
            .verify(&self.owner_setup_stage_source_checks(id, stage, &signed, current, now)?)?;
        let mut next = self.clone();
        let source = &mut next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?
            .source;
        source.verification = Some(signed);
        source.verified_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn owner_setup_stage_checks(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let signed = self
            .owner_setup_outbox(id, stage)
            .and_then(|o| o.source.verification.as_ref())
            .ok_or_else(invalid)?;
        self.owner_setup_stage_source_checks(id, stage, signed, current, now)
    }
}
