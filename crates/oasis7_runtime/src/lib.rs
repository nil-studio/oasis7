//! Authoritative native Runtime implementation and shared agent/body value model.
//! WASM exposes only the value model; execution and recovery remain native.
#[cfg(not(target_arch = "wasm32"))]
include!("native.rs");
#[cfg(target_arch = "wasm32")]
pub mod agent_state;
