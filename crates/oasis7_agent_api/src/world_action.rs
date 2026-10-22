//! Canonical typed world action contract shared by clients and execution adapters.
mod action;
mod geometry;
mod governance_authorization;
mod ledger;
mod main_token;
mod support;
pub use action::*;
pub use geometry::GeoPos;
pub use governance_authorization::*;
pub use ledger::MaterialLedgerId;
pub use main_token::*;
pub use support::*;
type ProposalId = u64;
type WorldTime = u64;

/// Whether an action requires the existing main-token authorization path.
pub fn main_token_action_auth_required(action: &Action) -> bool {
    matches!(
        action,
        Action::TransferMainToken { .. }
            | Action::ClaimMainTokenVesting { .. }
            | Action::InitializeMainTokenGenesis { .. }
            | Action::DistributeMainTokenTreasury { .. }
            | Action::TopUpRestrictedStarterClaimLiveopsPool { .. }
            | Action::UpdateRestrictedStarterClaimAdminRegistry { .. }
    )
}

mod world_rule;
pub use world_rule::*;

mod world_rule_organization;
pub use world_rule_organization::*;

mod world_rule_supply;
pub use world_rule_supply::*;

mod world_rule_minimum_service;
pub use world_rule_minimum_service::*;

mod industrial_work;
pub use industrial_work::*;

mod industrial_work_command;
pub use industrial_work_command::*;
mod recipe_substitution;
pub use recipe_substitution::*;
