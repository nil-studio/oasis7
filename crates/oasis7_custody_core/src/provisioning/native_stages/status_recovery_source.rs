use super::*;
use crate::approval::{OriginStateV1, VerificationPurposeV1};
impl AccountProvisionApprovalState {
    pub fn session_recovery_origin_checks(
        &self,
        id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let r = self.recovery_current(id, current, now)?;
        let source = self.recovery_source_record(&r.funding)?;
        validate_recovery_origin_fact(source, &r.source_inspection, signed, now)?;
        let p = &source.input().approval.proof;
        let s = &current.approval.source;
        let v = &signed.verification;
        if s.manager_account_id != p.manager_account_id
            || s.manager_session_ref != p.manager_session_ref
            || s.origin_credential_id != p.origin_credential_id
            || s.origin_credential_version != p.origin_credential_version
            || s.passkey_credential_id != p.passkey_credential_id
            || s.passkey_credential_version != p.passkey_credential_version
            || s.manager_policy_version != p.manager_policy_version
            || s.manager_security_version != p.manager_security_version
            || s.session_expires_at != v.source_session_expires_at
            || s.session_idle_expires_at != v.source_session_idle_expires_at
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        if r.source_inspection
            .verification
            .as_ref()
            .is_some_and(|v| v != signed)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        Ok(registered_origin_checks(
            source.input(),
            &r.source_inspection.operation_id,
            &r.source_inspection.verify_nonce,
            current.approval,
            signed,
        )?)
    }
    pub fn complete_session_recovery_origin(
        &mut self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.session_recovery_origin_checks(id, &signed, current, now)?)?;
        let mut next = self.clone();
        let r = &mut next
            .session_status_recoveries
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?
            .source_inspection;
        r.verified_at = Some(DecimalU64(now));
        r.verification = Some(signed);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn session_status_recovery_checks(
        &self,
        id: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let r = self.recovery_current(id, current, now)?;
        if r.result.is_some() || scope.http_origin != r.http_origin {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let signed = r
            .source_inspection
            .verification
            .as_ref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        let checks = self.session_recovery_origin_checks(id, signed, current, now)?;
        let history = self
            .native_history
            .get(&r.request.history_digest)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let NativeStageResultV1::SessionStatus(status) = &history.result else {
            return Err(ProvisionApprovalError::BindingMismatch);
        };
        self.validate_known_native_history(history)?;
        Ok(append_native_history_checks(
            checks,
            current.approval,
            NativeFact::Status(status),
            scope.native_root_key_id,
            scope.native_root_public_key,
            &r.request.digest()?,
        )?)
    }
}
pub(super) fn validate_recovery_origin_fact(
    source: &ApprovalAdmissionRecord,
    inspection: &ProvisionOriginInspectionV1,
    signed: &SignedApprovalVerificationV1,
    now: u64,
) -> Result<()> {
    let input = source.input();
    let p = &input.approval.proof;
    let v = &signed.verification;
    let claimed = inspection
        .claimed_at
        .ok_or(ApprovalAdmissionError::Missing)?
        .0;
    if now >= inspection.expires_at.0
        || v.version != 1
        || v.purpose != VerificationPurposeV1::InspectOrigin
        || v.origin_state != OriginStateV1::Active
        || v.identity_namespace != p.identity_namespace
        || v.identity_instance_epoch != p.identity_instance_epoch
        || v.proof_digest
            != input
                .approval
                .digest()
                .map_err(|_| ProvisionApprovalError::BindingMismatch)?
        || v.approval_request_id != p.approval_request_id
        || v.audience != p.context.audience
        || v.intent_digest != p.intent_digest
        || v.verify_nonce != inspection.verify_nonce
        || v.proof_expires_at != p.expires_at
        || v.manager_security_version != p.manager_security_version
        || v.manager_policy_version != p.manager_policy_version
        || v.credential_version != p.passkey_credential_version
        || v.origin_credential_version != p.origin_credential_version
        || v.observed_at.0 < claimed
        || v.observed_at.0 > now
        || now - v.observed_at.0 > 5000
        || now >= v.source_session_expires_at.0
        || now >= v.source_session_idle_expires_at.0
    {
        return Err(ProvisionApprovalError::BindingMismatch);
    }
    Ok(())
}
