//! Final positive successor budget is consumed through its original authenticated context.
use super::*;
#[cfg(any(test, feature = "test_tier_required"))]
#[path = "llm_sidecar_hosted_final_budget_probe.rs"]
mod probe;
use crate::runtime::{AgentContinuation, CognitionBudgetConsumptionV1, ContinuationStatusV1};
use crate::world_service::wire::WorldServicePayloadV1;

impl HostedResume {
    pub(super) fn budget_operation(&self) -> SchedulerOperationV1 {
        SchedulerOperationV1::ConsumeContinuationBudget {
            continuation_id: self.wake.continuation_id.clone(),
            budget_spent: 1,
            current_context: crate::runtime::CognitionContextDigestsV1 {
                baseline_observation_digest: self
                    .current
                    .authority
                    .baseline_observation_digest
                    .clone(),
                goal_digest: self.current.authority.goal_digest.clone(),
                policy_digest: self.current.authority.policy_digest.clone(),
                precondition_digest: self.current.authority.precondition_digest.clone(),
            },
        }
    }
    fn budget_phase(&self) -> String {
        format!("consume:{}", self.wake.wake_id)
    }
    fn validate_budget_projection(
        &self,
        view: &crate::world_service::projection::WorldServiceProjection,
        terminal: bool,
    ) -> Result<AgentContinuation, String> {
        let request = &self.context.request_context;
        let binding = view
            .runtime_binding
            .as_ref()
            .ok_or("final budget current binding missing")?;
        let old = &request.runtime_binding;
        let agent = view
            .agent_context
            .as_ref()
            .ok_or("final budget Agent authority missing")?;
        let old_catalog = request
            .base_decision_request
            .capability_catalog
            .as_ref()
            .ok_or("final budget original catalog missing")?;
        let catalog = &agent.capability_catalog;
        if binding.world_id != old.world_id
            || binding.branch_id != old.branch_id
            || binding.reorg_epoch != old.reorg_epoch
            || binding.finality_epoch != old.finality_epoch
            || binding.finality_block_hash != old.finality_block_hash
            || binding.finality_status != old.finality_status
            || binding.runtime_manifest_hash != old.runtime_manifest_hash
            || agent.agent_id != request.agent_subject
            || catalog.revocation_epoch != old_catalog.revocation_epoch
            || catalog.policy_hash != old_catalog.policy_hash
            || catalog.module_registry_hash != old_catalog.module_registry_hash
            || catalog.subject != old_catalog.subject
            || catalog.presenter != old_catalog.presenter
            || catalog.audience != old_catalog.audience
        {
            return Err("final budget authority or branch changed; fenced".into());
        }
        let actual = view
            .continuations
            .iter()
            .find(|entry| entry.continuation_id == self.wake.continuation_id)
            .ok_or("final budget canonical continuation missing")?;
        actual.validate_authoritative().map_err(|e| e.to_string())?;
        let original = self
            .budget_predecessor
            .as_ref()
            .ok_or("final budget predecessor missing")?;
        let mut expected = original.clone();
        expected.logical_tick = actual.logical_tick;
        expected.status = actual.status;
        expected.continuation_status_digest = actual.continuation_status_digest.clone();
        if terminal {
            expected.remaining_budget = actual.remaining_budget.clone();
            expected.terminal_disposition = actual.terminal_disposition.clone();
        }
        if &expected != actual
            || actual.agent_id != request.agent_subject
            || actual.wake_id != self.wake.wake_id
            || actual.continuation_proposal_id != self.proposal.continuation_proposal_id
            || actual.world_id != binding.world_id
            || actual.branch_id != binding.branch_id
            || actual.reorg_epoch != binding.reorg_epoch
        {
            return Err("final budget canonical identity changed".into());
        }
        let authority = view
            .continuation_contexts
            .get(&actual.continuation_id)
            .ok_or("final budget canonical context missing")?;
        if authority.baseline_observation_digest
            != self.current.authority.baseline_observation_digest
            || authority.goal_digest != self.current.authority.goal_digest
            || authority.policy_digest != self.current.authority.policy_digest
            || authority.precondition_digest != self.current.authority.precondition_digest
        {
            return Err("final budget canonical context changed".into());
        }
        if terminal {
            let receipt = self
                .budget_receipt
                .as_ref()
                .ok_or("final budget authenticated receipt missing")?;
            if actual.status != ContinuationStatusV1::Completed
                || actual.remaining_budget.value != 0
                || actual.remaining_budget.unit != self.proposal.remaining_budget.unit
                || receipt.consumed != 1
                || receipt.continuation_id != actual.continuation_id
                || receipt.wake_id != actual.wake_id
                || receipt.remaining_budget != actual.remaining_budget
                || receipt.status != actual.status
                || Some(&receipt.continuation_status_digest)
                    != actual.continuation_status_digest.as_ref()
                || actual.terminal_disposition.as_deref() != Some("budget_exhausted")
                || view
                    .scheduler_wakes
                    .iter()
                    .any(|wake| wake.wake_id == actual.wake_id)
            {
                return Err("final budget terminal receipt/View mismatch".into());
            }
        } else if actual.remaining_budget.value != 1
            || !matches!(
                actual.status,
                ContinuationStatusV1::Scheduled
                    | ContinuationStatusV1::Pending
                    | ContinuationStatusV1::Waking
            )
            || !view.scheduler_wakes.iter().any(|wake| wake == &self.wake)
        {
            return Err("final budget selected wake changed".into());
        }
        Ok(actual.clone())
    }
}
impl RuntimeLlmSidecar {
    pub(super) fn validate_final_budget_original(
        &mut self,
        resume: &HostedResume,
    ) -> Result<(), String> {
        resume.validate_original()?;
        if !resume.final_budget
            || resume.proposal.remaining_budget.value != 1
            || resume.runtime.remaining_budget.value != 1
            || resume.receipt.is_some()
            || resume.rejection.is_some()
            || resume.rejected_predecessor.is_some()
            || resume.budget_predecessor.as_ref().is_none_or(|old| {
                old.remaining_budget.value != 1
                    || old.wake_id != resume.wake.wake_id
                    || old.continuation_id != resume.wake.continuation_id
                    || old.continuation_proposal_id != resume.proposal.continuation_proposal_id
            })
        {
            return Err("final budget mixed or invalid original state".into());
        }
        let request = &resume.context.request_context;
        let predecessor = resume
            .budget_predecessor
            .as_ref()
            .ok_or("final budget predecessor missing")?;
        // Runtime retains the raw manifest; the provider prepared binding hashes it.
        // Preserve both domains, and authenticate the full original signed Resume below.
        if resume.runtime.runtime_manifest_hash != predecessor.runtime_manifest_hash
            || crate::simulator::h_v1(
                "oasis7.runtime.manifest.v1",
                &resume.runtime.runtime_manifest_hash,
            )
            .to_string()
                != request.runtime_binding.runtime_manifest_hash.to_string()
            || resume.runtime.branch_id != predecessor.branch_id
            || resume.runtime.finality_epoch != predecessor.finality_epoch
            || resume.runtime.finality_block_hash != predecessor.finality_block_hash
            || resume.runtime.finality_status != predecessor.finality_status
            || resume.runtime.reorg_epoch != predecessor.reorg_epoch
        {
            return Err("final budget original Runtime generation mismatch".into());
        }
        let prefix = format!("{}:resume:", request.provider_invocation_key());
        let sources = self
            .provider_scheduler_pending
            .iter()
            .filter(|(id, pending)| {
                id.starts_with(&prefix)
                    && pending.resume_context.as_ref().is_some_and(|context| {
                        serde_json::to_value(context).ok()
                            == serde_json::to_value(&resume.context).ok()
                    })
            })
            .map(|(id, pending)| (id.clone(), pending.clone()))
            .collect::<Vec<_>>();
        if sources.len() != 1 {
            return Err("final budget unique prepared Resume source missing".into());
        }
        let (id, pending) = &sources[0];
        let WorldServicePayloadV1::Scheduler(signed) = &pending.payload else {
            return Err("final budget signed source missing".into());
        };
        let SchedulerOperationV1::ResumeWake { proposal, .. } = &signed.request.operation else {
            return Err("final budget source operation changed".into());
        };
        if serde_json::to_value(proposal).map_err(|e| e.to_string())?
            != serde_json::to_value(&resume.runtime).map_err(|e| e.to_string())?
            || pending.resume_current_context.as_ref() != Some(&resume.current)
        {
            return Err("final budget prepared source changed".into());
        }
        let phase = id
            .strip_prefix(&format!("{}:", request.provider_invocation_key()))
            .ok_or("final budget source phase invalid")?;
        let (_, existed) = self.prepare_service_scheduler_checkpoint(
            request,
            phase,
            signed.request.operation.clone(),
            Some((resume.context.clone(), resume.current.clone())),
        )?;
        if !existed {
            return Err("final budget original source disappeared".into());
        }
        let consume_id = format!(
            "{}:{}",
            request.provider_invocation_key(),
            resume.budget_phase()
        );
        if let Some(consume) = self.provider_scheduler_pending.get(&consume_id).cloned() {
            if consume.resume_context.as_ref().is_none_or(|original| {
                serde_json::to_value(original).ok() != serde_json::to_value(&resume.context).ok()
            }) || consume.resume_current_context.as_ref() != Some(&resume.current)
            {
                return Err("final budget Consume prepared context changed".into());
            }
            self.prepare_service_scheduler_checkpoint(
                request,
                &resume.budget_phase(),
                resume.budget_operation(),
                Some((resume.context.clone(), resume.current.clone())),
            )?;
        }
        Ok(())
    }
    pub(super) fn capture_hosted_final_budget(&mut self) -> Result<bool, String> {
        if self.hosted_admission.is_some()
            || !self.provider_cognition_leases.is_empty()
            || !self.provider_active_turns.is_empty()
            || !self.provider_completed_decisions.is_empty()
            || !self.provider_held_decisions.is_empty()
        {
            return Ok(false);
        }
        let Some(metadata_identity) = self.fresh_provider_metadata_identity()? else {
            return Ok(false);
        };
        let view = self
            .provider_service_projection
            .clone()
            .ok_or("final budget view missing")?;
        let Some(wake) = view
            .scheduler_wakes
            .iter()
            .min_by_key(|wake| (wake.wake_seq, wake.wake_id.as_str()))
            .cloned()
        else {
            return Ok(false);
        };
        let predecessor = view
            .continuations
            .iter()
            .find(|entry| entry.continuation_id == wake.continuation_id)
            .ok_or("final budget selected predecessor missing")?
            .clone();
        if predecessor.remaining_budget.value != 1 {
            return Ok(false);
        }
        let candidates = self
            .provider_scheduler_pending
            .values()
            .filter_map(|pending| {
                let context = pending.resume_context.as_ref()?;
                let proposal = context.turn_context.continuation.as_ref()?;
                (proposal.continuation_proposal_id == predecessor.continuation_proposal_id
                    && context.request_context.agent_subject == wake.agent_id)
                    .then(|| {
                        (
                            context.clone(),
                            proposal.clone(),
                            pending.resume_current_context.clone(),
                        )
                    })
            })
            .collect::<Vec<_>>();
        // Consume and Resume checkpoints may both contain the same prepared source after a crash;
        // capture only happens before Consume exists and requires exactly one original Resume.
        if candidates.len() != 1 {
            return Err("final budget original prepared context ambiguous".into());
        }
        let (context, proposal, current) = candidates[0].clone();
        let current = current.ok_or("final budget original current context missing")?;
        let mut runtime: crate::runtime::CognitionContinuationProposalV1 =
            serde_json::from_value(serde_json::to_value(&proposal).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        runtime.branch_id = context.request_context.runtime_binding.branch_id.clone();
        runtime.finality_epoch = context.request_context.runtime_binding.finality_epoch;
        runtime.finality_block_hash = context
            .request_context
            .runtime_binding
            .finality_block_hash
            .as_ref()
            .map(ToString::to_string);
        runtime.finality_status = context
            .request_context
            .runtime_binding
            .finality_status
            .clone();
        runtime.reorg_epoch = context.request_context.runtime_binding.reorg_epoch;
        runtime.runtime_manifest_hash = predecessor.runtime_manifest_hash.clone();
        runtime.proposal_digest = runtime.proposal_digest();
        let resume = HostedResume {
            context,
            observation: current.observation.clone(),
            proposal,
            runtime,
            current,
            wake,
            predecessor_proposal_id: predecessor.continuation_proposal_id.clone(),
            metadata_identity,
            stage: "budget_view".into(),
            commit: None,
            receipt: None,
            rejection: None,
            rejected_predecessor: None,
            final_budget: true,
            budget_receipt: None,
            budget_predecessor: Some(predecessor),
        };
        self.validate_final_budget_original(&resume)?;
        resume.validate_budget_projection(&view, false)?;
        self.hosted_resume = Some(resume);
        if let Err(error) = self.persist_provider_lineage() {
            self.hosted_resume = None;
            return Err(error);
        }
        Ok(true)
    }
    pub(super) fn restore_hosted_final_budget(
        &mut self,
        resume: HostedResume,
    ) -> Result<(), String> {
        self.validate_final_budget_original(&resume)?;
        let id = format!(
            "{}:{}",
            resume.context.request_context.provider_invocation_key(),
            resume.budget_phase()
        );
        let pending = self.provider_scheduler_pending.get(&id).cloned();
        if pending.is_some() {
            self.prepare_service_scheduler_checkpoint(
                &resume.context.request_context,
                &resume.budget_phase(),
                resume.budget_operation(),
                Some((resume.context.clone(), resume.current.clone())),
            )?;
        } else if resume.stage != "budget_view" {
            return Err("restored issued final budget checkpoint missing".into());
        }
        self.hosted_restored_resume = Some((resume.clone(), pending));
        if let Some(original) = self.hosted_resume.as_mut() {
            original.stage = if self
                .hosted_restored_resume
                .as_ref()
                .is_some_and(|(_, pending)| pending.is_some())
            {
                "budget_consume"
            } else {
                "budget_view"
            }
            .into();
            original.commit = None;
        }
        Ok(())
    }
    fn complete_hosted_final_budget(&mut self, resume: &HostedResume) -> Result<(), String> {
        self.validate_final_budget_original(resume)?;
        let view = self
            .provider_service_projection
            .as_ref()
            .ok_or("final budget terminal view missing")?;
        let terminal = resume.validate_budget_projection(view, true)?;
        let authority = self.current_budget_authority(resume)?;
        let request = &resume.context.request_context;
        if self
            .provider_cognition_leases
            .contains_key(&request.agent_subject)
            || self
                .provider_active_turns
                .contains_key(&request.agent_subject)
            || self.provider_contexts.contains_key(&request.agent_subject)
            || self
                .provider_held_decisions
                .contains_key(&request.agent_subject)
            || self
                .provider_completed_decisions
                .iter()
                .any(|decision| decision.agent_id == request.agent_subject)
        {
            return Err("final budget cleanup conflicts with newer provider work".into());
        }
        let backup = (
            self.provider_continuation_proposals.clone(),
            self.pending_runtime_wakes.clone(),
            self.provider_terminal_states.clone(),
            self.hosted_resume.clone(),
            self.hosted_restored_resume.clone(),
        );
        let restored = self.hosted_restored_resume.is_some();
        if restored && self.runner.is_none() {
            self.ensure_runner_initialized()?;
        }
        #[cfg(any(test, feature = "test_tier_required"))]
        let test_persist = self
            .test_final_budget_root()?
            .map(|root| {
                let native = self
                    .runner
                    .as_ref()
                    .and_then(RuntimeDecisionRunner::async_runner)
                    .ok_or("test final budget runner missing")?;
                self.test_final_budget_state(native.rejected_wait_test_ledger_digests())
                    .map(|state| (root, state))
            })
            .transpose()?;
        #[cfg(any(test, feature = "test_tier_required"))]
        let mut test_persist_attempted = false;
        let mut runner = self
            .runner
            .take()
            .ok_or("final budget native runner missing")?;
        let result = (|| {
            let native = runner
                .async_runner_mut()
                .ok_or("final budget native runner unavailable")?;
            let mut persist = |_digests: Option<[String; 5]>| {
                self.provider_continuation_proposals
                    .remove(&resume.proposal.continuation_proposal_id);
                self.pending_runtime_wakes.remove(&resume.wake.wake_id);
                self.hosted_resume = None;
                self.hosted_restored_resume = None;
                self.provider_terminal_states.insert(
                    request.agent_subject.clone(),
                    lineage_persistence::ProviderTerminalState {
                        agent_id: request.agent_subject.clone(),
                        agent_session_id: request.agent_session_id.clone(),
                        agent_turn_id: request.agent_turn_id.clone(),
                        decision_request_id: request.decision_request_id.clone(),
                        request_digest: request.request_digest.to_string(),
                        status: "completed".into(),
                        reject_reason: None,
                        feedback_id: None,
                        feedback: None,
                    },
                );
                #[cfg(any(test, feature = "test_tier_required"))]
                if let Some((root, before)) = &test_persist {
                    let staged = self.test_final_budget_state(
                        _digests.ok_or("test final budget staged ledgers missing")?,
                    )?;
                    Self::test_write_resume_handoff_file(
                        root,
                        "final-budget-before.json",
                        &serde_json::to_vec(before).map_err(|e| e.to_string())?,
                    )?;
                    Self::test_write_resume_handoff_file(
                        root,
                        "final-budget-staged.json",
                        &serde_json::to_vec(&staged).map_err(|e| e.to_string())?,
                    )?;
                    Self::test_write_resume_handoff_file(
                        root,
                        "final-budget-before-persist",
                        b"ready",
                    )?;
                    Self::test_wait_resume_handoff_file(root, "final-budget-release")?;
                    test_persist_attempted = true;
                }
                self.persist_provider_lineage()
            };
            let predecessor = resume
                .budget_predecessor
                .as_ref()
                .ok_or("final budget predecessor missing")?;
            #[cfg(any(test, feature = "test_tier_required"))]
            if test_persist.is_some() {
                return native
                    .with_completed_final_budget_cleanup_observed(
                        &request.agent_subject,
                        &resume.proposal,
                        predecessor,
                        terminal,
                        &authority,
                        restored,
                        |digests| persist(Some(digests)),
                    )
                    .map_err(|e| e.to_string());
            }
            native
                .with_completed_final_budget_cleanup(
                    &request.agent_subject,
                    &resume.proposal,
                    predecessor,
                    terminal,
                    &authority,
                    restored,
                    || persist(None),
                )
                .map_err(|e| e.to_string())
        })();
        self.runner = Some(runner);
        if result.is_err() {
            self.provider_continuation_proposals = backup.0;
            self.pending_runtime_wakes = backup.1;
            self.provider_terminal_states = backup.2;
            self.hosted_resume = backup.3;
            self.hosted_restored_resume = backup.4;
            #[cfg(any(test, feature = "test_tier_required"))]
            if test_persist_attempted && let Some((root, _)) = &test_persist {
                let native = self
                    .runner
                    .as_ref()
                    .and_then(RuntimeDecisionRunner::async_runner)
                    .ok_or("test final budget rollback runner missing")?;
                let after =
                    self.test_final_budget_state(native.rejected_wait_test_ledger_digests())?;
                Self::test_write_resume_handoff_file(
                    root,
                    "final-budget-after.json",
                    &serde_json::to_vec(&after).map_err(|e| e.to_string())?,
                )?;
                Self::test_write_resume_handoff_file(
                    root,
                    "final-budget-rollback",
                    b"rolled-back",
                )?;
            }
        }
        result
    }
    fn current_budget_authority(
        &self,
        resume: &HostedResume,
    ) -> Result<crate::simulator::ContinuationAuthorityContextV1, String> {
        let context = self
            .provider_service_projection
            .as_ref()
            .and_then(|view| view.continuation_contexts.get(&resume.wake.continuation_id))
            .ok_or("final budget authority missing")?;
        Ok(crate::simulator::ContinuationAuthorityContextV1 {
            baseline_observation_digest: context.baseline_observation_digest.clone(),
            goal_digest: context.goal_digest.clone(),
            policy_digest: context.policy_digest.clone(),
            precondition_digest: context.precondition_digest.clone(),
        })
    }
}
impl crate::viewer::ViewerRuntimeLiveServer {
    pub(super) fn prepare_hosted_final_budget_io(
        &mut self,
        resume: HostedResume,
    ) -> Result<AgentServiceProgress, String> {
        self.llm_sidecar.validate_final_budget_original(&resume)?;
        if resume.stage == "budget_finalize" {
            self.llm_sidecar.complete_hosted_final_budget(&resume)?;
            self.defer_next_auto_play_step_after_completion(self.config.play_step_interval);
            return Ok(AgentServiceProgress::Advanced);
        }
        let client = self
            .world_service_client()
            .map_err(|e| format!("{e:?}"))?
            .ok_or("final budget client missing")?;
        let operation = if resume.stage.ends_with("_view") {
            AgentServiceIoOperation::View(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: 1,
                world: client.config().expected_world.clone(),
                scope_id: client.config().scope_id.clone(),
                min_commit: resume.commit.clone(),
                fixed_commit: None,
                deadline_unix_ms: None,
            })
        } else if resume.stage == "budget_consume" {
            let (checkpoint, existed) = self.llm_sidecar.prepare_service_scheduler_checkpoint(
                &resume.context.request_context,
                &resume.budget_phase(),
                resume.budget_operation(),
                Some((resume.context.clone(), resume.current.clone())),
            )?;
            crate::viewer::runtime_live::agent_service_phase::original_intent_io(
                &checkpoint.correlation,
                &checkpoint.payload,
                !existed,
            )
        } else {
            return Err("final budget stage invalid".into());
        };
        self.llm_sidecar.hosted_service_generation =
            self.llm_sidecar.hosted_service_generation.saturating_add(1);
        let token = AgentServiceIoToken {
            generation: self.llm_sidecar.hosted_service_generation,
            config_digest: self.hosted_service_config_digest()?,
            phase_digest: format!(
                "resume:{}:{}",
                resume.stage,
                resume.context.request_context.provider_invocation_key()
            ),
        };
        self.llm_sidecar.hosted_service_inflight = Some(token.clone());
        Ok(AgentServiceProgress::NeedsIo(Box::new(AgentServiceIoJob {
            token,
            client: Some(client),
            operation,
        })))
    }
    pub(super) fn apply_hosted_final_budget_io(
        &mut self,
        mut resume: HostedResume,
        response: AgentServiceIoResponse,
    ) -> Result<AgentServiceProgress, String> {
        if resume.stage.ends_with("_view") {
            let AgentServiceIoResponse::View(view) = response else {
                return Err("final budget verified View missing".into());
            };
            if let Some(commit) = resume.commit.as_ref()
                && !view
                    .version()
                    .commit
                    .satisfies_minimum(commit)
                    .map_err(|e| e.to_string())?
            {
                return Err("final budget View precedes receipt".into());
            }
            self.apply_hosted_verified_view(*view)?;
            let terminal = resume.stage == "budget_committed_view";
            resume.validate_budget_projection(
                self.llm_sidecar
                    .provider_service_projection
                    .as_ref()
                    .ok_or("final budget view missing")?,
                terminal,
            )?;
            resume.stage = if terminal {
                "budget_finalize"
            } else {
                "budget_consume"
            }
            .into();
        } else {
            let Some(response) =
                crate::viewer::runtime_live::agent_service_phase::original_response(response)?
            else {
                return Ok(AgentServiceProgress::Advanced);
            };
            let id = format!(
                "{}:{}",
                resume.context.request_context.provider_invocation_key(),
                resume.budget_phase()
            );
            let pending = self
                .llm_sidecar
                .provider_scheduler_pending
                .get(&id)
                .ok_or("final budget immutable Consume checkpoint missing")?;
            response
                .validate(&pending.correlation)
                .map_err(|e| e.to_string())?;
            match response.outcome {
                IntentOutcome::Committed { commit, receipt } => {
                    let actual: CognitionBudgetConsumptionV1 =
                        serde_json::from_value(receipt).map_err(|e| e.to_string())?;
                    if actual.continuation_id != resume.wake.continuation_id
                        || actual.wake_id != resume.wake.wake_id
                        || actual.consumed != 1
                        || actual.remaining_budget.value != 0
                        || actual.remaining_budget.unit != resume.proposal.remaining_budget.unit
                        || actual.status != ContinuationStatusV1::Completed
                        || resume
                            .budget_receipt
                            .as_ref()
                            .is_some_and(|old| old != &actual)
                    {
                        return Err("final budget authenticated receipt conflict".into());
                    }
                    resume.budget_receipt = Some(actual);
                    resume.commit = Some(*commit);
                    resume.stage = "budget_committed_view".into();
                }
                IntentOutcome::Received { .. }
                | IntentOutcome::Pending
                | IntentOutcome::Unknown => {}
                _ => return Err("final budget Consume not committed; original retained".into()),
            }
        }
        self.llm_sidecar.hosted_resume = Some(resume);
        self.llm_sidecar.persist_provider_lineage()?;
        Ok(AgentServiceProgress::Advanced)
    }
}
