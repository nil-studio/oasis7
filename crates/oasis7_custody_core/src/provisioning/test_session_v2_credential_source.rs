//! Control-funded V2 private-session metadata and a single paid lifetime read.
use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionCredentialLiveDescriptorV2 {
    pub api_version: u8,
    pub approved_input_digest: String,
    pub session_id: String,
    pub purpose: SessionPurposeV1,
    pub deadline: UnixMillis,
    pub authority: crate::approval::ApprovalContextV1,
    pub session: ParticipantSessionRefV1,
    pub native_binding: crate::native_binding::NativeBindingReceiptV1,
    pub source_operation_id: String,
    pub source_funding: SessionSourceFundingV1,
    pub lifetime_nonce: String,
    pub credential_generation: Option<DecimalU64>,
}
impl core::fmt::Debug for TestSessionCredentialLiveDescriptorV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionCredentialLiveDescriptorV2(<private>)")
    }
}
pub(in crate::provisioning) fn lifetime_attempt_v2(
    s: &TestSessionAuthorityV2,
    source: &str,
) -> Result<String> {
    Ok(format!(
        "client-v2-lifetime-{}",
        digest(&(
            "oasis7.custody.client-lifetime-transport.v2",
            s.input_digest()?,
            &s.input.operation_id,
            source,
        ))?
    ))
}
impl AccountProvisionApprovalState {
    /// Immutable approved-input routing commitment, never a dispatch grant.
    pub fn test_session_v2_credential_input_digest(&self, id: &str) -> Result<String> {
        self.test_session_v2_record(id)?.input_digest()
    }
    /// Persisted ticket metadata only; this does not reserve or grant a port.
    pub fn test_session_v2_credential_source_ticket(
        &self,
        id: &str,
        source_id: &str,
    ) -> Result<u8> {
        let s = self.test_session_v2_record(id)?;
        let source = s.sources.get(source_id).ok_or_else(invalid)?;
        if source.api_version != 2 || source.approved_input_digest != s.input_digest()? {
            return Err(invalid());
        }
        s.source_oracle_reads
            .get(source_id)
            .copied()
            .ok_or_else(invalid)
    }

    /// Metadata only. Store/Identity independently buy and verify each Source
    /// observation, Directory/RF and current private generation before/after
    /// the fixed four-actor private read.
    pub fn test_session_v2_credential_live_descriptor(
        &self,
        id: &str,
        purpose: SessionPurposeV1,
        source_id: &str,
    ) -> Result<TestSessionCredentialLiveDescriptorV2> {
        let s = self.test_session_v2_record(id)?;
        super::v2_credential_http::native_registration(s)?;
        if purpose == SessionPurposeV1::TerminationPlan
            || s.source_funding.get(source_id) != Some(&SessionSourceFundingV1::Control)
        {
            return Err(invalid());
        }
        let source = s.sources.get(source_id).ok_or_else(invalid)?;
        let root = s.roots.get(&purpose).ok_or_else(invalid)?;
        if source.api_version != 2
            || source.approved_input_digest != s.input_digest()?
            || source.source.verification.is_none()
            || root.approved_input_digest != s.input_digest()?
        {
            return Err(invalid());
        }
        Ok(TestSessionCredentialLiveDescriptorV2 {
            api_version: 2,
            approved_input_digest: s.input_digest()?,
            session_id: id.into(),
            purpose,
            deadline: root.deadline,
            authority: s.input.request.context.clone(),
            session: s.session.clone(),
            native_binding: s
                .native_lifetime
                .as_ref()
                .ok_or_else(invalid)?
                .lifetime
                .status
                .receipt
                .clone(),
            source_operation_id: source_id.into(),
            source_funding: SessionSourceFundingV1::Control,
            lifetime_nonce: self.test_session_v2_lifetime_nonce(id, source_id)?,
            credential_generation: root.credential.as_ref().map(|c| c.generation),
        })
    }
    /// Exactly the actual Native SessionLifetime HTTP, not a generic hop
    /// allocator. Commit the original Control budget before dispatch. A second
    /// claim of this same dispatch is unknown, never another permit.
    pub fn claim_test_session_v2_credential_lifetime(
        &mut self,
        id: &str,
        source_id: &str,
        proof: SessionHttpAdmission<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<String> {
        let s = self.test_session_v2_record(id)?;
        super::v2_credential_http::public_receipt(s, proof.possession, now)?;
        self.require_test_session_v2_claim_http_paid(
            id,
            proof.possession,
            proof.ingress_attempt_id,
        )?;
        if s.source_funding.get(source_id) != Some(&SessionSourceFundingV1::Control)
            || s.source_oracle_reads.get(source_id) != Some(&3)
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
        if super::control_lifetime_claimed_v2(s, source_id)? {
            return Err(invalid());
        }
        let attempt = lifetime_attempt_v2(s, source_id)?;
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
        super::v2_credential_http::close_if_control_exhausted(s, now);
        next.validate_persistence()?;
        *self = next;
        Ok(nonce)
    }
}
