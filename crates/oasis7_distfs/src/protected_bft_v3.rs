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
    BftCandidateReadPolicyV3, BftSignerServiceV3, BftSuccessorWindowDriverVerifierV1,
    BftSuccessorWindowSupportV1,
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
