//! Pure custody foundations. No clocks, randomness, storage or network access.
//! Callers must persist related mutations in one authority transaction.
#![forbid(unsafe_code)]
pub use oasis7_agent_api::game::DecimalU64;
pub mod approval;
pub mod approval_admission;
pub mod budget;
pub mod envelope;
pub mod http;
pub mod lifecycle;
pub mod operations;
pub mod provisioning;
pub mod recovery;

pub mod source_revocation;

pub mod role_key_slot;

pub mod native_binding;

pub mod native_history;

pub mod http_pop;
pub mod time_units;

pub mod client_enrollment;

/// Opt-in bounded diagnostics; never an authorization or signature-result cache.
#[cfg(feature = "signature-metrics")]
pub use oasis7_client_api::world_service::private_goal::signature_metrics;
