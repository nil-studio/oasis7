//! Authenticated Resume rejection retires its admitted predecessor, never a successor.
use super::*;
#[cfg(any(test, feature = "test_tier_required"))]
#[path = "llm_sidecar_hosted_resume_rejection_probe.rs"]
mod probe;
use crate::runtime::{AgentContinuation, ContinuationStatusV1};

impl HostedResume {
    fn original_checkpoint_id(&self) -> String {
        format!(
            "{}:resume:{}",
            self.context.request_context.provider_invocation_key(),
            self.wake.wake_id
        )
    }
    fn compensation_checkpoint_id(&self) -> String {
        format!(
            "{}:reject_resume_predecessor",
            self.context.request_context.provider_invocation_key()
        )
    }
    pub(super) fn accept_rejection(
        &mut self,
        response: IntentResponse<serde_json::Value>,
        pending: &lineage_persistence::PendingProviderSchedulerIntent,
    ) -> Result<(), String> {
        response
            .validate(&pending.correlation)
            .map_err(|e| e.to_string())?;
        if !matches!(response.outcome, IntentOutcome::Rejected { .. })
            || self.receipt.is_some()
            || self
                .rejection
                .as_ref()
                .is_some_and(|prior| prior != &response)
        {
            return Err("Resume original rejection or committed successor conflict".into());
        }
        self.rejection = Some(response);
        Ok(())
    }
    pub(super) fn validate_rejection(&self, sidecar: &RuntimeLlmSidecar) -> Result<(), String> {
        self.validate_original()?;
        let pending = sidecar
            .provider_scheduler_pending
            .get(&self.original_checkpoint_id())
            .ok_or("rejected Resume original checkpoint missing")?;
        let mut original = self.clone();
        original.accept_rejection(
            self.rejection
                .clone()
                .ok_or("Resume compensation lacks original rejection")?,
            pending,
        )
    }
    fn validate_projection(
        &self,
        projection: &crate::world_service::projection::WorldServiceProjection,
        terminal: bool,
    ) -> Result<AgentContinuation, String> {
        let request = &self.context.request_context;
        let binding = projection
            .runtime_binding
            .as_ref()
            .ok_or("Resume compensation binding missing")?;
        let captured = &request.runtime_binding;
        let agent = projection
            .agent_context
            .as_ref()
            .ok_or("Resume compensation authorized Agent missing")?;
        let old = request
            .base_decision_request
            .capability_catalog
            .as_ref()
            .ok_or("Resume original capability catalog missing")?;
        let catalog = &agent.capability_catalog;
        if binding.world_id != captured.world_id
            || binding.branch_id != captured.branch_id
            || binding.reorg_epoch != captured.reorg_epoch
            || binding.finality_epoch != captured.finality_epoch
            || binding.runtime_manifest_hash != captured.runtime_manifest_hash
            || agent.agent_id != request.agent_subject
            || catalog.revocation_epoch != old.revocation_epoch
            || catalog.policy_hash != old.policy_hash
            || catalog.module_registry_hash != old.module_registry_hash
            || catalog.subject != old.subject
            || catalog.presenter != old.presenter
            || catalog.audience != old.audience
        {
            return Err("Resume compensation authority or branch changed; fenced".into());
        }
        let actual = projection
            .continuations
            .iter()
            .find(|entry| entry.continuation_id == self.wake.continuation_id)
            .ok_or("Resume compensation predecessor missing")?;
        if actual.continuation_proposal_id != self.predecessor_proposal_id
            || actual.agent_id != request.agent_subject
            || actual.wake_id != self.wake.wake_id
            || actual.world_id != binding.world_id
            || actual.branch_id != binding.branch_id
            || actual.reorg_epoch != binding.reorg_epoch
            || actual.finality_epoch != binding.finality_epoch
            || projection.continuations.iter().any(|entry| {
                entry.continuation_proposal_id == self.proposal.continuation_proposal_id
            })
        {
            return Err("Resume compensation predecessor or successor identity conflict".into());
        }
        if terminal {
            if actual.status != ContinuationStatusV1::Rejected
                || self.rejected_predecessor.as_ref() != Some(actual)
                || projection
                    .scheduler_wakes
                    .iter()
                    .any(|wake| wake.wake_id == self.wake.wake_id)
            {
                return Err("Resume compensation terminal projection mismatch".into());
            }
        } else if actual.status == ContinuationStatusV1::Rejected {
            // A restart still looks up the original compensation checkpoint; this
            // view alone is not permission to invent a receipt or skip that Lookup.
            if !projection
                .scheduler_wakes
                .iter()
                .all(|wake| wake.wake_id != self.wake.wake_id)
            {
                return Err("Resume rejected predecessor still has an active wake".into());
            }
        } else if !matches!(
            actual.status,
            ContinuationStatusV1::Scheduled
                | ContinuationStatusV1::Pending
                | ContinuationStatusV1::Waking
        ) || !projection
            .scheduler_wakes
            .iter()
            .any(|wake| wake == &self.wake)
        {
            return Err("Resume compensation requires the exact unconsumed selected wake".into());
        }
        Ok(actual.clone())
    }
}

impl crate::viewer::ViewerRuntimeLiveServer {
    pub(super) fn prepare_rejected_resume_operation(
        &mut self,
        resume: &HostedResume,
        client: &crate::world_service::client::RemoteWorldServiceClient,
    ) -> Result<Option<AgentServiceIoOperation>, String> {
        if !resume.stage.starts_with("rejection_")
            && !resume.stage.starts_with("reject_predecessor")
        {
            return Ok(None);
        }
        resume.validate_rejection(&self.llm_sidecar)?;
        if resume.stage.ends_with("_view") {
            return Ok(Some(AgentServiceIoOperation::View(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: client.config().expected_world.clone(),
                scope_id: client.config().scope_id.clone(),
                min_commit: resume.commit.clone(),
                fixed_commit: None,
                deadline_unix_ms: None,
            })));
        }
        if resume.stage != "reject_predecessor" {
            return Err("Resume compensation stage invalid".into());
        }
        let projection = self
            .llm_sidecar
            .provider_service_projection
            .as_ref()
            .ok_or("Resume compensation projection missing")?;
        resume.validate_projection(projection, false)?;
        let operation = if let Some(pending) = self
            .llm_sidecar
            .provider_scheduler_pending
            .get(&resume.compensation_checkpoint_id())
        {
            let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
                &pending.payload
            else {
                return Err("Resume compensation checkpoint payload mismatch".into());
            };
            match &signed.request.operation {
                SchedulerOperationV1::TransitionContinuation {
                    continuation_id,
                    to: ContinuationStatusV1::Rejected,
                    ..
                } if continuation_id == &resume.wake.continuation_id => {
                    signed.request.operation.clone()
                }
                _ => return Err("Resume compensation checkpoint identity mismatch".into()),
            }
        } else {
            SchedulerOperationV1::TransitionContinuation {
                continuation_id: resume.wake.continuation_id.clone(),
                to: ContinuationStatusV1::Rejected,
                logical_tick: projection.state.time,
            }
        };
        let (checkpoint, existed) = self.llm_sidecar.prepare_service_scheduler_checkpoint(
            &resume.context.request_context,
            "reject_resume_predecessor",
            operation,
            None,
        )?;
        Ok(Some(
            crate::viewer::runtime_live::agent_service_phase::original_intent_io(
                &checkpoint.correlation,
                &checkpoint.payload,
                !existed,
            ),
        ))
    }
    pub(super) fn apply_rejected_resume_response(
        &mut self,
        resume: &mut HostedResume,
        response: AgentServiceIoResponse,
    ) -> Result<(), String> {
        if resume.stage.ends_with("_view") {
            let AgentServiceIoResponse::View(view) = response else {
                return Err("Resume compensation authorized view missing".into());
            };
            if let Some(commit) = &resume.commit {
                if !view
                    .version()
                    .commit
                    .satisfies_minimum(commit)
                    .map_err(|e| e.to_string())?
                {
                    return Err("Resume compensation view precedes transition".into());
                }
            } else if resume.stage != "rejection_view" {
                return Err("Resume compensation commit missing".into());
            }
            self.apply_hosted_verified_view(*view)?;
            let terminal = resume.stage == "reject_predecessor_view";
            resume.validate_projection(
                self.llm_sidecar
                    .provider_service_projection
                    .as_ref()
                    .ok_or("Resume compensation projection missing")?,
                terminal,
            )?;
            resume.stage = if terminal {
                "rejected_finalize"
            } else {
                "reject_predecessor"
            }
            .into();
            resume.commit = None;
            return Ok(());
        }
        let Some(response) =
            crate::viewer::runtime_live::agent_service_phase::original_response(response)?
        else {
            return Ok(());
        };
        let pending = self
            .llm_sidecar
            .provider_scheduler_pending
            .get(&resume.compensation_checkpoint_id())
            .ok_or("Resume compensation checkpoint missing")?;
        response
            .validate(&pending.correlation)
            .map_err(|e| e.to_string())?;
        match response.outcome {
            IntentOutcome::Committed { commit, receipt } => {
                let predecessor: AgentContinuation =
                    serde_json::from_value(receipt).map_err(|e| e.to_string())?;
                if predecessor.continuation_id != resume.wake.continuation_id
                    || predecessor.wake_id != resume.wake.wake_id
                    || predecessor.continuation_proposal_id != resume.predecessor_proposal_id
                    || predecessor.agent_id != resume.context.request_context.agent_subject
                    || predecessor.status != ContinuationStatusV1::Rejected
                {
                    return Err("Resume compensation receipt identity mismatch".into());
                }
                resume.rejected_predecessor = Some(predecessor);
                resume.commit = Some(*commit);
                resume.stage = "reject_predecessor_view".into();
                Ok(())
            }
            IntentOutcome::Received { .. } | IntentOutcome::Pending | IntentOutcome::Unknown => {
                Ok(())
            }
            _ => Err("Resume predecessor compensation not committed; original retained".into()),
        }
    }
}

impl RuntimeLlmSidecar {
    pub(super) fn finalize_rejected_resume(&mut self) -> Result<(), String> {
        let resume = self
            .hosted_resume
            .clone()
            .ok_or("Resume rejected cleanup original missing")?;
        resume.validate_rejection(self)?;
        if resume.stage != "rejected_finalize" {
            return Err("Resume cleanup precedes canonical rejection".into());
        }
        let projection = self
            .provider_service_projection
            .as_ref()
            .ok_or("Resume cleanup verified projection missing")?;
        let predecessor = resume.validate_projection(projection, true)?;
        let authority = projection
            .continuation_contexts
            .get(&predecessor.continuation_id)
            .ok_or("Resume cleanup predecessor authority missing")?;
        let authority = crate::simulator::ContinuationAuthorityContextV1 {
            baseline_observation_digest: authority.baseline_observation_digest.clone(),
            goal_digest: authority.goal_digest.clone(),
            policy_digest: authority.policy_digest.clone(),
            precondition_digest: authority.precondition_digest.clone(),
        };
        let request = &resume.context.request_context;
        if self.hosted_admission.is_some()
            || self
                .provider_cognition_leases
                .contains_key(&request.agent_subject)
            || self.provider_completed_decisions.iter().any(|decision| {
                decision.cognition.as_ref().is_some_and(|c| {
                    c.request.request_context.agent_subject == request.agent_subject
                })
            })
        {
            return Err("Resume cleanup conflicts with newer admission or decision".into());
        }
        for context in [
            self.provider_contexts.get(&request.agent_subject),
            self.provider_active_turns.get(&request.agent_subject),
        ]
        .into_iter()
        .flatten()
        {
            let old = &context.request_context;
            if old != request
                && !(old.agent_session_id == predecessor.agent_session_id
                    && old.agent_turn_id == predecessor.agent_turn_id
                    && old.decision_request_id == predecessor.decision_request_id
                    && old.request_digest.to_string() == predecessor.origin_request_digest)
            {
                return Err("Resume cleanup newer context conflict".into());
            }
        }
        if self
            .provider_held_decisions
            .contains_key(&request.agent_subject)
        {
            return Err("Resume cleanup conflicts with a held decision".into());
        }
        if self.hosted_restored_resume.is_some() && self.runner.is_none() {
            // An issued recovery must complete while Pause fences fresh admission.
            // Register local actors only; this does not start a provider turn or lease.
            self.ensure_runner_initialized()?;
        }
        let backup = (
            self.provider_continuation_proposals.clone(),
            self.pending_runtime_wakes.clone(),
            self.provider_contexts.clone(),
            self.provider_active_turns.clone(),
            self.provider_terminal_states.clone(),
            self.hosted_resume.clone(),
            self.hosted_restored_resume.clone(),
        );
        #[cfg(any(test, feature = "test_tier_required"))]
        let test_persist = self
            .test_resume_rejection_root()?
            .map(|root| {
                let native = self
                    .runner
                    .as_ref()
                    .and_then(RuntimeDecisionRunner::async_runner)
                    .ok_or("test rejected Resume native runner missing")?;
                self.test_resume_rejection_state(native.rejected_wait_test_ledger_digests())
                    .map(|before| (root, before))
            })
            .transpose()?;
        #[cfg(any(test, feature = "test_tier_required"))]
        let mut test_persist_attempted = false;
        let mut runner = self.runner.take().ok_or("Resume cleanup runner missing")?;
        let result = (|| {
            let native = runner
                .async_runner_mut()
                .ok_or("Resume cleanup native runner missing")?;
            let restored = self.hosted_restored_resume.is_some();
            if restored
                && !native
                    .rejected_unprojected_resume_is_absent(
                        &request.agent_subject,
                        &resume.proposal,
                        request,
                        &resume.context.turn_context,
                    )
                    .map_err(|e| e.to_string())?
            {
                return Err("restored Resume rejection local continuation conflict".into());
            }
            let mut persist = |_digests: Option<[String; 5]>| {
                self.provider_continuation_proposals
                    .remove(&resume.predecessor_proposal_id);
                self.provider_continuation_proposals
                    .remove(&resume.proposal.continuation_proposal_id);
                self.pending_runtime_wakes.remove(&resume.wake.wake_id);
                self.provider_contexts.remove(&request.agent_subject);
                self.provider_active_turns.remove(&request.agent_subject);
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
                        status: "rejected".into(),
                        reject_reason: Some("canonical_resume_rejected".into()),
                        feedback_id: None,
                        feedback: None,
                    },
                );
                #[cfg(any(test, feature = "test_tier_required"))]
                if let Some((root, before)) = &test_persist {
                    let staged = self.test_resume_rejection_state(
                        _digests.ok_or("test rejected Resume staged ledgers missing")?,
                    )?;
                    Self::test_write_resume_handoff_file(
                        root,
                        "resume-rejection-before.json",
                        &serde_json::to_vec(before).map_err(|_| "test Resume snapshot failed")?,
                    )?;
                    Self::test_write_resume_handoff_file(
                        root,
                        "resume-rejection-staged.json",
                        &serde_json::to_vec(&staged).map_err(|_| "test Resume snapshot failed")?,
                    )?;
                    Self::test_write_resume_handoff_file(
                        root,
                        "resume-rejection-before-persist",
                        b"ready",
                    )?;
                    Self::test_wait_resume_handoff_file(root, "resume-rejection-release")?;
                    test_persist_attempted = true;
                }
                self.persist_provider_lineage()
            };
            if restored {
                #[cfg(any(test, feature = "test_tier_required"))]
                let digests = Some(native.rejected_wait_test_ledger_digests());
                #[cfg(not(any(test, feature = "test_tier_required")))]
                let digests = None;
                persist(digests)
            } else {
                #[cfg(any(test, feature = "test_tier_required"))]
                if test_persist.is_some() {
                    return native
                        .with_rejected_runtime_continuation_cleanup_observed(
                            &request.agent_subject,
                            predecessor,
                            &authority,
                            |digests| persist(Some(digests)),
                        )
                        .map(|_| ())
                        .map_err(|e| e.to_string());
                }
                native
                    .with_rejected_runtime_continuation_cleanup(
                        &request.agent_subject,
                        predecessor,
                        &authority,
                        || persist(None),
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        })();
        self.runner = Some(runner);
        if result.is_err() {
            (
                self.provider_continuation_proposals,
                self.pending_runtime_wakes,
                self.provider_contexts,
                self.provider_active_turns,
                self.provider_terminal_states,
                self.hosted_resume,
                self.hosted_restored_resume,
            ) = backup;
            #[cfg(any(test, feature = "test_tier_required"))]
            if test_persist_attempted && let Some((root, _)) = &test_persist {
                let recorded = self
                    .runner
                    .as_ref()
                    .and_then(RuntimeDecisionRunner::async_runner)
                    .ok_or_else(|| "test Resume rollback runner missing".to_string())
                    .and_then(|native| {
                        self.test_resume_rejection_state(native.rejected_wait_test_ledger_digests())
                    })
                    .and_then(|after| {
                        Self::test_write_resume_handoff_file(
                            root,
                            "resume-rejection-after.json",
                            &serde_json::to_vec(&after)
                                .map_err(|_| "test Resume rollback snapshot failed")?,
                        )?;
                        Self::test_write_resume_handoff_file(
                            root,
                            "resume-rejection-rollback",
                            b"ready",
                        )
                    });
                if recorded.is_ok() {
                    eprintln!("hosted_resume_rejection_persistence_failed");
                    let _ =
                        Self::test_wait_resume_handoff_file(root, "resume-rejection-retry-release");
                }
            }
        }
        result
    }
}
