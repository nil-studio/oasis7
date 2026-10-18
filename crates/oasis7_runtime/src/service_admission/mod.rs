//! Runtime-owned canonical admission and durable task/result validation.
//! Online execution, restore, replay and retained history use the same rules.
pub mod agent_authority;
pub mod agent_chat;
pub mod correlation;
pub mod game_task;
pub(crate) mod retained_queries;
pub mod wire;
pub use correlation::derive_correlation;
use oasis7_client_api::world_service::private_goal as private_goal_prepare;
pub use oasis7_client_api::world_service::*;
pub use wire::*;
