//! Durable state used by the versioned trusted-module authorization lane.
//!
//! The legacy [`CapabilityGrant`](super::effect::CapabilityGrant) map is kept
//! deliberately separate.  Values in this module are audit state, not a
//! process-local cache: they are copied into and out of `Snapshot` and are
//! included in the staged world transaction used by the v2 executor.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};

pub use oasis7_agent_api::CapabilityInvocationContext;
use oasis7_wasm_abi::{CapabilitySubject, canonical_hash, encode_canonical_cbor};

use super::governance::GovernanceFinalityCertificate;
use super::types::ProposalId;

/// A finalized governance record installed by the host before a grant can be
/// used.  The executor never treats a process-local key/epoch pair as an
/// authority source: every issuer field is checked against this record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityAuthorityRecord {
    pub issuer_id: String,
    pub issuer_kind: String,
    pub key_id: String,
    pub public_key_hex: String,
    pub issuer_key_epoch: u64,
    pub governance_epoch: u64,
    pub finalized_receipt_id: String,
    #[serde(default)]
    pub authority_rotation_receipt_id: Option<String>,
    pub world_id: String,
    pub branch_id: String,
    pub finality_epoch: u64,
    pub finality_block_hash: String,
    pub finality_status: String,
    pub revocation_epoch: u64,
    #[serde(default)]
    pub revoked_grant_ids: BTreeSet<String>,
    #[serde(default)]
    pub superseded_by: BTreeMap<String, String>,
}

/// Runtime identity binding for an Agent capability subject.
///
/// The simulation's gameplay `AgentState` intentionally does not own
/// authorization credentials.  This record is the durable runtime authority
/// that binds the ABI's owner/generation pair to a live agent id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityAgentIdentity {
    pub owner_binding: String,
    pub generation: u64,
}

/// The authority metadata that must be covered by the finality signer set.
///
/// `GovernanceFinalityCertificate` predates capability authority admission and
/// only signs proposal/validator metadata.  This separate versioned payload
/// keeps that certificate compatible for other governance paths while making
/// capability authority receipt, key, branch, block, and status bindings
/// explicit and replay-verifiable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityAuthorityFinalityBinding {
    pub authority_record_hash: String,
    pub issuer_id: String,
    pub issuer_kind: String,
    pub key_id: String,
    pub issuer_public_key_hex: String,
    pub issuer_key_epoch: u64,
    pub governance_epoch: u64,
    pub finalized_receipt_id: String,
    #[serde(default)]
    pub authority_rotation_receipt_id: Option<String>,
    pub world_id: String,
    pub branch_id: String,
    pub finality_epoch: u64,
    pub finality_block_hash: String,
    pub finality_status: String,
}

impl CapabilityAuthorityFinalityBinding {
    pub fn from_record(record: &CapabilityAuthorityRecord) -> Result<Self, String> {
        Ok(Self {
            authority_record_hash: canonical_hash(record).map_err(|error| error.to_string())?,
            issuer_id: record.issuer_id.clone(),
            issuer_kind: record.issuer_kind.clone(),
            key_id: record.key_id.clone(),
            issuer_public_key_hex: record.public_key_hex.clone(),
            issuer_key_epoch: record.issuer_key_epoch,
            governance_epoch: record.governance_epoch,
            finalized_receipt_id: record.finalized_receipt_id.clone(),
            authority_rotation_receipt_id: record.authority_rotation_receipt_id.clone(),
            world_id: record.world_id.clone(),
            branch_id: record.branch_id.clone(),
            finality_epoch: record.finality_epoch,
            finality_block_hash: record.finality_block_hash.clone(),
            finality_status: record.finality_status.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityAuthorityFinalityProof {
    pub proof_version: u16,
    pub certificate: GovernanceFinalityCertificate,
    pub binding: CapabilityAuthorityFinalityBinding,
    pub signatures: BTreeMap<String, String>,
}

impl CapabilityAuthorityFinalityProof {
    pub const PROOF_VERSION_V1: u16 = 1;
    pub const SIGNATURE_PREFIX_ED25519_V1: &'static str = "capauthsig:ed25519:v1:";

    /// Canonical payload signed by each historical finality signer.
    pub fn signing_payload_v1(&self, signer_node_id: &str) -> Result<Vec<u8>, String> {
        let payload = CapabilityAuthorityFinalitySigningPayload {
            proof_version: self.proof_version,
            proposal_id: self.certificate.proposal_id,
            manifest_hash: &self.certificate.manifest_hash,
            consensus_height: self.certificate.consensus_height,
            epoch_id: self.certificate.epoch_id,
            validator_set_hash: &self.certificate.validator_set_hash,
            stake_root: &self.certificate.stake_root,
            threshold_bps: self.certificate.threshold_bps,
            min_unique_signers: self.certificate.min_unique_signers,
            threshold: self.certificate.threshold,
            binding: &self.binding,
            signer_node_id,
        };
        encode_canonical_cbor(&payload).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Serialize)]
struct CapabilityAuthorityFinalitySigningPayload<'a> {
    proof_version: u16,
    proposal_id: ProposalId,
    manifest_hash: &'a str,
    consensus_height: u64,
    epoch_id: u64,
    validator_set_hash: &'a str,
    stake_root: &'a str,
    threshold_bps: u16,
    min_unique_signers: u16,
    threshold: u16,
    binding: &'a CapabilityAuthorityFinalityBinding,
    signer_node_id: &'a str,
}

/// Durable logical execution budget for one subject/grant pair.  Runtime
/// resource fees remain separately metered; this account prevents a grant
/// from being replayed indefinitely even when those resources are unbounded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityBudgetAccount {
    pub subject: CapabilitySubject,
    pub grant_id: String,
    pub remaining_units: i64,
    #[serde(default)]
    pub reserved_units: i64,
    #[serde(default)]
    pub spent_units: i64,
}

/// The live trust and revocation view used for v2 authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CapabilityRevocationState {
    #[serde(default)]
    pub epoch: u64,
    #[serde(default)]
    pub revoked_grant_ids: BTreeSet<String>,
    #[serde(default)]
    pub superseded_by: BTreeMap<String, String>,
    /// Issuer id -> finalized governance authority record.
    #[serde(default)]
    pub authority_records: BTreeMap<String, CapabilityAuthorityRecord>,
    /// Issuer id -> immutable quorum-signed finality proof for the authority
    /// record. Keeping the proof in snapshots lets recovery revalidate a
    /// trust root even when its installation event predates the snapshot.
    #[serde(default)]
    pub authority_finality_proofs: BTreeMap<String, CapabilityAuthorityFinalityProof>,
    #[serde(default)]
    pub finalized_receipt_id: Option<String>,
    /// Live agent id -> owner/generation binding used by capability subjects.
    #[serde(default)]
    pub agent_identities: BTreeMap<String, CapabilityAgentIdentity>,
    /// Canonical owner-signed service delegation, retained in snapshots.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_signer_delegations:
        BTreeMap<String, crate::service_admission::AgentSignerDelegationChangeV1>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub world_service_results: BTreeMap<String, serde_json::Value>,
    /// Runtime-bound system id -> current authorization epoch.  System
    /// subjects are not inferred from an enum value; the trusted host must
    /// journal a binding before the subject can execute.
    #[serde(default)]
    pub system_identities: BTreeMap<String, u64>,
}

/// A durable record for the complete authorization nonce tuple.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityAuthorizationNonceRecord {
    pub request_hash: String,
    pub outcome_hash: String,
    #[serde(default)]
    pub committed_receipt_id: Option<String>,
    /// `reserved`, `committed`, or `aborted`.
    pub state: String,
}

/// Runtime audit receipt for one presented v2 command.
///
/// Subject, presenter and audience are retained as canonical JSON values so
/// the runtime can persist ABI additions without another migration while
/// keeping their exact signed/canonical representation for audit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityAuthorizationAuditReceipt {
    pub receipt_id: String,
    #[serde(default)]
    pub root_receipt_id: Option<String>,
    #[serde(default)]
    pub grant_id: Option<String>,
    pub subject: JsonValue,
    #[serde(default)]
    pub presenter: Option<JsonValue>,
    pub audience: JsonValue,
    pub scope_hash: String,
    #[serde(default)]
    pub module_id: Option<String>,
    #[serde(default)]
    pub module_version: Option<String>,
    #[serde(default)]
    pub manifest_hash: Option<String>,
    #[serde(default)]
    pub catalog_snapshot_id: Option<String>,
    #[serde(default)]
    pub response_nonce: Option<String>,
    #[serde(default)]
    pub authorization_nonce_key_hash: Option<String>,
    /// `accepted`, `denied`, `idempotent`, or `pending`.
    pub decision: String,
    #[serde(default)]
    pub denial_code: Option<String>,
    #[serde(default)]
    pub budget_before: i64,
    #[serde(default)]
    pub budget_after: Option<i64>,
    pub world_head_before: u64,
    #[serde(default)]
    pub world_head_after: Option<u64>,
    pub branch_id: String,
    pub finality_epoch: u64,
    #[serde(default)]
    pub finality_block_hash: Option<String>,
    pub finality_status: String,
    pub state_hash_before: String,
    #[serde(default)]
    pub state_hash_after: Option<String>,
    #[serde(default)]
    pub committed_effect_receipt_id: Option<String>,
    /// Intent ids whose external receipts have been durably committed for
    /// this authorization.  `committed_effect_receipt_id` remains the first
    /// receipt for snapshots and consumers written before multi-effect
    /// closure was supported; this set is the complete replayable record.
    #[serde(default)]
    pub committed_effect_receipt_ids: BTreeSet<String>,
    pub canonical_request_hash: String,
    pub canonical_result_hash: String,
}

/// Durable association between a queued module effect and its authorization
/// audit receipt.  The association is removed only when the external effect
/// receipt is ingested, at which point the audit receipt records that actual
/// receipt's intent id.  A command never invents an effect receipt locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEffectReceiptLink {
    pub authorization_receipt_id: String,
}
