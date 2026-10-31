//! V2 persisted outbox shape. Construction is private to the actual Account
//! factory; a deserialized book or public wire is never a dispatch permission.
use super::*;

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
    client_nonce: String,
    gateway_nonce: String,
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
    original: Option<PublicStageOriginalV2>,
    send_claim: Option<MoveStageSendClaimV2>,
    lookup_claims: BTreeMap<String, MoveStageLookupClaimV2>,
    /// An immutable, typed-and-verified public fact's canonical bytes. It does
    /// not carry Current or mean the action succeeded; admission is separate.
    archived_fact_cbor_hex: Option<String>,
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

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct SessionExecutionBookV2 {
    api_version: u8,
    session_id: String,
    plan_digest: String,
    execution_deadline: UnixMillis,
    action_request_id: String,
    approved_position_cm: [i64; 3],
    /// Internal allocation provenance only, never a delegation HWM/floor.
    last_allocation_sequence: DecimalU64,
    stages: BTreeMap<MoveExecutionStageV2, MoveStageOutboxV2>,
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
    pub(in crate::provisioning) fn transport_id_in_use(&self, id: &str) -> bool {
        self.session_id == id
            || self.action_request_id == id
            || self.stages.values().any(|stage| {
                let allocation = &stage.allocation;
                allocation.operation_id == id
                    || allocation.source_operation_id == id
                    || allocation.client_nonce == id
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
