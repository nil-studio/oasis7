//! Versioned formal-profile contracts and verification foundation. This module
//! does not activate a runtime, assert physical fault domains or open writes.
mod maintenance_encoding;
pub use super::fencing;
use oasis7_formal_protocol::controlled_authority::formal::{
    activation, crypto, durable_set_activation, durable_set_bootstrap, durable_set_install,
    durable_set_transition, frozen, proof, record, recovery, types,
};
pub mod coordinator;
pub mod endpoint;
pub mod full_sync_issuer;
pub mod issuer;
pub mod transport;
pub use activation::*;
pub use frozen::*;
pub use full_sync_issuer::DurableFullSyncIssuerV2;
pub use proof::*;
pub use record::*;
pub use recovery::*;
pub use types::*;
#[cfg(test)]
mod tests {
    pub(crate) use oasis7_formal_protocol::controlled_authority::formal::test_fixtures::{
        alternate_recovery_fixture, recovery_fixture,
    };
}

pub use oasis7_formal_protocol::controlled_authority::formal::{
    business_history, business_index, recovery_bootstrap,
};
pub mod business_material;
mod business_pack;
mod physical;
pub use business_material::*;
pub use physical::FormalPhysicalReclaimReport;

pub use business_history::*;

pub use recovery_bootstrap::*;

pub use durable_set_activation::*;
pub use durable_set_transition::*;

pub use durable_set_install::*;

pub use durable_set_bootstrap::*;

mod storage_shadow;
pub use storage_shadow::{
    FormalStorageShadowOptions, FormalStorageShadowProgress, FormalStorageShadowReport,
    FormalStorageShadowSession, advance_formal_storage_shadow,
    advance_formal_storage_shadow_with_session, build_formal_storage_shadow,
    build_formal_storage_shadow_optimized, build_formal_storage_shadow_with_options,
};

pub mod native_environment;
pub use native_environment::{
    NativeEnvironmentChunkBytesV1, NativeEnvironmentVerificationProgressV1,
};

pub use oasis7_formal_protocol::native_environment as native_environment_protocol;

// Bounded physical bytes only; this grants no execution or publication authority.
#[cfg(unix)]
pub use physical::{PhysicalReadWorkV1, read_unknown_length_durable_v1};
