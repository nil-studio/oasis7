//! Authoritative native Runtime implementation and shared agent/body value model.
//! WASM exposes only the value model; execution and recovery remain native.
#[cfg(not(target_arch = "wasm32"))]
include!("native.rs");
#[cfg(target_arch = "wasm32")]
pub use oasis7_world_state::agent_state;

// Closed DATA layout inventory shared with the upper consensus payload codec.
// This exports no executor, Current qualification or constructor.
pub use oasis7_world_state::incremental_pending_action_family::{
    PendingActionOwnedLayoutV1, pending_action_owned_decode_layout_v1,
};

#[cfg(not(target_arch = "wasm32"))]
pub use world::{decode_provider_bootstrap_data_v1, provider_bootstrap_owned_layout_v1};

#[cfg(not(target_arch = "wasm32"))]
pub use world::PaidProjectionRetainedOriginalReaderV1;
