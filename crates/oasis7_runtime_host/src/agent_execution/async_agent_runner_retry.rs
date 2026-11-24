use super::*;

impl AsyncAgentRunner {
    /// Retry a provider transport for a turn whose actor outcome is awaiting
    /// Runtime disposition. The semantic request and reduced turn context
    /// must be the exact awaiting values; this method creates the next wire
    /// attempt by changing only `transport_attempt`, keeps the original
    /// [`AsyncTurnId`], and does not re-admit the request to the feedback
    /// single-flight store.
    ///
    /// Callers pass the previously accepted request context (with its current
    /// transport attempt). A successful retry increments that value by one
    /// before dispatch. A fresh observation is accepted for the actor, but it
    /// cannot alter the outer request identity or its Runtime binding.
    pub fn retry_awaiting_turn_with_request_context_and_observation(
        &mut self,
        agent_id: &str,
        observation: Observation,
        context: ContinuousAgentTurnContextV1,
        request_context: ContinuousAgentRequestContextV1,
    ) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        context
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        request_context
            .validate_production_lane()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if request_context.agent_subject != agent_id {
            return Err(AsyncAgentRunnerError::Cognition(
                "retry request subject does not match the actor".to_string(),
            ));
        }
        if observation.agent_id != agent_id {
            return Err(AsyncAgentRunnerError::AgentIdentityMismatch {
                expected: agent_id.to_string(),
                observed: observation.agent_id,
            });
        }

        let Some(&turn_id) = self.awaiting_runtime.get(agent_id) else {
            return Err(AsyncAgentRunnerError::Cognition(
                "retry requires an active awaiting Runtime turn".to_string(),
            ));
        };
        let Some(outcome) = self.awaiting_outcomes.get(&turn_id) else {
            return Err(AsyncAgentRunnerError::Cognition(
                "awaiting Runtime turn identity is unavailable".to_string(),
            ));
        };
        // `poll_completed` moves the lease into the awaiting Runtime outcome.
        // Put that exact immutable lease back on the transport retry so the
        // next authoritative outcome cannot silently lose its admission.
        let cognition_lease = outcome.cognition_lease.clone();
        let expected_context = outcome.prepared_context.clone().ok_or_else(|| {
            AsyncAgentRunnerError::Cognition(
                "retry requires the awaiting turn's cognition context".to_string(),
            )
        })?;
        let expected_request_context =
            outcome.prepared_request_context.clone().ok_or_else(|| {
                AsyncAgentRunnerError::Cognition(
                    "retry requires the awaiting turn's outer request context".to_string(),
                )
            })?;
        if context != expected_context {
            return Err(AsyncAgentRunnerError::Cognition(
                "retry cognition identity does not match the awaiting turn".to_string(),
            ));
        }
        if request_context != expected_request_context {
            return Err(AsyncAgentRunnerError::Cognition(
                "retry request identity does not match the awaiting turn".to_string(),
            ));
        }
        let next_transport_attempt = request_context
            .transport_attempt
            .checked_add(1)
            .ok_or_else(|| {
                AsyncAgentRunnerError::Cognition(
                    "retry transport_attempt cannot be incremented".to_string(),
                )
            })?;
        let mut retry_request_context = request_context;
        retry_request_context.transport_attempt = next_transport_attempt;

        let actor = self
            .actors
            .get(agent_id)
            .ok_or_else(|| AsyncAgentRunnerError::AgentNotRegistered(agent_id.to_string()))?;
        if self.harness.provider_is_still_in_flight(agent_id) {
            return Err(AsyncAgentRunnerError::AgentBusy(agent_id.to_string()));
        }
        let harness_turn_id = *self.harness_turns.get(&turn_id).ok_or_else(|| {
            AsyncAgentRunnerError::Cognition("retry Harness turn identity is unavailable".into())
        })?;
        let key = production_key_from_context(&retry_request_context);
        actor
            .control
            .install_production_input(
                key.clone(),
                observation,
                context,
                retry_request_context.clone(),
            )
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if let Err(error) = self.harness.retry_awaiting_turn(
            harness_turn_id,
            &super::super::agent_api_compat::request_context_to_api(&expected_request_context),
        ) {
            actor.control.clear_production_input(&key);
            return Err(map_harness_error(error).with_agent(agent_id));
        }
        if let Some(metadata) = self.turn_metadata.get_mut(&turn_id) {
            metadata.request_context = Some(retry_request_context);
        }
        if let Some(lease) = cognition_lease {
            self.cognition_leases.insert(turn_id, lease);
        }
        self.active_turns = self.active_turns.saturating_add(1);
        // The caller has already observed this failed/completed outcome and
        // is explicitly retrying it. Remove the stale world-facing queue
        // entry while retaining the awaiting Runtime correlation record.
        self.completed.retain(|outcome| outcome.turn_id != turn_id);
        Ok(turn_id)
    }
}
