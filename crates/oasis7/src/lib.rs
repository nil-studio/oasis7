extern crate self as oasis7;

pub mod capability_invocation_context;
pub mod chain_resource_schema;
pub mod collect_data_auth;
pub mod geometry;
#[cfg(not(target_arch = "wasm32"))]
pub mod governance_audit_report;
pub mod industry_types;
pub mod models;
pub mod network_tier_manifest;
#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_arch = "wasm32"))]
pub mod observability;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime_host;
pub mod simulator;
#[cfg(not(target_arch = "wasm32"))]
pub mod world_service;
#[cfg(not(target_arch = "wasm32"))]
mod world_service_feedback_host;

pub use geometry::{
    DEFAULT_CLOUD_DEPTH_CM, DEFAULT_CLOUD_DEPTH_KM, DEFAULT_CLOUD_HEIGHT_CM,
    DEFAULT_CLOUD_HEIGHT_KM, DEFAULT_CLOUD_WIDTH_CM, DEFAULT_CLOUD_WIDTH_KM, GeoPos, SPACE_UNIT_CM,
    space_distance_cm, space_distance_m,
};
pub use models::{AgentState, BodyKernelView, DEFAULT_AGENT_HEIGHT_CM, RobotBodySpec};
