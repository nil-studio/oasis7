//! Pure custody foundations. No clocks, randomness, storage or network access.
//! Callers must persist related mutations in one authority transaction.
#![forbid(unsafe_code)]
pub use oasis7_agent_api::game::DecimalU64;
pub mod approval;
pub mod budget;
pub mod envelope;
pub mod http;
pub mod lifecycle;
pub mod operations;
pub mod provisioning;
pub mod recovery;
