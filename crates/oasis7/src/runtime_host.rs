//! Compatibility façade for native World host assembly. The concrete owner lives in oasis7_runtime_host.
pub use oasis7_runtime_host::{install_runtime_capabilities, world};

pub mod bootstrap;
pub use oasis7_runtime_host::feedback;
pub use oasis7_runtime_host::harness_authority;

/// Composite native host archive extensions.
pub mod archive {
    pub use crate::simulator::persist::*;
}

pub use oasis7_runtime_host::runner;
