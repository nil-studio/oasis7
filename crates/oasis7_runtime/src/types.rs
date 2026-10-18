//! Type aliases and basic type definitions for the runtime module.

pub use oasis7_agent_api::world_action::MaterialLedgerId;

pub type WorldTime = u64;
pub type WorldEventId = u64;
pub type ActionId = u64;
pub type IntentSeq = u64;
pub type ProposalId = u64;
pub type PatchPath = Vec<String>;
