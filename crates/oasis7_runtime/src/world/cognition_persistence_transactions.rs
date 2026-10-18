use super::super::cognition_persistence_validation::{
    append_cognition_event, cognition_validation,
};
use super::World;
use crate::cognition_scheduler::{CognitionScheduler, SchedulerWakeV1};
use crate::cognition_wake::AgentContinuation;
use crate::error::WorldError;
use serde_json::json;

impl World {
    pub(in crate::world) fn cognition_commit_scheduler_transaction(
        &mut self,
        scheduler: &CognitionScheduler,
        kind: &str,
        wake: Option<&SchedulerWakeV1>,
    ) -> Result<(), WorldError> {
        let mut next = self.cognition.clone();
        next["scheduler_state"] = scheduler.snapshot_json();
        let mut details = json!({
            "scheduler_policy_digest": scheduler.policy_config_digest(),
            "cursor_seq": scheduler.cursor_seq(),
        });
        if let Some(wake) = wake {
            let object = details
                .as_object_mut()
                .ok_or_else(|| cognition_validation("cognition_event_details_not_object"))?;
            object.extend(
                serde_json::to_value(wake)
                    .map_err(WorldError::from)?
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        append_cognition_event(&mut next, kind, details)?;
        self.cognition = next;
        Ok(())
    }

    pub(in crate::world) fn cognition_commit_continuation_transaction(
        &mut self,
        continuations: &[AgentContinuation],
        scheduler: &CognitionScheduler,
        wake: &SchedulerWakeV1,
    ) -> Result<(), WorldError> {
        let mut next = self.cognition.clone();
        next["continuations"] = serde_json::to_value(continuations).map_err(WorldError::from)?;
        next["scheduler_state"] = scheduler.snapshot_json();
        let continuation = continuations
            .iter()
            .find(|continuation| continuation.continuation_id == wake.continuation_id);
        append_cognition_event(
            &mut next,
            "ContinuationScheduled",
            json!({
                "continuation_id": wake.continuation_id,
                "wake_id": wake.wake_id,
                "world_id": wake.world_id,
                "agent_id": wake.agent_id,
                "agent_session_id": wake.agent_session_id,
                "agent_turn_id": wake.agent_turn_id,
                "decision_request_id": wake.decision_request_id,
                "status": continuation.map(|value| value.status),
                "continuation_status_digest":
                    continuation.and_then(|value| value.continuation_status_digest.clone()),
                "scheduler_policy_digest": scheduler.policy_config_digest(),
                "cursor_seq": scheduler.cursor_seq(),
            }),
        )?;
        self.cognition = next;
        Ok(())
    }

    pub(in crate::world) fn cognition_commit_continuation_lifecycle_transaction(
        &mut self,
        continuations: &[AgentContinuation],
        scheduler: &CognitionScheduler,
        kind: &str,
        continuation: &AgentContinuation,
        deactivated_wake: Option<&SchedulerWakeV1>,
    ) -> Result<(), WorldError> {
        let mut next = self.cognition.clone();
        next["continuations"] = serde_json::to_value(continuations).map_err(WorldError::from)?;
        next["scheduler_state"] = scheduler.snapshot_json();
        let mut details = json!({
            "continuation_id": continuation.continuation_id,
            "wake_id": continuation.wake_id,
            "world_id": continuation.world_id,
            "agent_id": continuation.agent_id,
            "agent_session_id": continuation.agent_session_id,
            "agent_turn_id": continuation.agent_turn_id,
            "decision_request_id": continuation.decision_request_id,
            "status": continuation.status,
            "continuation_status_digest": continuation.continuation_status_digest,
            "scheduler_policy_digest": scheduler.policy_config_digest(),
            "cursor_seq": scheduler.cursor_seq(),
        });
        if let Some(wake) = deactivated_wake {
            details["scheduler_action"] = json!("deactivated");
            details["wake"] = serde_json::to_value(wake).map_err(WorldError::from)?;
        }
        append_cognition_event(&mut next, kind, details)?;
        self.cognition = next;
        Ok(())
    }
}
