use super::*;
impl AccountProvisionApprovalState {
    pub fn claim_owner_setup_source(
        &mut self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<crate::approval::ApprovalVerifyRequestV1> {
        self.owner_setup_live(id, current, now)?;
        let mut next = self.clone();
        let v = next.owner_setups.get_mut(id).ok_or_else(invalid)?;
        let r = v.book.original(id).ok_or_else(invalid)?;
        let source = v.source.as_mut().ok_or_else(invalid)?;
        if source.claimed_at.is_some() || now >= source.expires_at.0 {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut request = r.native_verify_request()?;
        request.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
        request.verify_nonce = source.verify_nonce.clone();
        source.request_bytes = Some(serde_json::to_string(&request).map_err(|_| invalid())?);
        source.claimed_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
    pub(super) fn owner_setup_live(
        &self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<()> {
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        let r = v
            .book
            .original(id)
            .filter(|r| r.stage() == ApprovalAdmissionStage::Consumed)
            .ok_or_else(invalid)?;
        r.check_recorded_context(current.approval)?;
        let c = current.approval;
        let expected = &r.input().request.context;
        if self.frozen()
            || self.provision.security_epoch() != expected.security_version
            || c.authority != *expected
            || c.frozen
            || c.fence.check_epoch(expected.service_epoch.0).is_err()
            || c.identity_namespace != r.input().approval.proof.identity_namespace
            || c.identity_instance_epoch != r.input().approval.proof.identity_instance_epoch
            || now < r.consumed_at().ok_or_else(invalid)?
            || now >= v.plan.deadline.0
            || now >= c.source.session_expires_at.0
            || now >= c.source.session_idle_expires_at.0
            || current.policy.version != v.plan.bootstrap_config_version.0
            || current.world_identity != &v.plan.world_identity
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn owner_setup_source_checks(
        &self,
        id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        self.owner_setup_live(id, current, now)?;
        self.validate_owner_setup_source_fact(id, signed, current.approval, now)
    }
    pub(super) fn validate_owner_setup_source_fact(
        &self,
        id: &str,
        signed: &SignedApprovalVerificationV1,
        context: &ApprovalAdmissionContext,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let i = self
            .owner_setups
            .get(id)
            .and_then(|v| v.source.as_ref())
            .ok_or_else(invalid)?;
        self.validate_owner_setup_inspection(id, i, signed, context, now)
    }
    pub(super) fn validate_owner_setup_inspection(
        &self,
        id: &str,
        i: &ProvisionOriginInspectionV1,
        signed: &SignedApprovalVerificationV1,
        context: &ApprovalAdmissionContext,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let setup = self.owner_setups.get(id).ok_or_else(invalid)?;
        let r = setup.book.original(id).ok_or_else(invalid)?;
        let p = &r.input().approval.proof;
        let v = &signed.verification;
        let s = &context.source;
        if i.claimed_at.is_none()
            || now >= i.expires_at.0
            || v.version != 1
            || v.purpose != crate::approval::VerificationPurposeV1::InspectOrigin
            || v.origin_state != crate::approval::OriginStateV1::Active
            || v.proof_state
                != if now >= p.expires_at.0 {
                    crate::approval::ProofStateV1::Expired
                } else {
                    crate::approval::ProofStateV1::Active
                }
            || v.proof_digest != r.input().approval.digest().map_err(|_| invalid())?
            || v.identity_namespace != p.identity_namespace
            || v.identity_instance_epoch != p.identity_instance_epoch
            || v.approval_request_id != p.approval_request_id
            || v.audience != p.context.audience
            || v.intent_digest != p.intent_digest
            || v.verify_nonce != i.verify_nonce
            || v.proof_expires_at != p.expires_at
            || v.manager_security_version != s.manager_security_version
            || v.manager_policy_version != s.manager_policy_version
            || v.credential_version != s.passkey_credential_version
            || v.origin_credential_version != s.origin_credential_version
            || v.source_session_expires_at != s.session_expires_at
            || v.source_session_idle_expires_at != s.session_idle_expires_at
            || v.observed_at.0 < i.claimed_at.ok_or_else(invalid)?.0
            || v.observed_at.0 > now
            || now - v.observed_at.0 > 5000
            || i.verification.as_ref().is_some_and(|s| s != signed)
        {
            return Err(invalid());
        }
        Ok(registered_origin_checks(
            r.input(),
            &i.operation_id,
            &i.verify_nonce,
            context,
            signed,
        )?)
    }
    pub fn complete_owner_setup_source(
        &mut self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.owner_setup_source_checks(id, &signed, current, now)?)?;
        let mut next = self.clone();
        let i = next
            .owner_setups
            .get_mut(id)
            .ok_or_else(invalid)?
            .source
            .as_mut()
            .ok_or_else(invalid)?;
        i.verification = Some(signed);
        i.verified_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn owner_setup_checks(
        &self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let signed = self
            .owner_setups
            .get(id)
            .and_then(|v| v.source.as_ref())
            .and_then(|v| v.verification.as_ref())
            .ok_or_else(invalid)?;
        self.owner_setup_source_checks(id, signed, current, now)
    }
}
