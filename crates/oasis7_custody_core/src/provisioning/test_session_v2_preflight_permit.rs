use super::*;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPreflightPermitV2 {
    pub descriptor: TestSessionPreflightDescriptorV2,
    pub role: Role,
    pub purpose: OwnerSetupReadPurposeV1,
    pub ticket: u8,
    pub authority_revision: String,
    pub service_epoch: DecimalU64,
    pub target_security_epoch: DecimalU64,
    pub observed_at: UnixMillis,
    pub expires_at: UnixMillis,
}
impl core::fmt::Debug for TestSessionPreflightPermitV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionPreflightPermitV2(<private-read-only>)")
    }
}
impl AccountProvisionApprovalState {
    pub fn test_session_v2_preflight_permit(
        &self,
        id: &str,
        role: Role,
        purpose: OwnerSetupReadPurposeV1,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TestSessionPreflightPermitV2> {
        let ticket = current.oracle_ticket;
        if !match ticket {
            1 | 2 => role == Role::Identity && purpose == OwnerSetupReadPurposeV1::Session,
            3..=5 => role == Role::Owner && purpose == OwnerSetupReadPurposeV1::Sign,
            _ => false,
        } {
            return Err(invalid());
        }
        verifier.verify(&self.test_session_v2_preflight_checks(id, current, now)?)?;
        let s = self.test_sessions_v2.get(id).ok_or_else(invalid)?;
        let wrapper = s
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
        if wrapper.api_version != 2 || wrapper.approved_input_digest != s.input_digest()? {
            return Err(invalid());
        }
        let source = &wrapper.source;
        let expires = source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification
            .observed_at
            .0
            .checked_add(5000)
            .ok_or_else(invalid)?
            .min(source.expires_at.0)
            .min(
                current
                    .observed_at
                    .get()
                    .checked_add(5000)
                    .ok_or_else(invalid)?,
            )
            .min(admission_v2::pairing_v2(&s.input.request.intent)?.2);
        if now.get() >= expires {
            return Err(invalid());
        }
        Ok(TestSessionPreflightPermitV2 {
            descriptor: self.test_session_v2_preflight_descriptor(id)?,
            role,
            purpose,
            ticket,
            service_epoch: current.provision.approval.authority.service_epoch,
            target_security_epoch: current.provision.approval.authority.security_version,
            authority_revision: digest(&(
                "oasis7.custody.test-session-preflight-authority.v2",
                &s.input,
                &s.session,
                &current.provision.approval.authority,
                &current.provision.approval.source,
                current.source_operation_id,
                &source.verify_nonce,
            ))?,
            observed_at: current.observed_at,
            expires_at: UnixMillis::new(expires),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPreflightSigningBundleV2 {
    pub owner: ws::SignedReadRequest<oasis7_agent_api::game::GameParticipantRegisterV1>,
    pub admission: ws::SignedReadRequest<oasis7_agent_api::game::GameParticipantAdmissionV1>,
    pub read: ws::ReadWorldViewRequest,
}
impl AccountProvisionApprovalState {
    pub fn test_session_v2_preflight_signing_bundle(
        &self,
        id: &str,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TestSessionPreflightSigningBundleV2> {
        let read = self.test_session_v2_preflight_signing_request(id, current, now, verifier)?;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let admission = self
            .test_sessions_v2
            .get(id)
            .and_then(|s| s.preflight_access.as_ref())
            .ok_or_else(invalid)?
            .admission
            .clone();
        Ok(TestSessionPreflightSigningBundleV2 {
            owner: participant
                .bundle
                .as_ref()
                .ok_or_else(invalid)?
                .owner
                .clone(),
            admission,
            read,
        })
    }
}
