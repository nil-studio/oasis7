extern crate self as oasis7;

pub mod geometry;
pub mod industry_types;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use oasis7_runtime as runtime;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime_host;
pub mod simulator;
#[cfg(not(target_arch = "wasm32"))]
pub mod world_service;
#[cfg(not(target_arch = "wasm32"))]
mod world_service_feedback_host;

pub use geometry::GeoPos;
pub use oasis7_runtime::agent_state::AgentState;
