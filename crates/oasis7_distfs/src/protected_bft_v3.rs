//! Qualified-H protected header signer. State and wire domains are independent
//! from V1/V2 and carry public protocol material only. Backends must provide
//! independently provisioned fencing and anti-rollback guarantees.
mod backend;
mod binding;
mod client;
mod crypto;
mod install;
mod membership;
mod service;
mod state;
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
pub use service::{BftCandidateReadPolicyV3, BftSignerServiceV3};
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
