//! Pure formal-profile contracts and cryptographic verification; no storage services.
pub mod bft;
pub mod controlled_authority;

pub mod snapshot_delta;

pub mod native_environment;
mod merkle;
pub mod incremental_state;
pub mod incremental_value;
pub mod incremental_execution;
