//! Non-World live authority checks shared by V2 cleanup and other purpose gates.
use super::*;
impl AccountProvisionApprovalState {
    pub fn test_session_v2_lifetime_nonce(&self, id: &str, source_id: &str) -> Result<String> {
        let s = self.test_session_v2_record(id)?;
        let source = s.sources.get(source_id).ok_or_else(invalid)?;
        if source.api_version != 2 || source.approved_input_digest != s.input_digest()? {
            return Err(invalid());
        }
        digest(&(
            "oasis7.custody.test-session-current-lifetime.v2",
            id,
            s.input_digest()?,
            source_id,
            &source.source.verify_nonce,
        ))
    }
    pub fn test_session_v2_live_scope(
        &self,
        id: &str,
        purpose: SessionPurposeV1,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        self.test_session_v2_local(id, purpose, now)?;
        self.test_session_v2_live_scope_after_local(id, purpose, current, now)
    }
    /// Only closed prepaid consumers may perform their own exact local gate.
    pub(in crate::provisioning) fn test_session_v2_live_scope_after_local(
        &self,
        id: &str,
        purpose: SessionPurposeV1,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_session_v2_record(id)?;
        let c = current.provision.approval;
        let r = s
            .book
            .original(id)
            .filter(|r| r.stage() == ApprovalAdmissionStage::Consumed)
            .ok_or_else(invalid)?;
        r.check_recorded_context(c)?;
        let wrapper = s
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
        if wrapper.api_version != 2 || wrapper.approved_input_digest != s.input_digest()? {
            return Err(invalid());
        }
        let source = &wrapper.source;
        let signed = source.verification.as_ref().ok_or_else(invalid)?;
        if current.source_ticket == 0
            || current.source_ticket
                > *s.source_oracle_reads
                    .get(current.source_operation_id)
                    .ok_or_else(invalid)?
        {
            return Err(invalid());
        }
        if c.authority != s.input.request.context
            || c.frozen
            || c.fence.check_epoch(c.authority.service_epoch.0).is_err()
            || current.provision.world_identity != &self.draft.world_identity
            || current.observed_at.get() > now.get()
            || now.get() - current.observed_at.get() > 5000
            || now.get() >= c.source.session_expires_at.0
            || now.get() >= c.source.session_idle_expires_at.0
            || current.provision.policy.version != self.draft.entry.bootstrap_config_version.0
            || !current.provision.policy.managers.iter().any(|m| {
                m.principal == self.draft.entry.manager
                    && m.environments.contains(&self.draft.entry.environment)
            })
        {
            return Err(invalid());
        }
        let checks = super::source::validate_inspection_record(r, source, signed, c, now.get())?;
        let status = &current.lifetime.lifetime.status;
        let nonce = self.test_session_v2_lifetime_nonce(id, current.source_operation_id)?;
        let original = s.native_lifetime.as_ref().ok_or_else(invalid)?;
        current
            .lifetime
            .lifetime
            .validate_for(
                &original.lifetime.status.receipt,
                &nonce,
                s.session.native_binding_generation,
                c.authority.service_epoch,
                now.get(),
            )
            .map_err(|_| invalid())?;
        current
            .lifetime
            .lifetime
            .covers(s.roots.get(&purpose).ok_or_else(invalid)?.deadline)
            .map_err(|_| invalid())?;
        if current.private_sid_generation != s.session.private_sid_generation
            || status.session_ref != s.session.session_ref
            || status.receipt.identity_namespace != current.provision.native_identity_namespace
        {
            return Err(invalid());
        }
        Ok(append_native_history_checks(
            checks,
            c,
            NativeFact::Lifetime(current.lifetime),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &current.lifetime.lifetime.digest().map_err(|_| invalid())?,
        )?)
    }
}
