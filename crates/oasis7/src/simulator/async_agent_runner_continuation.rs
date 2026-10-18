//! Async runner integration for strict continuation admission and wake handoff.

use crate::runtime::AgentContinuation as RuntimeAgentContinuation;
use crate::simulator::cognition_policy::{
    ContinuationAuthorityContextV1, ContinuationCurrentContextV1, ContinuationHandle,
    ContinuationInvalidationReason, ContinuationProposalV1, RuntimeContinuationStatusV1,
};
use crate::simulator::continuous_agent_harness::{
    ContinuousAgentRequestContextV1, ContinuousAgentTurnContextV1,
};
use oasis7_simulator::Observation;

use super::{AsyncAgentRunner, AsyncAgentRunnerError, AsyncTurnId};

impl AsyncAgentRunner {
    /// Opaque same-process test witnesses, never canonical or restart evidence.
    /// Order: Harness, continuations, awaiting runtime, awaiting outcomes, feedback.
    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn rejected_wait_test_ledger_digests(&self) -> [String; 5] {
        fn digest(domain: &str, value: &impl std::fmt::Debug) -> String {
            let mut hash = blake3::Hasher::new();
            hash.update(b"oasis7.rejected-wait.local-test-ledger.v1\0");
            hash.update(domain.as_bytes());
            hash.update(b"\0");
            hash.update(format!("{value:?}").as_bytes());
            hash.finalize().to_hex().to_string()
        }
        [
            digest("harness", &self.continuation_harness),
            digest("continuations", &self.continuations),
            digest("awaiting-runtime", &self.awaiting_runtime),
            digest("awaiting-outcomes", &self.awaiting_outcomes),
            digest(
                "feedback-store",
                &self.harness.cognition_state_digest_for_test(),
            ),
        ]
    }

    /// Local absence only; this does not authenticate restored rejection or settlement.
    pub fn rejected_unprojected_wait_is_absent(
        &self,
        agent_id: &str,
        proposal: &ContinuationProposalV1,
        original: &ContinuousAgentRequestContextV1,
    ) -> Result<bool, AsyncAgentRunnerError> {
        original
            .validate_production_lane()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        proposal
            .validate()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if proposal.agent_id != agent_id
            || original.agent_subject != agent_id
            || proposal.world_id != original.runtime_binding.world_id
            || proposal.origin_turn_id != original.agent_turn_id
            || proposal.agent_session_id != original.agent_session_id
            || proposal.agent_turn_id != original.agent_turn_id
            || proposal.decision_request_id != original.decision_request_id
            || proposal.origin_request_digest != original.request_digest.to_string()
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "rejected Wait absence identity mismatch".to_string(),
            ));
        }
        Ok(!self.continuations.contains_key(agent_id)
            && !self.awaiting_runtime.contains_key(agent_id)
            && !self.awaiting_outcomes.values().any(|outcome| {
                outcome.agent_id == agent_id
                    || outcome.prepared_request_context.as_ref() == Some(original)
            }))
    }
    /// Resume preserves the predecessor's origin while binding the new turn separately.
    /// This witnesses local absence only; the caller authenticates canonical rejection.
    pub fn rejected_unprojected_resume_is_absent(
        &self,
        agent_id: &str,
        proposal: &ContinuationProposalV1,
        request: &ContinuousAgentRequestContextV1,
        turn: &ContinuousAgentTurnContextV1,
    ) -> Result<bool, AsyncAgentRunnerError> {
        request
            .validate_production_lane()
            .map_err(|e| AsyncAgentRunnerError::Cognition(e.to_string()))?;
        turn.validate_for_agent(agent_id)
            .map_err(|e| AsyncAgentRunnerError::Cognition(e.to_string()))?;
        proposal
            .validate()
            .map_err(|e| AsyncAgentRunnerError::Cognition(e.to_string()))?;
        if request.agent_subject != agent_id
            || proposal.agent_id != agent_id
            || proposal.world_id != request.runtime_binding.world_id
            || proposal.agent_session_id != request.agent_session_id
            || proposal.agent_turn_id != request.agent_turn_id
            || proposal.decision_request_id != request.decision_request_id
            || turn.agent_session_id != request.agent_session_id
            || turn.agent_turn_id != request.agent_turn_id
            || turn.decision_request_id != request.decision_request_id
            || turn.request_digest != request.request_digest
            || turn.continuation.as_ref() != Some(proposal)
            || crate::simulator::h_v1("oasis7.cognition.continuation.v1", &turn.continuation)
                != request.continuation_digest
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "rejected Resume absence identity mismatch".into(),
            ));
        }
        if !self.actors.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentNotRegistered(
                agent_id.to_string(),
            ));
        }
        Ok(!self.harness.provider_is_still_in_flight(agent_id)
            && !self.continuations.contains_key(agent_id)
            && !self.awaiting_runtime.contains_key(agent_id)
            && !self.awaiting_outcomes.values().any(|outcome| {
                outcome.agent_id == agent_id
                    || outcome.prepared_request_context.as_ref() == Some(request)
            }))
    }

    /// Retire only the pristine local Wait whose canonical admission was rejected.
    /// The caller must authenticate rejection/settlement before this local transaction.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_rejected_unprojected_wait_cleanup<F>(
        &mut self,
        agent_id: &str,
        proposal: &ContinuationProposalV1,
        current: &ContinuationCurrentContextV1,
        original: &ContinuousAgentRequestContextV1,
        persist: F,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError>
    where
        F: FnOnce(&ContinuationHandle) -> Result<(), String>,
    {
        self.rejected_unprojected_wait_cleanup_core(
            agent_id,
            proposal,
            current,
            original,
            |_, handle| persist(handle),
        )
    }

    /// Inspect staged local ledgers immediately before the sole persistence callback.
    /// Only opaque hashes leave the runner; actors are not exposed or replaced.
    #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test_tier_required")))]
    pub fn with_rejected_unprojected_wait_cleanup_observed<F>(
        &mut self,
        agent_id: &str,
        proposal: &ContinuationProposalV1,
        current: &ContinuationCurrentContextV1,
        original: &ContinuousAgentRequestContextV1,
        persist: F,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError>
    where
        F: FnOnce(&ContinuationHandle, [String; 5]) -> Result<(), String>,
    {
        self.rejected_unprojected_wait_cleanup_core(
            agent_id,
            proposal,
            current,
            original,
            |runner, handle| persist(handle, runner.rejected_wait_test_ledger_digests()),
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn rejected_unprojected_wait_cleanup_core<F>(
        &mut self,
        agent_id: &str,
        proposal: &ContinuationProposalV1,
        current: &ContinuationCurrentContextV1,
        original: &ContinuousAgentRequestContextV1,
        persist: F,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError>
    where
        F: FnOnce(&Self, &ContinuationHandle) -> Result<(), String>,
    {
        let invalid = || {
            AsyncAgentRunnerError::Cognition(
                "rejected Wait cleanup identity or pristine state mismatch".to_string(),
            )
        };
        current
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        current
            .authority
            .validate_proposal(proposal)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        proposal
            .validate()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        original
            .validate_production_lane()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        let handle = self.continuations.get(agent_id).ok_or_else(invalid)?;
        if handle.proposal != *proposal
            || proposal.agent_id != agent_id
            || original.agent_subject != agent_id
            || proposal.world_id != original.runtime_binding.world_id
            || proposal.origin_turn_id != original.agent_turn_id
            || proposal.agent_session_id != original.agent_session_id
            || proposal.agent_turn_id != original.agent_turn_id
            || proposal.decision_request_id != original.decision_request_id
            || proposal.origin_request_digest != original.request_digest.to_string()
            || !handle.active
            || handle.status != "scheduled"
            || handle.provenance != "harness_policy"
            || !handle.continuation_id.is_empty()
            || !handle.wake_id.is_empty()
            || !handle.continuation_digest.is_empty()
            || !handle.continuation_status_digest.is_empty()
            || handle.wake_seq != 0
            || handle.world_effect
            || handle.provider_invocation_count != 0
            || handle.consumed_budget != 0
            || handle.remaining_budget != proposal.remaining_budget
            || handle.terminal_disposition.is_some()
        {
            return Err(invalid());
        }
        let turn_id = self.awaiting_runtime.get(agent_id).ok_or_else(invalid)?;
        let outcome = self.awaiting_outcomes.get(turn_id).ok_or_else(invalid)?;
        let context = outcome.prepared_context.as_ref().ok_or_else(invalid)?;
        if outcome.agent_id != agent_id
            || outcome.turn_id != *turn_id
            || !matches!(
                outcome.feedback,
                super::AsyncTurnFeedback::Wait | super::AsyncTurnFeedback::WaitTicks(_)
            )
            || outcome.world_effect != super::AsyncWorldEffect::NoEffect
            || outcome.prepared_request_context.as_ref() != Some(original)
            || context.agent_id != agent_id
            || context.agent_session_id != original.agent_session_id
            || context.agent_turn_id != original.agent_turn_id
            || context.decision_request_id != original.decision_request_id
            || context.request_digest != original.request_digest
        {
            return Err(invalid());
        }
        let mut staged = self.continuation_harness.clone();
        let rejected = staged
            .invalidate(handle.clone(), ContinuationInvalidationReason::Rejected)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if rejected.proposal != *proposal
            || rejected.chain_id != handle.chain_id
            || rejected.remaining_budget != handle.remaining_budget
            || rejected.consumed_budget != handle.consumed_budget
        {
            return Err(invalid());
        }
        let snapshot = (
            self.continuation_harness.clone(),
            self.continuations.clone(),
            self.awaiting_runtime.clone(),
            self.awaiting_outcomes.clone(),
        );
        self.continuation_harness = staged;
        self.continuations.remove(agent_id);
        let result = self.with_runtime_turn_release_for_continuation(
            agent_id,
            &original.agent_session_id,
            &original.agent_turn_id,
            &original.decision_request_id,
            original.request_digest.as_str(),
            |runner| persist(runner, &rejected),
        );
        if let Err(error) = result {
            (
                self.continuation_harness,
                self.continuations,
                self.awaiting_runtime,
                self.awaiting_outcomes,
            ) = snapshot;
            return Err(error);
        }
        Ok(rejected)
    }
    /// Return the proposal identity currently occupying an Agent's
    /// continuation slot.  Runtime-owned continuation identity is checked by
    /// the Viewer before a restarted wake is allowed to proceed.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn active_continuation_proposal_id(&self, agent_id: &str) -> Option<&str> {
        self.continuations
            .get(agent_id)
            .filter(|continuation| continuation.active)
            .map(|continuation| continuation.proposal.continuation_proposal_id.as_str())
    }

    /// Validate an already-local Harness handle against a fresh Runtime
    /// projection without advancing either local ledger.  This closes the
    /// same-process path's pre-mutation identity check just as hydration does
    /// for a restarted runner.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn validate_active_continuation_with_authority(
        &self,
        agent_id: &str,
        authority: &ContinuationAuthorityContextV1,
        runtime: &RuntimeAgentContinuation,
    ) -> Result<(), AsyncAgentRunnerError> {
        let existing = self
            .continuations
            .get(agent_id)
            .filter(|continuation| continuation.active)
            .cloned()
            .ok_or_else(|| AsyncAgentRunnerError::Cognition("unknown continuation".to_string()))?;
        runtime.validate_authoritative().map_err(|error| {
            AsyncAgentRunnerError::Cognition(format!(
                "Runtime continuation projection invalid: {error}"
            ))
        })?;
        let mut harness = self.continuation_harness.clone();
        harness
            .consume_runtime_projection_with_context(existing, runtime, authority)
            .map(|_| ())
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))
    }

    pub fn submit_continuation_proposal(
        &mut self,
        agent_id: &str,
        proposal: ContinuationProposalV1,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        if !self.actors.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentNotRegistered(
                agent_id.to_string(),
            ));
        }
        if proposal.agent_id != agent_id {
            return Err(AsyncAgentRunnerError::Cognition(
                "continuation proposal agent identity mismatch".to_string(),
            ));
        }
        proposal
            .validate()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if self
            .continuations
            .get(agent_id)
            .is_some_and(|continuation| continuation.active)
        {
            return Err(AsyncAgentRunnerError::AgentBusy(agent_id.to_string()));
        }
        // Even the explicit compatibility lane must register the proposal in
        // the Harness chain ledger.  Keeping an untracked local handle would
        // let terminal feedback be followed by a fresh admission of the same
        // proposal, bypassing the chain's terminal anti-revival fence.
        let handle = self
            .continuation_harness
            .submit(proposal)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        self.continuations
            .insert(agent_id.to_string(), handle.clone());
        Ok(handle)
    }

    /// Production continuation admission. The proposal is checked against
    /// the current Runtime-derived observation, goal, policy and precondition
    /// digests before it can occupy the agent's continuation slot.
    fn submit_continuation_proposal_using_authority(
        &mut self,
        agent_id: &str,
        proposal: ContinuationProposalV1,
        context: &ContinuationAuthorityContextV1,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        if !self.actors.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentNotRegistered(
                agent_id.to_string(),
            ));
        }
        if proposal.agent_id != agent_id {
            return Err(AsyncAgentRunnerError::Cognition(
                "continuation proposal agent identity mismatch".to_string(),
            ));
        }
        if self
            .continuations
            .get(agent_id)
            .is_some_and(|continuation| continuation.active)
        {
            return Err(AsyncAgentRunnerError::AgentBusy(agent_id.to_string()));
        }
        let handle = self
            .continuation_harness
            .submit_with_context(proposal, context)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        self.continuations
            .insert(agent_id.to_string(), handle.clone());
        Ok(handle)
    }

    /// Compatibility fence: target admission must include the actual current
    /// observation, not just historical digest strings.
    pub fn submit_continuation_proposal_with_context(
        &mut self,
        agent_id: &str,
        proposal: ContinuationProposalV1,
        context: &ContinuationAuthorityContextV1,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        let _ = (agent_id, proposal, context);
        Err(AsyncAgentRunnerError::Cognition(
            "current cognition observation is required for target continuation admission"
                .to_string(),
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn submit_continuation_proposal_with_current_context(
        &mut self,
        agent_id: &str,
        proposal: ContinuationProposalV1,
        current: &ContinuationCurrentContextV1,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        current
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        self.submit_continuation_proposal_using_authority(agent_id, proposal, &current.authority)
    }

    /// Reconcile the exact Runtime wake transition with the Harness chain.
    /// Runtime returns the consumed predecessor and (when budget remains) a
    /// newly admitted continuation proposal. Keeping this transition here
    /// prevents the Viewer from treating a durable wake as resumed before the
    /// Harness has retired the predecessor and admitted the next bounded turn.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reconcile_runtime_wake_with_current_context(
        &mut self,
        agent_id: &str,
        current: &ContinuationCurrentContextV1,
        consumed_runtime: &RuntimeAgentContinuation,
        next_proposal: Option<ContinuationProposalV1>,
    ) -> Result<(), AsyncAgentRunnerError> {
        current
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        let previous_harness = self.continuation_harness.clone();
        let previous_continuations = self.continuations.clone();
        let result = (|| {
            let existing = self.continuations.get(agent_id).cloned().ok_or_else(|| {
                AsyncAgentRunnerError::Cognition("unknown continuation".to_string())
            })?;
            let retired = self
                .continuation_harness
                .advance_ready_wake(existing, consumed_runtime, &current.authority)
                .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
            self.continuations.remove(agent_id);
            if let Some(next_proposal) = next_proposal {
                self.submit_continuation_proposal_with_current_context(
                    agent_id,
                    next_proposal,
                    current,
                )?;
            }
            debug_assert!(!retired.active);
            Ok(())
        })();
        if result.is_err() {
            self.continuation_harness = previous_harness;
            self.continuations = previous_continuations;
        }
        result
    }

    /// Close a locally admitted proposal when Runtime rejects the paired
    /// durable admission. This is an explicit rollback of the Harness
    /// projection; it never claims that Runtime itself was admitted.
    pub fn invalidate_continuation_for_agent(
        &mut self,
        agent_id: &str,
        reason: ContinuationInvalidationReason,
    ) -> Result<(), AsyncAgentRunnerError> {
        let Some(handle) = self.continuations.remove(agent_id) else {
            return Ok(());
        };
        self.continuation_harness
            .invalidate(handle, reason)
            .map(|_| ())
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))
    }

    pub fn apply_runtime_continuation_status(
        &mut self,
        _agent_id: &str,
        runtime: RuntimeContinuationStatusV1,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        let _ = runtime;
        Err(AsyncAgentRunnerError::Cognition(
            "legacy Runtime continuation status lacks an authoritative projection".to_string(),
        ))
    }

    /// Apply a complete Runtime-owned continuation projection.  Runtime, not
    /// this runner, allocates schedule IDs, sequence and status digests.
    pub fn apply_runtime_continuation_projection(
        &mut self,
        agent_id: &str,
        runtime: RuntimeAgentContinuation,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        let _ = (agent_id, runtime);
        Err(AsyncAgentRunnerError::Cognition(
            "Runtime projection requires strict continuation admission and current context"
                .to_string(),
        ))
    }

    /// Production Runtime projection path with current cognition-context
    /// revalidation. The reduced compatibility method above remains available
    /// for legacy fixtures, but cannot admit an unverified context update.
    #[cfg(not(target_arch = "wasm32"))]
    fn apply_runtime_continuation_projection_using_authority(
        &mut self,
        agent_id: &str,
        runtime: RuntimeAgentContinuation,
        authority_context: &ContinuationAuthorityContextV1,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        let existing =
            self.continuations.get(agent_id).cloned().ok_or_else(|| {
                AsyncAgentRunnerError::Cognition("unknown continuation".to_string())
            })?;
        if existing.chain_id.is_empty() {
            return Err(AsyncAgentRunnerError::Cognition(
                "Runtime projection requires strict continuation admission".to_string(),
            ));
        }
        let handle = self
            .continuation_harness
            .consume_runtime_projection_with_context(existing, &runtime, authority_context)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if handle.active {
            self.continuations
                .insert(agent_id.to_string(), handle.clone());
        } else {
            self.continuations.remove(agent_id);
        }
        Ok(handle)
    }

    /// Project the exact rejected predecessor and retire its local turn atomically
    /// with the caller's durable sidecar checkpoint. Never dispatches an actor.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_rejected_runtime_continuation_cleanup<F>(
        &mut self,
        agent_id: &str,
        runtime: RuntimeAgentContinuation,
        authority: &ContinuationAuthorityContextV1,
        persist: F,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError>
    where
        F: FnOnce() -> Result<(), String>,
    {
        self.rejected_runtime_continuation_cleanup_core(agent_id, runtime, authority, |_| persist())
    }

    #[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test_tier_required")))]
    pub fn with_rejected_runtime_continuation_cleanup_observed<F>(
        &mut self,
        agent_id: &str,
        runtime: RuntimeAgentContinuation,
        authority: &ContinuationAuthorityContextV1,
        persist: F,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError>
    where
        F: FnOnce([String; 5]) -> Result<(), String>,
    {
        self.rejected_runtime_continuation_cleanup_core(agent_id, runtime, authority, |runner| {
            persist(runner.rejected_wait_test_ledger_digests())
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn rejected_runtime_continuation_cleanup_core<F>(
        &mut self,
        agent_id: &str,
        runtime: RuntimeAgentContinuation,
        authority: &ContinuationAuthorityContextV1,
        persist: F,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError>
    where
        F: FnOnce(&Self) -> Result<(), String>,
    {
        if runtime.status != crate::runtime::ContinuationStatusV1::Rejected {
            return Err(AsyncAgentRunnerError::Cognition(
                "rejected cleanup requires Runtime rejection".into(),
            ));
        }
        if !self.actors.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentNotRegistered(
                agent_id.to_string(),
            ));
        }
        if self.harness.provider_is_still_in_flight(agent_id) {
            return Err(AsyncAgentRunnerError::AgentBusy(agent_id.to_string()));
        }
        let existing = self.continuations.get(agent_id).ok_or_else(|| {
            AsyncAgentRunnerError::Cognition("rejected predecessor local handle missing".into())
        })?;
        if existing.continuation_id != runtime.continuation_id
            || existing.wake_id != runtime.wake_id
            || existing.wake_seq != runtime.wake_seq
            || existing.remaining_budget.unit != runtime.remaining_budget.unit
            || existing.remaining_budget.value != runtime.remaining_budget.value
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "rejected predecessor local identity changed".into(),
            ));
        }
        let outcomes: Vec<_> = self
            .awaiting_outcomes
            .iter()
            .filter(|(_, outcome)| outcome.agent_id == agent_id)
            .collect();
        match (self.awaiting_runtime.get(agent_id), outcomes.as_slice()) {
            (None, []) => {}
            (Some(mapped), [(id, outcome)]) if mapped == *id && outcome.turn_id == **id => {}
            _ => {
                return Err(AsyncAgentRunnerError::Cognition(
                    "rejected predecessor pending turn map or outcomes conflict".into(),
                ));
            }
        }
        let snapshot = (
            self.continuation_harness.clone(),
            self.continuations.clone(),
            self.awaiting_runtime.clone(),
            self.awaiting_outcomes.clone(),
        );
        let result = (|| {
            let handle = self.apply_runtime_terminal_continuation_projection(
                agent_id,
                runtime.clone(),
                authority,
            )?;
            // A committed Wait can already have released its original turn.
            // Any remaining turn must still match the exact predecessor identity.
            if self.awaiting_runtime.contains_key(agent_id) {
                self.with_runtime_turn_release_for_continuation(
                    agent_id,
                    &runtime.agent_session_id,
                    &runtime.agent_turn_id,
                    &runtime.decision_request_id,
                    &runtime.origin_request_digest,
                    persist,
                )?;
            } else {
                persist(self).map_err(AsyncAgentRunnerError::Cognition)?;
            }
            Ok(handle)
        })();
        if result.is_err() {
            (
                self.continuation_harness,
                self.continuations,
                self.awaiting_runtime,
                self.awaiting_outcomes,
            ) = snapshot;
        }
        result
    }

    /// Reconcile a terminal Runtime projection after Runtime has already
    /// validated the authoritative context for the exact wake. Terminal
    /// cleanup does not dispatch a new provider turn, so it must not require
    /// a fresh observation whose precondition may have changed after the
    /// preceding committed action.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn apply_runtime_terminal_continuation_projection(
        &mut self,
        agent_id: &str,
        runtime: RuntimeAgentContinuation,
        authority_context: &ContinuationAuthorityContextV1,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        if !matches!(
            runtime.status,
            crate::runtime::ContinuationStatusV1::Completed
                | crate::runtime::ContinuationStatusV1::Cancelled
                | crate::runtime::ContinuationStatusV1::Invalidated
                | crate::runtime::ContinuationStatusV1::Expired
                | crate::runtime::ContinuationStatusV1::Rejected
        ) {
            return Err(AsyncAgentRunnerError::Cognition(
                "Runtime terminal continuation projection has a non-terminal status".to_string(),
            ));
        }
        self.apply_runtime_continuation_projection_using_authority(
            agent_id,
            runtime,
            authority_context,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn apply_runtime_continuation_projection_with_context(
        &mut self,
        agent_id: &str,
        runtime: RuntimeAgentContinuation,
        authority_context: &ContinuationAuthorityContextV1,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        let _ = (agent_id, runtime, authority_context);
        Err(AsyncAgentRunnerError::Cognition(
            "current cognition observation is required for Runtime projection".to_string(),
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn apply_runtime_continuation_projection_with_current_context(
        &mut self,
        agent_id: &str,
        runtime: RuntimeAgentContinuation,
        current: &ContinuationCurrentContextV1,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        current
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        self.apply_runtime_continuation_projection_using_authority(
            agent_id,
            runtime,
            &current.authority,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn retire_ready_continuation(
        &mut self,
        agent_id: &str,
        turn_context: &ContinuousAgentTurnContextV1,
        authority_context: &ContinuationAuthorityContextV1,
        runtime: &RuntimeAgentContinuation,
    ) -> Result<(bool, ContinuationHandle), AsyncAgentRunnerError> {
        if !self.actors.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentNotRegistered(
                agent_id.to_string(),
            ));
        }
        turn_context
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if turn_context.goal_snapshot.digest != authority_context.goal_digest {
            return Err(AsyncAgentRunnerError::Cognition(
                "continuation next-turn goal does not match the authoritative context".to_string(),
            ));
        }
        let existing =
            self.continuations.get(agent_id).cloned().ok_or_else(|| {
                AsyncAgentRunnerError::Cognition("unknown continuation".to_string())
            })?;
        let projection = self
            .continuation_harness
            .advance_ready_wake(existing, runtime, authority_context)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        self.continuations.remove(agent_id);
        Ok((
            runtime.status == crate::runtime::ContinuationStatusV1::Consumed,
            projection,
        ))
    }

    /// Apply the Runtime wake transition and the next actor dispatch as one
    /// local state transaction. The actor send path is fallible (for example
    /// when its mailbox is full or the actor is unavailable), so a failed
    /// dispatch must restore the Harness, continuation, feedback and turn
    /// counters exactly as they were before the wake was retired.
    #[cfg(not(target_arch = "wasm32"))]
    fn retire_and_dispatch_continuation<F>(
        &mut self,
        agent_id: &str,
        turn_context: &ContinuousAgentTurnContextV1,
        authority_context: &ContinuationAuthorityContextV1,
        runtime: &RuntimeAgentContinuation,
        dispatch: F,
    ) -> Result<Result<AsyncTurnId, ContinuationHandle>, AsyncAgentRunnerError>
    where
        F: FnOnce(&mut Self) -> Result<AsyncTurnId, AsyncAgentRunnerError>,
    {
        let previous_harness = self.continuation_harness.clone();
        let previous_continuations = self.continuations.clone();
        let previous_next_turn_id = self.next_turn_id;
        let previous_active_turns = self.active_turns;
        let result = (|| {
            let (consumed, projection) =
                self.retire_ready_continuation(agent_id, turn_context, authority_context, runtime)?;
            if !consumed {
                return Ok(Err(projection));
            }
            dispatch(self).map(Ok)
        })();
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                self.continuation_harness = previous_harness;
                self.continuations = previous_continuations;
                self.next_turn_id = previous_next_turn_id;
                self.active_turns = previous_active_turns;
                Err(error)
            }
        }
    }

    /// Revalidate a consumed Runtime wake and dispatch exactly one new actor
    /// turn for replan/next action. A terminal Runtime projection returns its
    /// terminal handle and does not invoke the actor. A lease acknowledgement
    /// or pending wake cannot enter this path.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn resume_consumed_continuation(
        &mut self,
        agent_id: &str,
        observation: Observation,
        turn_context: ContinuousAgentTurnContextV1,
        authority_context: &ContinuationAuthorityContextV1,
        runtime: RuntimeAgentContinuation,
    ) -> Result<Result<AsyncTurnId, ContinuationHandle>, AsyncAgentRunnerError> {
        Self::validate_current_observation(&observation, authority_context, agent_id)?;
        let dispatch_turn_context = turn_context.clone();
        self.retire_and_dispatch_continuation(
            agent_id,
            &turn_context,
            authority_context,
            &runtime,
            |runner| {
                runner.start_turn_with_context_and_observation_and_request(
                    agent_id,
                    observation,
                    Some(dispatch_turn_context),
                    None,
                    None,
                )
            },
        )
    }

    fn validate_current_observation(
        observation: &Observation,
        authority_context: &ContinuationAuthorityContextV1,
        agent_id: &str,
    ) -> Result<(), AsyncAgentRunnerError> {
        ContinuationCurrentContextV1 {
            observation: observation.clone(),
            authority: authority_context.clone(),
        }
        .validate_for_agent(agent_id)
        .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))
    }

    /// Production variant retaining the complete outer request lineage for
    /// the resumed turn. It shares the same strict wake retirement path, then
    /// enters the normal request-context correlation checks before dispatch.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn resume_consumed_continuation_with_request_context(
        &mut self,
        agent_id: &str,
        observation: Observation,
        turn_context: ContinuousAgentTurnContextV1,
        request_context: ContinuousAgentRequestContextV1,
        authority_context: &ContinuationAuthorityContextV1,
        runtime: RuntimeAgentContinuation,
    ) -> Result<Result<AsyncTurnId, ContinuationHandle>, AsyncAgentRunnerError> {
        turn_context
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        request_context
            .validate_production_lane()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if request_context.agent_subject != agent_id
            || request_context.agent_session_id != turn_context.agent_session_id
            || request_context.agent_turn_id != turn_context.agent_turn_id
            || request_context.decision_request_id != turn_context.decision_request_id
            || request_context.request_digest != turn_context.request_digest
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "outer and reduced cognition contexts do not correlate".to_string(),
            ));
        }
        if request_context.observation_digest.as_str()
            != authority_context.baseline_observation_digest
            || request_context.goal_snapshot_digest.as_str() != authority_context.goal_digest
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "outer request cognition digests do not match the current wake context".to_string(),
            ));
        }
        Self::validate_current_observation(&observation, authority_context, agent_id)?;
        let dispatch_turn_context = turn_context.clone();
        self.retire_and_dispatch_continuation(
            agent_id,
            &turn_context,
            authority_context,
            &runtime,
            |runner| {
                runner.start_turn_with_context_and_observation_and_request(
                    agent_id,
                    observation,
                    Some(dispatch_turn_context),
                    Some(request_context),
                    None,
                )
            },
        )
    }

    /// Production wake consumer: one consumed Runtime wake yields one next
    /// actor turn, while a terminal wake returns without invoking the actor.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn dispatch_ready_cognition_wake(
        &mut self,
        agent_id: &str,
        current: ContinuationCurrentContextV1,
        turn_context: ContinuousAgentTurnContextV1,
        request_context: ContinuousAgentRequestContextV1,
        runtime: RuntimeAgentContinuation,
    ) -> Result<Result<AsyncTurnId, ContinuationHandle>, AsyncAgentRunnerError> {
        current
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if turn_context.goal_snapshot.digest != current.authority.goal_digest
            || request_context.observation_digest.as_str()
                != current.authority.baseline_observation_digest
            || request_context.goal_snapshot_digest.as_str() != current.authority.goal_digest
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "next cognition turn does not match the current wake context".to_string(),
            ));
        }
        self.resume_consumed_continuation_with_request_context(
            agent_id,
            current.observation,
            turn_context,
            request_context,
            &current.authority,
            runtime,
        )
    }

    /// Rebuild local continuation state in a fresh Agent runner from the
    /// Runtime-owned proposal and status projection.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn hydrate_runtime_continuation(
        &mut self,
        agent_id: &str,
        proposal: ContinuationProposalV1,
        current: &ContinuationCurrentContextV1,
        runtime: RuntimeAgentContinuation,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        current
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if proposal.agent_id != agent_id {
            return Err(AsyncAgentRunnerError::Cognition(
                "hydrated continuation agent identity mismatch".to_string(),
            ));
        }
        if self
            .continuations
            .get(agent_id)
            .is_some_and(|continuation| continuation.active)
        {
            return Err(AsyncAgentRunnerError::AgentBusy(agent_id.to_string()));
        }
        let admitted = self
            .continuation_harness
            .submit_with_context(proposal, &current.authority)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        let handle = self
            .continuation_harness
            .consume_runtime_projection_with_context(admitted, &runtime, &current.authority)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if handle.active {
            self.continuations
                .insert(agent_id.to_string(), handle.clone());
        }
        Ok(handle)
    }

    /// Rebuild a fresh runner's exact Harness continuation from the
    /// Runtime-owned projection.  The Harness is cloned and all validation is
    /// performed against the clone before replacing local state, so a stale or
    /// mismatched restart checkpoint cannot partially mutate the new runner.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn hydrate_runtime_continuation_with_authority(
        &mut self,
        agent_id: &str,
        proposal: ContinuationProposalV1,
        authority: &ContinuationAuthorityContextV1,
        runtime: RuntimeAgentContinuation,
    ) -> Result<ContinuationHandle, AsyncAgentRunnerError> {
        if !self.actors.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentNotRegistered(
                agent_id.to_string(),
            ));
        }
        if proposal.agent_id != agent_id || runtime.agent_id != agent_id {
            return Err(AsyncAgentRunnerError::Cognition(
                "hydrated continuation agent identity mismatch".to_string(),
            ));
        }
        if self.active_continuation_proposal_id(agent_id).is_some() {
            return Err(AsyncAgentRunnerError::AgentBusy(agent_id.to_string()));
        }
        runtime.validate_authoritative().map_err(|error| {
            AsyncAgentRunnerError::Cognition(format!(
                "Runtime continuation projection invalid: {error}"
            ))
        })?;
        authority
            .validate_proposal(&proposal)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;

        let mut harness = self.continuation_harness.clone();
        let admitted = harness
            .submit_with_context(proposal, authority)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        let handle = harness
            .consume_runtime_projection_with_context(admitted, &runtime, authority)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;

        self.continuation_harness = harness;
        if handle.active {
            self.continuations
                .insert(agent_id.to_string(), handle.clone());
        } else {
            self.continuations.remove(agent_id);
        }
        Ok(handle)
    }
}

#[cfg(test)]
#[path = "async_agent_runner_rejected_wait_tests.rs"]
mod rejected_wait_tests;
