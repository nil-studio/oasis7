//! Bounded-context domain coordination modules for `oasis7_runtime`.
//!
//! Pure state reducers remain in `oasis7_world_state`; each domain module here
//! coordinates multi-slice staging and produces [`crate::SubsystemDelta`]s installed
//! through [`crate::RootInstallContext`].

pub use crate::world::industrial;
pub mod institutions;
pub mod module_lifecycle;
pub use crate::world::capability_authz;
