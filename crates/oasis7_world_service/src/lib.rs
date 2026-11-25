//! Authenticated topology-independent service wire and adapters.
pub use oasis7_runtime::service_admission::{
    agent_authority, agent_chat, game_task, participant, starter, wire,
};
pub mod authority {
    pub use oasis7_client_api::world_service::{
        request_digest, sign_read_request, sign_service_response, signing_bytes,
        verify_read_request, verify_service_response,
    };
}
// Client implementation is owned by oasis7_game_client; integrated below.
pub mod client;
#[cfg(not(target_arch = "wasm32"))]
pub mod consensus_action_payload;
pub mod correlation;
pub use oasis7_runtime::cognition_disposition::{
    AuthenticatedCognitionDisposition, ValidatedStaleCognitionRejection,
};
pub mod formal_bft_header_read_service;
pub mod formal_bft_prefix_read_service;
pub mod formal_bft_read_service;
pub mod formal_projection;
mod formal_projection_header;
pub use oasis7_world_projection::formal_read;
#[cfg(test)]
mod formal_read_adapter_tests;
pub mod formal_read_error;
#[cfg(test)]
pub(crate) mod formal_read_fixtures;
pub mod formal_read_service;
pub mod formal_recovery_lineage;
pub mod formal_result;
pub mod gameplay;
pub mod membership;
#[cfg(not(target_arch = "wasm32"))]
pub mod module_source_compiler;
pub mod private_goal_prepare {
    pub use oasis7_client_api::world_service::private_goal::{
        SignedGoalPrepareV1, sign_goal_prepare, verify_owner_signature_strict,
    };
}
#[cfg(not(target_arch = "wasm32"))]
pub mod private_goal_store;
pub mod projection;
#[cfg(not(target_arch = "wasm32"))]
pub mod provider_context;
pub mod provider_material;
pub mod provider_material_codec;
pub mod verified_view {
    pub use oasis7_game_client::verified_world_view::{
        CONTROLLED_PREREQUISITE_READ_SCOPE, ReadEvidencePolicy,
    };
    pub use oasis7_world_projection::VerifiedWorldView;
}
pub use authority::*;
pub use correlation::derive_correlation;
pub use oasis7_client_api::world_service::*;
pub use wire::*;

#[cfg(not(target_arch = "wasm32"))]
pub use oasis7_client_api::world_service::private_goal_read;

#[cfg(not(target_arch = "wasm32"))]
pub mod private_task_context;

#[cfg(not(target_arch = "wasm32"))]
pub mod private_key_file {
    pub use oasis7_client_storage::private_key_file::read_private_key_file;
}

#[cfg(not(target_arch = "wasm32"))]
pub mod task_factory_quote;
#[cfg(not(target_arch = "wasm32"))]
pub mod task_recipe_quote;

#[cfg(test)]
mod game_client_conformance;

#[cfg(test)]
pub(crate) mod formal_read_response_fixture;

#[cfg(not(target_arch = "wasm32"))]
pub mod ordinary_starter;

#[cfg(test)]
mod task_cleanup_v2_formal_projection_tests;
pub mod task_cleanup_v2_observation;
