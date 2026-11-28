//! Qualified-H protected header signer. State and wire domains are independent
//! from V1/V2 and carry public protocol material only. Backends must provide
//! independently provisioned fencing and anti-rollback guarantees.
mod admission_input_stream;
mod admission_input_transfer_plan;
mod backend;
mod backend_conformance;
mod binding;
mod bootstrap_rotation;
mod client;
mod crypto;
mod first_reservation;
mod fresh_coverage;
mod install;
#[cfg(feature = "protected-bft-luna-key")]
mod luna_key;
#[cfg(feature = "protected-bft-luna-key")]
pub use luna_key::LunaBftSigningKeyV3;
mod membership;
#[cfg(not(target_arch = "wasm32"))]
pub mod postgres_backend;
mod private_retention_alternatives;
mod private_retention_input;
mod private_retention_input_authorization;
mod private_retention_operation;
mod private_retention_ready;
mod rotation;
mod service;
mod state;
mod successor_archive_chain;
mod successor_archive_store;
mod successor_manifest;
mod successor_source;
#[cfg(test)]
mod tests;
mod wire;

pub use successor_source::{
    BFT_SUCCESSOR_WINDOW_ORIGIN_DOMAIN_V1, BftSuccessorWindowSourceV1,
    successor_source_from_binding_v3, validate_successor_source_for_binding_v3,
};

pub use backend::{
    BFT_SIGNER_BACKEND_NAMESPACE_V3, BftFreshCoverageSnapshotV1, BftSignerAcquireOutcomeV3,
    BftSignerBackendSnapshotV3, BftSignerBackendV3,
};
pub use backend_conformance::exercise_disposable_backend_lease_handoff_v3;
pub use binding::{
    BftSignerBindingV3, BftSignerErrorV3, BftSignerKeyIdentityV3, BftSignerTrustV3,
    BftSigningKeyHandleV3,
};
pub use client::{
    BftSignerLeaseV3, BftSignerProviderV3, RemoteBftSignerLeaseV3, RemoteBftSignerProviderV3,
};
pub use service::{
    BftCandidateReadPolicyV3, BftFirstReserveSourceV1, BftFreshCoverageSupportV1,
    BftFreshSourceVerifierV1, BftSignerServiceV3,
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

pub use successor_archive_store::{
    BftSuccessorArchiveStoreV1, BftSuccessorWindowArchiveRefV1, LocalBftSuccessorArchiveStoreV1,
};

pub use successor_manifest::{
    BFT_SUCCESSOR_WINDOW_ARCHIVE_DOMAIN_V1, BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1,
    BFT_SUCCESSOR_WINDOW_MAX_REFS_V1, BFT_SUCCESSOR_WINDOW_MAX_SEGMENTS_V1,
    BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1, BftSuccessorWindowArchiveEntryV1,
    BftSuccessorWindowArchiveKindV1, BftSuccessorWindowArchiveManifestV1,
    BftSuccessorWindowArchiveUsageV1, BftSuccessorWindowCutV1, BftSuccessorWindowPublicIndexV1,
};

pub use private_retention_alternatives::BftOriginalAlternativesInventoryV1;
pub use private_retention_operation::{
    BftOriginalRetentionTxnSnapshotV1, BftRetainedOriginalOperationV1,
};
pub use private_retention_ready::{
    BftCurrentRetentionV1, BftPrivateRetentionSelectionCommitmentV1,
    BftPrivateRetentionSelectionIndexV1, BftPrivateRetentionSelectionKindV1,
    BftPrivateRetentionSelectionOperationV1, BftPrivateRetentionSelectionV1,
};

pub use admission_input_stream::BftAdmissionInputV2StreamStage;
pub use admission_input_transfer_plan::{
    BFT_ADMISSION_INPUT_V2_FRAME_BYTES, BFT_ADMISSION_INPUT_V2_PART_BYTES,
    BFT_ADMISSION_INPUT_V2_PARTS_PER_MESSAGE, BftAdmissionInputMessageV2, BftAdmissionInputPartV2,
    BftAdmissionInputTransferPlanV2,
};

pub use private_retention_input::{
    BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1,
    BFT_PRIVATE_RETENTION_INPUT_MAX_DEPENDENCY_OBJECT_BYTES_V1,
    BftPrivateRetentionAdmissionInputRefV1, BftPrivateRetentionAdmissionInputV1,
    BftPrivateRetentionInputObjectKindV1, BftPrivateRetentionInputObjectV1,
    BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1,
};

pub use fresh_coverage::{
    BftPrivateRetentionActivationV1, BftPrivateRetentionAttestorV1,
    BftPrivateRetentionCreationMarkerV1, BftPrivateRetentionEmptyMapV1, BftPrivateRetentionHeadV1,
    BftPrivateRetentionReceiptBodyV1, BftPrivateRetentionReceiptV1,
};

pub use first_reservation::{
    BftAuthorizedOriginalReserveInputV1, BftFirstReservationPriceV1, BftFirstReserveSnapshotV1,
    BftFirstReservedOriginalV1, BftPrivateRetentionLedgerV1, BftPrivateRetentionMapV1,
};

pub use private_retention_input_authorization::{
    BftPrivateRetentionAdmissionReadGrantBodyV1, BftPrivateRetentionAdmissionReadGrantV1,
    BftPrivateRetentionAdmissionReadPurposeV1,
};
