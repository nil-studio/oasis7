//! Canonical typed world action contract shared by clients and execution adapters.
mod action;
mod geometry;
mod ledger;
mod main_token;
mod support;
pub use action::*;
pub use geometry::GeoPos;
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
