//! Workers implementation of dev-test-accounts-v1. World authority is remote.
#![forbid(unsafe_code)]
#[cfg(any(target_arch = "wasm32", test))]
mod config;
#[cfg(target_arch = "wasm32")]
mod platform;
#[cfg(any(target_arch = "wasm32", test))]
mod protocol;
#[cfg(any(target_arch = "wasm32", test))]
mod state;
#[cfg(target_arch = "wasm32")]
mod store;

#[cfg(target_arch = "wasm32")]
mod cache;
#[cfg(target_arch = "wasm32")]
mod native;

#[cfg(target_arch = "wasm32")]
mod operations;

#[cfg(test)]
mod config_tests;
#[cfg(test)]
mod protocol_tests;
