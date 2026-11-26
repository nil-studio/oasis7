//! Viewer application, sessions, recovery orchestration and network transports.
pub(crate) mod geometry {
    pub use oasis7_agent_api::world_action::GeoPos;
    pub use oasis7_agent_api::world_geometry::{
        DEFAULT_CLOUD_DEPTH_CM, DEFAULT_CLOUD_DEPTH_KM, DEFAULT_CLOUD_HEIGHT_CM,
        DEFAULT_CLOUD_HEIGHT_KM, DEFAULT_CLOUD_WIDTH_CM, DEFAULT_CLOUD_WIDTH_KM, SPACE_UNIT_CM,
        space_distance_cm, space_distance_m,
    };
}
pub(crate) use oasis7::simulator;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use oasis7::{runtime_host, world_service};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use oasis7_runtime as runtime;
#[cfg(test)]
pub(crate) use oasis7_runtime::agent_state as models;
pub use oasis7_simulator::GeoPos;
#[cfg(not(target_arch = "wasm32"))]
mod game_read_api;
#[cfg(not(target_arch = "wasm32"))]
mod server;
pub mod viewer;
#[cfg(not(target_arch = "wasm32"))]
mod web_bridge;

#[cfg(not(target_arch = "wasm32"))]
pub use server::{ViewerServer, ViewerServerConfig, ViewerServerError};
#[cfg(not(target_arch = "wasm32"))]
pub use web_bridge::{ViewerWebBridge, ViewerWebBridgeConfig, ViewerWebBridgeError};

#[cfg(test)]
mod native_geometry_owner_tests {
    #[test]
    fn viewer_geometry_types_and_units_are_the_original_l0_contract() {
        assert_eq!(
            std::any::TypeId::of::<super::geometry::GeoPos>(),
            std::any::TypeId::of::<oasis7_agent_api::world_action::GeoPos>()
        );
        assert_eq!(
            super::geometry::SPACE_UNIT_CM,
            oasis7_agent_api::world_geometry::SPACE_UNIT_CM
        );
        assert_eq!(
            super::geometry::DEFAULT_CLOUD_WIDTH_CM,
            oasis7_agent_api::world_geometry::DEFAULT_CLOUD_WIDTH_CM
        );
    }
}
