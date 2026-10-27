use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_session_preflight_persistence(
        &self,
        id: &str,
        s: &TestSessionAuthorityV1,
    ) -> Result<()> {
        let (_, _, write, read, cleanup) = admission::pairing(&s.input.request.intent)?;
        if s.preflight_source_nonce.is_some() != s.consumed_at.is_some()
            || s.preflight_source_nonce.as_ref().is_some_and(|n| !hex64(n))
            || s.preflight_oracle_reads > 5
        {
            return Err(invalid());
        }
        let logical_id = digest(&("oasis7.custody.test-session-preflight-control-id.v1", id))?;
        let logical_body = digest(&(
            "oasis7.custody.test-session-preflight-control-body.v1",
            &s.input,
        ))?;
        if s.control_logicals.get(&logical_id) != Some(&logical_body) {
            return Err(invalid());
        }
        let Some(claimed) = s.preflight_claimed_at else {
            return if s.preflight_steps.is_empty()
                && s.preflight_oracle_reads == 0
                && s.preflight_access.is_none()
                && s.preflight_read.is_none()
                && s.preflight_lifetime.is_none()
                && s.approved_at.is_none()
            {
                Ok(())
            } else {
                Err(invalid())
            };
        };
        if s.consumed_at.is_none_or(|t| claimed.get() < t.get()) || claimed.get() >= write {
            return Err(invalid());
        }
        let d = self.test_session_preflight_descriptor(id)?;
        let source = s.sources.get(&d.source_operation_id).ok_or_else(invalid)?;
        if source.verify_nonce != d.source_nonce
            || source.created_at.0 != claimed.get()
            || source.expires_at.0 > write
            || s.source_funding.get(&d.source_operation_id)
                != Some(&SessionSourceFundingV1::Control)
        {
            return Err(invalid());
        }
        for (step, t) in &s.preflight_steps {
            if t.get() < claimed.get()
                || t.get() >= write
                || matches!(
                    step,
                    SessionPreflightStepV1::OwnerRead | SessionPreflightStepV1::Lifetime
                ) && s.preflight_read.is_none()
                || *step == SessionPreflightStepV1::OwnerRead && s.preflight_lifetime.is_none()
            {
                return Err(invalid());
            }
        }
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        if let Some(e) = &s.preflight_access {
            if s.preflight_steps
                .get(&SessionPreflightStepV1::FreshAccessPair)
                .is_none()
                || s.preflight_oracle_reads < 3
            {
                return Err(invalid());
            }
            policy_for_descriptor(&participant.descriptor, &d.access_nonces.policy, &e.policy)?;
            admission_for_descriptor(&participant.descriptor, &e.policy, &e.admission)?;
            for (nonce, status) in [
                (&d.access_nonces.policy_session, &e.policy_session),
                (&d.access_nonces.authorize_session, &e.authorize_session),
            ] {
                let r = &status.status.status;
                if &r.nonce != nonce
                    || r.receipt != participant.descriptor.native_binding
                    || r.session_ref != s.session.session_ref
                    || status.private_sid_generation != s.session.private_sid_generation
                    || r.generation != s.session.native_binding_generation
                    || r.service_epoch != s.input.request.context.service_epoch
                    || !r.active
                    || r.frozen
                    || r.pending
                    || r.observed_at.0
                        < s.preflight_steps
                            .get(&SessionPreflightStepV1::FreshAccessPair)
                            .ok_or_else(invalid)?
                            .get()
                {
                    return Err(invalid());
                }
            }
        }
        if let Some(original) = &s.preflight_read {
            let access = s.preflight_access.as_ref().ok_or_else(invalid)?;
            if s.preflight_oracle_reads != 5
                || original.read.request != d.owner_read
                || original.read.subject_public_key != s.termination.initial_fence.owner_public_key
                || original.owner != participant.bundle.as_ref().ok_or_else(invalid)?.owner
                || original.admission != access.admission
                || original.response_profile
                    != Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1)
            {
                return Err(invalid());
            }
            validate_owner_setup_signature_fact(OwnerSetupSignatureFact::Read(
                original,
                &s.termination.initial_fence.owner_public_key,
            ))?;
        }
        if let Some(fact) = &s.preflight_lifetime {
            let time = s
                .preflight_steps
                .get(&SessionPreflightStepV1::Lifetime)
                .ok_or_else(invalid)?;
            fact.validate().map_err(|_| invalid())?;
            let status = &fact.lifetime.status;
            if status.receipt != participant.descriptor.native_binding
                || status.nonce != d.lifetime_nonce
                || status.generation != s.session.native_binding_generation
                || status.service_epoch != s.input.request.context.service_epoch
                || status.session_ref != s.session.session_ref
                || status.observed_at.0 < time.get()
                || !status.active
                || status.frozen
                || status.pending
            {
                return Err(invalid());
            }
            fact.lifetime
                .covers(UnixMillis::new(write.max(read).max(cleanup)))
                .map_err(|_| invalid())?;
        }
        if s.approved_at.is_some()
            && (s.native_lifetime != s.preflight_lifetime
                || s.preflight_steps
                    .get(&SessionPreflightStepV1::OwnerRead)
                    .is_none())
        {
            return Err(invalid());
        }
        Ok(())
    }
}
