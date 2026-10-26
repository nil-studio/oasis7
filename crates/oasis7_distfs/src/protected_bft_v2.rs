//! Protected signer adapter for dynamic-set BFT V2.
//!
//! V2 has a separate schema and signature domain from the fixed-set V1
//! adapter. It never upgrades, resets, or interprets V1 signer state.

mod backend;
mod binding;
mod client;
mod crypto;
mod install;
mod service;
mod state;
mod wire;

#[cfg(all(test, unix))]
mod tests;

pub use backend::{BftSignerAcquireOutcomeV2, BftSignerBackendSnapshotV2, BftSignerBackendV2};
pub use binding::{
    BftSignerBindingV2, BftSignerErrorV2, BftSignerKeyIdentityV2, BftSignerTrustV2,
    BftSigningKeyHandleV2,
};
pub use client::{
    BftSignerLeaseV2, BftSignerProviderV2, RemoteBftSignerLeaseV2, RemoteBftSignerProviderV2,
};
pub use install::{
    BftLocalInstallEvidenceV2, BftSignedTransitionInstallAckV2, BftTransitionInstallIntentBodyV2,
    BftTransitionInstallIntentV2, BftTransitionInstallPreparationV2,
    BftTransitionInstallReceiptBodyV2, BftTransitionInstallReceiptV2, BftTransitionInstallResultV2,
};
pub use service::BftSignerServiceV2;
pub use state::{BftSignerCandidateV2, BftSignerCheckpointV2, BftSignerStateV2};
