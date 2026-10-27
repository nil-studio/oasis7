use super::*;
impl AccountProvisionApprovalState {
    pub(super) fn check_native_current(
        &self,
        current: &ProvisionApprovalContext<'_>,
        expected: u64,
        now: u64,
    ) -> Result<()> {
        self.current(current, now)?;
        let approved = self.accepted_approval()?;
        self.historical_scope(&approved.input().operation_id, current)?;
        let proof = &approved.input().approval.proof;
        let a = &current.approval.authority;
        let p = &proof.context;
        let source = &current.approval.source;
        if self.cancellation_operation_id.is_some()
            || current.approval.frozen
            || current
                .approval
                .fence
                .check_epoch(p.service_epoch.0)
                .is_err()
            || self.provision.security_epoch().0 != expected
            || a.security_version.0 != expected
            || p.security_version.0 != 0
            || a.environment != p.environment
            || a.custody_account_id != p.custody_account_id
            || a.audience != p.audience
            || a.policy_version != p.policy_version
            || a.service_epoch != p.service_epoch
            || source.manager_session_ref != proof.manager_session_ref
            || source.origin_credential_id != proof.origin_credential_id
            || source.origin_credential_version != proof.origin_credential_version
            || source.passkey_credential_id != proof.passkey_credential_id
            || source.passkey_credential_version != proof.passkey_credential_version
            || source.manager_policy_version != proof.manager_policy_version
            || source.manager_security_version != proof.manager_security_version
            || now >= source.session_expires_at.0
            || now >= source.session_idle_expires_at.0
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        if expected != 0 && (expected != 1 || self.bootstrap_activation.is_none()) {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
    pub(super) fn reserve_native_origin(&mut self, id: &str, nonce: &str, now: u64) -> Result<()> {
        let source = &self
            .accepted_approval()?
            .verification()
            .ok_or(ApprovalAdmissionError::Missing)?
            .verification;
        self.provision
            .check_current(self.provision.security_epoch().0, now)?;
        if self.cancellation_operation_id.is_some()
            || !super::super::types::valid_id(id)
            || nonce.len() != 64
            || !nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.origin_inspections.contains_key(id)
            || self.role_key_dispatches.contains_key(id)
            || self
                .origin_inspections
                .values()
                .any(|v| v.verify_nonce == nonce)
            || self.book.records().any(|v| v.verify_nonce() == nonce)
            || now >= source.source_session_expires_at.0
            || now >= source.source_session_idle_expires_at.0
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let expires = now
            .checked_add(30_000)
            .ok_or(ProvisionApprovalError::Conflict)?
            .min(self.draft.plan.expires_at.0)
            .min(source.source_session_expires_at.0)
            .min(source.source_session_idle_expires_at.0);
        for _ in 0..8 {
            self.provision
                .charge_transport(self.provision.security_epoch().0, now)?;
        }
        self.origin_inspections.insert(
            id.into(),
            ProvisionOriginInspectionV1 {
                operation_id: id.into(),
                verify_nonce: nonce.into(),
                created_at: DecimalU64(now),
                expires_at: DecimalU64(expires),
                claimed_at: None,
                request_bytes: None,
                verified_at: None,
                verification: None,
            },
        );
        Ok(())
    }
    pub fn claim_native_origin_inspection(
        &mut self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<crate::approval::ApprovalVerifyRequestV1> {
        let stage = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.check_native_current(current, stage.target_security_epoch.0, now)?;
        let inspection = self
            .origin_inspections
            .get(&stage.inspection_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if inspection.claimed_at.is_some() || now >= inspection.expires_at.0 {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut request = self.accepted_approval()?.native_verify_request()?;
        request.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
        request.verify_nonce = inspection.verify_nonce.clone();
        let inspection_id = stage.inspection_id.clone();
        let mut next = self.clone();
        let saved = next
            .origin_inspections
            .get_mut(&inspection_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        saved.claimed_at = Some(DecimalU64(now));
        saved.request_bytes = Some(
            serde_json::to_string(&request)
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?,
        );
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
    pub fn native_origin_result_checks(
        &self,
        id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let stage = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.check_native_current(current, stage.target_security_epoch.0, now)?;
        let inspection = self
            .origin_inspections
            .get(&stage.inspection_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.validate_origin_response(inspection, signed, now)?;
        if inspection
            .verification
            .as_ref()
            .is_some_and(|v| v != signed)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        Ok(registered_origin_checks(
            self.accepted_approval()?.input(),
            &stage.inspection_id,
            &inspection.verify_nonce,
            current.approval,
            signed,
        )?)
    }
    pub fn complete_native_origin_inspection(
        &mut self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.native_origin_result_checks(id, &signed, current, now)?)?;
        let inspection_id = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?
            .inspection_id
            .clone();
        let mut next = self.clone();
        let saved = next
            .origin_inspections
            .get_mut(&inspection_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        saved.verified_at = Some(DecimalU64(now));
        saved.verification = Some(signed);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub(super) fn native_origin_checks(
        &self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let stage = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let inspection = self
            .origin_inspections
            .get(&stage.inspection_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let signed = inspection
            .verification
            .as_ref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.native_origin_result_checks(id, signed, current, now)
    }
}
