use super::*;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPreflightPermitV1 {
    pub descriptor: TestSessionPreflightDescriptorV1,
    pub role: Role,
    pub purpose: OwnerSetupReadPurposeV1,
    pub ticket: u8,
    pub authority_revision: String,
    pub service_epoch: DecimalU64,
    pub target_security_epoch: DecimalU64,
    pub observed_at: UnixMillis,
    pub expires_at: UnixMillis,
}
impl core::fmt::Debug for TestSessionPreflightPermitV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionPreflightPermitV1(<private-read-only>)")
    }
}
impl AccountProvisionApprovalState {
    pub fn test_session_preflight_permit(
        &self,
        id: &str,
        role: Role,
        purpose: OwnerSetupReadPurposeV1,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TestSessionPreflightPermitV1> {
        let ticket = current.oracle_ticket;
        if !match ticket {
            1 | 2 => role == Role::Identity && purpose == OwnerSetupReadPurposeV1::Session,
            3..=5 => role == Role::Owner && purpose == OwnerSetupReadPurposeV1::Sign,
            _ => false,
        } {
            return Err(invalid());
        }
        verifier.verify(&self.test_session_preflight_checks(id, current, now)?)?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let source = s
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
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
            .min(admission::pairing(&s.input.request.intent)?.2);
        if now.get() >= expires {
            return Err(invalid());
        }
        Ok(TestSessionPreflightPermitV1 {
            descriptor: self.test_session_preflight_descriptor(id)?,
            role,
            purpose,
            ticket,
            service_epoch: current.provision.approval.authority.service_epoch,
            target_security_epoch: current.provision.approval.authority.security_version,
            authority_revision: digest(&(
                "oasis7.custody.test-session-preflight-authority.v1",
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
pub struct TestSessionPreflightSigningBundleV1 {
    pub owner: ws::SignedReadRequest<oasis7_agent_api::game::GameParticipantRegisterV1>,
    pub admission: ws::SignedReadRequest<oasis7_agent_api::game::GameParticipantAdmissionV1>,
    pub read: ws::ReadWorldViewRequest,
}
impl AccountProvisionApprovalState {
    pub fn test_session_preflight_signing_bundle(
        &self,
        id: &str,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TestSessionPreflightSigningBundleV1> {
        let read = self.test_session_preflight_signing_request(id, current, now, verifier)?;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let admission = self
            .test_sessions
            .get(id)
            .and_then(|s| s.preflight_access.as_ref())
            .ok_or_else(invalid)?
            .admission
            .clone();
        Ok(TestSessionPreflightSigningBundleV1 {
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
