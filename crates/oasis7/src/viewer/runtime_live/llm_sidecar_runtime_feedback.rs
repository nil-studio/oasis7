use super::*;
use crate::runtime::{
    CausedBy as RuntimeCausedBy, DomainEvent as RuntimeDomainEvent,
    WorldEvent as RuntimeWorldEvent, WorldEventBody as RuntimeWorldEventBody,
};
#[cfg(target_arch = "wasm32")]
use crate::simulator::ActionResult;
use crate::simulator::FeedbackEnvelopeV1;
use oasis7_simulator::Action as SimulatorAction;
use serde_json::Value as JsonValue;

#[cfg(test)]
thread_local! { static TERMINAL_CHECKPOINT_CUT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

impl RuntimeLlmSidecar {
    #[cfg(test)]
    pub(in crate::viewer::runtime_live) fn cut_terminal_checkpoint_for_test() {
        TERMINAL_CHECKPOINT_CUT.with(|cut| cut.set(true));
    }

    fn checkpoint_terminal_feedbacks_for_recovery(
        &self,
        outbox: &[crate::runtime::RuntimeFeedbackOutboxRecordV1],
    ) -> Result<Vec<FeedbackEnvelopeV1>, String> {
        self.provider_terminal_states
            .values()
            .filter(|terminal| matches!(terminal.status.as_str(), "rejected" | "failed"))
            .map(|terminal| {
                // Native outbox truth wins over optional compatibility metadata.
                if let Some(native) = outbox.iter().find(|record| {
                    record.agent_subject == terminal.agent_id
                        && record.agent_session_id == terminal.agent_session_id
                        && record.agent_turn_id == terminal.agent_turn_id
                        && record.decision_request_id == terminal.decision_request_id
                        && record.request_digest == terminal.request_digest
                        && record.payload["status"] == terminal.status
                }) {
                    FeedbackEnvelopeV1::validate_value(&native.payload)
                        .map_err(|e| e.to_string())?;
                    if terminal
                        .feedback
                        .as_ref()
                        .is_some_and(|feedback| feedback.feedback_seq != native.feedback_seq)
                    {
                        return Err("terminal/native feedback sequence conflict".to_string());
                    }
                    return Ok(None);
                }
                let feedback = terminal
                    .feedback
                    .clone()
                    .ok_or_else(|| "legacy terminal feedback envelope unavailable".to_string())?;
                if feedback.agent_subject != terminal.agent_id
                    || feedback.agent_session_id != terminal.agent_session_id
                    || feedback.agent_turn_id != terminal.agent_turn_id
                    || feedback.decision_request_id != terminal.decision_request_id
                    || feedback.request_digest.as_str() != terminal.request_digest
                    || feedback.status != terminal.status
                    || feedback.reject_reason != terminal.reject_reason
                    || Some(&feedback.feedback_id) != terminal.feedback_id.as_ref()
                    || feedback.feedback_seq == 0
                    || feedback.runtime_receipt_id.is_some()
                    || feedback.candidate_action_id.is_some()
                    || feedback.provenance != "runtime_authoritative"
                {
                    return Err("checkpoint terminal feedback identity invalid".to_string());
                }
                FeedbackEnvelopeV1::validate_value(
                    &serde_json::to_value(&feedback).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                Ok(Some(feedback))
            })
            .collect::<Result<Vec<_>, String>>()
            .map(|values| values.into_iter().flatten().collect())
    }

    pub(in crate::viewer::runtime_live) fn pending_provider_action_for_recovery(
        &self,
    ) -> Option<(u64, String, RuntimeProviderActionContext)> {
        self.pending_actions
            .iter()
            .find_map(|(action_id, pending)| {
                pending
                    .cognition
                    .clone()
                    .map(|cognition| (*action_id, pending.agent_id.clone(), cognition))
            })
    }

    pub(in crate::viewer::runtime_live) fn schedule_provider_stale_replan(
        &mut self,
        agent_id: &str,
        parent_agent_turn_id: &str,
        parent_decision_request_id: &str,
    ) -> bool {
        let state = self
            .provider_stale_replans
            .entry(agent_id.to_string())
            .or_insert_with(|| ProviderStaleReplanState {
                count: 0,
                pending_cause: None,
            });
        if state.count >= MAX_PROVIDER_STALE_REPLANS {
            return false;
        }
        state.count = state.count.saturating_add(1);
        state.pending_cause = Some(ProviderStaleReplanCause {
            parent_agent_turn_id: parent_agent_turn_id.to_string(),
            parent_decision_request_id: parent_decision_request_id.to_string(),
            count: state.count,
        });
        self.persist_provider_lineage_best_effort();
        true
    }

    pub(in crate::viewer::runtime_live) fn provider_stale_replan_cause(
        &self,
        agent_id: &str,
    ) -> Option<ProviderStaleReplanCause> {
        self.provider_stale_replans
            .get(agent_id)
            .and_then(|state| state.pending_cause.clone())
    }

    pub(in crate::viewer::runtime_live) fn mark_provider_stale_replan_dispatched(
        &mut self,
        agent_id: &str,
    ) {
        if let Some(state) = self.provider_stale_replans.get_mut(agent_id) {
            state.pending_cause = None;
            self.persist_provider_lineage_best_effort();
        }
    }

    pub(in crate::viewer::runtime_live) fn clear_provider_stale_replans(&mut self, agent_id: &str) {
        if self.provider_stale_replans.remove(agent_id).is_some() {
            self.persist_provider_lineage_best_effort();
        }
    }

    pub(in crate::viewer::runtime_live) fn provider_stale_replan_exhausted_agent(
        &self,
    ) -> Option<String> {
        self.provider_stale_replans
            .iter()
            .find(|(_, state)| state.count >= MAX_PROVIDER_STALE_REPLANS)
            .map(|(agent_id, _)| agent_id.clone())
    }

    pub(in crate::viewer::runtime_live) fn track_action(
        &mut self,
        action_id: u64,
        agent_id: String,
        action: SimulatorAction,
        cognition: Option<RuntimeProviderActionContext>,
    ) {
        self.pending_actions.insert(
            action_id,
            RuntimePendingAction {
                agent_id,
                action,
                cognition,
                feedback_emitted: false,
            },
        );
        self.persist_provider_lineage_best_effort();
    }

    pub(in crate::viewer::runtime_live) fn notify_action_result(
        &mut self,
        action_id: u64,
        _event: WorldEvent,
        rejected: bool,
    ) {
        let Some(mut pending) = self.pending_actions.remove(&action_id) else {
            return;
        };
        if pending.feedback_emitted {
            self.pending_actions.insert(action_id, pending);
            return;
        }
        // A provider-backed action is finalized from the Runtime receipt
        // before the corresponding world event reaches this adapter.  The
        // later ActionAccepted callback is an informational replay and must
        // not resurrect an already terminal single-flight action.
        if let Some(cognition) = pending.cognition.as_ref()
            && self.provider_terminal_matches_request(
                pending.agent_id.as_str(),
                &cognition.request.request_context,
            )
        {
            self.persist_provider_lineage_best_effort();
            return;
        }
        #[cfg(target_arch = "wasm32")]
        let action_result = ActionResult {
            action: pending.action.clone(),
            action_id,
            success: !rejected,
            event: _event,
        };
        #[cfg(target_arch = "wasm32")]
        if let Some(RuntimeDecisionRunner::Builtin(runner)) = self.runner.as_mut() {
            let _ = runner.notify_action_result(pending.agent_id.as_str(), &action_result);
        }
        // Native Builtin and ProviderBacked actors both use the typed Runtime
        // feedback seam. Sending the legacy action result here would bypass
        // receipt identity and make the two production lanes diverge.
        pending.feedback_emitted = true;
        if rejected {
            self.clear_provider_stale_replans(pending.agent_id.as_str());
            self.release_provider_turn(pending.agent_id.as_str());
        } else {
            // ActionAccepted is an intermediate Runtime acknowledgement. The
            // pending action and provider turn remain occupied until a real
            // Runtime receipt/terminal callback is supplied.
            self.pending_actions.insert(action_id, pending);
        }
        self.persist_provider_lineage_best_effort();
    }

    /// Finalize a Runtime-committed provider action only after the sidecar
    /// release is durably checkpointed. Runtime owns the receipt, so a
    /// checkpoint failure must retain the exact pending action and identity
    /// for an idempotent retry instead of silently dropping the provider turn.
    pub(in crate::viewer::runtime_live) fn finalize_provider_action_with_feedback_checked(
        &mut self,
        action_id: u64,
        feedback: FeedbackEnvelopeV1,
    ) -> Result<FeedbackEnvelopeV1, String> {
        let Some(pending) = self.pending_actions.remove(&action_id) else {
            let terminal = self.provider_terminal_states.values().find(|terminal| {
                terminal.feedback_id.as_deref() == Some(feedback.feedback_id.as_str())
                    && terminal.status == feedback.status
            });
            return terminal
                .map(|_| feedback)
                .ok_or_else(|| {
                    "provider cognition single-flight action is missing during Runtime receipt finalization"
                        .to_string()
                });
        };
        let Some(cognition) = pending.cognition.clone() else {
            // Builtin/legacy actions have no provider envelope to finalize.
            self.release_provider_turn(pending.agent_id.as_str());
            return Ok(feedback);
        };

        let agent_id = pending.agent_id.clone();
        let terminal_backup = self
            .provider_terminal_states
            .get(agent_id.as_str())
            .cloned();
        let wake_recovery_backup = self
            .provider_wake_recovery_pending
            .get(agent_id.as_str())
            .cloned();
        self.record_provider_terminal_state_for_request(
            agent_id.as_str(),
            &cognition.request.request_context,
            feedback.status.as_str(),
            feedback.reject_reason.clone(),
            Some(feedback.feedback_id.clone()),
        );
        if self.has_pending_runtime_wake_for_agent(agent_id.as_str()) {
            let status = if feedback.status == "committed" {
                crate::runtime::ContinuationStatusV1::Completed
            } else {
                crate::runtime::ContinuationStatusV1::Rejected
            };
            self.provider_wake_recovery_pending.insert(
                agent_id.clone(),
                lineage_persistence::ProviderWakeRecoveryPending {
                    active: cognition.request.clone(),
                    status,
                    reason: feedback
                        .reject_reason
                        .clone()
                        .unwrap_or_else(|| feedback.status.clone()),
                },
            );
        }
        self.provider_held_decisions.remove(agent_id.as_str());
        if let Err(error) = self.release_provider_turn_checked(agent_id.as_str()) {
            self.pending_actions.insert(action_id, pending);
            if let Some(previous) = terminal_backup {
                self.provider_terminal_states
                    .insert(agent_id.clone(), previous);
            } else {
                self.provider_terminal_states.remove(agent_id.as_str());
            }
            if let Some(previous) = wake_recovery_backup {
                self.provider_wake_recovery_pending
                    .insert(agent_id.clone(), previous);
            } else {
                self.provider_wake_recovery_pending
                    .remove(agent_id.as_str());
            }
            // Keep the exact action/context in memory even when the previous
            // checkpoint is unavailable; a later receipt-driven retry can
            // complete the same transition without another provider call.
            self.persist_provider_lineage_best_effort();
            return Err(error);
        }
        Ok(feedback)
    }

    /// Apply intents captured by a native actor only after the Runtime has
    /// read back and verified the committed receipt lineage. Rejected or
    /// pending dispositions never enter the MemoryWriteStore. Native actors
    /// retain their awaiting outcome in AsyncAgentRunner. The `wasm32` cfg
    /// branch below is legacy source inside `viewer::runtime_live`, a module
    /// that is itself excluded from wasm32 builds. Pass `None` for a canonical
    /// WorldService receipt absent from this local World: that path preserves
    /// lifecycle/replay handling without projecting local memory.
    pub(in crate::viewer::runtime_live) fn consume_provider_memory_after_receipt(
        &mut self,
        world: Option<&crate::runtime::World>,
        agent_id: &str,
        feedback: FeedbackEnvelopeV1,
        receipt: &crate::runtime::RuntimeReceiptLineageV1,
        memory_write_intents: &[crate::simulator::MemoryWriteIntent],
    ) -> Result<(), String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = memory_write_intents;
            let Some(runner) = self
                .runner
                .as_mut()
                .and_then(RuntimeDecisionRunner::async_runner_mut)
            else {
                return Ok(());
            };
            if let Some(world) = world {
                let authority = super::super::harness_authority::RuntimeWorldAuthority::new(world);
                runner
                    .consume_runtime_feedback_with_world_readback_from_outcome(
                        agent_id,
                        feedback,
                        receipt,
                        &authority,
                        &mut self.provider_memory_store,
                    )
                    .map_err(|error| {
                        format!("provider memory receipt gate rejected intents: {error}")
                    })?;
                Ok(())
            } else {
                // Canonical WorldService receipts are not necessarily present
                // in this process's local Runtime World. Preserve Harness
                // lifecycle/replay handling, but do not infer memory authority
                // from the service DTO or commit its memory intents locally.
                runner
                    .consume_runtime_feedback_with_lineage(
                        agent_id,
                        feedback,
                        Some(receipt),
                        &mut self.provider_memory_store,
                    )
                    .map_err(|error| {
                        format!("provider feedback lifecycle gate rejected: {error}")
                    })?;
                Ok(())
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = world;
            if feedback.status != "committed" {
                return Ok(());
            }
            let Some(context) = self.provider_contexts.get(agent_id) else {
                return Err("WASM memory receipt gate has no active cognition context".to_string());
            };
            if feedback.agent_subject != agent_id
                || feedback.agent_session_id != context.request_context.agent_session_id
                || feedback.agent_turn_id != context.request_context.agent_turn_id
                || feedback.decision_request_id != context.request_context.decision_request_id
                || feedback.request_digest != context.request_context.request_digest
                || feedback.runtime_receipt_id.as_deref() != Some(receipt.receipt_id.as_str())
            {
                return Err("WASM memory receipt gate context mismatch".to_string());
            }
            receipt
                .validate()
                .map_err(|error| format!("WASM memory receipt lineage invalid: {error}"))?;
            let source = match self.runner.as_ref() {
                Some(RuntimeDecisionRunner::Builtin(_)) => "builtin",
                Some(RuntimeDecisionRunner::ProviderBacked(_)) => "provider",
                None => return Err("WASM memory runner is unavailable".to_string()),
            };
            let policy_context = MemoryWritePolicyContextV1 {
                agent_id: context.request_context.agent_subject.clone(),
                agent_session_id: context.request_context.agent_session_id.clone(),
                agent_turn_id: context.request_context.agent_turn_id.clone(),
                request_digest: context.request_context.request_digest.to_string(),
                source: source.to_string(),
                provenance: if source == "provider" {
                    "provider_unverified".to_string()
                } else {
                    "builtin_unverified".to_string()
                },
            };
            let policy = MemoryWriteIntentPolicyV1::default();
            for intent in memory_write_intents {
                let intent = MemoryWriteIntentV1 {
                    schema_version: 1,
                    scope: intent.scope.clone(),
                    summary: Some(intent.summary.clone()),
                    tags: intent.tags.clone(),
                    compatibility_reason: None,
                };
                let normalized = match policy.normalize(intent, &policy_context) {
                    Ok(intent) => intent,
                    Err(error) => {
                        tracing::warn!(
                            agent_id,
                            code = error.code(),
                            error = %error,
                            "WASM memory intent rejected after Runtime commit"
                        );
                        continue;
                    }
                };
                let digest = match policy.intent_digest(&normalized, &policy_context) {
                    Ok(digest) => digest,
                    Err(error) => {
                        tracing::warn!(
                            agent_id,
                            code = error.code(),
                            error = %error,
                            "WASM memory intent digest rejected after Runtime commit"
                        );
                        continue;
                    }
                };
                if let Err(error) = self
                    .provider_memory_store
                    .apply_runtime_receipt_with_context(
                        normalized,
                        digest,
                        receipt,
                        Some(&policy_context),
                    )
                {
                    tracing::warn!(
                        agent_id,
                        code = error.code(),
                        error = %error,
                        "WASM memory projection rejected after Runtime commit"
                    );
                }
            }
            Ok(())
        }
    }

    /// Close a provider response that did not produce a Runtime action (for
    /// example an unmappable or non-retryable candidate).
    pub(in crate::viewer::runtime_live) fn fail_provider_turn(&mut self, agent_id: &str) {
        if let Some(context) = self.provider_contexts.get(agent_id) {
            self.provider_memory_store.ignore_corrections_for_decision(
                agent_id,
                &context.request_context.decision_request_id,
                context.request_context.request_digest.as_str(),
                "provider_turn_failed_without_runtime_effect",
            );
        }
        self.clear_provider_stale_replans(agent_id);
        self.provider_held_decisions.remove(agent_id);
        self.release_provider_turn(agent_id);
    }

    /// Close a successful provider response that cannot enter the typed
    /// Runtime action lane (for example a query or module command).  The
    /// Runtime failure is recorded before the Viewer exposes the rejected
    /// disposition, so these responses cannot leave a running turn behind.
    pub(in crate::viewer::runtime_live) fn fail_provider_cognition_turn(
        &self,
        world: &mut RuntimeWorld,
        agent_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let Some(context) = self.provider_recovery_context(agent_id) else {
            return Err(format!(
                "provider recovery context missing for agent {agent_id}"
            ));
        };
        super::async_support::runtime_provider_failure(world, &context, reason)
    }

    /// Release a provider identity only after the Runtime failure has been
    /// durably accepted. If sidecar persistence fails, restore the in-memory
    /// identity so the next pass can retry the same terminal transition.
    pub(in crate::viewer::runtime_live) fn release_provider_turn_checked(
        &mut self,
        agent_id: &str,
    ) -> Result<(), String> {
        let context = self.provider_recovery_context(agent_id);
        // Establish the wake recovery record before actor release. If the
        // actor authority itself returns an error, the Runtime wake and the
        // exact provider identity still need a durable retry path.
        if !self.provider_wake_recovery_pending.contains_key(agent_id)
            && self.has_pending_runtime_wake_for_agent(agent_id)
            && let Some(context) = context.as_ref()
        {
            self.provider_wake_recovery_pending.insert(
                agent_id.to_string(),
                lineage_persistence::ProviderWakeRecoveryPending {
                    active: context.clone(),
                    status: crate::runtime::ContinuationStatusV1::Rejected,
                    reason: "provider_release_waiting_runtime_wake".to_string(),
                },
            );
        }
        if let Some(context) = context.as_ref()
            && let Some(runner) = self
                .runner
                .as_mut()
                .and_then(RuntimeDecisionRunner::async_runner_mut)
            && let Err(error) = runner.expire_runtime_turn(
                context.request_context.agent_subject.as_str(),
                context.request_context.agent_session_id.as_str(),
                context.request_context.agent_turn_id.as_str(),
                context.request_context.decision_request_id.as_str(),
                context.request_context.request_digest.to_string().as_str(),
            )
        {
            let message = error.to_string();
            if !message.contains("unknown pending Runtime turn") {
                self.persist_provider_lineage_best_effort();
                return Err(format!("provider actor release failed: {error}"));
            }
        }

        let context_backup = self.provider_contexts.get(agent_id).cloned();
        let active_backup = self.provider_active_turns.get(agent_id).cloned();
        let wait_backup = self.provider_wait_until.get(agent_id).copied();
        let held_backup = self.provider_held_decisions.get(agent_id).cloned();
        let recovery_backup = self.provider_recovery_pending.get(agent_id).cloned();
        let wake_recovery_backup = self.provider_wake_recovery_pending.get(agent_id).cloned();
        let exhausted_backup = self.provider_transport_exhausted.contains(agent_id);
        let cognition_lease_backup = self.provider_cognition_leases.get(agent_id).cloned();
        self.provider_contexts.remove(agent_id);
        self.provider_active_turns.remove(agent_id);
        self.provider_wait_until.remove(agent_id);
        self.provider_held_decisions.remove(agent_id);
        self.provider_recovery_pending.remove(agent_id);
        self.provider_transport_exhausted.remove(agent_id);
        self.provider_cognition_leases.remove(agent_id);
        if let Err(error) = self.persist_provider_lineage() {
            if let Some(context) = context_backup {
                self.provider_contexts.insert(agent_id.to_string(), context);
            }
            if let Some(context) = active_backup {
                self.provider_active_turns
                    .insert(agent_id.to_string(), context);
            }
            if let Some(wait_until) = wait_backup {
                self.provider_wait_until
                    .insert(agent_id.to_string(), wait_until);
            }
            if let Some(decision) = held_backup {
                self.provider_held_decisions
                    .insert(agent_id.to_string(), decision);
            }
            if let Some(recovery) = recovery_backup {
                self.provider_recovery_pending
                    .insert(agent_id.to_string(), recovery);
            }
            if let Some(wake_recovery) = wake_recovery_backup {
                self.provider_wake_recovery_pending
                    .insert(agent_id.to_string(), wake_recovery);
            } else {
                self.provider_wake_recovery_pending.remove(agent_id);
            }
            if exhausted_backup {
                self.provider_transport_exhausted
                    .insert(agent_id.to_string());
            }
            if let Some(lease) = cognition_lease_backup {
                self.provider_cognition_leases
                    .insert(agent_id.to_string(), lease);
            }
            return Err(format!(
                "provider lineage release persistence failed: {error}"
            ));
        }
        Ok(())
    }

    /// Create a typed terminal disposition for a provider request that never
    /// produced a Runtime action. The request context remains available until
    /// this method builds the envelope, so failures cannot silently fall back
    /// to legacy action feedback.
    pub(in crate::viewer::runtime_live) fn fail_provider_turn_with_feedback(
        &mut self,
        agent_id: &str,
        status: &str,
        reject_reason: impl Into<String>,
    ) -> Result<Option<FeedbackEnvelopeV1>, String> {
        if !matches!(status, "rejected" | "failed") {
            return Ok(None);
        }
        let context = self.provider_recovery_context(agent_id);
        let reject_reason = reject_reason.into();
        let Some(context) = context else {
            // Successful Wait compensation has already closed its authority
            // context. The polling adapter can still retain its error result;
            // release that result without inventing another feedback identity.
            if self.provider_held_decisions.contains_key(agent_id)
                && !self.provider_active_turns.contains_key(agent_id)
                && !self.provider_cognition_leases.contains_key(agent_id)
                && !self.provider_recovery_pending.contains_key(agent_id)
                && !self
                    .provider_continuation_recovery_pending
                    .contains_key(agent_id)
                && !self.has_pending_runtime_wake_for_agent(agent_id)
            {
                self.release_provider_turn_checked(agent_id)?;
            }
            return Ok(None);
        };
        let existing = self
            .provider_terminal_states
            .get(agent_id)
            .filter(|terminal| {
                self.provider_terminal_matches_request(agent_id, &context.request_context)
                    && terminal.status == status
                    && terminal.reject_reason.as_deref() == Some(reject_reason.as_str())
            });
        let previous_sequences = self.provider_feedback_seq.clone();
        let previous_session_sequences = self.provider_feedback_seq_by_session.clone();
        let previous_terminals = self.provider_terminal_states.clone();
        let feedback = if let Some(terminal) = existing {
            terminal.feedback.clone().ok_or_else(|| {
                "terminal feedback envelope unavailable; cannot infer legacy sequence".to_string()
            })?
        } else {
            let feedback = self.stage_provider_feedback_for_request(
                &context.request_context,
                None,
                status,
                None,
                None,
                Some(reject_reason.clone()),
            );
            self.provider_terminal_states.insert(
                agent_id.to_string(),
                lineage_persistence::ProviderTerminalState {
                    agent_id: agent_id.to_string(),
                    agent_session_id: context.request_context.agent_session_id.clone(),
                    agent_turn_id: context.request_context.agent_turn_id.clone(),
                    decision_request_id: context.request_context.decision_request_id.clone(),
                    request_digest: context.request_context.request_digest.to_string(),
                    status: status.to_string(),
                    reject_reason: Some(reject_reason.clone()),
                    feedback_id: Some(feedback.feedback_id.clone()),
                    feedback: Some(feedback.clone()),
                },
            );
            feedback
        };
        // Sequence, full envelope and exact terminal identity are one checkpoint.
        // A failed write cannot leave a consumed sequence or release the actor.
        // Exact retries also write strictly; an in-memory marker is not durability.
        if let Err(error) = self.persist_provider_lineage() {
            self.provider_feedback_seq = previous_sequences;
            self.provider_feedback_seq_by_session = previous_session_sequences;
            self.provider_terminal_states = previous_terminals;
            return Err(error);
        }
        #[cfg(test)]
        if TERMINAL_CHECKPOINT_CUT.with(|cut| cut.replace(false)) {
            return Err("test crash cut after strict terminal checkpoint".to_string());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(runner) = self
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
        {
            runner
                .consume_runtime_feedback_with_lineage(
                    agent_id,
                    feedback.clone(),
                    None,
                    &mut self.provider_memory_store,
                )
                .map_err(|error| format!("provider terminal feedback gate rejected: {error}"))?;
        }
        {
            if reject_reason != "stale_base" {
                // Finalize only corrections bound to this exact request before
                // the terminal marker is persisted or its identity is released.
                self.provider_memory_store.ignore_corrections_for_decision(
                    agent_id,
                    &context.request_context.decision_request_id,
                    context.request_context.request_digest.as_str(),
                    "provider_turn_failed_without_runtime_effect",
                );
            }
        }
        if reject_reason != "stale_base" {
            self.clear_provider_stale_replans(agent_id);
        }
        self.release_provider_turn_checked(agent_id)?;
        Ok(Some(feedback))
    }

    /// Build a stable terminal feedback envelope without releasing the
    /// provider identity. Recovery callers must enqueue/deliver this envelope
    /// and persist the sidecar release as separate, checked transitions.
    pub(in crate::viewer::runtime_live) fn provider_failure_feedback(
        &mut self,
        agent_id: &str,
        status: &str,
        reject_reason: &str,
    ) -> Option<FeedbackEnvelopeV1> {
        if !matches!(status, "rejected" | "failed") {
            return None;
        }
        let context = self.provider_recovery_context(agent_id)?;
        let feedback_id = self
            .provider_terminal_states
            .get(agent_id)
            .filter(|terminal| {
                self.provider_terminal_matches_request(agent_id, &context.request_context)
                    && terminal.status == status
                    && terminal.reject_reason.as_deref() == Some(reject_reason)
            })
            .and_then(|terminal| terminal.feedback_id.clone());
        Some(self.provider_feedback_for_request(
            &context.request_context,
            None,
            status,
            None,
            feedback_id,
            Some(reject_reason.to_string()),
        ))
    }

    pub(in crate::viewer::runtime_live) fn provider_feedback(
        &mut self,
        cognition: &RuntimeProviderActionContext,
        candidate_action_id: Option<u64>,
        status: &str,
        runtime_receipt_id: Option<String>,
        feedback_id: Option<String>,
        reject_reason: Option<String>,
    ) -> FeedbackEnvelopeV1 {
        self.provider_feedback_for_request(
            &cognition.request.request_context,
            candidate_action_id,
            status,
            runtime_receipt_id,
            feedback_id,
            reject_reason,
        )
    }

    pub(in crate::viewer::runtime_live) fn provider_feedback_for_request(
        &mut self,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        candidate_action_id: Option<u64>,
        status: &str,
        runtime_receipt_id: Option<String>,
        feedback_id: Option<String>,
        reject_reason: Option<String>,
    ) -> FeedbackEnvelopeV1 {
        let feedback = self.stage_provider_feedback_for_request(
            request,
            candidate_action_id,
            status,
            runtime_receipt_id,
            feedback_id,
            reject_reason,
        );
        if matches!(status, "committed" | "rejected" | "failed") {
            self.record_provider_terminal_state_for_request(
                request.agent_subject.as_str(),
                request,
                status,
                feedback.reject_reason.clone(),
                Some(feedback.feedback_id.clone()),
            );
        }
        self.persist_provider_lineage_best_effort();
        feedback
    }

    fn stage_provider_feedback_for_request(
        &mut self,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        candidate_action_id: Option<u64>,
        status: &str,
        runtime_receipt_id: Option<String>,
        feedback_id: Option<String>,
        reject_reason: Option<String>,
    ) -> FeedbackEnvelopeV1 {
        let session_key = lineage::provider_feedback_session_key(
            request.agent_subject.as_str(),
            request.agent_session_id.as_str(),
        );
        let feedback_seq = self
            .provider_feedback_seq_by_session
            .entry(session_key)
            // Compatibility feedback cursors are intentionally scoped to the
            // Runtime Agent session. The legacy per-agent cursor remains a
            // high-water mark for migration/observability, but must never
            // seed a fresh session and leak sequence state across sessions.
            .or_insert(1);
        let sequence = (*feedback_seq).max(1);
        *feedback_seq = sequence.saturating_add(1);
        self.provider_feedback_seq
            .entry(request.agent_subject.clone())
            .and_modify(|current| *current = (*current).max(sequence.saturating_add(1)))
            .or_insert(sequence.saturating_add(1));
        let feedback_id = feedback_id.unwrap_or_else(|| {
            format!(
                "runtime-feedback:{}:{}:{}",
                request.agent_subject, request.agent_session_id, sequence
            )
        });
        FeedbackEnvelopeV1 {
            feedback_id,
            feedback_seq: sequence,
            agent_subject: request.agent_subject.clone(),
            agent_session_id: request.agent_session_id.clone(),
            agent_turn_id: request.agent_turn_id.clone(),
            decision_request_id: request.decision_request_id.clone(),
            candidate_action_id,
            runtime_receipt_id,
            status: status.to_string(),
            request_digest: request.request_digest.clone(),
            reject_reason,
            provenance: "runtime_authoritative".to_string(),
        }
    }

    pub(in crate::viewer::runtime_live) fn service_feedback_is_builtin(&self) -> bool {
        matches!(self.runner, Some(RuntimeDecisionRunner::Builtin(_)))
    }
    pub(in crate::viewer::runtime_live) fn service_feedback_transport(
        &self,
    ) -> Result<ProviderLoopbackHttpClient, String> {
        let settings = provider_settings_from_env()?
            .ok_or("provider settings missing for feedback delivery")?;
        ProviderLoopbackHttpClient::new_with_transport(
            &settings.base_url,
            settings.auth_token.as_deref(),
            settings.connect_timeout_ms,
            &settings.provider_transport,
        )
        .map_err(|e| e.to_string())
    }

    /// Deliver the Runtime-committed feedback through the same provider
    /// transport used for decisions. The receipt lineage has already been
    /// read back and verified by Runtime before this method is called.
    pub(in crate::viewer::runtime_live) fn deliver_provider_cognition_feedback(
        &self,
        payload: &JsonValue,
    ) -> Result<(), String> {
        if matches!(self.runner, Some(RuntimeDecisionRunner::Builtin(_))) {
            // Builtin responses are already inside the host Harness. Runtime
            // feedback is still allocated/acked for one shared lifecycle, but
            // there is no external provider endpoint to call.
            return Ok(());
        }
        let settings = provider_settings_from_env()?
            .ok_or_else(|| "provider settings disappeared before feedback delivery".to_string())?;
        let client = ProviderLoopbackHttpClient::new_with_transport(
            settings.base_url.as_str(),
            settings.auth_token.as_deref(),
            settings.connect_timeout_ms,
            settings.provider_transport.as_str(),
        )
        .map_err(|error| format!("provider feedback client initialization failed: {error}"))?;
        let ack = client
            .submit_feedback_context_payload(payload)
            .map_err(|error| format!("provider cognition feedback delivery failed: {error}"))?;
        if !ack.ok {
            return Err(format!(
                "provider rejected Runtime cognition feedback: {}",
                ack.error
                    .or(ack.error_code)
                    .unwrap_or_else(|| "unknown provider feedback error".to_string())
            ));
        }
        Ok(())
    }

    fn release_provider_turn(&mut self, agent_id: &str) {
        let context = self.provider_recovery_context(agent_id);
        if let Some(context) = context.as_ref() {
            self.expire_async_runtime_turn(context);
        }
        if !self.provider_wake_recovery_pending.contains_key(agent_id)
            && self.has_pending_runtime_wake_for_agent(agent_id)
            && let Some(context) = context
        {
            self.provider_wake_recovery_pending.insert(
                agent_id.to_string(),
                lineage_persistence::ProviderWakeRecoveryPending {
                    active: context,
                    status: crate::runtime::ContinuationStatusV1::Rejected,
                    reason: "provider_release_waiting_runtime_wake".to_string(),
                },
            );
        }
        self.provider_active_turns.remove(agent_id);
        self.provider_wait_until.remove(agent_id);
        self.provider_contexts.remove(agent_id);
        self.provider_held_decisions.remove(agent_id);
        self.provider_cognition_leases.remove(agent_id);
        self.persist_provider_lineage_best_effort();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::viewer::runtime_live) fn expire_async_runtime_turn(
        &mut self,
        context: &cognition_context::ProviderContextState,
    ) {
        if let Some(runner) = self
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
        {
            let _ = runner.expire_runtime_turn(
                context.request_context.agent_subject.as_str(),
                context.request_context.agent_session_id.as_str(),
                context.request_context.agent_turn_id.as_str(),
                context.request_context.decision_request_id.as_str(),
                context.request_context.request_digest.to_string().as_str(),
            );
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(in crate::viewer::runtime_live) fn expire_async_runtime_turn(
        &mut self,
        _context: &cognition_context::ProviderContextState,
    ) {
    }

    pub(in crate::viewer::runtime_live) fn schedule_provider_wait(
        &mut self,
        agent_id: &str,
        now: u64,
        ticks: u64,
    ) {
        let wait_until = now.saturating_add(ticks.max(1));
        self.provider_wait_until
            .insert(agent_id.to_string(), wait_until);
        self.persist_provider_lineage_best_effort();
    }

    pub(in crate::viewer::runtime_live) fn release_due_provider_waits(
        &mut self,
        world: &mut RuntimeWorld,
    ) -> Result<(), String> {
        let now = world.state().time;
        let due_agents: Vec<String> = self
            .provider_wait_until
            .iter()
            .filter_map(|(agent_id, wait_until)| (*wait_until <= now).then_some(agent_id.clone()))
            .collect();
        for agent_id in due_agents {
            if let Some(lease) = self.provider_cognition_lease(agent_id.as_str()) {
                if self.provider_service_required {
                    let context = self
                        .provider_contexts
                        .get(agent_id.as_str())
                        .cloned()
                        .ok_or("canonical wait lease release requires original context")?;
                    self.validate_provider_cognition_lease_for_request(
                        world,
                        &agent_id,
                        &context.request_context,
                        &lease,
                        "wait release",
                    )?;
                    self.release_provider_lease_at_authority(
                        world,
                        &context.request_context,
                        &lease,
                    )?;
                } else {
                    let economy = world.cognition_economy().map_err(|error| {
                        format!(
                            "cognition lease wait release economy read failed for {}: {error:?}",
                            lease.lease_id
                        )
                    })?;
                    if let Some(runtime_lease) = economy.leases.get(lease.lease_id.as_str()) {
                        if runtime_lease.lease_id != lease.lease_id
                            || runtime_lease.idempotency_key != lease.idempotency_key
                            || runtime_lease.account_id != lease.account_id
                            || runtime_lease.agent_id != lease.agent_id
                            || runtime_lease.agent_session_id != lease.agent_session_id
                            || runtime_lease.agent_turn_id != lease.agent_turn_id
                            || runtime_lease.decision_request_id != lease.decision_request_id
                            || runtime_lease.request_digest != lease.request_digest
                            || runtime_lease.quote != lease.quote
                            || runtime_lease.reserved_amount != lease.reserved_amount
                        {
                            return Err(format!(
                                "cognition lease wait release Runtime identity mismatch for {agent_id}"
                            ));
                        }
                        if runtime_lease.status == crate::runtime::CognitionLeaseStatusV1::Reserved
                        {
                            self.validate_provider_cognition_lease_for_agent(
                                world,
                                agent_id.as_str(),
                                &lease,
                                "wait release",
                            )?;
                            world
                            .release_cognition_lease(lease.lease_id.as_str())
                            .map_err(|error| {
                                format!(
                                    "cognition lease release failed after provider wait for {}: {error:?}",
                                    lease.lease_id
                                )
                            })?;
                        }
                    }
                }
                self.clear_provider_cognition_lease(agent_id.as_str());
            }
            if let Some(context) = self.provider_contexts.get(agent_id.as_str()).cloned() {
                let feedback = self.provider_feedback_for_request(
                    &context.request_context,
                    None,
                    "rejected",
                    None,
                    None,
                    Some("no_effect".to_string()),
                );
                world
                    .enqueue_runtime_feedback(crate::simulator::agent_api_compat::feedback_to_api(
                        &feedback,
                    ))
                    .map_err(|error| {
                        format!(
                            "Runtime feedback outbox enqueue failed after provider wait: {error:?}"
                        )
                    })?;
                self.record_provider_terminal_state(
                    agent_id.as_str(),
                    &context,
                    "rejected",
                    Some("no_effect".to_string()),
                    None,
                );
                self.clear_provider_stale_replans(agent_id.as_str());
            }
            self.release_provider_turn(agent_id.as_str());
        }
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn notify_action_result_if_needed(
        &mut self,
        runtime_event: &RuntimeWorldEvent,
        mapped_event: WorldEvent,
    ) {
        let Some(caused_by) = runtime_event.caused_by.as_ref() else {
            return;
        };
        let RuntimeCausedBy::Action(action_id) = caused_by else {
            return;
        };
        let rejected = matches!(
            runtime_event.body,
            RuntimeWorldEventBody::Domain(RuntimeDomainEvent::ActionRejected { .. })
        );
        self.notify_action_result(*action_id, mapped_event, rejected);
    }
}

impl crate::viewer::runtime_live::ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn recover_checkpoint_terminal_feedback(
        &mut self,
    ) -> Result<(), String> {
        let native_outbox = self
            .world
            .runtime_feedback_outbox()
            .map_err(|e| format!("{e:?}"))?;
        let feedbacks = self
            .llm_sidecar
            .checkpoint_terminal_feedbacks_for_recovery(&native_outbox)?;
        let economy = self
            .world
            .cognition_economy()
            .map_err(|e| format!("{e:?}"))?;
        for feedback in feedbacks {
            let known = self
                .world
                .runtime_feedback_outbox()
                .map_err(|e| format!("{e:?}"))?
                .iter()
                .any(|record| {
                    record.agent_subject == feedback.agent_subject
                        && record.agent_session_id == feedback.agent_session_id
                        && record.agent_turn_id == feedback.agent_turn_id
                        && record.decision_request_id == feedback.decision_request_id
                        && record.request_digest == feedback.request_digest.as_str()
                        && record.payload["status"] == feedback.status
                        && record.payload["reject_reason"]
                            == serde_json::to_value(&feedback.reject_reason).unwrap()
                });
            let admitted = economy.leases.values().any(|lease| {
                lease.agent_id == feedback.agent_subject
                    && lease.agent_session_id == feedback.agent_session_id
                    && lease.agent_turn_id == feedback.agent_turn_id
                    && lease.decision_request_id == feedback.decision_request_id
                    && lease.request_digest == feedback.request_digest.as_str()
            });
            if !known && !admitted {
                return Err("terminal feedback lacks exact Runtime admission authority".to_string());
            }
            let mut transaction = self.world.clone();
            let record = transaction
                .allocate_runtime_feedback(crate::runtime::RuntimeFeedbackRequestV1 {
                    feedback_id: None,
                    agent_subject: feedback.agent_subject,
                    agent_session_id: feedback.agent_session_id,
                    agent_turn_id: feedback.agent_turn_id,
                    decision_request_id: feedback.decision_request_id,
                    candidate_action_id: None,
                    runtime_receipt_id: None,
                    status: feedback.status,
                    request_digest: feedback.request_digest.to_string(),
                    reject_reason: feedback.reject_reason,
                })
                .map_err(|e| format!("terminal feedback recovery allocation rejected: {e:?}"))?;
            if record.feedback_seq != feedback.feedback_seq {
                return Err("terminal feedback recovery sequence mismatch".to_string());
            }
            self.world = transaction;
        }
        self.drain_provider_feedback_outbox();
        Ok(())
    }
}
