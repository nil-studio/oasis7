//! Exact paid operation continuation; never a general Closing exemption.
use super::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
impl AccountProvisionApprovalState {
    pub fn test_session_v2_prepare_dispatch_checks(
        &self,
        id: &str,
        expected: &SignedExecutionPreparePermitsV2,
        accepted_ordinal: u64,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_session_v2_record(id)?;
        let book = s.execution_book.as_ref().ok_or_else(invalid)?;
        let record = book.prepare_service_record().ok_or_else(invalid)?;
        if record.accepted_ordinal != accepted_ordinal || record.permits != *expected {
            return Err(invalid());
        }
        self.test_session_v2_prepaid_prepare_local(id, record, now)?;
        let p = &record.permits.sign.permit;
        let root = s
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?;
        let source = s
            .sources
            .get(&book.source_operation_id)
            .ok_or_else(invalid)?;
        if p.source_operation_id != current.source_operation_id
            || p.source_operation_id != book.source_operation_id
            || p.source_nonce != source.source.verify_nonce
            || p.approved_input_digest != s.input_digest()?
            || root.approved_input_digest != p.approved_input_digest
            || root
                .credential
                .as_ref()
                .is_none_or(|c| c.generation != p.credential_generation)
            || now.get() < p.issued_at.get()
            || now.get() >= p.expires_at.get()
            || p.expires_at.get() > current.provision.approval.handoff_until.0
            || p.expires_at.get() > current.provision.approval.source.session_expires_at.0
            || p.expires_at.get() > current.provision.approval.source.session_idle_expires_at.0
            || p.expires_at.get() > current.lifetime.lifetime.status.expires_at.0
            || p.expires_at.get() > current.lifetime.lifetime.session_expires_at.0
            || p.expires_at.get() > current.lifetime.lifetime.session_idle_expires_at.0
        {
            return Err(invalid());
        }
        self.test_session_v2_live_scope_after_local(id, SessionPurposeV1::Execution, current, now)
    }
    pub fn continue_test_session_v2_prepare_dispatch(
        &self,
        id: &str,
        expected: &SignedExecutionPreparePermitsV2,
        accepted_ordinal: u64,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.test_session_v2_prepare_dispatch_checks(
            id,
            expected,
            accepted_ordinal,
            current,
            now,
        )?)?;
        Ok(())
    }
}
