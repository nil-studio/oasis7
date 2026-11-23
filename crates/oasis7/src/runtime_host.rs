//! Compatibility façade for native World host assembly. The concrete owner lives in oasis7_runtime_host.
pub use oasis7_runtime_host::{install_runtime_capabilities, world};

#[cfg(test)]
mod source_compiler_tests;

#[cfg(test)]
mod agent_chat_projection_tests;
#[cfg(test)]
mod game_task_projection_tests;

pub mod provider_context;

pub mod bootstrap;
pub use oasis7_runtime_host::gameplay_actions;
pub mod harness_authority;
pub mod ordinary_starter;

pub mod feedback;

/// Composite native host archive extensions.
pub mod archive {
    pub use crate::simulator::persist::*;
}

pub mod runner;
