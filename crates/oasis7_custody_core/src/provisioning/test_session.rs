//! Account-local sibling authorities. No serialized Current or proof flag grants rights.
use super::*;
use crate::{
    budget::{BudgetLedger, OperationBudgets},
    client_enrollment::*,
    http_pop::{HttpPopProfileV1, HttpPossessionReceipt},
    native_binding::SignedManagedNativeSessionLifetimeV1,
    time_units::UnixMillis,
};
use oasis7_agent_api::game::{GameExecutorControlV1, GameExecutorOperationV1, GameTaskFenceV1};
use oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPurposeV1 {
    Execution,
    ReconcileRead,
    TerminationPlan,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSourceFundingV1 {
    Control,
    Execution,
    ReconcileRead,
    TerminationPlan,
}
impl SessionSourceFundingV1 {
    pub fn purpose(self) -> Option<SessionPurposeV1> {
        match self {
            Self::Control => None,
            Self::Execution => Some(SessionPurposeV1::Execution),
            Self::ReconcileRead => Some(SessionPurposeV1::ReconcileRead),
            Self::TerminationPlan => Some(SessionPurposeV1::TerminationPlan),
        }
    }
}
pub struct SessionSourceReservation<'a> {
    pub operation_id: &'a str,
    pub nonce: &'a str,
    pub funding: SessionSourceFundingV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SessionInventoryV1 {
    #[default]
    UnknownLegacy,
    Known,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestSessionClosingReasonV1 {
    Finished,
    Aborted,
    Deadline,
    LeaseExpired,
    BudgetExhausted,
    Revoked,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCredentialV1 {
    pub digest: String,
    pub generation: DecimalU64,
    pub replacements: u8,
    pub claimed_at: UnixMillis,
    pub claim_request_id: String,
    pub claim_request: ClientCredentialClaimRequestV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRootV1 {
    pub purpose: SessionPurposeV1,
    pub deadline: UnixMillis,
    pub budgets: OperationBudgets,
    pub credential: Option<SessionCredentialV1>,
    pub credential_history: Vec<SessionCredentialV1>,
    pub revoked: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "control",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SessionChallengeKindV1 {
    Tool,
    Control(SessionControlKindV1),
    CredentialClaim,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionChallengeV1 {
    pub request_id: String,
    pub nonce: String,
    pub purpose: SessionPurposeV1,
    pub kind: SessionChallengeKindV1,
    pub logical_request_id: String,
    pub credential_generation: DecimalU64,
    pub body_digest: String,
    pub created_at: UnixMillis,
    pub expires_at: UnixMillis,
    pub consumed_by: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionActionOriginalV1 {
    pub request_id: String,
    pub request: SessionToolRequestV1,
    pub purpose: SessionPurposeV1,
    pub body_digest: String,
    pub business: DecimalU64,
    pub admitted_at: UnixMillis,
    pub transport_attempts: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionControlAckV1 {
    pub request_id: String,
    pub kind: SessionControlKindV1,
    pub acknowledged_at: UnixMillis,
    pub lease_until: UnixMillis,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionControlKindV1 {
    Heartbeat,
    Initialize,
    ToolsList,
    Finish,
    Abort,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionControlHopV1 {
    Host,
    Connector,
    Identity,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionAuthorityV1 {
    pub input: ApprovalAdmissionInput,
    pub book: ApprovalAdmissionBook,
    pub termination: TerminationPlanV1,
    pub enrollment: ProvedEnrollmentFactV1,
    pub session: ParticipantSessionRefV1,
    pub native_lifetime: Option<SignedManagedNativeSessionLifetimeV1>,
    pub approved_at: Option<UnixMillis>,
    pub budget_authorized_at: UnixMillis,
    pub approval_verification_claimed_at: Option<UnixMillis>,
    pub consumed_at: Option<UnixMillis>,
    pub preflight_source_nonce: Option<String>,
    pub preflight_claimed_at: Option<UnixMillis>,
    pub preflight_steps: BTreeMap<SessionPreflightStepV1, UnixMillis>,
    pub preflight_oracle_reads: u8,
    pub preflight_access: Option<OwnerSetupAccessEvidenceV1>,
    pub preflight_lifetime: Option<SignedManagedNativeSessionLifetimeV1>,
    pub preflight_read: Option<Box<ws::participant_discovery::ParticipantOwnerViewRequestV1>>,
    pub sources: BTreeMap<String, ProvisionOriginInspectionV1>,
    pub source_oracle_reads: BTreeMap<String, u8>,
    pub source_funding: BTreeMap<String, SessionSourceFundingV1>,
    pub purpose_transports: BTreeMap<String, SessionPurposeV1>,
    pub roots: BTreeMap<SessionPurposeV1, SessionRootV1>,
    pub control: Option<OperationBudgets>,
    pub identity_transport: Option<BudgetLedger>,
    pub connector_transport: Option<BudgetLedger>,
    pub control_transports: BTreeMap<String, SessionControlHopV1>,
    pub client_http_admissions: BTreeMap<String, SessionClientHttpAdmissionV1>,
    pub lease_until: Option<UnixMillis>,
    pub last_heartbeat: Option<UnixMillis>,
    pub closing: Option<(TestSessionClosingReasonV1, UnixMillis)>,
    pub controls: BTreeMap<String, SessionControlAckV1>,
    pub control_logicals: BTreeMap<String, String>,
    pub challenges: BTreeMap<String, SessionChallengeV1>,
    pub actions: BTreeMap<String, SessionActionOriginalV1>,
    pub termination_slots: BTreeMap<TerminationOperationV1, TerminationOriginalV1>,
}
/// Actual fixed namespace observation, current manager source and independently verified native SID facts.
/// This struct is never deserialized from an HTTP request.
pub struct TestSessionApprovalCurrent<'a, 'b> {
    pub provision: &'a ProvisionApprovalContext<'b>,
    pub native_scope: &'a NativeStageScope<'b>,
    pub enrollment: &'a ProvedEnrollmentObservation<'a>,
    pub deployments: &'a HostDeploymentAllowlist,
    pub observed_at: UnixMillis,
}
pub struct TestSessionCurrent<'a, 'b> {
    pub approval: TestSessionApprovalCurrent<'a, 'b>,
    pub lifetime: &'a SignedManagedNativeSessionLifetimeV1,
    pub private_sid_generation: DecimalU64,
    pub world: &'a OwnerSetupWorldEvidenceV1,
    pub source_operation_id: &'a str,
}
pub struct TestSessionLiveCurrent<'a, 'b> {
    pub provision: &'a ProvisionApprovalContext<'b>,
    pub native_scope: &'a NativeStageScope<'b>,
    pub lifetime: &'a SignedManagedNativeSessionLifetimeV1,
    pub private_sid_generation: DecimalU64,
    pub source_ticket: u8,
    pub source_operation_id: &'a str,
    pub observed_at: UnixMillis,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionAdmissionV1 {
    pub input: ApprovalAdmissionInput,
    pub termination: TerminationPlanV1,
    pub verify_nonce: String,
}
impl core::fmt::Debug for TestSessionAdmissionV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionAdmissionV1(<private>)")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitTestSessionV1 {
    pub session_id: String,
    pub signed: SignedApprovalVerificationV1,
    pub source_nonce: String,
}
impl core::fmt::Debug for CommitTestSessionV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CommitTestSessionV1(<private>)")
    }
}
#[path = "test_session_termination.rs"]
mod termination;
pub use termination::*;
#[path = "test_session_admission.rs"]
mod admission;
#[path = "test_session_approval.rs"]
mod approval;
#[path = "test_session_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::*;
#[path = "test_session_restore.rs"]
mod restore;
#[path = "test_session_roots.rs"]
mod roots;
pub use roots::*;

#[path = "test_session_preflight.rs"]
mod preflight;
pub use preflight::*;

impl AccountProvisionApprovalState {
    pub(super) fn require_session_source_funding(
        &self,
        id: &str,
        source: &str,
        funding: SessionSourceFundingV1,
    ) -> Result<()> {
        if self
            .test_sessions
            .get(id)
            .and_then(|s| s.source_funding.get(source))
            != Some(&funding)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl SessionPurposeV1 {
    pub fn source_funding(self) -> SessionSourceFundingV1 {
        match self {
            Self::Execution => SessionSourceFundingV1::Execution,
            Self::ReconcileRead => SessionSourceFundingV1::ReconcileRead,
            Self::TerminationPlan => SessionSourceFundingV1::TerminationPlan,
        }
    }
}

pub(super) fn session_control_exhausted(s: &TestSessionAuthorityV1) -> bool {
    s.control
        .as_ref()
        .is_some_and(|b| b.requests.remaining() == Ok(0) || b.transport.remaining() == Ok(0))
        || s.identity_transport
            .as_ref()
            .is_some_and(|b| b.remaining() == Ok(0))
        || s.connector_transport
            .as_ref()
            .is_some_and(|b| b.remaining() == Ok(0))
}

impl AccountProvisionApprovalState {
    pub(super) fn require_session_http_paid(
        &self,
        id: &str,
        proof: &HttpPossessionReceipt,
        ingress_attempt_id: &str,
        purpose: Option<SessionPurposeV1>,
    ) -> Result<()> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let key = proof.transport_attempt_digest();
        if let Some(purpose) = purpose {
            if s.purpose_transports.get(&key) != Some(&purpose) {
                return Err(invalid());
            }
        } else if s.control_transports.get(ingress_attempt_id) != Some(&SessionControlHopV1::Host)
            || !s
                .client_http_admissions
                .get(ingress_attempt_id)
                .is_some_and(|a| {
                    a.signed_attempt_digest == key && a.hop == SessionControlHopV1::Host
                })
        {
            return Err(invalid());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionLiveDescriptorV1 {
    pub api_version: u8,
    pub session_id: String,
    pub purpose: SessionPurposeV1,
    pub deadline: UnixMillis,
    pub authority: crate::approval::ApprovalContextV1,
    pub world: crate::approval::ApprovedWorldV1,
    pub task_fence: GameTaskFenceV1,
    pub owner_key_ref: String,
    pub executor_key_ref: String,
    pub session: ParticipantSessionRefV1,
    pub native_binding: crate::native_binding::NativeBindingReceiptV1,
    pub source_operation_id: String,
    pub source_funding: SessionSourceFundingV1,
    pub lifetime_nonce: String,
    pub credential_generation: Option<DecimalU64>,
}
impl ClientCredentialClaimRequestV1 {
    pub fn test_session_digest(&self) -> Result<String> {
        self.validate().map_err(|_| invalid())?;
        digest(&("oasis7.custody.test-session-claim.v1", self))
    }
}
impl AccountProvisionApprovalState {
    /// Fixed-selector metadata only. Never authorizes decrypt, signing, I/O or a client action.
    pub fn test_session_live_descriptor(
        &self,
        id: &str,
        purpose: SessionPurposeV1,
        source_id: &str,
    ) -> Result<TestSessionLiveDescriptorV1> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let root = s.roots.get(&purpose).ok_or_else(invalid)?;
        let source = s.sources.get(source_id).ok_or_else(invalid)?;
        let native_binding = s
            .native_lifetime
            .as_ref()
            .ok_or_else(invalid)?
            .lifetime
            .status
            .receipt
            .clone();
        Ok(TestSessionLiveDescriptorV1 {
            api_version: 1,
            session_id: id.into(),
            purpose,
            deadline: root.deadline,
            authority: s.input.request.context.clone(),
            world: s.termination.world.clone(),
            task_fence: s.termination.initial_fence.clone(),
            owner_key_ref: s.termination.owner_key_ref.clone(),
            executor_key_ref: s.termination.executor_key_ref.clone(),
            session: s.session.clone(),
            native_binding,
            source_operation_id: source_id.into(),
            source_funding: *s.source_funding.get(source_id).ok_or_else(invalid)?,
            lifetime_nonce: digest(&(
                "oasis7.custody.test-session-current-lifetime.v1",
                id,
                source_id,
                &source.verify_nonce,
            ))?,
            credential_generation: root.credential.as_ref().map(|c| c.generation),
        })
    }
}

#[path = "test_session_management.rs"]
mod management;
pub use management::*;

#[path = "test_session_progress.rs"]
mod progress;
pub use progress::*;

#[path = "test_session_client_http.rs"]
mod client_http;
pub use client_http::*;
