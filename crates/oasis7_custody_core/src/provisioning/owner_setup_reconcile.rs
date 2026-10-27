//! Separately approved, paid reconciliation of an immutable setup original.
use super::*;
pub const OWNER_SETUP_RECONCILE_ATTEMPT_TRANSPORT_COST: u64 = 104;
use crate::approval::{ApprovalOptionsRequestV1, ApprovalVerifyRequestV1, VerificationPurposeV1};
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcilePlanV1 {
    pub api_version: u8,
    pub reconcile_id: String,
    pub setup_id: String,
    pub provision_id: String,
    pub stage: OwnerSetupStageV1,
    pub setup_plan_digest: String,
    pub original_digest: String,
    pub correlation_digest: String,
    pub world_identity: crate::approval::ApprovedWorldV1,
    pub owner_key_ref: String,
    pub executor_key_ref: String,
    pub original_deadline: DecimalU64,
    pub deadline: DecimalU64,
    pub attempt_limit: u8,
    pub transport_limit: DecimalU64,
}
impl OwnerSetupReconcilePlanV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1
            || ![
                &self.reconcile_id,
                &self.setup_id,
                &self.provision_id,
                &self.owner_key_ref,
                &self.executor_key_ref,
            ]
            .iter()
            .all(|s| id(s))
            || ![
                &self.setup_plan_digest,
                &self.original_digest,
                &self.correlation_digest,
            ]
            .iter()
            .all(|s| hex64(s))
            || !(1..=3).contains(&self.attempt_limit)
            || self.deadline.0 == 0
            || self.deadline.0 > self.original_deadline.0
            || self.transport_limit.0
                != 8 + OWNER_SETUP_RECONCILE_ATTEMPT_TRANSPORT_COST * u64::from(self.attempt_limit)
            || self.transport_limit.0 > 400
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        digest(&("oasis7.custody.owner-setup-reconcile-plan.v1", self))
    }
    pub fn intent(&self) -> Result<crate::approval::CustodyIntentV1> {
        self.validate()?;
        Ok(crate::approval::CustodyIntentV1::OwnerSetupReconcile {
            plan: Box::new(self.clone()),
        })
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcileApprovalV1 {
    pub plan: OwnerSetupReconcilePlanV1,
    pub book: ApprovalAdmissionBook,
    pub transport: Option<BudgetLedger>,
    pub source: Option<ProvisionOriginInspectionV1>,
    pub session: Option<ParticipantSessionRefV1>,
    pub attempts: BTreeMap<String, OwnerSetupReconcileAttemptV1>,
}
impl core::fmt::Debug for OwnerSetupReconcileApprovalV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupReconcileApprovalV1(<private>)")
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcileAttemptV1 {
    pub attempt_id: String,
    pub nonce: String,
    pub original_lookup_bytes: Option<String>,
    pub reserved_at: DecimalU64,
    pub source: ProvisionOriginInspectionV1,
    pub oracle_reads: u8,
    pub access_claimed_at: Option<DecimalU64>,
    pub access: Option<OwnerSetupAccessEvidenceV1>,
    pub access_recorded_at: Option<DecimalU64>,
    pub observation: Option<ws::participant_discovery::ParticipantOwnerViewRequestV1>,
    pub goal_request: Option<ws::SignedReadRequest<ws::private_goal_read::CurrentGoalRequest>>,
    pub published_at: Option<DecimalU64>,
    pub admission_reserved_at: Option<DecimalU64>,
    pub admission: Option<OwnerSetupAdmissionEvidenceV1>,
    pub admitted_at: Option<DecimalU64>,
    pub lookup_reserved_at: Option<DecimalU64>,
    pub lookup_claimed_at: Option<DecimalU64>,
    pub fact: Option<OwnerSetupCanonicalFactV1>,
    pub completion_reserved_at: Option<DecimalU64>,
    pub result: Option<OwnerSetupResultV1>,
    pub result_session: Option<ParticipantSessionEvidenceV1>,
    pub result_lookup_session: Option<ParticipantSessionEvidenceV1>,
    pub final_world: Option<OwnerSetupWorldEvidenceV1>,
    pub completed_at: Option<DecimalU64>,
}
impl Eq for OwnerSetupReconcileAttemptV1 {}
impl core::fmt::Debug for OwnerSetupReconcileAttemptV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupReconcileAttemptV1(<private>)")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OwnerSetupResultProvenanceV1 {
    Normal,
    Reconcile {
        reconcile_id: String,
        attempt_id: String,
    },
}
#[derive(Clone)]
pub struct OwnerSetupReconcileAdmissionV1 {
    pub input: ApprovalAdmissionInput,
    pub plan: OwnerSetupReconcilePlanV1,
    pub verify_nonce: String,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcilePermitV1 {
    pub reconcile_id: String,
    pub setup_id: String,
    pub provision_id: String,
    pub attempt_id: String,
    pub kind: OwnerSetupReconcilePermitKindV1,
    pub read_purpose: OwnerSetupReadPurposeV1,
    pub read_ticket: u8,
    pub target_security_epoch: DecimalU64,
    pub source_revision: String,
    pub stage: OwnerSetupStageV1,
    pub participant: ParticipantDescriptorV1,
    pub participant_access: ws::participant_discovery::ParticipantOwnerViewRequestV1,
    pub observation: ws::ReadWorldViewRequest,
    pub original_lookup_bytes: Option<String>,
    pub final_goal: Option<ws::private_goal_read::CurrentGoalRequest>,
    pub nonces: OwnerSetupAccessNoncesV1,
    pub preflight_session_nonce: String,
    pub lookup_session_nonce: String,
    pub poststate_session_nonce: String,
    pub authority_revision: String,
    pub service_epoch: DecimalU64,
    pub observed_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl Eq for OwnerSetupReconcilePermitV1 {}
impl core::fmt::Debug for OwnerSetupReconcilePermitV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupReconcilePermitV1(<private>)")
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcileReadCandidateV1 {
    pub observation: ws::participant_discovery::ParticipantOwnerViewRequestV1,
    pub goal: Option<ws::SignedReadRequest<ws::private_goal_read::CurrentGoalRequest>>,
}
impl Eq for OwnerSetupReconcileReadCandidateV1 {}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcileCompletionV1 {
    pub lookup_session: Option<ParticipantSessionEvidenceV1>,
    pub result: OwnerSetupResultV1,
    pub session: ParticipantSessionEvidenceV1,
    pub world: OwnerSetupWorldEvidenceV1,
}
#[path = "owner_setup_reconcile_admission.rs"]
mod admission;
#[path = "owner_setup_reconcile_dispatch.rs"]
mod dispatch;
#[path = "owner_setup_reconcile_restore.rs"]
mod restore;
#[path = "owner_setup_reconcile_result.rs"]
mod result;
#[path = "owner_setup_reconcile_runtime.rs"]
mod runtime;
#[path = "owner_setup_reconcile_source.rs"]
mod source;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerSetupReconcileCallbackV1 {
    Admission,
    Lookup,
    Result,
}

#[path = "owner_setup_reconcile_http.rs"]
mod http;
pub use http::*;

/// Trusted static role and the pre-I/O read reservation; never deserialized.
pub struct OwnerSetupReconcileRead<'a> {
    pub attempt_id: &'a str,
    pub read: OwnerSetupReadTicket,
    pub role: Role,
}

impl Eq for OwnerSetupReconcileCompletionV1 {}
impl core::fmt::Debug for OwnerSetupReconcileCompletionV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupReconcileCompletionV1(<private>)")
    }
}
impl core::fmt::Debug for OwnerSetupReconcileReadCandidateV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupReconcileReadCandidateV1(<private>)")
    }
}
impl core::fmt::Debug for OwnerSetupReconcileAdmissionV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupReconcileAdmissionV1(<private>)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerSetupReconcilePermitKindV1 {
    Session,
    Sign,
    Preflight,
    LookupReady,
    Lookup,
    Poststate,
}
