extern crate self as oasis7;

pub mod geometry {
    pub use oasis7_agent_api::world_action::GeoPos;
    pub use oasis7_agent_api::world_geometry::{
        DEFAULT_CLOUD_DEPTH_CM, DEFAULT_CLOUD_DEPTH_KM, DEFAULT_CLOUD_HEIGHT_CM,
        DEFAULT_CLOUD_HEIGHT_KM, DEFAULT_CLOUD_WIDTH_CM, DEFAULT_CLOUD_WIDTH_KM, SPACE_UNIT_CM,
        space_distance_cm, space_distance_m,
    };
}
pub mod industry_types {
    pub use oasis7_agent_api::world_industry::StarterIndustrialSettlementSummaryV1;
    pub use oasis7_domain_types::MaterialLedgerId;
}
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) use oasis7_runtime as runtime;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime_host;
pub mod simulator;
#[cfg(not(target_arch = "wasm32"))]
pub mod world_service;

pub use geometry::GeoPos;
pub use oasis7_runtime::agent_state::AgentState;
