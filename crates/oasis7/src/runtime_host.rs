//! Compatibility façade for native World host assembly. The concrete owner lives in oasis7_runtime_host.
pub use oasis7_runtime_host::{install_runtime_capabilities, world};

#[cfg(test)]
mod source_compiler_tests;

#[cfg(test)]
mod agent_chat_projection_tests;
#[cfg(test)]
mod game_task_projection_tests;

pub mod bootstrap;
pub use oasis7_runtime_host::harness_authority;
#[cfg(test)]
mod ordinary_starter {
    pub use oasis7_world_service::ordinary_starter::*;

    // Preserve the original signed-claim and publication integration tests here:
    // these intentionally require the full legacy application host assembly.
    mod legacy_claim_tests;
}

pub use oasis7_runtime_host::feedback;

/// Composite native host archive extensions.
pub mod archive {
    pub use crate::simulator::persist::*;
}

pub use oasis7_runtime_host::runner;
