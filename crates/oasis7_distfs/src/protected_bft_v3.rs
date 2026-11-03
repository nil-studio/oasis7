//! Qualified-H protected header signer. State and wire domains are independent
//! from V1/V2 and carry public protocol material only. Backends must provide
//! independently provisioned fencing and anti-rollback guarantees.
mod active_window;
mod active_window_reconstruction;
mod archive_query;
mod archive_transfer;
mod backend;
mod binding;
mod bootstrap_rotation;
mod client;
mod crypto;
mod install;
mod membership;
mod private_retention;
mod private_retention_selection;
pub use private_retention_selection::{
    BftPrivateRetentionSelectRequestV1, BftPrivateRetentionSelectionCommitmentV1,
    BftPrivateRetentionSelectionIndexV1, BftPrivateRetentionSelectionKindV1,
    BftPrivateRetentionSelectionOperationV1, BftPrivateRetentionSelectionResultV1,
    BftPrivateRetentionSelectionV1, VerifiedBftPrivateRetentionSelectionIdentityV1,
};
mod private_retention_input;
mod private_retention_input_authorization;
mod retention_proof_query;
pub use private_retention_input::{
    BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1, BftPrivateRetentionAdmissionInputReadV1,
    BftPrivateRetentionAdmissionInputReaderV1, BftPrivateRetentionAdmissionInputRefV1,
    BftPrivateRetentionAdmissionInputV1, BftPrivateRetentionEncodedArchiveReadV1,
    BftPrivateRetentionInputObjectKindV1, BftPrivateRetentionInputObjectV1,
};
pub use private_retention_input_authorization::{
    BftPrivateRetentionAdmissionReadGrantBodyV1, BftPrivateRetentionAdmissionReadGrantV1,
    BftPrivateRetentionAdmissionReadPurposeV1,
};
mod private_retention_backend;
mod private_retention_creation;
mod private_retention_current_anchor;
mod private_retention_history;
mod private_retention_operation_proof;
pub use crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3;
pub use private_retention_operation_proof::{
    BftPrivateRetentionOperationAccountingV1, BftPrivateRetentionOperationProofV1,
    private_retention_receipt_digest,
};
mod private_retention_memory;
mod private_retention_observation;
mod private_retention_seal;
mod private_retention_seal_context;
mod private_retention_sealed_current;
mod private_retention_verification_context;
pub use private_retention_current_anchor::{
    BftPrivateRetentionSealAnchorBodyV1, BftPrivateRetentionSealAnchorV1,
};
pub use private_retention_history::BftPrivateRetentionHistoryV1;
pub use private_retention_seal::{
    BftPrivateRetentionSealBodyV1, BftPrivateRetentionSealCasV1, BftPrivateRetentionSealV1,
    ValidatedBftPrivateRetentionSealChainV1,
};
pub use private_retention_seal_context::{
    BftPrivateRetentionHistoricalInputContextV1, BftPrivateRetentionReservationInputContextV1,
    BftPrivateRetentionSealVerificationContextV1,
};
pub use private_retention_sealed_current::BftPrivateRetentionSealedCurrentV1;
pub use private_retention_verification_context::{
    BftPrivateRetentionAuthenticatedAdmissionParentV1, BftPrivateRetentionHistoricalOwnedC0V1,
    BftPrivateRetentionVerificationContextV1,
};
#[cfg(test)]
mod private_retention_tests;
mod rotation;
mod service;
mod state;
mod successor_archive_store;
mod successor_public_archive;
mod successor_state;
mod successor_window;
#[cfg(test)]
mod tests;
mod wire;

pub use backend::{
    BFT_SIGNER_BACKEND_NAMESPACE_V3, BftSignerAcquireOutcomeV3, BftSignerBackendSnapshotV3,
    BftSignerBackendV3,
};
pub use binding::{
    BftSignerBindingV3, BftSignerErrorV3, BftSignerKeyIdentityV3, BftSignerTrustV3,
    BftSigningKeyHandleV3,
};
pub use client::{
    BftSignerLeaseV3, BftSignerProviderV3, RemoteBftSignerLeaseV3, RemoteBftSignerProviderV3,
};
pub use service::{
    BftCandidateReadPolicyV3, BftPrivateRetentionMaterialVerifierV1,
    BftPrivateRetentionSigningPurposeV1, BftPrivateRetentionSupportV1, BftSignerServiceV3,
    BftSuccessorWindowDriverVerifierV1, BftSuccessorWindowSupportV1,
};
pub use state::{
    BftDriverAppliedReceiptV3, BftDriverApplyIntentV3, BftSignerCandidateV3, BftSignerCheckpointV3,
    BftSignerStateV3,
};

pub use install::{
    BftLocalInstallEvidenceV3, BftTransitionInstallIntentBodyV3, BftTransitionInstallIntentV3,
    BftTransitionInstallPreparationV3, BftTransitionInstallReceiptBodyV3,
    BftTransitionInstallReceiptV3, BftTransitionInstallResultV3,
};
pub use membership::BftMembershipInstallStateV3;

pub use rotation::{BftFinalityRotationProofArchiveV3, BftFinalityRotationProofRecordV3};

pub use bootstrap_rotation::{
    BftBootstrapFinalityRotationProofArchiveV3, BftBootstrapFinalityRotationProofRecordV3,
};

pub use successor_archive_store::{BftSuccessorArchiveStoreV1, LocalBftSuccessorArchiveStoreV1};
pub use successor_window::{
    BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1, BFT_SUCCESSOR_WINDOW_MAX_SEGMENTS_V1,
    BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1, BFT_SUCCESSOR_WINDOW_SOFT_HEIGHTS_V1,
    BftSuccessorWindowAdoptedEvidenceV1, BftSuccessorWindowArchiveEntryV1,
    BftSuccessorWindowArchiveIdentityV1, BftSuccessorWindowArchiveKindV1,
    BftSuccessorWindowArchiveManifestV1, BftSuccessorWindowArchiveQueryV1,
    BftSuccessorWindowArchiveRefV1, BftSuccessorWindowArchiveReplyV1,
    BftSuccessorWindowArchiveUsageV1, BftSuccessorWindowCheckpointBodyV1, BftSuccessorWindowCutV1,
    BftSuccessorWindowInstallIntentBodyV1, BftSuccessorWindowInstallIntentV1,
    BftSuccessorWindowInstallReceiptBodyV1, BftSuccessorWindowInstallReceiptV1,
    BftSuccessorWindowMinimumV1, BftSuccessorWindowPersistedRequestV1, BftSuccessorWindowSourceV1,
    BftSuccessorWindowStatusV1,
};

pub use successor_state::{BftSuccessorWindowStateV1, BftSuccessorWindowStoredRequestV1};

pub use successor_public_archive::{
    BftSuccessorWindowOriginalSignedResultV1, index_whole_state_signed_results_v1,
    resolve_whole_state_signed_result_v1, whole_state_signed_result_entries_v1,
};
pub use successor_window::BftSuccessorWindowPublicIndexV1;

pub use archive_transfer::{
    BFT_SUCCESSOR_ARCHIVE_TRANSFER_MAX_RESERVATION_BYTES_V1, BftSuccessorArchiveTransferLedgerV1,
    reserve_successor_archive_transfer_v1, validate_successor_archive_budget,
    validate_successor_archive_transfer_observation_v1,
    validate_successor_archive_transfer_reservation_v1,
};

pub use archive_query::{
    BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1, BFT_SUCCESSOR_ARCHIVE_MAX_CHUNKS_V1,
    BftSuccessorArchiveChunkBodyV1, BftSuccessorArchiveChunkV1, BftSuccessorArchiveReadTargetV1,
    BftSuccessorArchiveReadV1, ValidatedBftSuccessorOriginalStateV1,
};

pub use active_window::{BftSignerActiveWindowV1, VerifiedBftSignerActiveWindowV1};

pub use active_window_reconstruction::ValidatedBftSuccessorInstalledStateV1;

pub use private_retention::{
    BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1, BFT_PRIVATE_RETENTION_MAX_ENTRIES_V1,
    BFT_PRIVATE_RETENTION_MAX_MAP_BYTES_V1, BftPrivateRetentionAttestorV1,
    BftPrivateRetentionEntryV1, BftPrivateRetentionFactV1, BftPrivateRetentionHeadV1,
    BftPrivateRetentionKeyV1, BftPrivateRetentionLifecycleV1, BftPrivateRetentionMapV1,
    BftPrivateRetentionReceiptBodyV1, BftPrivateRetentionReceiptV1, BftPrivateRetentionSourceV1,
};
pub use private_retention_backend::{
    BftPrivateRetentionCasV1, BftPrivateRetentionCurrentV1, BftPrivateRetentionInputInventoryV1,
    BftPrivateRetentionLedgerV1, BftPrivateRetentionSnapshotV1,
};
pub use private_retention_creation::{
    BftPrivateRetentionCreationMarkerV1, BftPrivateRetentionFreshNamespaceV1,
    BftPrivateRetentionFreshRecoveryV1, BftPrivateRetentionFreshSeedV1,
    ValidatedBftPrivateRetentionCreationV1,
};
pub use private_retention_memory::BftPrivateRetentionMemoryBackendV1;
pub use private_retention_observation::ValidatedBftPrivateRetentionObservationV1;

pub use service::{
    ObservedBftOriginalInputServingViewV1, bft_original_input_network_response_bytes_v1,
};
