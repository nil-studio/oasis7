//! Error types for the runtime module.

use std::io;

use super::types::ProposalId;
use oasis7_wasm_abi::ModuleCallErrorCode;

/// Errors that can occur in world operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldError {
    JournalMismatch,
    RollbackReplayJournalInvalid {
        index: usize,
        expected_event_id: u64,
        found_event_id: u64,
    },
    RollbackReplayTargetInvalid {
        snapshot_journal_len: usize,
        target_journal_len: usize,
        supplied_journal_len: usize,
    },
    RollbackTargetStateRootMismatch {
        expected: String,
        actual: String,
    },
    RollbackJournalCommitmentMismatch {
        expected: String,
        actual: String,
    },
    RollbackNonceConflict {
        nonce: String,
        committed_intent_hash: String,
        supplied_intent_hash: String,
    },
    RollbackAttributionResolutionConflict {
        source_batch_id: String,
        source_event_id: u64,
    },
    RollbackReconciliationFailed {
        tick: u64,
        reason: String,
    },
    AgentNotFound {
        agent_id: String,
    },
    ResourceBalanceInvalid {
        reason: String,
    },
    CapabilityMissing {
        cap_ref: String,
    },
    CapabilityExpired {
        cap_ref: String,
    },
    CapabilityNotAllowed {
        cap_ref: String,
        kind: String,
    },
    /// A v2 module command failed a live authorization check.  This is kept
    /// separate from the legacy effect capability errors so callers cannot
    /// accidentally treat a legacy `allow_all` grant as v2 authority.
    CapabilityAuthorizationDenied {
        reason: String,
    },
    /// A durable v2 execution nonce was already used for another request.
    CapabilityNonceConflict {
        nonce_key_hash: String,
        committed_request_hash: String,
        supplied_request_hash: String,
    },
    PolicyDenied {
        intent_id: String,
        reason: String,
    },
    ReceiptUnknownIntent {
        intent_id: String,
    },
    ReceiptSignatureInvalid {
        intent_id: String,
    },
    ProposalNotFound {
        proposal_id: ProposalId,
    },
    ProposalInvalidState {
        proposal_id: ProposalId,
        expected: String,
        found: String,
    },
    GovernanceFinalityInvalid {
        reason: String,
    },
    GovernancePolicyInvalid {
        reason: String,
    },
    PatchBaseMismatch {
        expected: String,
        found: String,
    },
    PatchInvalidPath {
        path: String,
    },
    PatchNonObject {
        path: String,
    },
    ModuleChangeInvalid {
        reason: String,
    },
    ModuleCallFailed {
        module_id: String,
        trace_id: String,
        code: ModuleCallErrorCode,
        detail: String,
    },
    ModuleStoreVersionMismatch {
        expected: u64,
        found: u64,
    },
    ModuleStoreArtifactMissing {
        wasm_hash: String,
    },
    ModuleStoreManifestMismatch {
        wasm_hash: String,
    },
    BlobNotFound {
        content_hash: String,
    },
    BlobHashMismatch {
        expected: String,
        actual: String,
    },
    BlobHashInvalid {
        content_hash: String,
    },
    NetworkProtocolUnavailable {
        protocol: String,
    },
    NetworkRequestFailed {
        code: oasis7_proto::distributed::DistributedErrorCode,
        message: String,
        retryable: bool,
    },
    DistributedValidationFailed {
        reason: String,
    },
    SignatureKeyInvalid,
    Io(String),
    Serde(String),
}

impl From<serde_json::Error> for WorldError {
    fn from(error: serde_json::Error) -> Self {
        WorldError::Serde(error.to_string())
    }
}

impl From<serde_cbor::Error> for WorldError {
    fn from(error: serde_cbor::Error) -> Self {
        WorldError::Serde(error.to_string())
    }
}

impl From<io::Error> for WorldError {
    fn from(error: io::Error) -> Self {
        WorldError::Io(error.to_string())
    }
}

impl From<oasis7_proto::world_error::WorldError> for WorldError {
    fn from(error: oasis7_proto::world_error::WorldError) -> Self {
        match error {
            oasis7_proto::world_error::WorldError::NetworkProtocolUnavailable { protocol } => {
                WorldError::NetworkProtocolUnavailable { protocol }
            }
            oasis7_proto::world_error::WorldError::NetworkRequestFailed {
                code,
                message,
                retryable,
            } => WorldError::NetworkRequestFailed {
                code,
                message,
                retryable,
            },
            oasis7_proto::world_error::WorldError::DistributedValidationFailed { reason } => {
                WorldError::DistributedValidationFailed { reason }
            }
            oasis7_proto::world_error::WorldError::BlobNotFound { content_hash } => {
                WorldError::BlobNotFound { content_hash }
            }
            oasis7_proto::world_error::WorldError::BlobHashMismatch { expected, actual } => {
                WorldError::BlobHashMismatch { expected, actual }
            }
            oasis7_proto::world_error::WorldError::BlobHashInvalid { content_hash } => {
                WorldError::BlobHashInvalid { content_hash }
            }
            oasis7_proto::world_error::WorldError::SignatureKeyInvalid => {
                WorldError::SignatureKeyInvalid
            }
            oasis7_proto::world_error::WorldError::Io(message) => WorldError::Io(message),
            oasis7_proto::world_error::WorldError::Serde(message) => WorldError::Serde(message),
        }
    }
}
