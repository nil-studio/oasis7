//! Pure custody foundations. No clocks, randomness, storage or network access.
//! Callers must persist related mutations in one authority transaction.
#![forbid(unsafe_code)]
pub use oasis7_agent_api::game::DecimalU64;
pub mod approval;
pub mod client_enrollment;
pub mod dev_test;
pub mod management_metadata;
pub mod native_binding;
pub mod provisioning;
pub mod source_revocation;
pub mod time_units;
