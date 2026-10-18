use super::*;

impl RuntimeLlmSidecar {
    pub(super) fn has_pending_service_resume_for_agent(&self, agent_id: &str) -> bool {
        self.provider_scheduler_pending.values().any(|pending| {
            matches!(&pending.payload,
                crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed)
                if signed.request.agent_id == agent_id
                    && matches!(signed.request.operation, crate::world_service::wire::SchedulerOperationV1::ResumeWake { .. })
                    && pending.resume_context.is_some() && pending.resume_current_context.is_some())
        })
    }

    /// A consumed wake is not proof of admission. Recover only by Lookup of
    /// the durable original signed operation, then reconcile its real receipt.
    pub(super) fn recover_consumed_service_resumes(
        &mut self,
        binding: &RuntimeBindingV1,
    ) -> Result<(), String> {
        if !self.provider_service_required {
            return Ok(());
        }
        let pending = self.provider_scheduler_pending.clone();
        for (id, checkpoint) in pending {
            let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
                &checkpoint.payload
            else {
                continue;
            };
            let crate::world_service::wire::SchedulerOperationV1::ResumeWake {
                wake_id,
                proposal,
                resume,
                current_context,
                ..
            } = &signed.request.operation
            else {
                continue;
            };
            if self.pending_runtime_wakes.contains_key(wake_id) {
                continue;
            }
            let original = checkpoint
                .resume_context
                .as_ref()
                .ok_or("pending ResumeWake original context missing; fenced")?;
            let current = checkpoint
                .resume_current_context
                .as_ref()
                .ok_or("pending ResumeWake current context missing; fenced")?;
            let request = &original.request_context;
            request
                .validate_production_lane()
                .map_err(|error| error.to_string())?;
            let capability = self
                .provider_service_projection
                .as_ref()
                .and_then(|view| view.agent_context.as_ref())
                .ok_or("authorized canonical Agent context missing")?;
            let old_invocation = request
                .base_decision_request
                .capability_invocation_context
                .as_ref()
                .ok_or("original ResumeWake capability missing")?;
            if request.agent_subject != signed.request.agent_id
                || capability.agent_id != signed.request.agent_id
                || old_invocation.subject != capability.capability_invocation_context.subject
                || old_invocation.grant_id != capability.capability_invocation_context.grant_id
                || request.runtime_binding.world_id != binding.world_id
                || request.runtime_binding.branch_id != binding.branch_id
                || request.runtime_binding.finality_epoch != binding.finality_epoch
                || request.runtime_binding.reorg_epoch != binding.reorg_epoch
                || request.agent_session_id != resume.agent_session_id
                || request.agent_turn_id != resume.agent_turn_id
                || request.decision_request_id != resume.decision_request_id
                || request.request_digest.to_string() != resume.request_digest
                || async_support::runtime_provider_context_digest(request) != resume.context_digest
                || current.authority.baseline_observation_digest
                    != current_context.baseline_observation_digest
                || current.authority.goal_digest != current_context.goal_digest
                || current.authority.policy_digest != current_context.policy_digest
                || current.authority.precondition_digest != current_context.precondition_digest
                || id != format!("{}:resume:{}", request.provider_invocation_key(), wake_id)
                || crate::world_service::derive_correlation(
                    checkpoint.correlation.key.world.clone(),
                    &checkpoint.payload,
                )? != checkpoint.correlation
            {
                return Err(
                    "pending ResumeWake original identity or delegation conflict; fenced".into(),
                );
            }
            let receipt = self.provider_scheduler_operation_with_resume_context(
                request,
                &format!("resume:{wake_id}"),
                signed.request.operation.clone(),
                Some((original.clone(), current.clone())),
            )?;
            let result: crate::runtime::CognitionWakeHandoffResultV1 =
                serde_json::from_value(receipt)
                    .map_err(|error| format!("canonical ResumeWake receipt invalid: {error}"))?;
            let next = original
                .turn_context
                .continuation
                .clone()
                .ok_or("original ResumeWake Harness proposal missing")?;
            let replanned = result
                .replanned_continuation
                .as_ref()
                .ok_or("canonical ResumeWake replanned continuation missing")?;
            if result.wake.wake_id != *wake_id
                || result.wake.agent_id != request.agent_subject
                || result.continuation.agent_id != request.agent_subject
                || result.continuation.continuation_id != result.wake.continuation_id
                || replanned.continuation_proposal_id != proposal.continuation_proposal_id
                || replanned.proposal_digest != proposal.proposal_digest
                || next.continuation_proposal_id != proposal.continuation_proposal_id
            {
                return Err(
                    "canonical ResumeWake handoff receipt identity mismatch; fenced".into(),
                );
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let runner = self
                    .runner
                    .as_mut()
                    .and_then(RuntimeDecisionRunner::async_runner_mut)
                    .ok_or("ResumeWake Harness runner missing")?;
                if runner.active_continuation_proposal_id(&request.agent_subject)
                    == Some(next.continuation_proposal_id.as_str())
                {
                    runner
                        .validate_active_continuation_with_authority(
                            &request.agent_subject,
                            &current.authority,
                            replanned,
                        )
                        .map_err(|error| {
                            format!("canonical ResumeWake Harness validation failed: {error}")
                        })?;
                } else {
                    runner
                        .reconcile_runtime_wake_with_current_context(
                            &request.agent_subject,
                            current,
                            &result.continuation,
                            Some(next),
                        )
                        .map_err(|error| {
                            format!("canonical ResumeWake Harness reconciliation failed: {error}")
                        })?;
                }
            }
            self.provider_contexts
                .insert(request.agent_subject.clone(), original.clone());
            self.provider_continuation_proposals
                .remove(&result.continuation.continuation_proposal_id);
            self.provider_scheduler_pending.remove(&id);
            if let Err(error) = self.persist_provider_lineage() {
                self.provider_scheduler_pending.insert(id, checkpoint);
                return Err(error);
            }
        }
        Ok(())
    }
}
