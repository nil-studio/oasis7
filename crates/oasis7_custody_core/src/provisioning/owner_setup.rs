//! Separately approved, fixed owner bootstrap. Private goal bodies never enter Debug.
use super::*;
use crate::{budget::BudgetLedger, operations::Operation};
use oasis7_agent_api::game::GoalContentV1;
use oasis7_client_api::{
    world_service as ws,
    world_service_wire::{AuthenticatedLookup, WorldServicePayloadV1},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const OWNER_SETUP_STAGE_TRANSPORT_COST: u64 = 96;
pub const OWNER_SETUP_MAX_ORACLE_READS: u8 = 5;
pub const OWNER_SETUP_MIN_TRANSPORT: u64 = 584;
pub const OWNER_SETUP_MAX_TRANSPORT: u64 = 600;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupPlanV1 {
    pub api_version: u8,
    pub setup_id: String,
    pub provision_id: String,
    pub participant_receipt_digest: String,
    pub world_identity: crate::approval::ApprovedWorldV1,
    pub owner_key_ref: String,
    pub executor_key_ref: String,
    pub bootstrap_config_version: DecimalU64,
    pub goal_content: GoalContentV1,
    pub create_operation_id: String,
    pub prepare_operation_id: String,
    pub prepare_nonce: String,
    pub publish_operation_id: String,
    pub delegation_operation_id: String,
    pub bind_operation_id: String,
    pub executor_id: String,
    pub executor_public_key: String,
    pub delegation_generation: DecimalU64,
    pub delegation_nonce: DecimalU64,
    pub resource_refs: Vec<String>,
    pub deadline: DecimalU64,
    pub request_limit: DecimalU64,
    pub transport_limit: DecimalU64,
    pub write_limit: DecimalU64,
}
impl core::fmt::Debug for OwnerSetupPlanV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupPlanV1(<private>)")
    }
}
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
fn hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn digest<T: Serialize>(v: &T) -> Result<String> {
    super::super::types::digest(v).map_err(Into::into)
}
impl OwnerSetupPlanV1 {
    pub fn validate(&self) -> Result<()> {
        let ops = [
            &self.create_operation_id,
            &self.prepare_operation_id,
            &self.publish_operation_id,
            &self.delegation_operation_id,
            &self.bind_operation_id,
        ];
        let unique: BTreeSet<_> = ops.iter().copied().collect();
        let resources: BTreeSet<_> = self.resource_refs.iter().collect();
        if self.api_version != 1
            || unique.len() != 5
            || ops.iter().any(|v| !id(v))
            || [
                &self.setup_id,
                &self.provision_id,
                &self.owner_key_ref,
                &self.executor_key_ref,
                &self.executor_id,
            ]
            .iter()
            .any(|v| !id(v))
            || !hex64(&self.prepare_nonce)
            || !hex64(&self.participant_receipt_digest)
            || !hex64(&self.executor_public_key)
            || !self.world_identity.valid()
            || self.bootstrap_config_version.0 == 0
            || self.delegation_generation.0 != 1
            || self.delegation_nonce.0 != 1
            || self.deadline.0 == 0
            || self.request_limit.0 != 6
            || !(OWNER_SETUP_MIN_TRANSPORT..=OWNER_SETUP_MAX_TRANSPORT)
                .contains(&self.transport_limit.0)
            || self.write_limit.0 != 4
            || !self.resource_refs.is_empty()
            || resources.len() != self.resource_refs.len()
            || self.resource_refs.iter().any(|v| !id(v))
        {
            return Err(invalid());
        }
        self.goal_content.validate().map_err(|_| invalid())?;
        // The public game codec computes the complete private goal digest later,
        // once the canonical task id is fixed. Here bound serialization size.
        if serde_json::to_vec(&self.goal_content)
            .map_err(|_| invalid())?
            .len()
            > 32 * 1024
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        digest(&("oasis7.custody.owner-setup-plan.v1", self))
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerSetupStageV1 {
    CreateTask,
    PrepareGoal,
    PublishGoal,
    Delegation,
    Bind,
    FinalObservation,
}
impl OwnerSetupStageV1 {
    pub fn name(self) -> &'static str {
        match self {
            Self::CreateTask => "create_task",
            Self::PrepareGoal => "prepare_goal",
            Self::PublishGoal => "publish_goal",
            Self::Delegation => "delegation",
            Self::Bind => "bind",
            Self::FinalObservation => "final_observation",
        }
    }
    pub fn previous(self) -> Option<Self> {
        match self {
            Self::CreateTask => None,
            Self::PrepareGoal => Some(Self::CreateTask),
            Self::PublishGoal => Some(Self::PrepareGoal),
            Self::Delegation => Some(Self::PublishGoal),
            Self::Bind => Some(Self::Delegation),
            Self::FinalObservation => Some(Self::Bind),
        }
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "original",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum OwnerSetupOriginalV1 {
    Canonical {
        submit: Box<ws::SubmitIntentRequest<WorldServicePayloadV1>>,
        lookup: Box<AuthenticatedLookup>,
    },
    PrepareGoal(Box<ws::private_goal::SignedGoalPrepareV1>),
    FinalObservation {
        owner: Box<ws::participant_discovery::ParticipantOwnerViewRequestV1>,
        goal: Box<ws::SignedReadRequest<ws::private_goal_read::CurrentGoalRequest>>,
    },
}
impl Eq for OwnerSetupOriginalV1 {}
impl core::fmt::Debug for OwnerSetupOriginalV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupOriginalV1(<private>)")
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "result",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum OwnerSetupResultV1 {
    Canonical {
        response: ws::SignedServiceResponse<ws::IntentResponse<serde_json::Value>>,
        from_lookup: bool,
    },
    PreparedGoal(ws::SignedServiceResponse<ws::private_goal::PreparedGoalReceiptV1>),
    FinalObservation {
        owner: Box<ws::SignedServiceResponse<ws::participant_discovery::OwnerSetupObservationV1>>,
        goal: Box<ws::SignedServiceResponse<ws::private_goal_read::CurrentGoalResponse>>,
    },
}
impl Eq for OwnerSetupResultV1 {}
impl core::fmt::Debug for OwnerSetupResultV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupResultV1(<private>)")
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupOutboxV1 {
    pub stage: OwnerSetupStageV1,
    pub source: ProvisionOriginInspectionV1,
    pub operation: Operation,
    pub original: Option<OwnerSetupOriginalV1>,
    pub world: Option<OwnerSetupWorldEvidenceV1>,
    pub access_claimed_at: Option<DecimalU64>,
    pub access: Option<OwnerSetupAccessEvidenceV1>,
    pub access_recorded_at: Option<DecimalU64>,
    pub observation: Option<ws::participant_discovery::ParticipantOwnerViewRequestV1>,
    pub observation_bytes: Option<String>,
    pub candidate_bytes: Option<String>,
    pub admission_session: Option<ParticipantSessionEvidenceV1>,
    pub admitted_at: Option<DecimalU64>,
    pub admission_callback_claimed_at: Option<DecimalU64>,
    pub session: Option<ParticipantSessionEvidenceV1>,
    pub original_bytes: Option<String>,
    pub published_at: Option<DecimalU64>,
    pub claimed_at: Option<DecimalU64>,
    pub result: Option<OwnerSetupResultV1>,
    pub last_observation: Option<OwnerSetupResultV1>,
    pub last_observation_session: Option<ParticipantSessionEvidenceV1>,
    pub last_observed_at: Option<DecimalU64>,
    pub canonical_history: BTreeMap<OwnerSetupCanonicalSourceV1, OwnerSetupCanonicalFactV1>,
    pub reserved_at: DecimalU64,
    pub oracle_reads: u8,
    pub callback_claimed_at: Option<DecimalU64>,
    pub lookup_claimed_at: Option<DecimalU64>,
    pub lookup_reserved_at: Option<DecimalU64>,
    pub completed_at: Option<DecimalU64>,
    pub session_nonce: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupApprovalV1 {
    pub(super) plan: OwnerSetupPlanV1,
    pub(super) book: ApprovalAdmissionBook,
    pub(super) transport: Option<BudgetLedger>,
    pub(super) source: Option<ProvisionOriginInspectionV1>,
    pub(super) session: Option<ParticipantSessionRefV1>,
    pub(super) stages: BTreeMap<OwnerSetupStageV1, OwnerSetupOutboxV1>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupAdmissionV1 {
    pub input: ApprovalAdmissionInput,
    pub plan: OwnerSetupPlanV1,
    pub verify_nonce: String,
}
impl core::fmt::Debug for OwnerSetupAdmissionV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupAdmissionV1(<private>)")
    }
}
/// Trusted adapter observations. Never accepted from HTTP or deserialized.
pub struct OwnerSetupCurrent<'a, 'b> {
    pub provision: &'a ProvisionApprovalContext<'b>,
    pub native_scope: &'a NativeStageScope<'b>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerSetupReadTicket {
    pub stage: OwnerSetupStageV1,
    pub ticket: u8,
    pub purpose: OwnerSetupReadPurposeV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerSetupReadPurposeV1 {
    Session,
    Sign,
}
impl OwnerSetupReadTicket {
    pub fn validate_role(self, role: Role) -> Result<()> {
        if !matches!(
            (role, self.purpose, self.ticket),
            (Role::Identity, OwnerSetupReadPurposeV1::Session, 1..=2)
                | (Role::Owner, OwnerSetupReadPurposeV1::Sign, 3..=5)
        ) {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupPermitViewV1 {
    pub setup_id: String,
    pub kind: OwnerSetupPermitKindV1,
    pub stage: OwnerSetupStageV1,
    pub read_purpose: OwnerSetupReadPurposeV1,
    pub plan: OwnerSetupPlanV1,
    pub participant: ParticipantDescriptorV1,
    pub participant_access: ws::participant_discovery::ParticipantOwnerViewRequestV1,
    pub original: Option<OwnerSetupOriginalV1>,
    pub observation: Option<ws::participant_discovery::ParticipantOwnerViewRequestV1>,
    pub access_nonces: OwnerSetupAccessNoncesV1,
    pub preflight_session_observation_nonce: String,
    pub session_observation_nonce: String,
    pub lookup_session_observation_nonce: Option<String>,
    pub source_revision: String,
    pub authority_revision: String,
    pub service_epoch: DecimalU64,
    pub target_security_epoch: DecimalU64,
    pub observed_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl core::fmt::Debug for OwnerSetupPermitViewV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupPermitViewV1(<private>)")
    }
}
#[path = "owner_setup_admission.rs"]
mod admission;
#[path = "owner_setup_ledger.rs"]
mod ledger;
#[path = "owner_setup_source.rs"]
mod source;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupWorldEvidenceV1 {
    pub request: ws::participant_discovery::ParticipantOwnerViewRequestV1,
    pub response: ws::SignedServiceResponse<ws::participant_discovery::OwnerSetupObservationV1>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "stage",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum OwnerSetupSigningRequestV1 {
    CreateTask(oasis7_agent_api::game::GameTaskCreateV1),
    PrepareGoal(oasis7_agent_api::game::GameGoalPrepareV1),
    PublishGoal(oasis7_agent_api::game::GameGoalPublishV1),
    Delegation(oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1),
    Bind(oasis7_agent_api::game::GameExecutorControlV1),
    FinalObservation {
        owner: Box<ws::ReadWorldViewRequest>,
        goal: ws::private_goal_read::CurrentGoalRequest,
    },
}
impl core::fmt::Debug for OwnerSetupSigningRequestV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupSigningRequestV1(<private>)")
    }
}
#[path = "owner_setup_publish.rs"]
mod publish;
#[path = "owner_setup_requests.rs"]
mod requests;
#[path = "owner_setup_restore.rs"]
mod restore;
#[path = "owner_setup_result.rs"]
mod result;

#[path = "owner_setup_http.rs"]
mod http;
pub use http::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupCompletionV1 {
    pub stage: OwnerSetupStageV1,
    pub result: OwnerSetupResultV1,
    pub session: ParticipantSessionEvidenceV1,
}
impl core::fmt::Debug for OwnerSetupCompletionV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupCompletionV1(<private>)")
    }
}
#[path = "owner_setup_stage_source.rs"]
mod stage_source;

impl core::fmt::Debug for OwnerSetupOutboxV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupOutboxV1(<private>)")
    }
}
impl core::fmt::Debug for OwnerSetupApprovalV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupApprovalV1(<private>)")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct CommitOwnerSetupV1 {
    pub id: String,
    pub signed: SignedApprovalVerificationV1,
    pub source_nonce: String,
}
impl core::fmt::Debug for CommitOwnerSetupV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CommitOwnerSetupV1(<private>)")
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupSigningBundleV1 {
    pub action: OwnerSetupSigningRequestV1,
    pub observation: ws::ReadWorldViewRequest,
    pub participant_access: ws::participant_discovery::ParticipantOwnerViewRequestV1,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupCandidateV1 {
    pub original: OwnerSetupOriginalV1,
    pub observation: ws::participant_discovery::ParticipantOwnerViewRequestV1,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupAdmissionEvidenceV1 {
    pub world: OwnerSetupWorldEvidenceV1,
    pub session: ParticipantSessionEvidenceV1,
}
impl core::fmt::Debug for OwnerSetupSigningBundleV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupSigningBundleV1(<private>)")
    }
}
impl core::fmt::Debug for OwnerSetupCandidateV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupCandidateV1(<private>)")
    }
}
impl core::fmt::Debug for OwnerSetupAdmissionEvidenceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupAdmissionEvidenceV1(<private>)")
    }
}
#[path = "owner_setup_world_admission.rs"]
mod world_admission;

#[path = "owner_setup_outbox_restore.rs"]
mod outbox_restore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerSetupPermitKindV1 {
    Session,
    Sign,
    Preflight,
    Effect,
    Lookup,
}
#[path = "owner_setup_permit.rs"]
mod permit;

impl Eq for OwnerSetupOutboxV1 {}

impl Eq for OwnerSetupPermitViewV1 {}

impl Eq for OwnerSetupWorldEvidenceV1 {}

impl Eq for OwnerSetupCandidateV1 {}

impl Eq for OwnerSetupSigningBundleV1 {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerSetupCanonicalSourceV1 {
    Submit,
    Lookup,
}
impl OwnerSetupCanonicalSourceV1 {
    pub fn response_domain(self) -> &'static str {
        match self {
            Self::Submit => ws::SUBMIT_PATH,
            Self::Lookup => ws::LOOKUP_PATH,
        }
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupCanonicalFactV1 {
    pub source: OwnerSetupCanonicalSourceV1,
    pub response: ws::SignedServiceResponse<ws::IntentResponse<serde_json::Value>>,
}
impl Eq for OwnerSetupCanonicalFactV1 {}
impl core::fmt::Debug for OwnerSetupCanonicalFactV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupCanonicalFactV1(<public-fact>)")
    }
}
/// Fixed public deployment configuration, never an HTTP-supplied claim.
pub struct OwnerSetupArchiveContext<'a> {
    pub world_public_key: &'a str,
}
impl OwnerSetupResultV1 {
    /// Historical classification only, not current authority.
    pub fn succeeded(&self) -> bool {
        match self {
            Self::Canonical { response, .. } => matches!(
                response.payload.outcome,
                ws::IntentOutcome::Committed { .. }
            ),
            Self::PreparedGoal(_) | Self::FinalObservation { .. } => true,
        }
    }
    pub fn terminal(&self) -> bool {
        match self {
            Self::Canonical { response, .. } => matches!(
                response.payload.outcome,
                ws::IntentOutcome::Committed { .. } | ws::IntentOutcome::Rejected { .. }
            ),
            _ => true,
        }
    }
    pub fn canonical_fact(&self) -> Option<OwnerSetupCanonicalFactV1> {
        match self {
            Self::Canonical {
                response,
                from_lookup,
            } => Some(OwnerSetupCanonicalFactV1 {
                source: if *from_lookup {
                    OwnerSetupCanonicalSourceV1::Lookup
                } else {
                    OwnerSetupCanonicalSourceV1::Submit
                },
                response: response.clone(),
            }),
            _ => None,
        }
    }
}
#[path = "owner_setup_archive.rs"]
mod archive;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupAccessNoncesV1 {
    pub policy: String,
    pub policy_session: String,
    pub authorize_session: String,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupAccessEvidenceV1 {
    pub policy: ws::participant_discovery::ParticipantPolicyOriginalV1,
    pub policy_session: ParticipantSessionEvidenceV1,
    pub admission: ws::SignedReadRequest<oasis7_agent_api::game::GameParticipantAdmissionV1>,
    pub authorize_session: ParticipantSessionEvidenceV1,
}
impl Eq for OwnerSetupAccessEvidenceV1 {}
impl core::fmt::Debug for OwnerSetupAccessEvidenceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupAccessEvidenceV1(<public-access>)")
    }
}
#[path = "owner_setup_access.rs"]
mod access;

#[path = "owner_setup_basis.rs"]
mod basis;
pub use basis::*;
