use super::*;
use crate::simulator::FeedbackEnvelopeV1;
use serde_json::Value;

impl RuntimeLlmSidecar {
    /// Rebuild the viewer's allocation cursors from Runtime-owned cognition
    /// records.  The sidecar remains an in-memory transport adapter, but it
    /// must never restart at sequence one after a process restart.
    pub(in crate::viewer::runtime_live) fn hydrate_provider_lineage(
        &mut self,
        world: &RuntimeWorld,
    ) {
        if self.provider_lineage_hydrated {
            return;
        }
        if self.provider_lineage_store.is_some()
            && !self.provider_lineage_restored
            && let Err(error) = self.restore_provider_lineage(world)
        {
            self.provider_lineage_recovery_pending = Some(error.clone());
            tracing::warn!(error, "provider lineage checkpoint restore failed");
        }
        if self.provider_lineage_recovery_pending.is_some() {
            // Do not rebuild fresh provider contexts from Runtime projections
            // while the durable sidecar checkpoint is undecodable. That would
            // erase active identity/recovery fences and permit a duplicate
            // provider invocation after restart.
            self.provider_lineage_hydrated = true;
            return;
        }
        if let Err(error) = self.sync_runtime_wakes(world) {
            self.provider_lineage_recovery_pending = Some(error.clone());
            // Runtime wake state is authoritative. Preserve the recovery
            // fence when a sidecar checkpoint is configured so a restart
            // cannot reinterpret an unreadable projection as no work.
            self.persist_provider_lineage_best_effort();
            tracing::warn!(error, "Runtime cognition wake projection unavailable");
            // Keep hydration incomplete. The caller observes the recovery
            // fence below, and a fresh sidecar can retry once Runtime has
            // repaired the authoritative projection.
            return;
        }
        self.provider_lineage_hydrated = true;
        if self.provider_service_required {
            return;
        }
        let projection = world.cognition();
        let journal_head_seq = projection
            .get("cognition_journal")
            .and_then(|journal| journal.get("head_seq"))
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let mut identities = Vec::new();
        let mut feedbacks = Vec::new();

        if let Some(records) = projection.get("commit_records").and_then(Value::as_array) {
            identities.extend(records.iter().filter_map(|record| {
                Some((
                    record.get("agent_id")?.as_str()?.to_string(),
                    record
                        .get("agent_session_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    record
                        .get("agent_turn_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    record
                        .get("decision_request_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    record
                        .get("response_retry_seq")
                        .and_then(Value::as_u64)
                        .unwrap_or_default(),
                ))
            }));
        }

        if let Some(records) = projection.get("feedback_outbox").and_then(Value::as_object) {
            for record in records.values() {
                let Some(payload) = record.get("payload") else {
                    continue;
                };
                if let Ok(feedback) = serde_json::from_value::<FeedbackEnvelopeV1>(payload.clone())
                {
                    identities.push((
                        feedback.agent_subject.clone(),
                        feedback.agent_session_id.clone(),
                        feedback.agent_turn_id.clone(),
                        feedback.decision_request_id.clone(),
                        0,
                    ));
                    feedbacks.push(feedback);
                }
            }
        }

        for (agent_id, session_id, _turn_id, _request_id, retry_seq) in identities {
            if agent_id.is_empty() {
                continue;
            }
            if !session_id.is_empty() {
                self.provider_session_ids
                    .entry(agent_id.clone())
                    .or_insert(session_id);
            }
            let inferred_seq = retry_seq.max(journal_head_seq);
            if inferred_seq > 0 {
                let next = inferred_seq.saturating_add(1);
                self.provider_context_seq
                    .entry(agent_id)
                    .and_modify(|current| *current = (*current).max(next))
                    .or_insert(next);
            }
        }

        for feedback in feedbacks {
            let next = feedback.feedback_seq.max(1).saturating_add(1);
            let session_key = provider_feedback_session_key(
                feedback.agent_subject.as_str(),
                feedback.agent_session_id.as_str(),
            );
            self.provider_feedback_seq
                .entry(feedback.agent_subject.clone())
                .and_modify(|current| *current = (*current).max(next))
                .or_insert(next);
            self.provider_feedback_seq_by_session
                .entry(session_key)
                .and_modify(|current| *current = (*current).max(next))
                .or_insert(next);
        }

        // A historical stale rejection only proves that the old turn closed.
        // It does not prove that a replan is still pending: the corresponding
        // provider response may already have been retried, committed, or
        // terminally rejected. Pending replans are restored only from the
        // sidecar checkpoint, where their exact causal identity is retained.
    }

    pub(in crate::viewer::runtime_live) fn mark_provider_transport_exhausted(
        &mut self,
        agent_id: String,
    ) {
        self.provider_transport_exhausted.insert(agent_id);
        self.persist_provider_lineage_best_effort();
    }

    /// Close a lease restored for an old Runtime binding before a stale
    /// replan can build a new request identity. Restore itself accepts an
    /// immutable world reference so it remains usable during bootstrap; the
    /// first mutable prepare pass performs this cleanup before dispatch.
    pub(in crate::viewer::runtime_live) fn release_binding_changed_provider_leases(
        &mut self,
        world: &mut RuntimeWorld,
    ) -> Result<(), String> {
        let stale_agents = self
            .provider_stale_replans
            .iter()
            // Once the replan request has been dispatched, the marker keeps
            // only its bounded count until the replacement response clears
            // it. Releasing the lease in that state would close the new
            // request's lease before its response can settle it.
            .filter(|(_, state)| state.pending_cause.is_some())
            .map(|(agent_id, _)| agent_id.clone())
            .collect::<Vec<_>>();
        let mut leases_to_clear = Vec::new();
        for agent_id in stale_agents {
            let Some(lease) = self.provider_cognition_leases.get(&agent_id).cloned() else {
                continue;
            };
            if self.provider_service_required {
                let context = self
                    .provider_active_turns
                    .get(&agent_id)
                    .or_else(|| self.provider_contexts.get(&agent_id))
                    .or_else(|| self.provider_retry_contexts.get(&agent_id))
                    .cloned()
                    .ok_or("canonical stale lease recovery requires original request")?;
                self.validate_provider_cognition_lease_for_request(
                    world,
                    &agent_id,
                    &context.request_context,
                    &lease,
                    "stale release",
                )?;
                self.release_provider_lease_at_authority(world, &context.request_context, &lease)?;
                leases_to_clear.push(agent_id);
                continue;
            }
            lease
                .validate()
                .map_err(|error| format!("stale provider cognition lease invalid: {error}"))?;
            let economy = world.cognition_economy().map_err(|error| {
                format!(
                    "stale provider cognition lease economy read failed for {agent_id}: {error:?}"
                )
            })?;
            let Some(runtime_lease) = economy.leases.get(lease.lease_id.as_str()) else {
                // Runtime already closed or discarded the old lease. There is
                // no economic effect left to release, so clear only this
                // sidecar mirror and continue the replan.
                leases_to_clear.push(agent_id);
                continue;
            };
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
                    "stale provider cognition lease Runtime identity mismatch for {agent_id}"
                ));
            }
            if runtime_lease.status == crate::runtime::CognitionLeaseStatusV1::Reserved {
                world
                    .release_cognition_lease(lease.lease_id.as_str())
                    .map_err(|error| {
                        format!(
                            "stale provider cognition lease release failed for {}: {error:?}",
                            lease.lease_id
                        )
                    })?;
            }
            leases_to_clear.push(agent_id);
        }
        if leases_to_clear.is_empty() {
            return Ok(());
        }
        let backups = leases_to_clear
            .iter()
            .filter_map(|agent_id| {
                self.provider_cognition_leases
                    .remove(agent_id)
                    .map(|lease| (agent_id.clone(), lease))
            })
            .collect::<Vec<_>>();
        if let Err(error) = self.persist_provider_lineage() {
            for (agent_id, lease) in backups {
                self.provider_cognition_leases.insert(agent_id, lease);
            }
            return Err(format!(
                "stale provider cognition lease cleanup persistence failed: {error}"
            ));
        }
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn provider_transport_exhausted_agent_excluding(
        &self,
        excluded_agent: Option<&str>,
    ) -> Option<String> {
        self.provider_transport_exhausted
            .iter()
            .find(|agent_id| Some(agent_id.as_str()) != excluded_agent)
            .cloned()
    }

    pub(in crate::viewer::runtime_live) fn provider_recovery_context(
        &self,
        agent_id: &str,
    ) -> Option<cognition_context::ProviderContextState> {
        self.provider_recovery_pending
            .get(agent_id)
            .map(|pending| pending.active.clone())
            .or_else(|| {
                self.provider_wake_recovery_pending
                    .get(agent_id)
                    .map(|pending| pending.active.clone())
            })
            .or_else(|| self.provider_contexts.get(agent_id).cloned())
            .or_else(|| self.provider_active_turns.get(agent_id).cloned())
    }

    /// Validate the signed handoff intent before preserving a terminal wake
    /// that a fresh canonical projection no longer exposes. The projection
    /// stays authoritative; this exception only lets an already checkpointed
    /// request finish its exact post-commit readback/cleanup sequence.
    pub(super) fn validate_preserved_service_wake_checkpoint(
        &self,
        wake: &crate::runtime::SchedulerWakeV1,
    ) -> Result<bool, String> {
        wake.validate()
            .map_err(|error| format!("canonical wake invalid during recovery: {error:?}"))?;
        let context = self
            .provider_wake_recovery_pending
            .get(&wake.agent_id)
            .map(|pending| pending.active.clone())
            .or_else(|| {
                self.provider_service_pending
                    .get(&wake.agent_id)
                    .map(|pending| pending.cognition.request.clone())
            })
            .or_else(|| self.provider_recovery_context(&wake.agent_id));
        let Some(context) = context else {
            return Ok(false);
        };
        if !lineage_persistence::provider_context_matches_wake(&context, wake) {
            return Ok(false);
        }

        let request = &context.request_context;
        let current_binding = self
            .provider_service_projection
            .as_ref()
            .and_then(|projection| projection.runtime_binding.as_ref())
            .ok_or("canonical wake recovery lacks verified runtime binding; fenced")?;
        let checkpoint_id = format!(
            "{}:handoff:{}",
            request.provider_invocation_key(),
            wake.wake_id
        );
        let checkpoint = self
            .provider_scheduler_pending
            .get(&checkpoint_id)
            .ok_or("canonical exact wake mirror lacks its signed handoff checkpoint; fenced")?;
        let expected_world = self
            .provider_service_config
            .as_ref()
            .ok_or("canonical provider service missing while recovering wake; fenced")?
            .expected_world
            .clone();
        Self::validate_scheduler_checkpoint_integrity(&expected_world, &checkpoint_id, checkpoint)?;
        let signer = self
            .provider_service_signer
            .as_ref()
            .ok_or("explicit canonical Agent signer missing while recovering wake; fenced")?;
        let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
            &checkpoint.payload
        else {
            return Err("canonical wake handoff checkpoint codec mismatch; fenced".into());
        };
        let signer_key: [u8; 32] = hex::decode(&signer.private_key_hex)
            .map_err(|_| "invalid canonical Agent signer key while recovering wake; fenced")?
            .try_into()
            .map_err(|_| "invalid canonical Agent signer key while recovering wake; fenced")?;
        let signer_public_key = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&signer_key)
                .verifying_key()
                .to_bytes(),
        );
        let crate::world_service::wire::SchedulerOperationV1::HandoffWake {
            wake_id,
            disposition: crate::runtime::CognitionWakeDispositionV1::Terminal { status, reason },
            ..
        } = &signed.request.operation
        else {
            return Err("canonical wake handoff checkpoint operation mismatch; fenced".into());
        };
        if wake_id != &wake.wake_id
            || *status != crate::runtime::ContinuationStatusV1::Completed
            || reason != "provider_action_committed"
            || signed.request.agent_id != request.agent_subject
            || signed.subject_public_key != signer_public_key
            || !lineage_persistence::handoff_checkpoint_matches_verified_binding(
                &request.runtime_binding,
                wake,
                &signed.request.captured_base_binding,
                current_binding,
            )
            || signed.request.delegation_generation != signer.delegation_generation
        {
            return Err("canonical wake handoff checkpoint identity mismatch; fenced".into());
        }
        if let Some(recovery) = self.provider_wake_recovery_pending.get(&wake.agent_id)
            && (!lineage_persistence::provider_context_matches_wake(&recovery.active, wake)
                || recovery.status != *status
                || recovery.reason != *reason)
        {
            return Err(
                "canonical wake recovery marker conflicts with signed handoff; fenced".into(),
            );
        }
        Ok(true)
    }

    /// Preserve an old sidecar wake only when the exact terminal handoff is
    /// already signed and durable. The verified Runtime projection remains
    /// authoritative for every other wake identity.
    pub(super) fn reconcile_projected_service_wakes(
        &self,
        projected_wakes: Vec<crate::runtime::SchedulerWakeV1>,
    ) -> Result<BTreeMap<String, crate::runtime::SchedulerWakeV1>, String> {
        let mut reconciled = BTreeMap::new();
        for wake in projected_wakes {
            if let Some(existing) = reconciled.insert(wake.wake_id.clone(), wake.clone())
                && existing != wake
            {
                return Err("canonical wake projection contains conflicting identities".into());
            }
        }
        for (wake_id, wake) in &self.pending_runtime_wakes {
            if let Some(projected) = reconciled.get(wake_id) {
                if projected != wake {
                    return Err(
                        "canonical wake projection and sidecar mirror identity conflict; fenced"
                            .into(),
                    );
                }
                continue;
            }
            if self.validate_preserved_service_wake_checkpoint(wake)? {
                reconciled.insert(wake_id.clone(), wake.clone());
            }
        }
        Ok(reconciled)
    }

    pub(in crate::viewer::runtime_live) fn provider_wake_recovery_pending_agent(
        &self,
    ) -> Option<String> {
        self.provider_wake_recovery_pending.keys().next().cloned()
    }

    pub(in crate::viewer::runtime_live) fn provider_wake_recovery_pending(
        &self,
        agent_id: &str,
    ) -> Option<lineage_persistence::ProviderWakeRecoveryPending> {
        self.provider_wake_recovery_pending.get(agent_id).cloned()
    }

    /// Retain the exact request identity while a Runtime wake handoff is
    /// unresolved. This is intentionally separate from generic transport
    /// recovery: the provider turn is already terminal, but Runtime still
    /// owns the wake acknowledgement.
    pub(in crate::viewer::runtime_live) fn retain_provider_wake_recovery_pending(
        &mut self,
        agent_id: &str,
        context: &cognition_context::ProviderContextState,
        status: crate::runtime::ContinuationStatusV1,
        reason: impl Into<String>,
    ) {
        self.provider_wake_recovery_pending.insert(
            agent_id.to_string(),
            lineage_persistence::ProviderWakeRecoveryPending {
                active: context.clone(),
                status,
                reason: reason.into(),
            },
        );
        self.provider_transport_exhausted
            .insert(agent_id.to_string());
        self.provider_active_turns
            .entry(agent_id.to_string())
            .or_insert_with(|| context.clone());
        self.provider_contexts
            .entry(agent_id.to_string())
            .or_insert_with(|| context.clone());
        self.persist_provider_lineage_best_effort();
    }

    /// Remove wake recovery state only after Runtime has accepted the wake.
    /// A checkpoint failure restores every identity-bearing mirror so the
    /// next pass can retry cleanup without redispatching the provider turn.
    pub(in crate::viewer::runtime_live) fn complete_provider_wake_recovery(
        &mut self,
        agent_id: &str,
    ) -> Result<(), String> {
        let wake_backup = self.provider_wake_recovery_pending.get(agent_id).cloned();
        let context_backup = self.provider_contexts.get(agent_id).cloned();
        let active_backup = self.provider_active_turns.get(agent_id).cloned();
        let retry_backup = self.provider_retry_contexts.get(agent_id).cloned();
        let wait_backup = self.provider_wait_until.get(agent_id).copied();
        let held_backup = self.provider_held_decisions.get(agent_id).cloned();
        let exhausted_backup = self.provider_transport_exhausted.contains(agent_id);
        let cognition_lease_backup = self.provider_cognition_leases.get(agent_id).cloned();
        self.provider_wake_recovery_pending.remove(agent_id);
        self.provider_contexts.remove(agent_id);
        self.provider_active_turns.remove(agent_id);
        self.provider_retry_contexts.remove(agent_id);
        self.provider_wait_until.remove(agent_id);
        self.provider_held_decisions.remove(agent_id);
        self.provider_transport_exhausted.remove(agent_id);
        self.provider_cognition_leases.remove(agent_id);
        if let Err(error) = self.persist_provider_lineage() {
            if let Some(wake) = wake_backup {
                self.provider_wake_recovery_pending
                    .insert(agent_id.to_string(), wake);
            }
            if let Some(context) = context_backup {
                self.provider_contexts.insert(agent_id.to_string(), context);
            }
            if let Some(active) = active_backup {
                self.provider_active_turns
                    .insert(agent_id.to_string(), active);
            }
            if let Some(retry) = retry_backup {
                self.provider_retry_contexts
                    .insert(agent_id.to_string(), retry);
            }
            if let Some(wait_until) = wait_backup {
                self.provider_wait_until
                    .insert(agent_id.to_string(), wait_until);
            }
            if let Some(held) = held_backup {
                self.provider_held_decisions
                    .insert(agent_id.to_string(), held);
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
                "provider wake recovery cleanup persistence failed: {error}"
            ));
        }
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn update_provider_wake_recovery(
        &mut self,
        agent_id: &str,
        status: crate::runtime::ContinuationStatusV1,
        reason: &str,
    ) -> Result<(), String> {
        let Some(recovery) = self.provider_wake_recovery_pending.get_mut(agent_id) else {
            return Ok(());
        };
        recovery.status = status;
        recovery.reason = reason.to_string();
        self.persist_provider_lineage()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Wake failure recovery carries the stable Runtime/provider identity tuple"
    )]
    pub(super) fn handle_provider_wake_resume_failure(
        &mut self,
        world: &mut RuntimeWorld,
        wake: &crate::runtime::SchedulerWakeV1,
        current_context: &crate::simulator::ContinuationCurrentContextV1,
        turn_context: &crate::simulator::ContinuousAgentTurnContextV1,
        request_context: &crate::simulator::ContinuousAgentRequestContextV1,
        predecessor_proposal_id: &str,
        next_proposal_id: &str,
        agent_id: &str,
        resume_error: &crate::runtime::WorldError,
    ) -> String {
        let recovery_context = cognition_context::ProviderContextState {
            turn_context: turn_context.clone(),
            request_context: request_context.clone(),
        };
        let handoff_error = world
            .handoff_cognition_wake_with_context(
                &wake.wake_id,
                crate::runtime::CognitionWakeDispositionV1::Terminal {
                    status: crate::runtime::ContinuationStatusV1::Rejected,
                    reason: "provider_wake_resume_failed".to_string(),
                },
                crate::runtime::CognitionContextDigestsV1 {
                    baseline_observation_digest: current_context
                        .authority
                        .baseline_observation_digest
                        .clone(),
                    goal_digest: current_context.authority.goal_digest.clone(),
                    policy_digest: current_context.authority.policy_digest.clone(),
                    precondition_digest: current_context.authority.precondition_digest.clone(),
                },
            )
            .err();
        self.provider_continuation_proposals
            .remove(predecessor_proposal_id);
        self.provider_continuation_proposals
            .remove(next_proposal_id);
        if let Some(handoff_error) = handoff_error {
            self.retain_provider_wake_recovery_pending(
                agent_id,
                &recovery_context,
                crate::runtime::ContinuationStatusV1::Rejected,
                "provider_wake_resume_failed",
            );
            return format!(
                "Runtime cognition wake resume rejected {}: {resume_error:?}; Runtime wake handoff failed: {handoff_error:?}",
                wake.wake_id
            );
        }
        self.pending_runtime_wakes.remove(&wake.wake_id);
        self.provider_contexts.remove(agent_id);
        self.provider_active_turns.remove(agent_id);
        self.provider_retry_contexts.remove(agent_id);
        self.provider_wait_until.remove(agent_id);
        self.persist_provider_lineage_best_effort();
        format!(
            "Runtime cognition wake resume rejected {}: {resume_error:?}",
            wake.wake_id
        )
    }

    /// Keep an exact interrupted request fenced until every Runtime/Harness
    /// recovery step has completed. This record is intentionally additive to
    /// the transport exhaustion marker so a failed checkpoint write cannot
    /// turn a known identity into an eligible fresh request.
    pub(in crate::viewer::runtime_live) fn retain_provider_recovery_pending(
        &mut self,
        agent_id: &str,
        context: &cognition_context::ProviderContextState,
        reason: impl Into<String>,
    ) {
        self.provider_recovery_pending.insert(
            agent_id.to_string(),
            lineage_persistence::ProviderRecoveryPending {
                active: context.clone(),
                reason: reason.into(),
            },
        );
        self.provider_transport_exhausted
            .insert(agent_id.to_string());
        self.provider_active_turns
            .entry(agent_id.to_string())
            .or_insert_with(|| context.clone());
        self.provider_contexts
            .entry(agent_id.to_string())
            .or_insert_with(|| context.clone());
        self.persist_provider_lineage_best_effort();
    }
}

pub(super) fn provider_feedback_session_key(agent_subject: &str, session_id: &str) -> String {
    format!("{agent_subject}\u{1f}{session_id}")
}
