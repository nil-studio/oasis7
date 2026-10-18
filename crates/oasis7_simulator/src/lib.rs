//! Deterministic simulation kernel, native state, replay, and queries.
//! Host archives, model execution, Agent runners, and compiler IO are external adapters.
mod geometry {
    pub use oasis7_agent_api::world_action::GeoPos;
    pub use oasis7_agent_api::world_geometry::*;
}
mod agent_query;
mod asteroid_fragment;
mod chain_resource;
mod chunking;
mod effect_trace;
mod frag_spawn;
mod fragment_physics;
mod init;
mod init_module_visual;
mod kernel;
mod kernel_snapshot;
mod memory_entry;
mod module_visual;
mod persist_error;
mod power;
mod scenario;
mod social;
mod source_compiler;
mod types;
mod world_model;
pub use agent_query::*;
pub use asteroid_fragment::*;
pub use chain_resource::*;
pub use chunking::*;
pub use effect_trace::*;
pub use fragment_physics::*;
pub use geometry::GeoPos;
pub use init::*;
pub use kernel::KernelState;
pub use kernel::*;
pub use kernel_snapshot::*;
pub use memory_entry::*;
pub use module_visual::*;
pub use persist_error::*;
pub use power::*;
pub use scenario::*;
pub use social::*;
pub use types::*;
pub use world_model::*;

mod native_resolution;
pub use native_resolution::*;
pub(crate) use oasis7_agent_api::world_quote::canonical_recipe_base_duration_ticks;

#[cfg(test)]
mod tests;
