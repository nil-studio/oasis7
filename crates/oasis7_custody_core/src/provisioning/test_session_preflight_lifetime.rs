use super::*;

impl AccountProvisionApprovalState {
    pub fn test_session_preflight_lifetime_checks(
        &self,
        id: &str,
        lifetime: &SignedManagedNativeSessionLifetimeV1,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.test_session_preflight_checks(id, current, now)?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let claimed = s
            .preflight_steps
            .get(&SessionPreflightStepV1::Lifetime)
            .ok_or_else(invalid)?;
        let (_, baseline) = self.participant_session(current.native_scope)?;
        let d = self.test_session_preflight_descriptor(id)?;
        lifetime
            .lifetime
            .validate_for(
                &baseline.status.receipt,
                &d.lifetime_nonce,
                s.session.native_binding_generation,
                current.provision.approval.authority.service_epoch,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let (_, _, write, read, cleanup) = admission::pairing(&s.input.request.intent)?;
        lifetime
            .lifetime
            .covers(UnixMillis::new(write.max(read).max(cleanup)))
            .map_err(|_| invalid())?;
        if lifetime.lifetime.status.observed_at.0 < claimed.get()
            || lifetime.lifetime.status.session_ref != s.session.session_ref
            || s.preflight_lifetime
                .as_ref()
                .is_some_and(|old| old != lifetime)
        {
            return Err(invalid());
        }
        Ok(append_native_history_checks(
            checks,
            current.provision.approval,
            NativeFact::Lifetime(lifetime),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &lifetime.lifetime.digest().map_err(|_| invalid())?,
        )?)
    }
    pub fn record_test_session_preflight_lifetime(
        &mut self,
        id: &str,
        lifetime: SignedManagedNativeSessionLifetimeV1,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier
            .verify(&self.test_session_preflight_lifetime_checks(id, &lifetime, current, now)?)?;
        let mut next = self.clone();
        next.test_sessions
            .get_mut(id)
            .ok_or_else(invalid)?
            .preflight_lifetime = Some(lifetime);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
