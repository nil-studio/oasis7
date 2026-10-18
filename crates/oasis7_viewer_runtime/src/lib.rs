//! Viewer application, sessions, recovery orchestration and network transports.
pub use oasis7::GeoPos;
pub use oasis7::{geometry, models, simulator};
#[cfg(not(target_arch = "wasm32"))]
pub use oasis7::{observability, runtime, runtime_host, world_service};
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
pub use oasis7::{chain_resource_schema, consensus_action_payload, governance_audit_report};
