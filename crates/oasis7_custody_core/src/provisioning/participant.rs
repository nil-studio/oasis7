//! Fixed first participant binding. Every envelope is the registered original, never a generic signer.
use super::*;
use crate::{native_binding::*, native_history::*, role_key_slot::RoleKeySlotScopeV1};
pub use oasis7_agent_api::game::{
    GameParticipantAdmissionV1, GameParticipantReceiptV1, GameParticipantRegisterV1,
};
pub use oasis7_client_api::{
    world_service::{
        self as ws, IntentResponse, ReadWorldViewRequest, SignedReadRequest, SignedServiceResponse,
        participant_discovery::{
            ParticipantOwnerControlResponseV1, ParticipantOwnerViewRequestV1,
            ParticipantPolicyOriginalV1,
        },
    },
    world_service_wire::{AuthenticatedLookup, WorldServicePayloadV1},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
type Result<T> = core::result::Result<T, ProvisionApprovalError>;
pub const PARTICIPANT_TRANSPORT_COST: u64 = 104;
pub const PARTICIPANT_ORACLE_READS: u8 = 8;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantProvisionRequestV1 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub operation_id: String,
}
impl ParticipantProvisionRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1
            || [
                &self.custody_account_id,
                &self.provision_id,
                &self.operation_id,
            ]
            .iter()
            .any(|v| !super::types::valid_id(v))
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        Ok(super::types::digest(&(
            "oasis7.custody.participant-request.v1",
            self,
        ))?)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantStepV1 {
    Policy,
    SignBundle,
    Authorize,
    Submit,
    OwnerView,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantSessionRefV1 {
    pub scope: NativeHistoryScopeV1,
    pub phase: NativeHistoryPhaseV1,
    pub source_permit_id: String,
    pub native_operation_id: String,
    pub session_ref: String,
    pub private_sid_generation: DecimalU64,
    pub native_binding_generation: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantDescriptorV1 {
    pub world: crate::approval::ApprovedWorldV1,
    pub native_binding: NativeBindingReceiptV1,
    pub session: ParticipantSessionRefV1,
    pub owner_scope: RoleKeySlotScopeV1,
    pub owner_key: PublicRoleKey,
    pub identity_scope: RoleKeySlotScopeV1,
    pub register: GameParticipantRegisterV1,
    pub owner_read: ReadWorldViewRequest,
    pub policy_nonce: String,
    pub owner_view_nonce: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantSignBundleV1 {
    pub owner: SignedReadRequest<GameParticipantRegisterV1>,
    pub read: SignedReadRequest<ReadWorldViewRequest>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantSessionEvidenceV1 {
    pub status: SignedManagedNativeSessionStatusV1,
    pub private_sid_generation: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantPermitViewV1 {
    pub request: ParticipantProvisionRequestV1,
    pub descriptor: ParticipantDescriptorV1,
    pub step: ParticipantStepV1,
    pub permit_id: String,
    pub source_revision: String,
    pub authority_revision: String,
    pub target_security_epoch: DecimalU64,
    pub service_epoch: DecimalU64,
    pub observed_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub session_observation_nonce: String,
    pub lookup_session_observation_nonce: Option<String>,
    pub bundle: Option<ParticipantSignBundleV1>,
    pub submit: Option<ws::SubmitIntentRequest<WorldServicePayloadV1>>,
    pub lookup: Option<AuthenticatedLookup>,
    pub owner_view: Option<ParticipantOwnerViewRequestV1>,
}
impl Eq for ParticipantPermitViewV1 {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantResultSourceV1 {
    Submit,
    Lookup,
}
impl ParticipantResultSourceV1 {
    pub fn response_domain(self) -> &'static str {
        match self {
            Self::Submit => ws::SUBMIT_PATH,
            Self::Lookup => ws::LOOKUP_PATH,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantFinalResultV1 {
    pub response_source: ParticipantResultSourceV1,
    pub response: SignedServiceResponse<IntentResponse<serde_json::Value>>,
    pub owner_response: SignedServiceResponse<ParticipantOwnerControlResponseV1>,
    pub submit_session: ParticipantSessionEvidenceV1,
    pub lookup_session: Option<ParticipantSessionEvidenceV1>,
}
impl Eq for ParticipantFinalResultV1 {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantOriginalV1 {
    pub request: ParticipantProvisionRequestV1,
    pub descriptor: ParticipantDescriptorV1,
    pub source_inspection: ProvisionOriginInspectionV1,
    pub created_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub target_security_epoch: DecimalU64,
    pub oracle_reads: u8,
    pub workflow_claimed_at: Option<DecimalU64>,
    pub session_nonce: String,
    pub claims: BTreeMap<String, DecimalU64>,
    pub callbacks: BTreeMap<String, DecimalU64>,
    pub policy: Option<ParticipantPolicyOriginalV1>,
    pub policy_session: Option<ParticipantSessionEvidenceV1>,
    pub policy_recorded_at: Option<DecimalU64>,
    pub bundle: Option<ParticipantSignBundleV1>,
    pub bundle_recorded_at: Option<DecimalU64>,
    pub admission: Option<SignedReadRequest<GameParticipantAdmissionV1>>,
    pub admission_session: Option<ParticipantSessionEvidenceV1>,
    pub admission_recorded_at: Option<DecimalU64>,
    pub submit: Option<ws::SubmitIntentRequest<WorldServicePayloadV1>>,
    pub submit_bytes: Option<String>,
    pub lookup: Option<AuthenticatedLookup>,
    pub owner_view: Option<ParticipantOwnerViewRequestV1>,
    pub result: Option<ParticipantFinalResultV1>,
    pub result_session: Option<ParticipantSessionEvidenceV1>,
    pub completed_at: Option<DecimalU64>,
}
impl Eq for ParticipantOriginalV1 {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantNoncesV1 {
    pub source: String,
    pub policy: String,
    pub owner_view: String,
    pub session: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantCallbackV1 {
    Policy,
    Admission,
    Final,
}
fn bad() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
fn nonce(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn step_name(step: ParticipantStepV1) -> &'static str {
    match step {
        ParticipantStepV1::Policy => "policy",
        ParticipantStepV1::SignBundle => "sign_bundle",
        ParticipantStepV1::Authorize => "authorize",
        ParticipantStepV1::Submit => "submit",
        ParticipantStepV1::OwnerView => "owner_view",
    }
}
fn callback_name(step: ParticipantCallbackV1) -> &'static str {
    match step {
        ParticipantCallbackV1::Policy => "policy",
        ParticipantCallbackV1::Admission => "admission",
        ParticipantCallbackV1::Final => "final",
    }
}
fn world(d: &ParticipantDescriptorV1) -> ws::WorldIdentity {
    ws::WorldIdentity {
        world_id: d.world.world_id.clone(),
        genesis_digest: d.world.genesis_digest.clone(),
    }
}
fn amount(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| {
        v.as_str()
            .and_then(|s| s.parse::<u64>().ok().filter(|v| v.to_string() == s))
    })
}
fn digest(v: &impl Serialize) -> Result<String> {
    Ok(super::types::digest(v)?)
}

#[path = "participant_descriptor.rs"]
mod descriptor;
#[path = "participant_ledger.rs"]
mod ledger;
#[path = "participant_records.rs"]
mod records;
#[path = "participant_restore.rs"]
mod restore;
#[path = "participant_source.rs"]
mod source;
#[path = "participant_validation.rs"]
pub(in crate::provisioning) mod validation;

/// A reserved read credit and the fixed workflow step it observes. This is not authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParticipantReadTicket {
    pub ticket: u8,
    pub step: ParticipantStepV1,
}
struct ParticipantSessionObservation<'a> {
    evidence: &'a ParticipantSessionEvidenceV1,
    checked_at: u64,
}
