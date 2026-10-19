//! Reusable chain host configuration, independent of the process entry point.

#[cfg(not(target_arch = "wasm32"))]
pub mod execution_config;
