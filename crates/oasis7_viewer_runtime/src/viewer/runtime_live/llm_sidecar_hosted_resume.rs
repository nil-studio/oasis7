//! Selected wake recovery retains the original prepared context and signed identity.
use super::*;
use crate::viewer::runtime_live::agent_service_io::*;
use crate::world_service::wire::SchedulerOperationV1;
use oasis7_client_api::world_service::*;
#[path = "llm_sidecar_hosted_final_budget.rs"]
mod final_budget;
#[path = "llm_sidecar_hosted_resume_rejection.rs"]
mod rejection;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::viewer::runtime_live) struct HostedResume {
    context: ProviderContextState,
    observation: Observation,
    proposal: SimulatorContinuationProposalV1,
    runtime: crate::runtime::CognitionContinuationProposalV1,
    current: crate::simulator::ContinuationCurrentContextV1,
    wake: crate::runtime::SchedulerWakeV1,
    predecessor_proposal_id: String,
    metadata_identity: String,
    stage: String,
    commit: Option<CommitRef>,
    receipt: Option<crate::runtime::CognitionWakeHandoffResultV1>,
    #[serde(default)]
    rejection: Option<IntentResponse<serde_json::Value>>,
    #[serde(default)]
    rejected_predecessor: Option<crate::runtime::AgentContinuation>,
    #[serde(default)]
    final_budget: bool,
    #[serde(default)]
    budget_receipt: Option<crate::runtime::CognitionBudgetConsumptionV1>,
    #[serde(default)]
    budget_predecessor: Option<crate::runtime::AgentContinuation>,
}
impl HostedResume {
    fn operation(&self) -> SchedulerOperationV1 {
        let request = &self.context.request_context;
        SchedulerOperationV1::ResumeWake {
            wake_id: self.wake.wake_id.clone(),
            proposal: self.runtime.clone(),
            budget_spent: 1,
            resume: crate::runtime::CognitionContinuationResumeRequestV1 {
                agent_session_id: request.agent_session_id.clone(),
                agent_turn_id: request.agent_turn_id.clone(),
                decision_request_id: request.decision_request_id.clone(),
                request_digest: request.request_digest.to_string(),
                context_digest: async_support::runtime_provider_context_digest(request),
            },
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
    fn original_inputs(&self) -> Result<serde_json::Value, String> {
        serde_json::to_value((
            &self.context,
            &self.observation,
            &self.proposal,
            &self.runtime,
            &self.current,
            &self.wake,
            &self.predecessor_proposal_id,
            &self.metadata_identity,
            &self.final_budget,
            &self.budget_predecessor,
        ))
        .map_err(|error| error.to_string())
    }
    fn validate_original(&self) -> Result<(), String> {
        if !self.final_budget
            && (self.budget_receipt.is_some() || self.budget_predecessor.is_some())
        {
            return Err("ordinary Resume contains final budget state".into());
        }
        let request = &self.context.request_context;
        let turn = &self.context.turn_context;
        request
            .validate_production_lane()
            .map_err(|error| error.to_string())?;
        turn.validate_for_agent(&request.agent_subject)
            .map_err(|error| error.to_string())?;
        self.proposal
            .validate()
            .map_err(|error| error.to_string())?;
        self.runtime.validate().map_err(|error| error.to_string())?;
        self.wake.validate().map_err(|error| format!("{error:?}"))?;
        let current = crate::simulator::ContinuationCurrentContextV1::from_observation(
            self.observation.clone(),
            &turn.goal_snapshot,
            provider_policy_context_digest(request),
            provider_wait_precondition_digest(&self.observation),
        );
        current
            .validate_for_agent(&request.agent_subject)
            .map_err(|error| error.to_string())?;
        current
            .authority
            .validate_proposal(&self.proposal)
            .map_err(|error| error.to_string())?;
        let runtime: crate::runtime::CognitionContinuationProposalV1 = serde_json::from_value(
            serde_json::to_value(&self.proposal).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if turn.agent_session_id != request.agent_session_id
            || turn.agent_turn_id != request.agent_turn_id
            || turn.decision_request_id != request.decision_request_id
            || turn.request_digest != request.request_digest
            || crate::simulator::Digest32::from(turn.memory_snapshot.digest.clone())
                != request.memory_snapshot_digest
            || crate::simulator::Digest32::from(turn.goal_snapshot.digest.clone())
                != request.goal_snapshot_digest
            || crate::simulator::h_v1("oasis7.cognition.continuation.v1", &turn.continuation)
                != request.continuation_digest
            || crate::simulator::h_v1(
                "oasis7.cognition.observation.v1",
                &request.base_decision_request.observation,
            ) != request.observation_digest
            || serde_json::to_value(&turn.continuation).map_err(|error| error.to_string())?
                != serde_json::to_value(Some(&self.proposal)).map_err(|error| error.to_string())?
            || current != self.current
            || runtime.proposal_digest() != self.runtime.proposal_digest
            || self.proposal.agent_id != request.agent_subject
            || self.proposal.agent_session_id != request.agent_session_id
            || self.proposal.agent_turn_id != request.agent_turn_id
            || self.proposal.decision_request_id != request.decision_request_id
            || self.wake.agent_id != request.agent_subject
            || self.observation.agent_id != request.agent_subject
            || self.observation.time != request.base_decision_request.observation.world_time
        {
            return Err("restored Resume original context or proposal mismatch".into());
        }
        Ok(())
    }
}
impl RuntimeLlmSidecar {
    #[cfg(any(test, feature = "test_tier_required"))]
    fn test_resume_handoff_state(&self) -> Result<serde_json::Value, String> {
        serde_json::to_value(serde_json::json!({
            "proposals": self.provider_continuation_proposals,
            "pending_wakes": self.pending_runtime_wakes,
            "resume": self.hosted_resume,
            "admission": self.hosted_admission,
            "restored_original": self.hosted_restored_resume,
            "leases": self.provider_cognition_leases,
            "completed_queue": self.provider_completed_decisions,
            "service_pending": self.provider_service_pending,
            "scheduler_pending": self.provider_scheduler_pending,
            "active_turns": self.provider_active_turns,
            "contexts": self.provider_contexts,
            "retry_contexts": self.provider_retry_contexts,
            "held_decisions": self.provider_held_decisions,
            "session_ids": self.provider_session_ids,
            "context_seq": self.provider_context_seq,
            "memory": self.provider_memory_store,
        }))
        .map_err(|_| "test Resume handoff snapshot failed".into())
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    fn test_resume_handoff_root(&self) -> Result<Option<std::path::PathBuf>, String> {
        let Some(root) = std::env::var_os("PRE2_RESUME_HANDOFF_FS_ROOT") else {
            return Ok(None);
        };
        let root = std::path::PathBuf::from(root);
        if !root.is_absolute() || !root.is_dir() {
            return Err("test Resume handoff private directory required".into());
        }
        if root.join("resume-handoff-before-persist").exists() {
            return Ok(None);
        }
        Ok(Some(root))
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    fn test_write_resume_handoff_file(
        root: &std::path::Path,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "test Resume handoff private directory failed")?;
            options.mode(0o600);
        }
        options
            .open(root.join(name))
            .and_then(|mut file| file.write_all(bytes))
            .map_err(|_| "test Resume handoff artifact write failed".into())
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    fn test_wait_resume_handoff_file(root: &std::path::Path, name: &str) -> Result<(), String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !root.join(name).exists() {
            if std::time::Instant::now() >= deadline {
                return Err("test Resume handoff barrier deadline".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Ok(())
    }
    fn validate_restored_hosted_resume(&mut self, resume: &HostedResume) -> Result<(), String> {
        let Some((original, signed_original)) = self.hosted_restored_resume.clone() else {
            return Ok(());
        };
        resume.validate_original()?;
        if original.original_inputs()? != resume.original_inputs()? {
            return Err("restored Resume original inputs changed; fenced".into());
        }
        if let Some(original) = signed_original {
            let request = &resume.context.request_context;
            let key = format!(
                "{}:resume:{}",
                request.provider_invocation_key(),
                resume.wake.wake_id
            );
            let actual = self
                .provider_scheduler_pending
                .get(&key)
                .ok_or("restored Resume signed original missing; fenced")?;
            if serde_json::to_value(actual).map_err(|error| error.to_string())?
                != serde_json::to_value(&original).map_err(|error| error.to_string())?
            {
                return Err("restored Resume signed checkpoint changed; fenced".into());
            }
            // The ordinary checkpoint validator checks the current signer and
            // delegation generation against the original signed operation.
            let (_, existed) = self.prepare_service_scheduler_checkpoint(
                request,
                &format!("resume:{}", resume.wake.wake_id),
                resume.operation(),
                Some((resume.context.clone(), resume.current.clone())),
            )?;
            if !existed {
                return Err("restored Resume original lookup checkpoint missing".into());
            }
        }
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn restore_hosted_resume_original(
        &mut self,
    ) -> Result<(), String> {
        self.hosted_restored_resume = None;
        if let Some(resume) = self.hosted_resume.clone()
            && resume.final_budget
        {
            return self.restore_hosted_final_budget(resume);
        }
        if let Some(resume) = self.hosted_resume.as_ref() {
            resume.validate_original()?;
            let key = format!(
                "{}:resume:{}",
                resume.context.request_context.provider_invocation_key(),
                resume.wake.wake_id
            );
            let pending = self.provider_scheduler_pending.get(&key).cloned();
            if pending.is_none() && (resume.stage != "resume" || resume.rejection.is_some()) {
                return Err("restored issued Resume signed checkpoint missing".into());
            }
            if let Some(pending) = &pending {
                let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
                    &pending.payload
                else {
                    return Err("restored Resume signed operation missing".into());
                };
                crate::world_service::verify_read_request("scheduler", signed)?;
                if signed.request.operation != resume.operation()
                    || crate::world_service::derive_correlation(
                        pending.correlation.key.world.clone(),
                        &pending.payload,
                    )? != pending.correlation
                    || serde_json::to_value(pending.resume_context.as_ref())
                        .map_err(|error| error.to_string())?
                        != serde_json::to_value(Some(&resume.context))
                            .map_err(|error| error.to_string())?
                    || pending.resume_current_context.as_ref() != Some(&resume.current)
                {
                    return Err("restored Resume signed original mismatch".into());
                }
            }
            if resume.rejection.is_some() {
                resume.validate_rejection(self)?;
            }
            self.hosted_restored_resume = Some((resume.clone(), pending));
        }
        if let Some(resume) = self.hosted_resume.as_mut() {
            resume.stage = "resume".into();
            resume.commit = None;
        }
        Ok(())
    }
    fn capture_hosted_resume(&mut self, world: &RuntimeWorld) -> Result<bool, String> {
        let Some(metadata_identity) = self.fresh_provider_metadata_identity()? else {
            return Ok(false);
        };
        let view = self
            .provider_service_projection
            .as_ref()
            .ok_or("hosted Resume canonical view missing")?;
        let Some(wake) = view
            .scheduler_wakes
            .iter()
            .min_by_key(|wake| (wake.wake_seq, wake.wake_id.as_str()))
            .cloned()
        else {
            return Ok(false);
        };
        wake.validate().map_err(|error| format!("{error:?}"))?;
        self.ensure_canonical_agent_durable_admission()?;
        let authority = view
            .agent_context
            .as_ref()
            .filter(|authority| authority.agent_id == wake.agent_id)
            .ok_or("hosted Resume authority missing")?;
        let predecessor = view
            .continuations
            .iter()
            .find(|entry| entry.continuation_id == wake.continuation_id)
            .ok_or("hosted Resume predecessor missing")?;
        let predecessor_proposal_id = predecessor.continuation_proposal_id.clone();
        let sequence = self
            .provider_context_seq
            .get(&wake.agent_id)
            .copied()
            .unwrap_or(1)
            .max(wake.retry_seq.saturating_add(1))
            .max(1);
        let session = format!(
            "{}-resume-{sequence}",
            authority
                .capability_invocation_context
                .presenter
                .session_id
                .clone()
                .ok_or("hosted Resume session missing")?
        );
        let capability = ProviderCapabilityContext {
            catalog: authority.capability_catalog.clone(),
            invocation: authority.capability_invocation_context.clone(),
            session_id: session.clone(),
        };
        let binding = view
            .runtime_binding
            .clone()
            .ok_or("hosted Resume binding missing")?;
        let (proposal, runtime) = runtime_continuation_for_wake_with_identity(
            world,
            Some(view),
            &wake,
            &session,
            sequence,
        )?;
        let observation = self
            .shadow_kernel
            .as_mut()
            .ok_or("hosted Resume observation missing")?
            .observe(&wake.agent_id)
            .map_err(|error| format!("{error:?}"))?;
        let settings =
            provider_settings_from_env()?.ok_or("hosted Resume provider settings missing")?;
        let (turn_context, request_context) = build_provider_context(ProviderContextInput {
            session_id: &session,
            sequence,
            agent_id: &wake.agent_id,
            observation: observation.clone(),
            settings: &settings,
            runtime_binding: binding,
            recent_event_summary: &[],
            capability_context: capability,
            replan_cause: None,
            continuation: Some(&proposal),
            memory_store: &self.provider_memory_store,
            goal_snapshot: trusted_provider_goal_snapshot(
                self.prompt_profiles.get(&wake.agent_id),
            )?,
        })?;
        let current = crate::simulator::ContinuationCurrentContextV1::from_observation(
            observation.clone(),
            &turn_context.goal_snapshot,
            provider_policy_context_digest(&request_context),
            provider_wait_precondition_digest(&observation),
        );
        current
            .validate_for_agent(&wake.agent_id)
            .map_err(|error| error.to_string())?;
        self.provider_continuation_proposals
            .insert(proposal.continuation_proposal_id.clone(), proposal.clone());
        self.provider_context_seq
            .insert(wake.agent_id.clone(), sequence.saturating_add(1));
        self.hosted_resume = Some(HostedResume {
            context: ProviderContextState {
                turn_context,
                request_context,
            },
            observation,
            proposal,
            runtime,
            current,
            wake,
            predecessor_proposal_id,
            metadata_identity,
            stage: "resume".into(),
            commit: None,
            receipt: None,
            rejection: None,
            rejected_predecessor: None,
            final_budget: false,
            budget_receipt: None,
            budget_predecessor: None,
        });
        self.persist_provider_lineage()?;
        Ok(true)
    }
    fn reconcile_hosted_resume(&mut self) -> Result<(), String> {
        let resume = self
            .hosted_resume
            .clone()
            .ok_or("hosted Resume original missing")?;
        let receipt = resume
            .receipt
            .as_ref()
            .ok_or("hosted Resume committed receipt missing")?;
        let next = receipt
            .replanned_continuation
            .as_ref()
            .ok_or("hosted Resume successor missing")?;
        let view = self
            .provider_service_projection
            .as_ref()
            .ok_or("hosted Resume view missing")?;
        let actual = view
            .continuations
            .iter()
            .find(|entry| entry.continuation_id == next.continuation_id)
            .ok_or("hosted Resume successor absent from verified view")?;
        if actual.continuation_proposal_id != resume.proposal.continuation_proposal_id
            || actual.origin_request_digest != resume.proposal.origin_request_digest
            || serde_json::to_value(actual).map_err(|error| error.to_string())?
                != serde_json::to_value(next).map_err(|error| error.to_string())?
        {
            return Err("hosted Resume verified successor mismatch".into());
        }
        let authority = view
            .continuation_contexts
            .get(&actual.continuation_id)
            .ok_or("hosted Resume verified successor authority missing")?;
        let authority = crate::simulator::ContinuationAuthorityContextV1 {
            baseline_observation_digest: authority.baseline_observation_digest.clone(),
            goal_digest: authority.goal_digest.clone(),
            policy_digest: authority.policy_digest.clone(),
            precondition_digest: authority.precondition_digest.clone(),
        };
        authority
            .validate_proposal(&resume.proposal)
            .map_err(|error| error.to_string())?;
        let restored = self.hosted_restored_resume.is_some();
        let runner = self
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .ok_or("hosted Resume runner missing")?;
        if runner.active_continuation_proposal_id(&resume.wake.agent_id)
            == Some(resume.proposal.continuation_proposal_id.as_str())
        {
            // A prior attempt may have reconciled the Harness before the final
            // checkpoint write failed. Reuse only this verified successor.
            runner
                .validate_active_continuation_with_authority(
                    &resume.wake.agent_id,
                    &authority,
                    actual,
                )
                .map_err(|error| error.to_string())?;
        } else if restored {
            runner
                .hydrate_runtime_continuation_with_authority(
                    &resume.wake.agent_id,
                    resume.proposal.clone(),
                    &authority,
                    actual.clone(),
                )
                .map_err(|error| format!("hosted restored Resume hydration failed: {error}"))?;
        } else {
            runner
                .reconcile_runtime_wake_with_current_context(
                    &resume.wake.agent_id,
                    &resume.current,
                    &receipt.continuation,
                    Some(resume.proposal.clone()),
                )
                .map_err(|error| {
                    #[cfg(any(test, feature = "test_tier_required"))]
                    if matches!(
                        &error,
                        crate::simulator::AsyncAgentRunnerError::Cognition(reason)
                            if reason == "unknown continuation"
                    ) {
                        static REPORTED: std::sync::atomic::AtomicBool =
                            std::sync::atomic::AtomicBool::new(false);
                        if !REPORTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                            eprintln!("hosted_resume_missing_harness_continuation");
                        }
                    }
                    format!("hosted Resume Harness reconciliation failed: {error}")
                })?;
        }
        let proposals_backup = self.provider_continuation_proposals.clone();
        let wakes_backup = self.pending_runtime_wakes.clone();
        let admission_backup = self.hosted_admission.clone();
        let restored_backup = self.hosted_restored_resume.clone();
        #[cfg(any(test, feature = "test_tier_required"))]
        let test_handoff = self
            .test_resume_handoff_root()?
            .map(|root| {
                self.test_resume_handoff_state()
                    .map(|before| (root, before))
            })
            .transpose()?;
        self.provider_continuation_proposals
            .remove(&resume.predecessor_proposal_id);
        self.pending_runtime_wakes.remove(&resume.wake.wake_id);
        self.install_resumed_hosted_admission(
            resume.context.clone(),
            resume.observation.clone(),
            resume.metadata_identity.clone(),
        )
        .inspect_err(|_| {
            self.provider_continuation_proposals = proposals_backup.clone();
            self.pending_runtime_wakes = wakes_backup.clone();
            self.hosted_admission = admission_backup.clone();
        })?;
        self.hosted_resume = None;
        self.hosted_restored_resume = None;
        // This is the sole durable handoff: successor admission and retirement
        // of the original Resume become visible together.
        #[cfg(any(test, feature = "test_tier_required"))]
        let mut test_persist_attempted = false;
        let persisted = (|| {
            #[cfg(any(test, feature = "test_tier_required"))]
            if let Some((root, before)) = &test_handoff {
                let staged = self.test_resume_handoff_state()?;
                Self::test_write_resume_handoff_file(
                    root,
                    "resume-handoff-before.json",
                    &serde_json::to_vec(before)
                        .map_err(|_| "test Resume handoff snapshot failed")?,
                )?;
                Self::test_write_resume_handoff_file(
                    root,
                    "resume-handoff-staged.json",
                    &serde_json::to_vec(&staged)
                        .map_err(|_| "test Resume handoff snapshot failed")?,
                )?;
                Self::test_write_resume_handoff_file(
                    root,
                    "resume-handoff-before-persist",
                    b"ready",
                )?;
                Self::test_wait_resume_handoff_file(root, "resume-handoff-release")?;
                test_persist_attempted = true;
            }
            self.persist_provider_lineage()
        })();
        if let Err(error) = persisted {
            self.provider_continuation_proposals = proposals_backup;
            self.pending_runtime_wakes = wakes_backup;
            self.hosted_admission = admission_backup;
            self.hosted_resume = Some(resume);
            self.hosted_restored_resume = restored_backup;
            #[cfg(any(test, feature = "test_tier_required"))]
            if test_persist_attempted && let Some((root, _)) = &test_handoff {
                // Reread the actual restored state; never report the backup as
                // evidence that rollback happened.
                let recorded = self.test_resume_handoff_state().and_then(|after| {
                    Self::test_write_resume_handoff_file(
                        root,
                        "resume-handoff-after.json",
                        &serde_json::to_vec(&after)
                            .map_err(|_| "test Resume handoff snapshot failed")?,
                    )?;
                    Self::test_write_resume_handoff_file(
                        root,
                        "resume-handoff-rollback",
                        b"rolled back",
                    )
                });
                if recorded.is_ok() {
                    eprintln!("hosted_resume_handoff_persistence_failed");
                    let _ =
                        Self::test_wait_resume_handoff_file(root, "resume-handoff-retry-release");
                }
            }
            return Err(error);
        }
        Ok(())
    }
}
impl crate::viewer::ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn prepare_hosted_resume_io(
        &mut self,
        eligible: bool,
    ) -> Result<AgentServiceProgress, String> {
        if self.llm_sidecar.hosted_service_inflight.is_some() {
            return Ok(AgentServiceProgress::Idle);
        };
        if self.llm_sidecar.hosted_resume.is_none() {
            if !eligible || self.llm_sidecar.hosted_admission.is_some() {
                return Ok(AgentServiceProgress::Idle);
            };
            self.llm_sidecar.sync_runtime_wakes(&self.world)?;
            if !self.llm_sidecar.capture_hosted_final_budget()?
                && !self.llm_sidecar.capture_hosted_resume(&self.world)?
            {
                return Ok(AgentServiceProgress::Idle);
            };
        }
        let resume = self
            .llm_sidecar
            .hosted_resume
            .clone()
            .ok_or("hosted Resume missing")?;
        if resume.final_budget {
            return self.prepare_hosted_final_budget_io(resume);
        }
        self.llm_sidecar.validate_restored_hosted_resume(&resume)?;
        if resume.stage == "rejected_finalize" {
            self.llm_sidecar.finalize_rejected_resume()?;
            self.defer_next_auto_play_step_after_completion(self.config.play_step_interval);
            return Ok(AgentServiceProgress::Advanced);
        }
        if resume.stage == "reconcile" {
            self.llm_sidecar.reconcile_hosted_resume()?;
            return Ok(AgentServiceProgress::Advanced);
        };
        self.llm_sidecar.persist_provider_lineage()?;
        let request = &resume.context.request_context;
        request
            .validate_production_lane()
            .map_err(|error| error.to_string())?;
        let client = self
            .world_service_client()
            .map_err(|error| format!("{error:?}"))?
            .ok_or("hosted Resume client missing")?;
        let operation =
            if let Some(operation) = self.prepare_rejected_resume_operation(&resume, &client)? {
                operation
            } else if resume.stage == "resume_view" {
                AgentServiceIoOperation::View(ReadWorldViewRequest {
                    observation_nonce: None,
                    formal_anchor: None,
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    world: client.config().expected_world.clone(),
                    scope_id: client.config().scope_id.clone(),
                    min_commit: resume.commit,
                    fixed_commit: None,
                    deadline_unix_ms: None,
                })
            } else {
                let operation = resume.operation();
                let (checkpoint, existed) = self.llm_sidecar.prepare_service_scheduler_checkpoint(
                    request,
                    &format!("resume:{}", resume.wake.wake_id),
                    operation,
                    Some((resume.context.clone(), resume.current.clone())),
                )?;
                crate::viewer::runtime_live::agent_service_phase::original_intent_io(
                    &checkpoint.correlation,
                    &checkpoint.payload,
                    !existed,
                )
            };
        self.llm_sidecar.hosted_service_generation =
            self.llm_sidecar.hosted_service_generation.saturating_add(1);
        let token = AgentServiceIoToken {
            generation: self.llm_sidecar.hosted_service_generation,
            config_digest: self.hosted_service_config_digest()?,
            phase_digest: format!(
                "resume:{}:{}",
                resume.stage,
                request.provider_invocation_key()
            ),
        };
        self.llm_sidecar.hosted_service_inflight = Some(token.clone());
        Ok(AgentServiceProgress::NeedsIo(Box::new(AgentServiceIoJob {
            token,
            client: Some(client),
            operation,
        })))
    }
    pub(in crate::viewer::runtime_live) fn apply_hosted_resume_io(
        &mut self,
        result: AgentServiceIoResult,
    ) -> Result<AgentServiceProgress, String> {
        if self.llm_sidecar.hosted_service_inflight.as_ref() != Some(&result.token) {
            return Err("hosted Resume token mismatch".into());
        };
        self.llm_sidecar.hosted_service_inflight = None;
        if result.token.config_digest != self.hosted_service_config_digest()? {
            return Err("hosted Resume configuration changed; fenced".into());
        };
        let mut resume = self
            .llm_sidecar
            .hosted_resume
            .clone()
            .ok_or("hosted Resume missing")?;
        let request = &resume.context.request_context;
        if result.token.phase_digest
            != format!(
                "resume:{}:{}",
                resume.stage,
                request.provider_invocation_key()
            )
        {
            return Err("hosted Resume phase mismatch".into());
        };
        let response = result.response?;
        if resume.final_budget {
            return self.apply_hosted_final_budget_io(resume, response);
        }
        if resume.stage.starts_with("rejection_") || resume.stage.starts_with("reject_predecessor")
        {
            self.apply_rejected_resume_response(&mut resume, response)?;
        } else if resume.stage == "resume_view" {
            let AgentServiceIoResponse::View(view) = response else {
                return Err("hosted Resume minimum view missing".into());
            };
            if !view
                .version()
                .commit
                .satisfies_minimum(
                    resume
                        .commit
                        .as_ref()
                        .ok_or("hosted Resume commit missing")?,
                )
                .map_err(|error| error.to_string())?
            {
                return Err("hosted Resume view precedes receipt".into());
            };
            self.apply_hosted_verified_view(*view)?;
            resume.stage = "reconcile".into();
            resume.commit = None;
        } else {
            let Some(response) =
                crate::viewer::runtime_live::agent_service_phase::original_response(response)?
            else {
                return Ok(AgentServiceProgress::Advanced);
            };
            let checkpoint = self
                .llm_sidecar
                .provider_scheduler_pending
                .get(&format!(
                    "{}:resume:{}",
                    request.provider_invocation_key(),
                    resume.wake.wake_id
                ))
                .ok_or("hosted Resume original checkpoint missing")?;
            response
                .validate(&checkpoint.correlation)
                .map_err(|error| error.to_string())?;
            let original_response = response.clone();
            match response.outcome {
                IntentOutcome::Committed { commit, receipt } => {
                    if resume.rejection.is_some() {
                        return Err("Resume outcome conflicts with saved rejection".into());
                    }
                    let receipt: crate::runtime::CognitionWakeHandoffResultV1 =
                        serde_json::from_value(receipt).map_err(|error| error.to_string())?;
                    let next = receipt
                        .replanned_continuation
                        .as_ref()
                        .ok_or("hosted Resume receipt successor missing")?;
                    if receipt.wake.wake_id != resume.wake.wake_id
                        || receipt.continuation.continuation_id != resume.wake.continuation_id
                        || receipt.continuation.continuation_proposal_id
                            != resume.predecessor_proposal_id
                        || receipt.continuation.agent_id != request.agent_subject
                        || receipt.continuation.status
                            != crate::runtime::ContinuationStatusV1::Consumed
                        || next.continuation_proposal_id != resume.proposal.continuation_proposal_id
                        || next.agent_session_id != request.agent_session_id
                        || next.agent_turn_id != request.agent_turn_id
                        || next.decision_request_id != request.decision_request_id
                        || next.origin_request_digest != resume.proposal.origin_request_digest
                        || next.status != crate::runtime::ContinuationStatusV1::Scheduled
                    {
                        return Err("hosted Resume receipt identity mismatch".into());
                    };
                    resume.receipt = Some(receipt);
                    resume.commit = Some(*commit);
                    resume.stage = "resume_view".into();
                }
                IntentOutcome::Received { .. }
                | IntentOutcome::Pending
                | IntentOutcome::Unknown => return Ok(AgentServiceProgress::Advanced),
                IntentOutcome::Rejected { .. } => {
                    resume.accept_rejection(original_response, checkpoint)?;
                    resume.stage = "rejection_view".into();
                    resume.commit = None;
                }
                _ => return Err("canonical Resume cannot complete original".into()),
            }
        }
        self.llm_sidecar.hosted_resume = Some(resume);
        self.llm_sidecar.persist_provider_lineage()?;
        Ok(AgentServiceProgress::Advanced)
    }
}
