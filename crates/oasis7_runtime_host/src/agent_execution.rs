//! Native owner of archive-bearing action results and agent execution contracts.
use crate::simulator_archive::archive::WorldEvent;
use oasis7_simulator::{Action, ActionId, RejectReason, WorldEventKind};
use serde::{Deserialize, Serialize};

/// Result of an action execution, providing feedback to the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResult {
    /// The action that was executed.
    pub action: Action,
    /// The action ID assigned by the kernel.
    pub action_id: ActionId,
    /// Whether the action succeeded.
    pub success: bool,
    /// The resulting event (success or rejection).
    pub event: WorldEvent,
}

impl ActionResult {
    /// Returns true if the action was rejected.
    pub fn is_rejected(&self) -> bool {
        matches!(self.event.kind, WorldEventKind::ActionRejected { .. })
    }

    /// Returns the rejection reason if the action was rejected.
    pub fn reject_reason(&self) -> Option<&RejectReason> {
        match &self.event.kind {
            WorldEventKind::ActionRejected { reason } => Some(reason),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_result_helpers() {
        let action = Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-2".to_string(),
        };

        let success_event = WorldEvent {
            id: 1,
            time: 1,
            kind: WorldEventKind::AgentMoved {
                agent_id: "agent-1".to_string(),
                from: "loc-1".to_string(),
                to: "loc-2".to_string(),
                distance_cm: 1000,
                electricity_cost: 1,
            },
            runtime_event: None,
        };
        let success_result = ActionResult {
            action: action.clone(),
            action_id: 1,
            success: true,
            event: success_event,
        };
        assert!(!success_result.is_rejected());
        assert!(success_result.reject_reason().is_none());

        let reject_event = WorldEvent {
            id: 2,
            time: 2,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::AgentNotFound {
                    agent_id: "agent-1".to_string(),
                },
            },
            runtime_event: None,
        };
        let reject_result = ActionResult {
            action,
            action_id: 2,
            success: false,
            event: reject_event,
        };
        assert!(reject_result.is_rejected());
        assert!(matches!(
            reject_result.reject_reason(),
            Some(RejectReason::AgentNotFound { .. })
        ));
    }
}
