//! V2 persisted outbox shape. Construction is private to the actual Account
//! factory; a deserialized book or public wire is never a dispatch permission.
use super::*;
use crate::time_units::UnixMillis;
fn required_option<'de, D, T>(deserializer: D) -> core::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum MoveExecutionStageV2 {
    Prepare,
    Prefix,
    Get,
    Cognition,
}

/// Complete canonical bytes of one fixed *public* signed stage original.
/// Private slot bodies/envelopes are not an accepted kind. The owning factory
/// must decode the exact SDK type, reencode, verify signatures and reconstruct
/// the expected candidate before accepting an original into this field.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicStageOriginalV2 {
    stage: MoveExecutionStageV2,
    canonical_cbor_hex: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveStageAllocationV2 {
    operation_id: String,
    allocation_sequence: DecimalU64,
    source_operation_id: String,
    source_digest: String,
    source_observed_at: UnixMillis,
    source_expires_at: UnixMillis,
    allocated_at: UnixMillis,
    factory_ingress_id: String,
    // Only schemas which actually have a client nonce may allocate one.
    #[serde(deserialize_with = "required_option")]
    client_nonce: Option<String>,
    #[serde(deserialize_with = "required_option")]
    observation_nonce: Option<String>,
    gateway_nonce: String,
    #[serde(deserialize_with = "required_option")]
    predecessor_digest: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveStageSendClaimV2 {
    attempt_id: String,
    source_operation_id: String,
    original_digest: String,
    claimed_at: UnixMillis,
    native_transports: MoveNativeTransportClaimV2,
}

/// Only the three demonstrated Native-operation edges. Actor, factory,
/// coordinator, Current/Directory/RF and reconciliation ports are separate
/// real charges; this tuple is not a complete execution graph.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveNativeTransportClaimV2 {
    identity_to_native: String,
    native_to_world_view: String,
    native_to_world_operation: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveStageOutboxV2 {
    allocation: MoveStageAllocationV2,
    candidate_digest: String,
    signing_admitted_at: UnixMillis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    service_record: Option<ExecutionPrepareServiceRecordV2>,
    #[serde(deserialize_with = "required_option")]
    original: Option<PublicStageOriginalV2>,
    #[serde(deserialize_with = "required_option")]
    send_claim: Option<MoveStageSendClaimV2>,
    lookup_claims: BTreeMap<String, MoveStageLookupClaimV2>,
    /// An immutable, typed-and-verified public fact's canonical bytes. It does
    /// not carry Current or mean the action succeeded; admission is separate.
    #[serde(deserialize_with = "required_option")]
    archived_fact_cbor_hex: Option<String>,
    #[serde(deserialize_with = "required_option")]
    admitted_at: Option<UnixMillis>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveStageLookupClaimV2 {
    purpose: SessionPurposeV1,
    source_operation_id: String,
    claimed_at: UnixMillis,
    original_digest: String,
    gateway_nonce: String,
    native_transports: MoveNativeTransportClaimV2,
}

// This stored wire container confers no verification. The source-safe profile
// rejects every nonempty slot; the queued SDK/Scope2 consumer must be composed
// explicitly before a validated public Ref can be inserted or used.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicSlotRefV2 {
    canonical_cbor_hex: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct SessionExecutionBookV2 {
    api_version: u8,
    session_id: String,
    plan_digest: String,
    approved_input_digest: String,
    session_intent_digest: String,
    world: crate::approval::ApprovedWorldV1,
    identity_scope: crate::role_key_slot::RoleKeySlotScopeV1,
    native_binding_digest: String,
    execution_deadline: UnixMillis,
    action_request_id: String,
    credential_generation: DecimalU64,
    approved_position_cm: [i64; 3],
    started_at: UnixMillis,
    challenge_request_id: String,
    http_ingress_attempt_id: String,
    source_operation_id: String,
    lifetime_transport_id: String,
    lifetime_claimed_at: UnixMillis,
    #[serde(deserialize_with = "required_option")]
    archived_lifetime: Option<crate::native_binding::SignedManagedNativeSessionLifetimeV1>,
    #[serde(deserialize_with = "required_option")]
    lifetime_archived_at: Option<UnixMillis>,
    #[serde(deserialize_with = "required_option")]
    lifetime_admitted_at: Option<UnixMillis>,
    /// Internal allocation provenance only, never a delegation HWM/floor.
    last_allocation_sequence: DecimalU64,
    stages: BTreeMap<MoveExecutionStageV2, MoveStageOutboxV2>,
    context_slots: BTreeMap<crate::context_body::ContextBodySlotKindV1, PublicSlotRefV2>,
}

impl core::fmt::Debug for SessionExecutionBookV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SessionExecutionBookV2")
            .field("api_version", &self.api_version)
            .field("stage_count", &self.stages.len())
            .finish_non_exhaustive()
    }
}

impl SessionExecutionBookV2 {
    /// Occupancy only. This does not authenticate the candidate id, fund a
    /// source, authorize a new attempt, or make historical work dispatchable.
    /// The referenced action/challenge/Host-ingress ids remain occupied by
    /// their original challenge/paid maps; they are not new transport ids.
    pub(in crate::provisioning) fn transport_id_in_use(&self, id: &str) -> bool {
        self.session_id == id
            || self.source_operation_id == id
            || self.lifetime_transport_id == id
            || self
                .archived_lifetime
                .as_ref()
                .is_some_and(|v| v.lifetime.status.nonce == id)
            || self.stages.values().any(|stage| {
                let allocation = &stage.allocation;
                allocation.operation_id == id
                    || allocation.factory_ingress_id == id
                    || allocation.source_operation_id == id
                    || allocation.client_nonce.as_deref() == Some(id)
                    || allocation.observation_nonce.as_deref() == Some(id)
                    || allocation.gateway_nonce == id
                    || stage.lookup_claims.iter().any(|(attempt, claim)| {
                        attempt == id
                            || claim.source_operation_id == id
                            || claim.gateway_nonce == id
                            || claim.native_transports.identity_to_native == id
                            || claim.native_transports.native_to_world_view == id
                            || claim.native_transports.native_to_world_operation == id
                    })
                    || stage.send_claim.as_ref().is_some_and(|claim| {
                        claim.attempt_id == id
                            || claim.source_operation_id == id
                            || claim.native_transports.identity_to_native == id
                            || claim.native_transports.native_to_world_view == id
                            || claim.native_transports.native_to_world_operation == id
                    })
            })
    }
}

#[path = "test_session_execution_source_binding_v2.rs"]
mod source_binding;

#[path = "test_session_execution_start_v2.rs"]
mod start;

#[path = "test_session_execution_lifetime_v2.rs"]
mod lifetime;

#[path = "test_session_execution_restore_v2.rs"]
mod restore;

/// Borrowed historical record view, never an authorization or dispatch token.
pub(in crate::provisioning) struct ExecutionJournalRecordV2<'a> {
    pub request_id: &'a str,
    pub ingress_attempt_id: &'a str,
    pub challenge_request_id: &'a str,
    pub source_operation_id: &'a str,
    pub started_at: UnixMillis,
    pub lifetime_claimed_at: UnixMillis,
    pub lifetime_archived_at: Option<UnixMillis>,
    pub lifetime_admitted_at: Option<UnixMillis>,
    pub credential_generation: DecimalU64,
}
impl SessionExecutionBookV2 {
    pub(in crate::provisioning) fn journal_record(&self) -> ExecutionJournalRecordV2<'_> {
        ExecutionJournalRecordV2 {
            request_id: &self.action_request_id,
            ingress_attempt_id: &self.http_ingress_attempt_id,
            challenge_request_id: &self.challenge_request_id,
            source_operation_id: &self.source_operation_id,
            started_at: self.started_at,
            lifetime_claimed_at: self.lifetime_claimed_at,
            lifetime_archived_at: self.lifetime_archived_at,
            lifetime_admitted_at: self.lifetime_admitted_at,
            credential_generation: self.credential_generation,
        }
    }
}

#[path = "test_session_execution_current_ports_v2.rs"]
mod execution_current_ports_v2;
pub use execution_current_ports_v2::{
    ExecutionPrepareCurrentPortV2, ExecutionPrepareCurrentPortsV2, ExecutionPrepareCurrentReadV2,
    ExecutionPrepareCurrentTransportKindV2, ExecutionPrepareCurrentTransportV2,
};

#[path = "test_session_execution_prepare_allocation_v2.rs"]
mod prepare_allocation;
#[path = "test_session_execution_prepare_paid_v2.rs"]
mod prepare_paid;
#[path = "test_session_execution_prepare_restore_v2.rs"]
mod prepare_restore;
pub use prepare_allocation::{ExecutionPrepareAllocationV2, ExecutionPrepareSigningAuthorityV2};
pub use prepare_paid::{
    ExecutionPrepareFactoryKindV2, ExecutionPrepareFactoryTransportV2, ExecutionPreparePaidIntentV2,
};
pub(in crate::provisioning) use prepare_restore::prepare_paid_id_in_use_v2;

impl SessionExecutionBookV2 {
    pub(in crate::provisioning) fn prepare_stage_count(&self) -> usize {
        self.stages.len()
    }
    pub(in crate::provisioning) fn prepare_journal_record(
        &self,
    ) -> Option<(&str, &str, UnixMillis)> {
        self.stages.get(&MoveExecutionStageV2::Prepare).map(|s| {
            (
                self.action_request_id.as_str(),
                s.allocation.factory_ingress_id.as_str(),
                s.signing_admitted_at,
            )
        })
    }
}

impl SessionExecutionBookV2 {
    pub(in crate::provisioning) fn owns_prepare_factory_transport(
        &self,
        s: &TestSessionAuthorityV2,
        key: &str,
    ) -> bool {
        matches!(s.purpose_transports.get(key), Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(p)) if p.intent.action_request_id == self.action_request_id && p.intent.approved_input_digest == self.approved_input_digest)
    }
    pub(in crate::provisioning) fn prepare_paid_journal_record<'a>(
        &self,
        s: &'a TestSessionAuthorityV2,
    ) -> Option<(&'a str, &'a str, UnixMillis)> {
        s.purpose_transports
            .iter()
            .find_map(|(key, row)| match row {
                SessionPurposeTransportV1::ExecutionFactoryCallbackV2(p) => Some((
                    p.intent.action_request_id.as_str(),
                    key.as_str(),
                    p.claimed_at,
                )),
                _ => None,
            })
    }
    pub(in crate::provisioning) fn prepare_paid_count(&self) -> usize {
        usize::from(self.last_allocation_sequence.0 != 0)
    }
}

impl SessionExecutionBookV2 {
    pub(in crate::provisioning) fn prepare_current_read<'a>(
        &self,
        s: &'a TestSessionAuthorityV2,
    ) -> Option<&'a ExecutionPrepareCurrentReadV2> {
        s.purpose_transports.values().find_map(|row| match row {
            SessionPurposeTransportV1::ExecutionFactoryCallbackV2(p) => p.current_read.as_ref(),
            _ => None,
        })
    }
}

impl SessionExecutionBookV2 {
    pub(in crate::provisioning) fn owns_prepare_current_transport(
        &self,
        s: &TestSessionAuthorityV2,
        key: &str,
    ) -> bool {
        match s.purpose_transports.get(key) {
            Some(SessionPurposeTransportV1::ExecutionPrepareCurrentV2(p)) => {
                execution_current_ports_v2::current_port_parent_valid(s, key, p).is_ok()
            }
            _ => false,
        }
    }
}

#[path = "test_session_execution_signer_permit_v2.rs"]
mod signer_permit_v2;
pub use signer_permit_v2::{
    AccountExecutionPreparePermitV2, ExecutionPreparePermitsV2, ExecutionPrepareRecipientV2,
    ExecutionPrepareServiceRecordV2, ExecutionPrepareSignerOperationV2,
    SignedAccountExecutionPreparePermitV2, SignedExecutionPreparePermitsV2,
    VerifiedExecutionPreparePermitV2,
};
#[path = "test_session_execution_prepare_signed_v2.rs"]
mod signed_prepare_v2;
pub use signed_prepare_v2::ExecutionPrepareSignedOriginalV2;

#[path = "test_session_execution_prepare_service_v2.rs"]
mod prepare_service_v2;
impl SessionExecutionBookV2 {
    pub(in crate::provisioning) fn prepare_service_record(
        &self,
    ) -> Option<&ExecutionPrepareServiceRecordV2> {
        self.stages
            .get(&MoveExecutionStageV2::Prepare)
            .and_then(|s| s.service_record.as_ref())
    }
}

#[path = "test_session_execution_rkv_ports_v2.rs"]
mod rkv_ports_v2;
pub use rkv_ports_v2::{
    ExecutionPrepareDispatchV2, ExecutionPreparePhysicalAttemptV2, ExecutionPrepareRkvGraphV2,
    ExecutionPrepareRkvHopV2, ExecutionPrepareRkvTransportV2,
    execution_prepare_physical_port_id_v2,
};
impl SessionExecutionBookV2 {
    pub(in crate::provisioning) fn owns_prepare_rkv_transport(
        &self,
        s: &TestSessionAuthorityV2,
        key: &str,
    ) -> bool {
        match s.purpose_transports.get(key) {
            Some(SessionPurposeTransportV1::ExecutionPrepareRkvV2(p)) => {
                rkv_ports_v2::parent_valid(s, key, p).is_ok()
            }
            _ => false,
        }
    }
}
