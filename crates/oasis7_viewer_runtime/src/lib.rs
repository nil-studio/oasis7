//! Viewer application, sessions, recovery orchestration and network transports.
pub use oasis7::{geometry, models, simulator};
#[cfg(not(target_arch = "wasm32"))]
pub use oasis7::{runtime, runtime_host, world_service};
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
