//! Actual V2 session authority. No V1 plan, Prepared fact or termination is copied.
use super::*;
fn required_nullable<'de, D, T>(d: D) -> core::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionAuthorityV2 {
    pub(in crate::provisioning) input: ApprovalAdmissionInput,
    pub(in crate::provisioning) book: ApprovalAdmissionBook,
    pub(in crate::provisioning) enrollment: ProvedEnrollmentFactV1,
    pub(in crate::provisioning) session: ParticipantSessionRefV1,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) native_lifetime: Option<SignedManagedNativeSessionLifetimeV1>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) approved_at: Option<UnixMillis>,
    pub(in crate::provisioning) budget_authorized_at: UnixMillis,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) approval_verification_claimed_at: Option<UnixMillis>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) consumed_at: Option<UnixMillis>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) preflight_source_nonce: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) preflight_claimed_at: Option<UnixMillis>,
    pub(in crate::provisioning) preflight_steps: BTreeMap<SessionPreflightStepV1, UnixMillis>,
    pub(in crate::provisioning) preflight_oracle_reads: u8,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) preflight_access: Option<OwnerSetupAccessEvidenceV1>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) preflight_lifetime: Option<SignedManagedNativeSessionLifetimeV1>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) preflight_read:
        Option<Box<ws::participant_discovery::ParticipantOwnerViewRequestV1>>,
    pub(in crate::provisioning) sources: BTreeMap<String, TestSessionSourceV2>,
    pub(in crate::provisioning) source_oracle_reads: BTreeMap<String, u8>,
    pub(in crate::provisioning) source_funding: BTreeMap<String, SessionSourceFundingV1>,
    pub(in crate::provisioning) purpose_transports: BTreeMap<String, SessionPurposeTransportV1>,
    pub(in crate::provisioning) purpose_http_admissions:
        BTreeMap<String, SessionPurposeHttpAdmissionV1>,
    pub(in crate::provisioning) tool_nonce_ingresses: BTreeMap<String, SessionToolNonceIngressV1>,
    pub(in crate::provisioning) tool_nonce_links: BTreeMap<String, SessionToolNonceLinkV1>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) context: Option<SessionContextV1>,
    pub(in crate::provisioning) roots: BTreeMap<SessionPurposeV1, SessionRootV2>,
    pub(in crate::provisioning) credential_claims: BTreeMap<String, SessionCredentialClaimRecordV2>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) control: Option<OperationBudgets>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) identity_transport: Option<BudgetLedger>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) connector_transport: Option<BudgetLedger>,
    pub(in crate::provisioning) control_transports: BTreeMap<String, SessionControlHopV1>,
    pub(in crate::provisioning) client_http_admissions:
        BTreeMap<String, SessionClientHttpAdmissionV1>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) lease_until: Option<UnixMillis>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) last_heartbeat: Option<UnixMillis>,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) closing: Option<(TestSessionClosingReasonV1, UnixMillis)>,
    pub(in crate::provisioning) controls: BTreeMap<String, SessionControlAckV1>,
    pub(in crate::provisioning) control_records: BTreeMap<String, SessionControlRecordV2>,
    pub(in crate::provisioning) control_logicals: BTreeMap<String, String>,
    pub(in crate::provisioning) challenges: BTreeMap<String, SessionChallengeV1>,
    pub(in crate::provisioning) actions: BTreeMap<String, SessionActionOriginalV1>,
    pub(in crate::provisioning) termination_state: TestSessionTerminationStateV2,
    #[serde(deserialize_with = "required_nullable")]
    pub(in crate::provisioning) execution_book: Option<SessionExecutionBookV2>,
}
impl core::fmt::Debug for TestSessionAuthorityV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionAuthorityV2(<private>)")
    }
}
impl TestSessionAuthorityV2 {
    pub(in crate::provisioning) fn header(
        &self,
    ) -> Result<(&TestSessionPlanV2, &ResourceTestSessionScopeV2)> {
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &self.input.request.intent else {
            return Err(invalid());
        };
        Ok((plan, scope))
    }
    pub(in crate::provisioning) fn validate_header(&self) -> Result<()> {
        let (plan, scope) = self.header()?;
        plan.validate().map_err(|_| invalid())?;
        plan.termination
            .validate_scope(scope)
            .map_err(|_| invalid())?;
        if self.input.operation_id != scope.session_id
            || self.book.records().count() != 1
            || self
                .book
                .original(&scope.session_id)
                .is_none_or(|r| r.input() != &self.input)
        {
            return Err(invalid());
        }
        if self.context.is_some() || self.execution_book.is_some() {
            return Err(invalid());
        }
        self.termination_state.validate_input_binding(&self.input)?;
        self.termination_state
            .validate_source_binding(&self.sources, &self.source_funding)?;
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn test_session_v2_record(
        &self,
        id: &str,
    ) -> Result<&TestSessionAuthorityV2> {
        if self.test_sessions.contains_key(id) {
            return Err(ProvisionApprovalError::Conflict);
        }
        let s = self.test_sessions_v2.get(id).ok_or_else(invalid)?;
        s.validate_header()?;
        Ok(s)
    }
    pub(in crate::provisioning) fn test_session_v2_record_mut(
        &mut self,
        id: &str,
    ) -> Result<&mut TestSessionAuthorityV2> {
        self.test_session_v2_record(id)?;
        self.test_sessions_v2.get_mut(id).ok_or_else(invalid)
    }
    pub fn test_session_v2_header(
        &self,
        id: &str,
    ) -> Result<(&TestSessionPlanV2, &ResourceTestSessionScopeV2)> {
        self.test_session_v2_record(id)?.header()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitTestSessionV2 {
    pub session_id: String,
    pub signed: SignedApprovalVerificationV1,
    pub source_nonce: String,
}
impl core::fmt::Debug for CommitTestSessionV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CommitTestSessionV2(<private>)")
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionSourceV2 {
    pub api_version: u8,
    pub approved_input_digest: String,
    pub source: ProvisionOriginInspectionV1,
}
impl core::fmt::Debug for TestSessionSourceV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionSourceV2(<private>)")
    }
}
impl TestSessionAuthorityV2 {
    pub(in crate::provisioning) fn input_digest(&self) -> Result<String> {
        digest(&("oasis7.custody.test-session-approved-input.v2", &self.input))
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRootV2 {
    pub(in crate::provisioning) approved_input_digest: String,
    pub(in crate::provisioning) root: SessionRootV1,
}
impl core::fmt::Debug for SessionRootV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SessionRootV2(<private>)")
    }
}
impl core::ops::Deref for SessionRootV2 {
    type Target = SessionRootV1;
    fn deref(&self) -> &Self::Target {
        &self.root
    }
}
impl core::ops::DerefMut for SessionRootV2 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.root
    }
}

impl AccountProvisionApprovalState {
    pub fn test_session_v2_original(&self, id: &str) -> Option<&ApprovalAdmissionRecord> {
        self.test_sessions_v2.get(id)?.book.original(id)
    }
    pub fn test_session_v2_snapshot(&self, id: &str) -> Result<&TestSessionAuthorityV2> {
        self.test_session_v2_record(id)
    }
}
impl TestSessionAuthorityV2 {
    pub fn input(&self) -> &ApprovalAdmissionInput {
        &self.input
    }
    pub fn approved_at(&self) -> Option<UnixMillis> {
        self.approved_at
    }
    pub fn session(&self) -> &ParticipantSessionRefV1 {
        &self.session
    }
    pub fn preflight_lifetime(&self) -> Option<&SignedManagedNativeSessionLifetimeV1> {
        self.preflight_lifetime.as_ref()
    }
}

impl TestSessionAuthorityV2 {
    pub fn roots(&self) -> &BTreeMap<SessionPurposeV1, SessionRootV2> {
        &self.roots
    }
    pub fn consumed_at(&self) -> Option<UnixMillis> {
        self.consumed_at
    }
}

impl AccountProvisionApprovalState {
    /// Local tightening from the trusted Account clock; no transport or deadline refresh.
    pub fn close_expired_test_session_v2(&mut self, id: &str, now: UnixMillis) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        if s.approved_at.is_none() || now.get() < s.header()?.0.write_deadline.0 {
            return Err(invalid());
        }
        if s.closing.is_some() {
            return Ok(());
        }
        let mut next = self.clone();
        next.test_session_v2_record_mut(id)?.closing =
            Some((TestSessionClosingReasonV1::Deadline, now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}

impl AccountProvisionApprovalState {
    pub fn test_session_v2_source(
        &self,
        id: &str,
        operation: &str,
    ) -> Option<&TestSessionSourceV2> {
        self.test_sessions_v2.get(id)?.sources.get(operation)
    }
}

impl AccountProvisionApprovalState {
    /// Historical signatures are checked against externally configured Native
    /// pins. This does not admit a current operation or refresh any deadline.
    pub fn test_session_v2_native_restore_checks(
        &self,
        id: &str,
        native: &NativeStageScope<'_>,
    ) -> Result<Vec<ExactSignatureChecks>> {
        let session = self.test_session_v2_record(id)?;
        session.validate_header()?;
        let mut checks = Vec::new();
        for lifetime in session
            .native_lifetime
            .iter()
            .chain(session.preflight_lifetime.iter())
            .chain(session.termination_state.stored_native_lifetimes())
            .chain(
                session
                    .credential_claims
                    .values()
                    .map(|claim| &claim.lifetime),
            )
            .chain(super::v2_control::stored_lifetimes_v2(session))
        {
            let digest = super::super::types::digest(&(
                "oasis7.custody.test-session-native-history.v2",
                lifetime,
            ))?;
            checks.push(registered_native_fact_checks(
                id,
                &digest,
                session
                    .book
                    .original(id)
                    .ok_or_else(invalid)?
                    .recorded_context(),
                NativeFact::Lifetime(lifetime),
                native.native_root_key_id,
                native.native_root_public_key,
                &digest,
            )?);
        }
        Ok(checks)
    }
}

impl TestSessionAuthorityV2 {
    pub fn preflight_access(&self) -> Option<&OwnerSetupAccessEvidenceV1> {
        self.preflight_access.as_ref()
    }
    pub fn preflight_read(
        &self,
    ) -> Option<
        &oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewRequestV1,
    > {
        self.preflight_read.as_deref()
    }
}

impl TestSessionAuthorityV2 {
    pub fn preflight_claimed_at(&self) -> Option<UnixMillis> {
        self.preflight_claimed_at
    }
    pub fn closing_metadata(&self) -> Option<(TestSessionClosingReasonV1, UnixMillis)> {
        self.closing
    }
}
