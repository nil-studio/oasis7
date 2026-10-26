//! Protected signer client and provider boundary for the fixed-set BFT engine.
//!
//! A provider implementation must meet the transactional backend contract; this
//! module does not claim that a signature or local database is nonrollback.

mod backend;
mod binding;
mod client;
mod service;
mod validation;
mod wire;

pub use backend::{
    BftSignerBackendSnapshotV1, BftSignerBackendV1, bft_signer_genesis_state_v1,
    bft_signer_history_bound_v1,
};
pub use binding::{BftSignerBindingV1, BftSignerError, BftSignerTrustV1, BftSigningKeyHandleV1};
pub use client::{BftSignerLeaseV1, BftSignerProviderV1, RemoteBftSignerProviderV1};
pub use service::BftSignerServiceV1;
pub use validation::{
    BFT_SIGNER_MAX_FRAME_BYTES_V1, BFT_SIGNER_MAX_HISTORY_BYTES_V1,
    BFT_SIGNER_MAX_JOURNAL_BYTES_V1, BftProtectedCheckpointV1, BftSignerCandidateV1,
    BftSignerStateV1,
};

#[cfg(all(test, unix))]
mod tests;
