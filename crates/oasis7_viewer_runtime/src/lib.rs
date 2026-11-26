//! Viewer application, sessions, recovery orchestration and network transports.
pub(crate) mod geometry {
    pub use oasis7_agent_api::world_action::GeoPos;
    pub use oasis7_agent_api::world_geometry::space_distance_cm;
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use oasis7::runtime_host;
pub(crate) use oasis7::simulator;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use oasis7_runtime as runtime;
#[cfg(test)]
pub(crate) use oasis7_runtime::agent_state as models;
pub use oasis7_simulator::GeoPos;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use oasis7_world_service as world_service;
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
        let origin = super::geometry::GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        };
        let nearby = super::geometry::GeoPos {
            x_cm: 300,
            y_cm: 400,
            z_cm: 0,
        };
        assert_eq!(super::geometry::space_distance_cm(origin, nearby), 500);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_world_service_owner_tests {
    #[test]
    fn viewer_world_service_and_original_gameplay_qualification_use_native_owner() {
        assert_eq!(
            std::any::TypeId::of::<super::world_service::WorldIdentity>(),
            std::any::TypeId::of::<oasis7_client_api::world_service::WorldIdentity>()
        );
        let state = oasis7_runtime::WorldState::default();
        let result = super::world_service::gameplay::resolve_collect_data_collector(
            &state,
            "not-an-owner",
            "not-an-authorized-key",
        );
        assert_eq!(
            result.unwrap_err().code(),
            "collect_data_owner_claim_missing"
        );
    }
}
