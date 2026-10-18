//! Compatibility path for pure private goal proof contracts.
pub use oasis7_client_api::world_service::private_goal::{
    SignedGoalPrepareV1, sign_goal_prepare, verify_owner_signature_strict,
};
