//! Atomic local projection of an authenticated canonical final budget debit.
use super::{AsyncAgentRunner, AsyncAgentRunnerError};
use crate::runtime::{AgentContinuation, ContinuationStatusV1};
use crate::simulator::{ContinuationAuthorityContextV1, ContinuationProposalV1};

impl AsyncAgentRunner {
    #[cfg(not(target_arch = "wasm32"))]
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the Agent, signed proposal, canonical predecessor/terminal, authority and restore flag explicit at this authenticated transaction boundary"
    )]
    pub fn with_completed_final_budget_cleanup<F>(
        &mut self,
        agent: &str,
        proposal: &ContinuationProposalV1,
        predecessor: &AgentContinuation,
        terminal: AgentContinuation,
        authority: &ContinuationAuthorityContextV1,
        restored: bool,
        persist: F,
    ) -> Result<(), AsyncAgentRunnerError>
    where
        F: FnOnce() -> Result<(), String>,
    {
        self.completed_final_budget_cleanup(
            agent,
            proposal,
            predecessor,
            terminal,
            authority,
            restored,
            |_| persist(),
        )
    }
    #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test_tier_required")))]
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the Agent, signed proposal, canonical predecessor/terminal, authority and restore flag explicit at this authenticated transaction boundary"
    )]
    pub fn with_completed_final_budget_cleanup_observed<F>(
        &mut self,
        agent: &str,
        proposal: &ContinuationProposalV1,
        predecessor: &AgentContinuation,
        terminal: AgentContinuation,
        authority: &ContinuationAuthorityContextV1,
        restored: bool,
        persist: F,
    ) -> Result<(), AsyncAgentRunnerError>
    where
        F: FnOnce([String; 5]) -> Result<(), String>,
    {
        self.completed_final_budget_cleanup(
            agent,
            proposal,
            predecessor,
            terminal,
            authority,
            restored,
            |native| persist(native.rejected_wait_test_ledger_digests()),
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the Agent, signed proposal, canonical predecessor/terminal, authority and restore flag explicit at this authenticated transaction boundary"
    )]
    fn completed_final_budget_cleanup<F>(
        &mut self,
        agent: &str,
        proposal: &ContinuationProposalV1,
        predecessor: &AgentContinuation,
        terminal: AgentContinuation,
        authority: &ContinuationAuthorityContextV1,
        restored: bool,
        persist: F,
    ) -> Result<(), AsyncAgentRunnerError>
    where
        F: FnOnce(&Self) -> Result<(), String>,
    {
        let error = |message: &str| AsyncAgentRunnerError::Cognition(message.to_owned());
        proposal.validate().map_err(|e| error(&e.to_string()))?;
        authority
            .validate_proposal(proposal)
            .map_err(|e| error(&e.to_string()))?;
        predecessor
            .validate_authoritative()
            .map_err(|e| error(&e.to_string()))?;
        terminal
            .validate_authoritative()
            .map_err(|e| error(&e.to_string()))?;
        if proposal.agent_id != agent
            || predecessor.agent_id != agent
            || proposal.continuation_proposal_id != predecessor.continuation_proposal_id
            || proposal.agent_session_id != predecessor.agent_session_id
            || proposal.agent_turn_id != predecessor.agent_turn_id
            || proposal.decision_request_id != predecessor.decision_request_id
            || proposal.origin_turn_id != predecessor.origin_turn_id
            || proposal.origin_request_digest != predecessor.origin_request_digest
            || proposal.remaining_budget.value != 1
            || predecessor.remaining_budget.value != 1
            || proposal.remaining_budget.unit != predecessor.remaining_budget.unit
            || terminal.remaining_budget.unit != predecessor.remaining_budget.unit
            || terminal.remaining_budget.value != 0
            || terminal.status != ContinuationStatusV1::Completed
            || terminal.terminal_disposition.as_deref() != Some("budget_exhausted")
            || terminal.logical_tick < predecessor.logical_tick
            || !matches!(
                predecessor.status,
                ContinuationStatusV1::Scheduled
                    | ContinuationStatusV1::Pending
                    | ContinuationStatusV1::Waking
            )
        {
            return Err(error("final budget original or terminal delta mismatch"));
        }
        let mut expected = predecessor.clone();
        expected.remaining_budget = terminal.remaining_budget.clone();
        expected.status = terminal.status;
        expected.logical_tick = terminal.logical_tick;
        expected.continuation_status_digest = terminal.continuation_status_digest.clone();
        expected.terminal_disposition = terminal.terminal_disposition.clone();
        if expected != terminal {
            return Err(error("final budget changed unrelated canonical identity"));
        }
        let mut bound: crate::runtime::CognitionContinuationProposalV1 = serde_json::from_value(
            serde_json::to_value(proposal).map_err(|e| error(&e.to_string()))?,
        )
        .map_err(|e| error(&e.to_string()))?;
        bound.proposal_digest = bound.proposal_digest();
        if bound.proposal_digest != predecessor.proposal_digest
            || bound.world_id != predecessor.world_id
            || bound.action_or_envelope_digest != predecessor.action_or_envelope_digest
            || bound.wake_conditions != predecessor.wake_conditions
            || bound.valid_until_tick != predecessor.valid_until_tick
            || bound.precondition_digest != predecessor.precondition_digest
        {
            return Err(error("final budget full signed proposal identity mismatch"));
        }
        self.actors
            .get(agent)
            .ok_or_else(|| AsyncAgentRunnerError::AgentNotRegistered(agent.into()))?;
        if self.harness.provider_is_still_in_flight(agent)
            || self.awaiting_runtime.contains_key(agent)
            || self
                .awaiting_outcomes
                .values()
                .any(|outcome| outcome.agent_id == agent)
        {
            return Err(error(
                "final budget cannot retire an active or awaiting provider turn",
            ));
        }
        let harness = self.continuation_harness.clone();
        let continuations = self.continuations.clone();
        let awaiting = self.awaiting_runtime.clone();
        let outcomes = self.awaiting_outcomes.clone();
        let result = (|| {
            if !self.continuations.contains_key(agent) {
                if !restored {
                    return Err(error("final budget original local continuation missing"));
                }
                self.hydrate_runtime_continuation_with_authority(
                    agent,
                    proposal.clone(),
                    authority,
                    predecessor.clone(),
                )?;
            }
            let existing = self
                .continuations
                .get(agent)
                .ok_or_else(|| error("final budget local handle missing"))?;
            if existing.continuation_id != predecessor.continuation_id
                || existing.wake_id != predecessor.wake_id
                || existing.wake_seq != predecessor.wake_seq
                || existing.remaining_budget.unit != predecessor.remaining_budget.unit
                || existing.remaining_budget.value != predecessor.remaining_budget.value
            {
                return Err(error(
                    "final budget changed local continuation, wake, or budget",
                ));
            }
            self.validate_active_continuation_with_authority(agent, authority, predecessor)?;
            self.apply_runtime_terminal_continuation_projection(agent, terminal, authority)?;
            persist(self).map_err(|e| error(&e))
        })();
        if result.is_err() {
            self.continuation_harness = harness;
            self.continuations = continuations;
            self.awaiting_runtime = awaiting;
            self.awaiting_outcomes = outcomes;
        }
        result
    }
}
