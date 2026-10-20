//! Chain host configuration and native runtime implementation.
//!
//! `execution_config` remains available without the network host feature.
//! The native host is compiled into this library so entry-point-only changes can
//! reuse its compiled implementation.

#[cfg(not(target_arch = "wasm32"))]
pub mod execution_config;

#[cfg(all(not(target_arch = "wasm32"), feature = "node-libp2p"))]
include!("host.rs");
