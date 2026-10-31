use super::*;
pub(super) fn lifetime_attempt(s: &TestSessionAuthorityV2, source: &str) -> Result<String> {
    Ok(format!(
        "control-v2-lifetime-{}",
        digest(&(
            "oasis7.custody.control-lifetime-transport.v2",
            s.input_digest()?,
            &s.input.operation_id,
            source
        ))?
    ))
}
pub(in crate::provisioning) fn control_lifetime_claimed_v2(
    s: &TestSessionAuthorityV2,
    source: &str,
) -> Result<bool> {
    match s.control_transports.get(&lifetime_attempt(s, source)?) {
        None => Ok(false),
        Some(SessionControlHopV1::Identity) => Ok(true),
        _ => Err(invalid()),
    }
}
impl AccountProvisionApprovalState {
    pub fn test_session_v2_control_purpose(
        &self,
        id: &str,
        proof: &HttpPossessionReceipt,
        now: UnixMillis,
    ) -> Result<SessionPurposeV1> {
        let s = self.test_session_v2_record(id)?;
        match (
            super::super::authenticated_receipt_v2(s, SessionPurposeV1::Execution, proof, now)
                .is_ok(),
            super::super::authenticated_receipt_v2(s, SessionPurposeV1::ReconcileRead, proof, now)
                .is_ok(),
        ) {
            (true, false) => Ok(SessionPurposeV1::Execution),
            (false, true) => Ok(SessionPurposeV1::ReconcileRead),
            _ => Err(invalid()),
        }
    }
    pub fn test_session_v2_control_live_descriptor(
        &self,
        id: &str,
        source_id: &str,
        proof: SessionHttpAdmission<'_>,
        now: UnixMillis,
    ) -> Result<TestSessionCredentialLiveDescriptorV2> {
        http::paid(self, id, proof.possession, proof.ingress_attempt_id)?;
        let p = self.test_session_v2_control_purpose(id, proof.possession, now)?;
        self.test_session_v2_credential_live_descriptor(id, p, source_id)
    }
    pub fn claim_test_session_v2_control_lifetime(
        &mut self,
        id: &str,
        source_id: &str,
        proof: SessionHttpAdmission<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<String> {
        http::paid(self, id, proof.possession, proof.ingress_attempt_id)?;
        let purpose = self.test_session_v2_control_purpose(id, proof.possession, now)?;
        self.test_session_v2_local(id, purpose, now)?;
        let s = self.test_session_v2_record(id)?;
        if s.source_funding.get(source_id) != Some(&SessionSourceFundingV1::Control)
            || s.source_oracle_reads.get(source_id) != Some(&3)
            || s.control_transports.contains_key(
                &super::super::v2_credential_source::lifetime_attempt_v2(s, source_id)?,
            )
            || control_lifetime_claimed_v2(s, source_id)?
        {
            return Err(invalid());
        }
        let source = s.sources.get(source_id).ok_or_else(invalid)?;
        verifier.verify(&self.test_session_v2_source_checks(
            id,
            source_id,
            source.source.verification.as_ref().ok_or_else(invalid)?,
            current,
            now,
        )?)?;
        let attempt = lifetime_attempt(s, source_id)?;
        self.test_session_v2_transport_id_available(id, &attempt)?;
        let nonce = self.test_session_v2_lifetime_nonce(id, source_id)?;
        let mut next = self.clone();
        let s = next.test_sessions_v2.get_mut(id).ok_or_else(invalid)?;
        s.identity_transport
            .as_mut()
            .ok_or_else(invalid)?
            .charge(1)
            .map_err(|_| invalid())?;
        if s.control_transports
            .insert(attempt, SessionControlHopV1::Identity)
            .is_some()
        {
            return Err(invalid());
        }
        super::super::v2_credential_http::close_if_control_exhausted(s, now);
        append_session_event_v2(
            s,
            SessionEventKindV2::LifetimeClaimed {
                source_operation_id: source_id.into(),
                ingress_attempt_id: proof.ingress_attempt_id.into(),
                profile: SessionJournalProfileV2::Control,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(nonce)
    }
}
pub(super) fn validate_observation(
    state: &AccountProvisionApprovalState,
    s: &TestSessionAuthorityV2,
    o: &ControlObservationV2,
    purpose: SessionPurposeV1,
    at: UnixMillis,
) -> Result<()> {
    if o.approved_input_digest != s.input_digest()?
        || o.source_ticket != 4
        || o.checked_at != at
        || o.private_sid_generation != s.session.private_sid_generation
        || s.source_funding.get(&o.source_operation_id) != Some(&SessionSourceFundingV1::Control)
        || s.source_oracle_reads
            .get(&o.source_operation_id)
            .is_none_or(|v| *v < 4)
        || !control_lifetime_claimed_v2(s, &o.source_operation_id)?
    {
        return Err(invalid());
    }
    let w = s.sources.get(&o.source_operation_id).ok_or_else(invalid)?;
    let source = &w.source;
    let v = &source
        .verification
        .as_ref()
        .ok_or_else(invalid)?
        .verification;
    if w.approved_input_digest != s.input_digest()?
        || w.api_version != 2
        || source.verified_at.is_none_or(|v| v.0 > at.get())
        || v.observed_at.0 > at.get()
        || at.get() - v.observed_at.0 > 5000
        || at.get() >= source.expires_at.0
        || v.verify_nonce != source.verify_nonce
        || v.proof_digest != s.input.approval.digest().map_err(|_| invalid())?
        || v.purpose != crate::approval::VerificationPurposeV1::InspectOrigin
        || v.origin_state != crate::approval::OriginStateV1::Active
        || at.get() >= v.source_session_expires_at.0
        || at.get() >= v.source_session_idle_expires_at.0
    {
        return Err(invalid());
    }
    o.lifetime.validate().map_err(|_| invalid())?;
    o.lifetime
        .lifetime
        .validate_for(
            &s.native_lifetime
                .as_ref()
                .ok_or_else(invalid)?
                .lifetime
                .status
                .receipt,
            &state.test_session_v2_lifetime_nonce(&s.input.operation_id, &o.source_operation_id)?,
            s.session.native_binding_generation,
            s.input.request.context.service_epoch,
            at.get(),
        )
        .map_err(|_| invalid())?;
    if o.lifetime.lifetime.status.session_ref != s.session.session_ref {
        return Err(invalid());
    }
    o.lifetime
        .lifetime
        .covers(s.roots.get(&purpose).ok_or_else(invalid)?.deadline)
        .map_err(|_| invalid())?;
    Ok(())
}
