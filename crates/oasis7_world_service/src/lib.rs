//! Authenticated topology-independent service wire and adapters.
pub mod agent_authority;
pub mod agent_chat;
pub mod authority;
// Client implementation is owned by oasis7_game_client; integrated below.
pub mod client;
mod cognition_disposition;
pub mod correlation;
pub use cognition_disposition::{
    AuthenticatedCognitionDisposition, ValidatedStaleCognitionRejection,
};
mod formal_governance;
pub mod formal_projection;
mod formal_projection_header;
pub mod formal_read;
#[cfg(test)]
pub(crate) mod formal_read_fixtures;
pub mod formal_read_service;
pub mod formal_recovery_lineage;
pub mod formal_result;
pub mod game_task;
pub mod gameplay;
pub mod membership;
pub mod private_goal_prepare;
#[cfg(not(target_arch = "wasm32"))]
pub mod private_goal_store;
pub mod projection;
pub mod verified_view;
pub mod wire;
pub use authority::*;
pub use correlation::derive_correlation;
pub use oasis7_client_api::world_service::*;
pub use wire::*;

#[cfg(not(target_arch = "wasm32"))]
pub mod private_goal_read;

#[cfg(not(target_arch = "wasm32"))]
pub mod private_task_context;

#[cfg(not(target_arch = "wasm32"))]
pub mod private_key_file;

#[cfg(not(target_arch = "wasm32"))]
pub mod task_factory_quote;
#[cfg(not(target_arch = "wasm32"))]
pub mod task_recipe_quote;

#[cfg(test)]
mod game_client_conformance;

#[cfg(test)]
pub(crate) mod formal_read_response_fixture;
