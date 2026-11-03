use super::*;
#[path = "action_to_event_gameplay_alliances.rs"]
mod action_to_event_gameplay_alliances;
#[path = "action_to_event_gameplay_claims.rs"]
mod action_to_event_gameplay_claims;
#[path = "action_to_event_gameplay_governance.rs"]
mod action_to_event_gameplay_governance;
#[path = "action_to_event_gameplay_war.rs"]
mod action_to_event_gameplay_war;

impl World {
    pub(super) fn action_to_event_gameplay(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        match action {
            Action::ClaimStarterOc { .. }
            | Action::ClaimAgent { .. }
            | Action::ReleaseAgentClaim { .. } => {
                self.action_to_event_gameplay_claims(action_id, action)
            }
            Action::FormAlliance { .. }
            | Action::JoinAlliance { .. }
            | Action::LeaveAlliance { .. }
            | Action::DissolveAlliance { .. } => {
                self.action_to_event_gameplay_alliances(action_id, action)
            }
            Action::DeclareWar { .. } => self.action_to_event_gameplay_war(action_id, action),
            Action::OpenGovernanceProposal { .. }
            | Action::CastGovernanceVote { .. }
            | Action::ResolveCrisis { .. }
            | Action::GrantMetaProgress { .. } => {
                self.action_to_event_gameplay_governance(action_id, action)
            }
            _ => unreachable!("action_to_event_gameplay received unsupported action variant"),
        }
    }
}
