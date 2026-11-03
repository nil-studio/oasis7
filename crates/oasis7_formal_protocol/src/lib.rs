//! Pure formal-profile contracts and cryptographic verification; no storage services.
pub mod bft;
pub mod controlled_authority;
pub mod incremental_value;

pub mod snapshot_delta;

pub mod native_environment;

pub mod incremental_execution;
pub mod incremental_record_root;
pub mod incremental_state;
mod merkle;

pub mod incremental_artifacts;
pub mod incremental_history;
pub mod incremental_schedule;

pub mod incremental_material;
