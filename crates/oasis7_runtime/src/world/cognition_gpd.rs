//! Typed Runtime handoff seams for gameplay-reviewed continuation invariants.
//!
//! The agent/provider owns the proposed observation, goal and policy values;
//! World owns their admitted digest, budget chain, wake lease and terminal
//! disposition. These methods keep that boundary explicit and transactional.

use super::World;
use super::cognition_persistence_validation::append_cognition_event;
use crate::cognition_wake::{
    AgentContinuation, CognitionBudgetConsumptionV1, CognitionContextDigestsV1,
    CognitionContinuationProposalV1, CognitionContinuationResumeRequestV1,
    CognitionWakeDispositionV1, CognitionWakeHandoffResultV1, ContinuationStatusV1,
    ContinuationTransition, WakeConditionValidator,
};
use crate::error::WorldError;
use serde_json::Value as JsonValue;

fn handoff_error(code: &str) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: format!("cognition handoff failed: {code}"),
    }
}

fn terminal_status(status: ContinuationStatusV1) -> bool {
    matches!(
        status,
        ContinuationStatusV1::Completed
            | ContinuationStatusV1::Cancelled
            | ContinuationStatusV1::Invalidated
            | ContinuationStatusV1::Expired
            | ContinuationStatusV1::Rejected
    )
}

#[expect(
    clippy::collapsible_if,
    reason = "The two-step Waking then Consumed transition is intentional state-machine sequencing."
)]
fn transition_terminal(
    continuation: &mut AgentContinuation,
    status: ContinuationStatusV1,
    logical_tick: u64,
) -> Result<(), WorldError> {
    if status == ContinuationStatusV1::Completed {
        if !matches!(continuation.status, ContinuationStatusV1::Consumed) {
            if !matches!(continuation.status, ContinuationStatusV1::Waking) {
                ContinuationTransition::apply_at_tick(
                    continuation,
                    ContinuationStatusV1::Waking,
                    logical_tick,
                )
                .map_err(|error| handoff_error(error.code()))?;
            }
            ContinuationTransition::apply_at_tick(
                continuation,
                ContinuationStatusV1::Consumed,
                logical_tick,
            )
            .map_err(|error| handoff_error(error.code()))?;
        }
    }
    ContinuationTransition::apply_at_tick(continuation, status, logical_tick)
        .map_err(|error| handoff_error(error.code()))
}

fn set_continuations(
    world: &mut World,
    continuations: &[AgentContinuation],
) -> Result<(), WorldError> {
    let mut projection = world.cognition.as_object().cloned().unwrap_or_default();
    projection.insert(
        "continuations".to_string(),
        serde_json::to_value(continuations).map_err(WorldError::from)?,
    );
    world.cognition = JsonValue::Object(projection);
    Ok(())
}

fn context_registry_entry(world: &World, continuation_id: &str) -> Option<JsonValue> {
    world
        .cognition
        .get("continuation_contexts")
        .and_then(JsonValue::as_object)
        .and_then(|contexts| contexts.get(continuation_id))
        .cloned()
}

fn proposal_context_matches(entry: &JsonValue, proposal: &CognitionContinuationProposalV1) -> bool {
    entry
        .get("baseline_observation_digest")
        .and_then(JsonValue::as_str)
        == Some(proposal.baseline_observation_digest.as_str())
        && entry.get("goal_digest").and_then(JsonValue::as_str)
            == Some(proposal.goal_digest.as_str())
        && entry.get("policy_digest").and_then(JsonValue::as_str)
            == Some(proposal.policy_digest.as_str())
        && entry.get("policy_revision").and_then(JsonValue::as_u64)
            == Some(proposal.policy_revision)
        && entry
            .get("precondition_summary")
            .and_then(JsonValue::as_str)
            == Some(proposal.precondition_summary.as_str())
        && entry.get("precondition_digest").and_then(JsonValue::as_str)
            == Some(proposal.precondition_digest.as_str())
}

fn current_context_matches_entry(current: &CognitionContextDigestsV1, entry: &JsonValue) -> bool {
    entry
        .get("baseline_observation_digest")
        .and_then(JsonValue::as_str)
        == Some(current.baseline_observation_digest.as_str())
        && entry.get("goal_digest").and_then(JsonValue::as_str)
            == Some(current.goal_digest.as_str())
        && entry.get("policy_digest").and_then(JsonValue::as_str)
            == Some(current.policy_digest.as_str())
        && entry.get("precondition_digest").and_then(JsonValue::as_str)
            == Some(current.precondition_digest.as_str())
}

fn register_cognition_resume_lifecycle(
    world: &mut World,
    wake_id: &str,
    continuation: &AgentContinuation,
    proposal: &CognitionContinuationProposalV1,
    resume: &CognitionContinuationResumeRequestV1,
) -> Result<(), WorldError> {
    resume
        .validate()
        .map_err(|error| handoff_error(error.code()))?;
    if wake_id != continuation.wake_id
        || proposal.agent_id != continuation.agent_id
        || proposal.origin_turn_id != continuation.origin_turn_id
        || proposal.origin_request_digest != continuation.origin_request_digest
        || resume.agent_session_id != proposal.agent_session_id
        || resume.agent_turn_id != proposal.agent_turn_id
        || resume.decision_request_id != proposal.decision_request_id
    {
        return Err(handoff_error("cognition_resume_lineage_mismatch"));
    }

    let events = world
        .cognition
        .get("cognition_journal")
        .and_then(JsonValue::as_object)
        .and_then(|journal| journal.get("events"))
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default();
    let same_request = |event: &JsonValue| {
        event.get("agent_id").and_then(JsonValue::as_str) == Some(proposal.agent_id.as_str())
            && event.get("agent_session_id").and_then(JsonValue::as_str)
                == Some(resume.agent_session_id.as_str())
            && event.get("agent_turn_id").and_then(JsonValue::as_str)
                == Some(resume.agent_turn_id.as_str())
            && event.get("decision_request_id").and_then(JsonValue::as_str)
                == Some(resume.decision_request_id.as_str())
            && event.get("request_digest").and_then(JsonValue::as_str)
                == Some(resume.request_digest.as_str())
    };
    if events.iter().any(|event| {
        same_request(event)
            && matches!(
                event.get("event_kind").and_then(JsonValue::as_str),
                Some("CognitionTurnFailed")
                    | Some("CognitionTurnCancelled")
                    | Some("CognitionTurnCompleted")
            )
    }) {
        return Err(handoff_error("cognition_turn_terminal"));
    }
    if let Some(existing) = events.iter().find(|event| {
        same_request(event)
            && event.get("event_kind").and_then(JsonValue::as_str) == Some("TurnStarted")
    }) {
        if existing.get("origin_turn_id").and_then(JsonValue::as_str)
            == Some(continuation.origin_turn_id.as_str())
            && existing
                .get("origin_request_digest")
                .and_then(JsonValue::as_str)
                == Some(continuation.origin_request_digest.as_str())
            && existing.get("continuation_id").and_then(JsonValue::as_str)
                == Some(continuation.continuation_id.as_str())
            && existing.get("wake_id").and_then(JsonValue::as_str) == Some(wake_id)
        {
            return Ok(());
        }
        return Err(handoff_error("cognition_resume_identity_conflict"));
    }

    append_cognition_event(
        &mut world.cognition,
        "TurnStarted",
        serde_json::json!({
            "agent_id": proposal.agent_id,
            "agent_session_id": resume.agent_session_id,
            "agent_turn_id": resume.agent_turn_id,
            "decision_request_id": resume.decision_request_id,
            "request_digest": resume.request_digest,
            "origin_turn_id": continuation.origin_turn_id,
            "origin_request_digest": continuation.origin_request_digest,
            "continuation_id": continuation.continuation_id,
            "wake_id": wake_id,
            "logical_tick": world.state.time,
            "status": "running",
        }),
    )?;
    append_cognition_event(
        &mut world.cognition,
        "ContextCaptured",
        serde_json::json!({
            "agent_id": proposal.agent_id,
            "agent_session_id": resume.agent_session_id,
            "agent_turn_id": resume.agent_turn_id,
            "decision_request_id": resume.decision_request_id,
            "request_digest": resume.request_digest,
            "context_digest": resume.context_digest,
            "origin_turn_id": continuation.origin_turn_id,
            "origin_request_digest": continuation.origin_request_digest,
            "continuation_id": continuation.continuation_id,
            "wake_id": wake_id,
            "logical_tick": world.state.time,
            "status": "running",
        }),
    )?;
    Ok(())
}

fn previously_resumed_cognition_wake(
    world: &World,
    wake_id: &str,
    proposal: &CognitionContinuationProposalV1,
    resume: &CognitionContinuationResumeRequestV1,
) -> Result<Option<CognitionWakeHandoffResultV1>, WorldError> {
    let events = world
        .cognition
        .get("cognition_journal")
        .and_then(JsonValue::as_object)
        .and_then(|journal| journal.get("events"))
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default();
    let Some(replanned) = events.iter().rev().find(|event| {
        event.get("event_kind").and_then(JsonValue::as_str) == Some("ContinuationReplanned")
            && event.get("wake_id").and_then(JsonValue::as_str) == Some(wake_id)
            && event.get("agent_id").and_then(JsonValue::as_str) == Some(proposal.agent_id.as_str())
    }) else {
        return Ok(None);
    };
    let Some(old_wake) = replanned.get("wake").cloned() else {
        return Ok(None);
    };
    let old_wake = serde_json::from_value(old_wake).map_err(WorldError::from)?;
    let all_continuations = world.cognition_continuations_typed()?;
    let Some(old_continuation_id) = replanned.get("continuation_id").and_then(JsonValue::as_str)
    else {
        return Ok(None);
    };
    let Some(old_continuation) = all_continuations
        .iter()
        .find(|continuation| continuation.continuation_id == old_continuation_id)
        .cloned()
    else {
        return Ok(None);
    };
    let Some(next_continuation) = all_continuations.into_iter().find(|continuation| {
        continuation.agent_id == proposal.agent_id
            && continuation.agent_session_id == resume.agent_session_id
            && continuation.agent_turn_id == resume.agent_turn_id
            && continuation.decision_request_id == resume.decision_request_id
            && continuation.proposal_digest == proposal.proposal_digest
            && continuation.origin_turn_id == proposal.origin_turn_id
            && continuation.origin_request_digest == proposal.origin_request_digest
    }) else {
        return Ok(None);
    };
    let context_matches = events.iter().any(|event| {
        event.get("event_kind").and_then(JsonValue::as_str) == Some("ContextCaptured")
            && event.get("agent_id").and_then(JsonValue::as_str) == Some(proposal.agent_id.as_str())
            && event.get("agent_session_id").and_then(JsonValue::as_str)
                == Some(resume.agent_session_id.as_str())
            && event.get("agent_turn_id").and_then(JsonValue::as_str)
                == Some(resume.agent_turn_id.as_str())
            && event.get("decision_request_id").and_then(JsonValue::as_str)
                == Some(resume.decision_request_id.as_str())
            && event.get("request_digest").and_then(JsonValue::as_str)
                == Some(resume.request_digest.as_str())
            && event.get("context_digest").and_then(JsonValue::as_str)
                == Some(resume.context_digest.as_str())
    });
    if !context_matches {
        return Ok(None);
    }
    Ok(Some(CognitionWakeHandoffResultV1 {
        wake: old_wake,
        continuation: old_continuation,
        replanned_continuation: Some(next_continuation),
    }))
}

impl World {
    fn live_cognition_wake_invalid_reason(
        &self,
        continuation_id: &str,
        continuation: &AgentContinuation,
    ) -> Option<&'static str> {
        if continuation
            .valid_until_tick
            .is_some_and(|valid_until| self.state.time > valid_until)
        {
            return Some("cognition_continuation_expired");
        }
        let evaluation = match self
            .evaluate_cognition_wake_at_tick(&continuation.wake_conditions, self.state.time)
        {
            Ok(evaluation) => evaluation,
            // A live evidence evaluation that cannot be reconstructed is not
            // safe to hand to an agent. Treat evaluator failures as stale
            // evidence and let the caller persist the terminal disposition.
            Err(_) => return Some("cognition_wake_evidence_stale"),
        };
        if evaluation.status == "expired" {
            return Some("cognition_continuation_expired");
        }
        if evaluation.status != "ready" {
            return Some("cognition_wake_evidence_stale");
        }
        if continuation.reorg_epoch != self.cognition_reorg_epoch() {
            return Some("cognition_wake_evidence_stale");
        }
        let Some(selected) = self
            .cognition
            .get("continuation_evaluations")
            .and_then(JsonValue::as_object)
            .and_then(|evaluations| evaluations.get(continuation_id))
        else {
            return Some("cognition_wake_evaluation_missing");
        };
        let current_head = match self.current_state_root_hash() {
            Ok(head) => head,
            Err(_) => return Some("cognition_wake_evidence_stale"),
        };
        if selected
            .get("evaluation_head_digest")
            .and_then(JsonValue::as_str)
            != Some(current_head.as_str())
        {
            return Some("cognition_wake_evidence_stale");
        }
        if selected.get("evaluation_tick").and_then(JsonValue::as_u64)
            != Some(evaluation.evaluation_tick)
            || selected
                .get("evaluation_digest")
                .and_then(JsonValue::as_str)
                != Some(evaluation.evaluation_digest.as_str())
            || selected
                .get("conditions_digest")
                .and_then(JsonValue::as_str)
                != Some(evaluation.conditions_digest.as_str())
        {
            return Some("cognition_wake_evidence_stale");
        }
        None
    }

    /// Atomically reject a live wake after expiry or evidence/context
    /// revalidation fails. The exact in-flight lease is deactivated in the
    /// same projection transaction, so a stale caller cannot retain a slot or
    /// spend continuation budget after receiving an error.
    fn terminalize_cognition_wake(
        &mut self,
        wake_id: &str,
        status: ContinuationStatusV1,
        reason: &str,
    ) -> Result<(), WorldError> {
        let mut transaction = self.clone();
        let mut scheduler = transaction.cognition_scheduler()?;
        let wake = scheduler
            .in_flight_wakes()
            .into_iter()
            .find(|wake| wake.wake_id == wake_id)
            .ok_or_else(|| handoff_error("wake_not_in_flight"))?;
        let mut continuations = transaction.cognition_continuations_typed()?;
        let continuation = continuations
            .iter_mut()
            .find(|value| value.continuation_id == wake.continuation_id)
            .ok_or_else(|| handoff_error("continuation_missing"))?;
        if terminal_status(continuation.status) {
            return Ok(());
        }
        transition_terminal(continuation, status, transaction.state.time)?;
        continuation.terminal_disposition = Some(reason.to_string());
        continuation.refresh_status_digest();
        let transitioned = continuation.clone();
        let deactivated = scheduler
            .deactivate_wake(&wake.wake_id)
            .map_err(|error| handoff_error(error.code()))?;
        transaction.cognition_commit_continuation_lifecycle_transaction(
            &continuations,
            &scheduler,
            World::continuation_lifecycle_event_kind(status),
            &transitioned,
            deactivated.as_ref(),
        )?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(())
    }

    /// Mutable counterpart to `validate_cognition_context_digests`. Adapter
    /// callers should use this seam immediately before a replan/handoff; a
    /// mismatch consumes neither budget nor a new proposal and closes the
    /// stale wake atomically.
    pub fn revalidate_cognition_context_digests(
        &mut self,
        continuation_id: &str,
        proposal: &CognitionContinuationProposalV1,
    ) -> Result<CognitionContextDigestsV1, WorldError> {
        match self.validate_cognition_context_digests(continuation_id, proposal) {
            Ok(context) => Ok(context),
            Err(error) => {
                let continuation = self
                    .cognition_continuations_typed()?
                    .into_iter()
                    .find(|value| value.continuation_id == continuation_id);
                if let Some(continuation) = continuation
                    && !terminal_status(continuation.status)
                    && self
                        .cognition_in_flight_wakes()?
                        .iter()
                        .any(|wake| wake.continuation_id == continuation_id)
                {
                    self.terminalize_cognition_wake(
                        &continuation.wake_id,
                        ContinuationStatusV1::Rejected,
                        "cognition_context_mismatch",
                    )?;
                }
                Err(error)
            }
        }
    }

    /// Revalidate an admitted proposal against a context rebuilt by the
    /// current Runtime/Harness boundary. A proposal's historical digests are
    /// not accepted as their own current-state proof.
    pub fn validate_cognition_context_digests_with_current_context(
        &self,
        continuation_id: &str,
        proposal: &CognitionContinuationProposalV1,
        current_context: CognitionContextDigestsV1,
    ) -> Result<CognitionContextDigestsV1, WorldError> {
        current_context
            .validate()
            .map_err(|error| handoff_error(error.code()))?;
        let expected = CognitionContextDigestsV1::from_proposal(proposal);
        if current_context != expected {
            return Err(handoff_error("cognition_context_mismatch"));
        }
        let context = self.validate_cognition_context_digests(continuation_id, proposal)?;
        if context != current_context {
            return Err(handoff_error("cognition_context_mismatch"));
        }
        Ok(current_context)
    }

    fn validate_current_context_for_continuation(
        &self,
        continuation_id: &str,
        current_context: &CognitionContextDigestsV1,
    ) -> Result<(), WorldError> {
        current_context
            .validate()
            .map_err(|error| handoff_error(error.code()))?;
        let entry = context_registry_entry(self, continuation_id)
            .ok_or_else(|| handoff_error("cognition_context_missing"))?;
        if !current_context_matches_entry(current_context, &entry) {
            return Err(handoff_error("cognition_context_mismatch"));
        }
        Ok(())
    }

    /// Consume one selected wake and atomically admit the next logical
    /// request in its continuation chain. The next request receives fresh
    /// Harness session/turn/request identity and its own request digest; the
    /// proposal's `origin_request_digest` remains the stable causal link to
    /// the original turn. Runtime records `TurnStarted` and `ContextCaptured`
    /// before returning, so the provider adapter can append
    /// `RequestDispatched` and perform I/O without a lifecycle gap.
    ///
    /// This is the production wake/resume seam. Callers must use the returned
    /// `replanned_continuation` projection and then dispatch the exact resume
    /// request context. A transport retry must use
    /// `dispatch_cognition_request` with the same request digest and a higher
    /// transport attempt; it must not call this method again.
    pub fn resume_cognition_wake(
        &mut self,
        wake_id: &str,
        proposal: CognitionContinuationProposalV1,
        budget_spent: u64,
        resume: CognitionContinuationResumeRequestV1,
    ) -> Result<CognitionWakeHandoffResultV1, WorldError> {
        self.resume_cognition_wake_inner(wake_id, proposal, budget_spent, resume, None)
    }

    pub fn resume_cognition_wake_with_context(
        &mut self,
        wake_id: &str,
        proposal: CognitionContinuationProposalV1,
        budget_spent: u64,
        resume: CognitionContinuationResumeRequestV1,
        current_context: CognitionContextDigestsV1,
    ) -> Result<CognitionWakeHandoffResultV1, WorldError> {
        self.resume_cognition_wake_inner(
            wake_id,
            proposal,
            budget_spent,
            resume,
            Some(current_context),
        )
    }

    fn resume_cognition_wake_inner(
        &mut self,
        wake_id: &str,
        proposal: CognitionContinuationProposalV1,
        budget_spent: u64,
        resume: CognitionContinuationResumeRequestV1,
        current_context: Option<CognitionContextDigestsV1>,
    ) -> Result<CognitionWakeHandoffResultV1, WorldError> {
        let mut transaction = self.clone();
        let Some(wake) = transaction
            .cognition_in_flight_wakes()?
            .into_iter()
            .find(|wake| wake.wake_id == wake_id)
        else {
            if let Some(result) =
                previously_resumed_cognition_wake(self, wake_id, &proposal, &resume)?
            {
                return Ok(result);
            }
            return Err(handoff_error("wake_not_in_flight"));
        };
        let continuation = transaction
            .cognition_continuations_typed()?
            .into_iter()
            .find(|value| value.continuation_id == wake.continuation_id)
            .ok_or_else(|| handoff_error("continuation_missing"))?;
        if let Some(current_context) = current_context.as_ref() {
            transaction.validate_current_context_for_continuation(
                &continuation.continuation_id,
                current_context,
            )?;
        }
        register_cognition_resume_lifecycle(
            &mut transaction,
            wake_id,
            &continuation,
            &proposal,
            &resume,
        )?;
        let result = transaction.handoff_cognition_wake_inner(
            wake_id,
            CognitionWakeDispositionV1::Replan {
                proposal,
                budget_spent,
            },
            current_context,
        );
        match result {
            Ok(result) => {
                *self = transaction;
                Ok(result)
            }
            Err(error) => {
                // `handoff_cognition_wake_inner` may terminalize a stale or
                // invalid wake before returning the error. Preserve that
                // durable cleanup instead of dropping the nested clone.
                if transaction.cognition != self.cognition {
                    transaction.persist_runtime_transaction_if_configured()?;
                    *self = transaction;
                }
                Err(error)
            }
        }
    }

    /// Verify all proposal context digests and its full canonical proposal
    /// identity against the durable continuation. This is the typed seam for
    /// admission/wake revalidation; Runtime intentionally has no product-level
    /// source from which to invent a replacement goal or policy.
    pub fn validate_cognition_context_digests(
        &self,
        continuation_id: &str,
        proposal: &CognitionContinuationProposalV1,
    ) -> Result<CognitionContextDigestsV1, WorldError> {
        let context = CognitionContextDigestsV1::from_proposal(proposal);
        context
            .validate()
            .map_err(|error| handoff_error(error.code()))?;
        proposal
            .validate()
            .map_err(|error| handoff_error(error.code()))?;
        let continuation = self
            .cognition_continuations_typed()?
            .into_iter()
            .find(|value| value.continuation_id == continuation_id)
            .ok_or_else(|| handoff_error("continuation_missing"))?;
        let context_entry = context_registry_entry(self, continuation_id)
            .ok_or_else(|| handoff_error("cognition_context_missing"))?;
        if !proposal_context_matches(&context_entry, proposal) {
            return Err(handoff_error("cognition_context_mismatch"));
        }
        let binding = self.current_cognition_runtime_binding()?;
        let binding_block_hash = binding
            .finality_block_hash
            .as_ref()
            .map(ToString::to_string);
        let proposal_matches_current = proposal.world_id == binding.world_id
            && proposal.branch_id == binding.branch_id
            && proposal.finality_epoch == binding.finality_epoch
            && proposal.finality_block_hash == binding_block_hash
            && proposal.finality_status == binding.finality_status
            && proposal.reorg_epoch == binding.reorg_epoch
            && proposal.runtime_manifest_hash == self.current_manifest_hash()?;
        let wake_conditions =
            WakeConditionValidator::canonicalize(proposal.wake_conditions.clone())
                .map_err(|error| handoff_error(error.code()))?;
        let continuation_matches = continuation.proposal_digest == proposal.proposal_digest
            && continuation.world_id == proposal.world_id
            && continuation.branch_id == proposal.branch_id
            && continuation.finality_epoch == proposal.finality_epoch
            && continuation.finality_block_hash == proposal.finality_block_hash
            && continuation.finality_status == proposal.finality_status
            && continuation.reorg_epoch == proposal.reorg_epoch
            && continuation.runtime_manifest_hash == proposal.runtime_manifest_hash
            && continuation.agent_id == proposal.agent_id
            && continuation.agent_session_id == proposal.agent_session_id
            && continuation.agent_turn_id == proposal.agent_turn_id
            && continuation.decision_request_id == proposal.decision_request_id
            && continuation.origin_turn_id == proposal.origin_turn_id
            && continuation.origin_request_digest == proposal.origin_request_digest
            && continuation.continuation_proposal_id == proposal.continuation_proposal_id
            && continuation.action_or_envelope_digest == proposal.action_or_envelope_digest
            && continuation.wake_conditions == wake_conditions
            && continuation.next_wake_tick == proposal.next_wake_tick
            && continuation.remaining_budget == proposal.remaining_budget
            && continuation.valid_until_tick == proposal.valid_until_tick
            && continuation.precondition_digest == proposal.precondition_digest;
        if !proposal_matches_current || !continuation_matches {
            return Err(handoff_error("cognition_context_mismatch"));
        }
        Ok(context)
    }

    /// Consume a positive amount from a leased wake's remaining budget. The
    /// lease is deactivated in the same World transaction, and exhaustion is
    /// represented by a terminal Completed continuation with zero budget.
    pub fn consume_cognition_continuation_budget(
        &mut self,
        continuation_id: &str,
        amount: u64,
    ) -> Result<CognitionBudgetConsumptionV1, WorldError> {
        self.consume_cognition_continuation_budget_inner(continuation_id, amount, None)
    }

    pub fn consume_cognition_continuation_budget_with_context(
        &mut self,
        continuation_id: &str,
        amount: u64,
        current_context: CognitionContextDigestsV1,
    ) -> Result<CognitionBudgetConsumptionV1, WorldError> {
        self.consume_cognition_continuation_budget_inner(
            continuation_id,
            amount,
            Some(current_context),
        )
    }

    fn consume_cognition_continuation_budget_inner(
        &mut self,
        continuation_id: &str,
        amount: u64,
        current_context: Option<CognitionContextDigestsV1>,
    ) -> Result<CognitionBudgetConsumptionV1, WorldError> {
        if amount == 0 {
            return Err(handoff_error("continuation_budget_amount_invalid"));
        }
        let live_continuation = self
            .cognition_continuations_typed()?
            .into_iter()
            .find(|value| value.continuation_id == continuation_id)
            .ok_or_else(|| handoff_error("continuation_missing"))?;
        if let Some(current_context) = current_context.as_ref()
            && let Err(error) =
                self.validate_current_context_for_continuation(continuation_id, current_context)
        {
            if !terminal_status(live_continuation.status)
                && self
                    .cognition_in_flight_wakes()?
                    .iter()
                    .any(|wake| wake.continuation_id == continuation_id)
            {
                self.terminalize_cognition_wake(
                    &live_continuation.wake_id,
                    ContinuationStatusV1::Rejected,
                    "cognition_context_mismatch",
                )?;
            }
            return Err(error);
        }
        if let Some(reason) =
            self.live_cognition_wake_invalid_reason(continuation_id, &live_continuation)
        {
            let status = if reason == "cognition_continuation_expired" {
                ContinuationStatusV1::Expired
            } else {
                ContinuationStatusV1::Rejected
            };
            self.terminalize_cognition_wake(&live_continuation.wake_id, status, reason)?;
            return Err(handoff_error(reason));
        }
        let mut transaction = self.clone();
        let mut scheduler = transaction.cognition_scheduler()?;
        let wake = scheduler
            .in_flight_wakes()
            .into_iter()
            .find(|wake| wake.continuation_id == continuation_id)
            .ok_or_else(|| handoff_error("wake_not_in_flight"))?;
        transaction.validate_cognition_wake_binding(&wake)?;
        let mut continuations = transaction.cognition_continuations_typed()?;
        let continuation = continuations
            .iter_mut()
            .find(|value| value.continuation_id == continuation_id)
            .ok_or_else(|| handoff_error("continuation_missing"))?;
        if amount > continuation.remaining_budget.value {
            return Err(handoff_error("continuation_budget_exhausted"));
        }
        if !matches!(
            continuation.status,
            ContinuationStatusV1::Scheduled
                | ContinuationStatusV1::Pending
                | ContinuationStatusV1::Waking
        ) {
            return Err(handoff_error("continuation_terminal"));
        }
        if !matches!(continuation.status, ContinuationStatusV1::Waking) {
            ContinuationTransition::apply_at_tick(
                continuation,
                ContinuationStatusV1::Waking,
                transaction.state.time,
            )
            .map_err(|error| handoff_error(error.code()))?;
        }
        continuation.remaining_budget.value -= amount;
        ContinuationTransition::apply_at_tick(
            continuation,
            ContinuationStatusV1::Consumed,
            transaction.state.time,
        )
        .map_err(|error| handoff_error(error.code()))?;
        if continuation.remaining_budget.value == 0 {
            transition_terminal(
                continuation,
                ContinuationStatusV1::Completed,
                transaction.state.time,
            )?;
        }
        if continuation.remaining_budget.value == 0 {
            continuation.terminal_disposition = Some("budget_exhausted".to_string());
            continuation.refresh_status_digest();
        }
        let deactivated = scheduler
            .deactivate_wake(&wake.wake_id)
            .map_err(|error| handoff_error(error.code()))?;
        if continuation.remaining_budget.value > 0 {
            // Partial debits keep the continuation live. Requeue its exact
            // wake so it cannot remain in `consumed` with no future lease.
            ContinuationTransition::apply_at_tick(
                continuation,
                ContinuationStatusV1::Scheduled,
                transaction.state.time,
            )
            .map_err(|error| handoff_error(error.code()))?;
            continuation.refresh_status_digest();
            let mut requeued = wake.clone();
            requeued.next_wake_tick = continuation.next_wake_tick.unwrap_or(u64::MAX);
            requeued.eligible_since_tick = transaction.state.time;
            requeued.pending_reason = "budget_remaining".to_string();
            let outcome = scheduler
                .try_enqueue(requeued)
                .map_err(|error| handoff_error(error.code()))?;
            if outcome.disposition == "pending" {
                ContinuationTransition::apply_at_tick(
                    continuation,
                    ContinuationStatusV1::Pending,
                    transaction.state.time,
                )
                .map_err(|error| handoff_error(error.code()))?;
                continuation.refresh_status_digest();
            }
        }
        let consumed = continuation.clone();
        transaction.cognition_commit_continuation_lifecycle_transaction(
            &continuations,
            &scheduler,
            if consumed.status == ContinuationStatusV1::Completed {
                "ContinuationCompleted"
            } else {
                "ContinuationBudgetConsumed"
            },
            &consumed,
            deactivated.as_ref(),
        )?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(CognitionBudgetConsumptionV1 {
            continuation_id: consumed.continuation_id,
            wake_id: consumed.wake_id,
            consumed: amount,
            remaining_budget: consumed.remaining_budget,
            status: consumed.status,
            continuation_status_digest: consumed
                .continuation_status_digest
                .ok_or_else(|| handoff_error("continuation_status_digest_missing"))?,
        })
    }

    /// Atomically hand a selected wake to either a terminal disposition or a
    /// fully validated next proposal. Replan budgets must be monotonic and
    /// the next proposal is admitted through the same Runtime gate.
    pub fn handoff_cognition_wake(
        &mut self,
        wake_id: &str,
        disposition: CognitionWakeDispositionV1,
    ) -> Result<CognitionWakeHandoffResultV1, WorldError> {
        self.handoff_cognition_wake_inner(wake_id, disposition, None)
    }

    pub fn handoff_cognition_wake_with_context(
        &mut self,
        wake_id: &str,
        disposition: CognitionWakeDispositionV1,
        current_context: CognitionContextDigestsV1,
    ) -> Result<CognitionWakeHandoffResultV1, WorldError> {
        self.handoff_cognition_wake_inner(wake_id, disposition, Some(current_context))
    }

    fn handoff_cognition_wake_inner(
        &mut self,
        wake_id: &str,
        disposition: CognitionWakeDispositionV1,
        current_context: Option<CognitionContextDigestsV1>,
    ) -> Result<CognitionWakeHandoffResultV1, WorldError> {
        let live_wake = self
            .cognition_in_flight_wakes()?
            .into_iter()
            .find(|wake| wake.wake_id == wake_id)
            .ok_or_else(|| handoff_error("wake_not_in_flight"))?;
        let live_continuation = self
            .cognition_continuations_typed()?
            .into_iter()
            .find(|value| value.continuation_id == live_wake.continuation_id)
            .ok_or_else(|| handoff_error("continuation_missing"))?;
        if let Some(current_context) = current_context.as_ref()
            && let Err(error) = self.validate_current_context_for_continuation(
                &live_continuation.continuation_id,
                current_context,
            )
        {
            self.terminalize_cognition_wake(
                &live_wake.wake_id,
                ContinuationStatusV1::Rejected,
                "cognition_context_mismatch",
            )?;
            return Err(error);
        }
        if let Some(reason) = self.live_cognition_wake_invalid_reason(
            &live_continuation.continuation_id,
            &live_continuation,
        ) {
            let status = if reason == "cognition_continuation_expired" {
                ContinuationStatusV1::Expired
            } else {
                ContinuationStatusV1::Rejected
            };
            self.terminalize_cognition_wake(&live_wake.wake_id, status, reason)?;
            return Err(handoff_error(reason));
        }
        if let CognitionWakeDispositionV1::Replan { proposal, .. } = &disposition {
            let context_matches = context_registry_entry(self, &live_continuation.continuation_id)
                .is_some_and(|entry| {
                    proposal_context_matches(&entry, proposal)
                        && current_context
                            .as_ref()
                            .is_none_or(|current| current_context_matches_entry(current, &entry))
                });
            if !context_matches {
                self.terminalize_cognition_wake(
                    &live_wake.wake_id,
                    ContinuationStatusV1::Rejected,
                    "cognition_context_mismatch",
                )?;
                return Err(handoff_error("cognition_context_mismatch"));
            }
        }
        let mut transaction = self.clone();
        let mut scheduler = transaction.cognition_scheduler()?;
        let wake = scheduler
            .in_flight_wakes()
            .into_iter()
            .find(|wake| wake.wake_id == wake_id)
            .ok_or_else(|| handoff_error("wake_not_in_flight"))?;
        transaction.validate_cognition_wake_binding(&wake)?;
        let mut continuations = transaction.cognition_continuations_typed()?;
        let continuation = continuations
            .iter_mut()
            .find(|value| value.continuation_id == wake.continuation_id)
            .ok_or_else(|| handoff_error("continuation_missing"))?;
        if !matches!(
            continuation.status,
            ContinuationStatusV1::Scheduled
                | ContinuationStatusV1::Pending
                | ContinuationStatusV1::Waking
        ) {
            return Err(handoff_error("continuation_terminal"));
        }
        let (event_kind, replanned_proposal) = match disposition {
            CognitionWakeDispositionV1::Terminal { status, reason } => {
                if !terminal_status(status) || reason.trim().is_empty() || reason.len() > 128 {
                    return Err(handoff_error("wake_terminal_disposition_invalid"));
                }
                transition_terminal(continuation, status, transaction.state.time)?;
                continuation.terminal_disposition = Some(reason);
                continuation.refresh_status_digest();
                ("ContinuationHandoffTerminal", None)
            }
            CognitionWakeDispositionV1::Replan {
                proposal,
                budget_spent,
            } => {
                if budget_spent == 0 || budget_spent > continuation.remaining_budget.value {
                    return Err(handoff_error("continuation_budget_exhausted"));
                }
                let remaining = continuation.remaining_budget.value - budget_spent;
                if proposal.remaining_budget.unit != continuation.remaining_budget.unit
                    || proposal.remaining_budget.value > remaining
                {
                    return Err(handoff_error("continuation_budget_non_monotonic"));
                }
                let Some(context_entry) =
                    context_registry_entry(&transaction, &wake.continuation_id)
                else {
                    return Err(handoff_error("cognition_context_missing"));
                };
                if !proposal_context_matches(&context_entry, &proposal) {
                    return Err(handoff_error("cognition_context_mismatch"));
                }
                if !matches!(continuation.status, ContinuationStatusV1::Waking) {
                    ContinuationTransition::apply_at_tick(
                        continuation,
                        ContinuationStatusV1::Waking,
                        transaction.state.time,
                    )
                    .map_err(|error| handoff_error(error.code()))?;
                }
                continuation.remaining_budget.value = remaining;
                ContinuationTransition::apply_at_tick(
                    continuation,
                    ContinuationStatusV1::Consumed,
                    transaction.state.time,
                )
                .map_err(|error| handoff_error(error.code()))?;
                ("ContinuationReplanned", Some(proposal))
            }
        };
        let transitioned = continuation.clone();
        let deactivated = scheduler
            .deactivate_wake(&wake.wake_id)
            .map_err(|error| handoff_error(error.code()))?;
        set_continuations(&mut transaction, &continuations)?;
        let replanned_continuation = if let Some(proposal) = replanned_proposal {
            transaction.cognition["scheduler_state"] = scheduler.snapshot_json();
            Some(transaction.admit_cognition_continuation_inner(proposal, true)?)
        } else {
            transaction.cognition["scheduler_state"] = scheduler.snapshot_json();
            None
        };
        let final_scheduler = transaction.cognition_scheduler()?;
        let final_continuations = transaction.cognition_continuations_typed()?;
        transaction.cognition_commit_continuation_lifecycle_transaction(
            &final_continuations,
            &final_scheduler,
            event_kind,
            &transitioned,
            deactivated.as_ref(),
        )?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(CognitionWakeHandoffResultV1 {
            wake,
            continuation: transitioned,
            replanned_continuation,
        })
    }
}
