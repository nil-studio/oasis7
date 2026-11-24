//! Viewer application, sessions, recovery orchestration and network transports.
pub use oasis7::{geometry, models, simulator};
#[cfg(not(target_arch = "wasm32"))]
pub use oasis7::{observability, runtime, runtime_host, world_service};
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

#[cfg(not(target_arch = "wasm32"))]
pub use oasis7::chain_resource_schema;
#[cfg(not(target_arch = "wasm32"))]
pub use oasis7_client_api::governance_audit_report;
#[cfg(not(target_arch = "wasm32"))]
pub use oasis7_world_service::consensus_action_payload;
