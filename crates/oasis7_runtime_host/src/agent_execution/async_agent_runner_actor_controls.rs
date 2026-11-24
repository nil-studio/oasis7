use super::*;

impl AsyncAgentRunner {
    /// Return whether the actor identity is currently registered. Viewer
    /// recovery uses this read-only check to quarantine durable notifications
    /// for deleted actors instead of retrying them forever.
    pub fn has_agent(&self, agent_id: &str) -> bool {
        self.actors.contains_key(agent_id)
    }

    /// Update the host-owned prompt projection through the same actor mailbox
    /// used for decisions. This is required when Builtin shares the
    /// ProviderBacked production Harness rather than exposing a mutable
    /// behavior handle on the world thread.
    pub fn set_prompt_overrides(
        &mut self,
        agent_id: &str,
        system_prompt: Option<String>,
        short_term_goal: Option<String>,
        long_term_goal: Option<String>,
    ) -> Result<(), AsyncAgentRunnerError> {
        if !self.actors.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentNotRegistered(
                agent_id.to_string(),
            ));
        }
        self.harness
            .notify_behavior_callback(
                agent_id,
                BehaviorCallback::PromptOverrides {
                    system_prompt,
                    short_term_goal,
                    long_term_goal,
                },
            )
            .map_err(map_harness_error)
            .map_err(|error| error.with_agent(agent_id))
    }

    /// Deliver player input to an actor without bypassing the shared Harness
    /// mailbox. The behavior decides how that input enters its bounded
    /// conversation/memory projection.
    pub fn notify_player_message(
        &mut self,
        agent_id: &str,
        world_time: WorldTime,
        message: impl Into<String>,
    ) -> Result<(), AsyncAgentRunnerError> {
        if !self.actors.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentNotRegistered(
                agent_id.to_string(),
            ));
        }
        self.harness
            .notify_behavior_callback(
                agent_id,
                BehaviorCallback::PlayerMessage {
                    world_time,
                    message: message.into(),
                },
            )
            .map_err(map_harness_error)
            .map_err(|error| error.with_feedback_agent(agent_id))
    }
}
