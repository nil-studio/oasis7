//! Versioned formal-profile contracts and verification foundation. This module
//! does not activate a runtime, assert physical fault domains or open writes.
mod activation;
mod crypto;
mod frozen;
mod maintenance_encoding;
mod proof;
mod record;
mod recovery;
mod types;
pub use super::fencing;
pub mod coordinator;
pub mod endpoint;
pub mod issuer;
pub mod transport;
pub use activation::*;
pub use frozen::*;
pub use proof::*;
pub use record::*;
pub use recovery::*;
pub use types::*;
#[cfg(test)]
mod tests;

pub mod business_index;
pub mod business_material;
mod business_pack;
mod physical;
pub use business_material::*;
pub use physical::FormalPhysicalReclaimReport;

pub mod business_history;
pub use business_history::*;

pub mod recovery_bootstrap;
pub use recovery_bootstrap::*;

mod durable_set_activation;
mod durable_set_transition;
pub use durable_set_activation::*;
pub use durable_set_transition::*;

mod durable_set_install;
pub use durable_set_install::*;

mod durable_set_bootstrap;
pub use durable_set_bootstrap::*;

mod storage_shadow;
pub use storage_shadow::{
    FormalStorageShadowOptions, FormalStorageShadowProgress, FormalStorageShadowReport,
    advance_formal_storage_shadow, build_formal_storage_shadow,
    build_formal_storage_shadow_optimized, build_formal_storage_shadow_with_options,
};

pub mod native_environment;
pub use native_environment::{
    NativeEnvironmentChunkBytesV1, NativeEnvironmentVerificationProgressV1,
};

pub use oasis7_formal_protocol::native_environment as native_environment_protocol;
