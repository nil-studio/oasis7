//! Original Wait admission and settlement through bounded transport jobs.
use super::*;
use crate::viewer::runtime_live::agent_service_io::*;
use crate::world_service::wire::SchedulerOperationV1;
use oasis7_client_api::world_service::*;
#[path = "llm_sidecar_hosted_wait_rejection.rs"]
mod rejection;
#[path = "llm_sidecar_hosted_wait_resumed.rs"]
mod resumed;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::viewer::runtime_live) struct HostedWait {
    decision: async_support::RuntimeLlmDecision,
    proposal: SimulatorContinuationProposalV1,
    runtime: crate::runtime::CognitionContinuationProposalV1,
    current: crate::simulator::ContinuationCurrentContextV1,
    stage: String,
    commit: Option<CommitRef>,
    admitted: Option<crate::runtime::AgentContinuation>,
    #[serde(default)]
    rejection: Option<IntentResponse<serde_json::Value>>,
}
impl HostedWait {
    fn validate_decision(decision: &async_support::RuntimeLlmDecision) -> Result<(), String> {
        let cognition = decision
            .cognition
            .as_ref()
            .ok_or("restored Wait cognition missing")?;
        let request = &cognition.request.request_context;
        let turn = &cognition.request.turn_context;
        request
            .validate_production_lane()
            .map_err(|error| error.to_string())?;
        turn.validate_for_agent(&request.agent_subject)
            .map_err(|error| error.to_string())?;
        cognition
            .response
            .validate_response_artifact_identity(&cognition.response.response_artifact_identity())
            .map_err(|error| error.to_string())?;
        let lease = cognition
            .cognition_lease
            .as_ref()
            .ok_or("restored Wait lease missing")?;
        lineage_generation_recovery::validate_provider_lease_identity(
            &request.agent_subject,
            request,
            lease,
        )?;
        let decision_matches = match (
            &decision.decision,
            &cognition.response.base_decision_response.decision,
        ) {
            (AgentDecision::Wait, crate::simulator::ProviderDecision::Wait) => true,
            (
                AgentDecision::WaitTicks(actual),
                crate::simulator::ProviderDecision::WaitTicks { ticks },
            ) => actual == ticks,
            _ => false,
        };
        if !decision_matches
            || decision.agent_id != request.agent_subject
            || cognition.response.context_discriminator != request.context_discriminator
            || cognition.response.context_version != request.context_version
            || cognition.response.agent_session_id != request.agent_session_id
            || cognition.response.agent_turn_id != request.agent_turn_id
            || cognition.response.decision_request_id != request.decision_request_id
            || cognition.response.request_digest != request.request_digest
            || cognition.response.retry_seq != request.retry_seq
            || cognition.response.transport_attempt != request.transport_attempt
            || turn.agent_session_id != request.agent_session_id
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
            || serde_json::to_value(&cognition.memory_write_intents)
                .map_err(|error| error.to_string())?
                != serde_json::to_value(
                    &cognition
                        .response
                        .base_decision_response
                        .memory_write_intents,
                )
                .map_err(|error| error.to_string())?
            || serde_json::to_value(&decision.memory_write_intents)
                .map_err(|error| error.to_string())?
                != serde_json::to_value(&cognition.memory_write_intents)
                    .map_err(|error| error.to_string())?
        {
            return Err("restored queued Wait original decision mismatch".into());
        }
        Ok(())
    }
    fn validate_original(&self) -> Result<(), String> {
        Self::validate_decision(&self.decision)?;
        let cognition = self
            .decision
            .cognition
            .as_ref()
            .ok_or("restored Wait cognition missing")?;
        let (proposal, mut runtime, current) = super::wait_proposal::build_hosted_wait_proposal(
            cognition,
            self.current.observation.clone(),
        )?;
        if cognition.request.turn_context.continuation.is_some() {
            // Provider binding hashes the raw Runtime manifest in a separate domain.
            // The retained authenticated Resume checkpoint checks the exact raw proposal.
            if crate::simulator::h_v1(
                "oasis7.runtime.manifest.v1",
                &self.runtime.runtime_manifest_hash,
            )
            .to_string()
                != cognition
                    .request
                    .request_context
                    .runtime_binding
                    .runtime_manifest_hash
                    .to_string()
            {
                return Err("resumed Wait raw manifest binding mismatch".into());
            }
            runtime.runtime_manifest_hash = self.runtime.runtime_manifest_hash.clone();
        }
        if serde_json::to_value(&proposal).map_err(|error| error.to_string())?
            != serde_json::to_value(&self.proposal).map_err(|error| error.to_string())?
            || serde_json::to_value(&runtime).map_err(|error| error.to_string())?
                != serde_json::to_value(&self.runtime).map_err(|error| error.to_string())?
            || current != self.current
        {
            return Err("restored Wait original proposal or current context mismatch".into());
        }
        Ok(())
    }
    fn original_inputs(&self) -> Result<serde_json::Value, String> {
        serde_json::to_value((&self.decision, &self.proposal, &self.runtime, &self.current))
            .map_err(|error| error.to_string())
    }
}
impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn restore_queued_wait_originals(
        &mut self,
    ) -> Result<(), String> {
        self.hosted_restored_wait_decisions.clear();
        for decision in &self.provider_completed_decisions {
            if matches!(
                decision.decision,
                AgentDecision::Wait | AgentDecision::WaitTicks(_)
            ) {
                HostedWait::validate_decision(decision)?;
                let request = &decision
                    .cognition
                    .as_ref()
                    .ok_or("restored queued Wait cognition missing")?
                    .request
                    .request_context;
                let key = request.provider_invocation_key().to_string();
                if self
                    .hosted_restored_wait_decisions
                    .insert(key, decision.clone())
                    .is_some()
                {
                    return Err("restored queued Wait original identity conflict".into());
                }
            }
        }
        Ok(())
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    fn test_wait_capture_root(&self) -> Result<Option<std::path::PathBuf>, String> {
        let Some(root) = std::env::var_os("PRE2_WAIT_CAPTURE_FS_ROOT") else {
            return Ok(None);
        };
        let root = std::path::PathBuf::from(root);
        if !root.is_absolute() || !root.is_dir() {
            return Err("test Wait capture private directory required".into());
        }
        if root.join("wait-capture-before-persist").exists() {
            return Ok(None);
        }
        Ok(Some(root))
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    fn test_wait_capture_state(&self, agent: &str) -> Result<serde_json::Value, String> {
        let mut state = self.test_wait_cleanup_state()?;
        #[cfg(not(target_arch = "wasm32"))]
        let proposal = self
            .runner
            .as_ref()
            .and_then(RuntimeDecisionRunner::async_runner)
            .and_then(|runner| runner.active_continuation_proposal_id(agent))
            .map(str::to_owned);
        #[cfg(target_arch = "wasm32")]
        let proposal: Option<String> = None;
        state
            .as_object_mut()
            .ok_or("test Wait capture snapshot invalid")?
            .insert(
                "active_harness_proposal".into(),
                serde_json::to_value(proposal).map_err(|_| "test Wait capture snapshot failed")?,
            );
        Ok(state)
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    fn test_wait_cleanup_state(&self) -> Result<serde_json::Value, String> {
        serde_json::to_value(serde_json::json!({
            "proposals": self.provider_continuation_proposals,
            "pending_wakes": self.pending_runtime_wakes,
            "wait": self.hosted_wait,
            "resume": self.hosted_resume,
            "restored_resume": self.hosted_restored_resume,
            "admission": self.hosted_admission,
            "restored_original": self.hosted_restored_wait,
            "restored_queued_waits": self.hosted_restored_wait_decisions,
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
        .map_err(|_| "test Wait cleanup snapshot failed".into())
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    fn test_wait_cleanup_root(&self) -> Result<Option<std::path::PathBuf>, String> {
        let Some(root) = std::env::var_os("PRE2_WAIT_CLEANUP_FS_ROOT") else {
            return Ok(None);
        };
        let root = std::path::PathBuf::from(root);
        if !root.is_absolute() || !root.is_dir() {
            return Err("test Wait cleanup private directory required".into());
        }
        if root.join("wait-cleanup-before-persist").exists() {
            return Ok(None);
        }
        Ok(Some(root))
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    fn test_write_wait_cleanup_file(
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
                .map_err(|_| "test Wait cleanup private directory failed")?;
            options.mode(0o600);
        }
        options
            .open(root.join(name))
            .and_then(|mut file| file.write_all(bytes))
            .map_err(|_| "test Wait cleanup artifact write failed".into())
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    fn test_wait_wait_cleanup_file(root: &std::path::Path, name: &str) -> Result<(), String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !root.join(name).exists() {
            if std::time::Instant::now() >= deadline {
                return Err("test Wait cleanup barrier deadline".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn restore_hosted_wait_original(
        &mut self,
    ) -> Result<(), String> {
        self.hosted_restored_wait = None;
        if let Some(wait) = self.hosted_wait.clone()
            && wait
                .decision
                .cognition
                .as_ref()
                .is_some_and(|cognition| cognition.request.turn_context.continuation.is_some())
        {
            let pending = self.validate_resumed_wait_checkpoint(&wait)?;
            self.hosted_restored_wait = Some((wait.clone(), Some(pending)));
            if let Some(current) = self.hosted_wait.as_mut() {
                current.stage = "resumed_view".into();
                current.commit = None;
            }
            return Ok(());
        }
        if let Some(wait) = self.hosted_wait.as_ref() {
            wait.validate_original()?;
            let request = &wait
                .decision
                .cognition
                .as_ref()
                .ok_or("restored Wait cognition missing")?
                .request
                .request_context;
            let pending = self
                .provider_scheduler_pending
                .get(&format!("{}:admit_wait", request.provider_invocation_key()))
                .cloned();
            if pending.is_none() && wait.stage != "admit" {
                return Err("restored issued Wait signed checkpoint missing".into());
            }
            if let Some(pending) = &pending {
                let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
                    &pending.payload
                else {
                    return Err("restored Wait signed operation missing".into());
                };
                crate::world_service::verify_read_request("scheduler", signed)?;
                if signed.request.operation
                    != SchedulerOperationV1::AdmitContinuation(wait.runtime.clone())
                    || crate::world_service::derive_correlation(
                        pending.correlation.key.world.clone(),
                        &pending.payload,
                    )? != pending.correlation
                {
                    return Err("restored Wait signed original mismatch".into());
                }
            }
            wait.validate_rejection(pending.as_ref())?;
            self.hosted_restored_wait = Some((wait.clone(), pending));
        }
        if let Some(wait) = self.hosted_wait.as_mut() {
            wait.stage = "admit".into();
            wait.commit = None;
        }
        Ok(())
    }
    fn validate_restored_hosted_wait(&mut self, wait: &HostedWait) -> Result<(), String> {
        let Some((original, signed_original)) = self.hosted_restored_wait.clone() else {
            return Ok(());
        };
        wait.validate_original()?;
        if wait
            .decision
            .cognition
            .as_ref()
            .is_some_and(|cognition| cognition.request.turn_context.continuation.is_some())
        {
            let actual = self.validate_resumed_wait_checkpoint(wait)?;
            if original.original_inputs()? != wait.original_inputs()?
                || signed_original.as_ref().is_none_or(|pending| {
                    serde_json::to_value(pending).ok() != serde_json::to_value(&actual).ok()
                })
            {
                return Err("restored resumed Wait original changed; fenced".into());
            }
            return Ok(());
        }
        if original.original_inputs()? != wait.original_inputs()? {
            return Err("restored Wait original changed; fenced".into());
        }
        if let Some(original) = signed_original {
            let request = &wait
                .decision
                .cognition
                .as_ref()
                .ok_or("restored Wait cognition missing")?
                .request
                .request_context;
            let actual = self
                .provider_scheduler_pending
                .get(&format!("{}:admit_wait", request.provider_invocation_key()))
                .ok_or("restored Wait signed original missing")?;
            if serde_json::to_value(actual).map_err(|error| error.to_string())?
                != serde_json::to_value(&original).map_err(|error| error.to_string())?
            {
                return Err("restored Wait signed checkpoint changed; fenced".into());
            }
            let (_, existed) = self.prepare_service_scheduler_checkpoint(
                request,
                "admit_wait",
                SchedulerOperationV1::AdmitContinuation(wait.runtime.clone()),
                None,
            )?;
            if !existed {
                return Err("restored Wait original checkpoint missing".into());
            }
        }
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn capture_hosted_wait(
        &mut self,
        decision: async_support::RuntimeLlmDecision,
    ) -> Result<(), String> {
        let cognition = decision
            .cognition
            .as_ref()
            .ok_or("hosted Wait original cognition missing")?;
        let request = &cognition.request.request_context;
        request
            .validate_production_lane()
            .map_err(|error| error.to_string())?;
        cognition
            .response
            .validate_response_artifact_identity(&cognition.response.response_artifact_identity())
            .map_err(|error| error.to_string())?;
        let observation = self
            .shadow_kernel
            .as_mut()
            .ok_or("hosted Wait observation missing")?
            .observe(&request.agent_subject)
            .map_err(|error| format!("{error:?}"))?;
        let (proposal, mut runtime, current) =
            super::wait_proposal::build_hosted_wait_proposal(cognition, observation)?;
        let queued_original = self
            .hosted_restored_wait_decisions
            .get(&request.provider_invocation_key().to_string())
            .cloned();
        if let Some(original) = &queued_original {
            HostedWait::validate_decision(original)?;
            if serde_json::to_value(original).map_err(|error| error.to_string())?
                != serde_json::to_value(&decision).map_err(|error| error.to_string())?
            {
                return Err("restored queued Wait decision changed; fenced".into());
            }
        }
        #[cfg(any(test, feature = "test_tier_required"))]
        let test_capture = self
            .test_wait_capture_root()?
            .map(|root| {
                self.test_wait_capture_state(&request.agent_subject)
                    .map(|before| (root, before))
            })
            .transpose()?;
        #[cfg(any(test, feature = "test_tier_required"))]
        let test_agent = request.agent_subject.clone();
        let runner = self
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .ok_or("hosted Wait runner missing")?;
        let resumed = cognition.request.turn_context.continuation.is_some();
        if !resumed {
            runner
                .submit_continuation_proposal_with_current_context(
                    &request.agent_subject,
                    proposal.clone(),
                    &current,
                )
                .map_err(|error| format!("hosted Wait Harness admission failed: {error}"))?;
        }
        let existing = if resumed {
            self.provider_service_projection
                .as_ref()
                .and_then(|view| {
                    view.continuations.iter().find(|entry| {
                        entry.continuation_proposal_id == proposal.continuation_proposal_id
                    })
                })
                .cloned()
        } else {
            None
        };
        if resumed && existing.is_none() {
            return Err("resumed Wait verified successor missing".into());
        }
        if let Some(successor) = &existing {
            runtime.runtime_manifest_hash = successor.runtime_manifest_hash.clone();
        }
        self.provider_continuation_proposals
            .insert(proposal.continuation_proposal_id.clone(), proposal.clone());
        self.provider_held_decisions
            .insert(decision.agent_id.clone(), decision.clone());
        self.hosted_wait = Some(HostedWait {
            decision,
            proposal,
            runtime,
            current,
            stage: if resumed { "resumed_view" } else { "admit" }.into(),
            commit: None,
            admitted: existing,
            rejection: None,
        });
        if queued_original.is_some() {
            let original = self
                .hosted_wait
                .as_ref()
                .ok_or("restored queued Wait capture missing")?;
            original.validate_original()?;
            self.hosted_restored_wait = Some((original.clone(), None));
        }
        #[cfg(any(test, feature = "test_tier_required"))]
        let mut test_persist_attempted = false;
        let persisted = (|| {
            #[cfg(any(test, feature = "test_tier_required"))]
            if let Some((root, before)) = &test_capture {
                let staged = self.test_wait_capture_state(&test_agent)?;
                Self::test_write_wait_cleanup_file(
                    root,
                    "wait-capture-before.json",
                    &serde_json::to_vec(before).map_err(|_| "test Wait capture snapshot failed")?,
                )?;
                Self::test_write_wait_cleanup_file(
                    root,
                    "wait-capture-staged.json",
                    &serde_json::to_vec(&staged)
                        .map_err(|_| "test Wait capture snapshot failed")?,
                )?;
                Self::test_write_wait_cleanup_file(root, "wait-capture-before-persist", b"ready")?;
                Self::test_wait_wait_cleanup_file(root, "wait-capture-release")?;
                test_persist_attempted = true;
            }
            self.persist_provider_lineage()
        })();
        #[cfg(any(test, feature = "test_tier_required"))]
        if persisted.is_err()
            && test_persist_attempted
            && let Some((root, _)) = &test_capture
        {
            let recorded = self.test_wait_capture_state(&test_agent).and_then(|after| {
                Self::test_write_wait_cleanup_file(
                    root,
                    "wait-capture-after.json",
                    &serde_json::to_vec(&after).map_err(|_| "test Wait capture snapshot failed")?,
                )?;
                Self::test_write_wait_cleanup_file(root, "wait-capture-failed", b"persist failed")
            });
            if recorded.is_ok() {
                eprintln!("hosted_wait_capture_persistence_failed");
                let _ = Self::test_wait_wait_cleanup_file(root, "wait-capture-crash-release");
            }
        }
        persisted
    }
    fn complete_hosted_wait(&mut self) -> Result<(), String> {
        let wait = self.hosted_wait.clone().ok_or("hosted Wait missing")?;
        let cognition = wait
            .decision
            .cognition
            .as_ref()
            .ok_or("hosted Wait cognition missing")?;
        let request = &cognition.request.request_context;
        let admitted = wait
            .admitted
            .as_ref()
            .ok_or("hosted Wait committed continuation missing")?;
        let view = self
            .provider_service_projection
            .as_ref()
            .ok_or("hosted Wait canonical view missing")?;
        let actual = view
            .continuations
            .iter()
            .find(|entry| entry.continuation_id == admitted.continuation_id)
            .ok_or("hosted Wait canonical continuation missing")?;
        if actual.agent_id != request.agent_subject
            || actual.agent_session_id != request.agent_session_id
            || actual.agent_turn_id != request.agent_turn_id
            || actual.decision_request_id != request.decision_request_id
            || actual.origin_request_digest != wait.proposal.origin_request_digest
            || actual.continuation_proposal_id != wait.proposal.continuation_proposal_id
        {
            return Err("hosted Wait committed identity mismatch".into());
        }
        let authority = view
            .continuation_contexts
            .get(&actual.continuation_id)
            .ok_or("hosted Wait verified authority missing")?;
        let authority = crate::simulator::ContinuationAuthorityContextV1 {
            baseline_observation_digest: authority.baseline_observation_digest.clone(),
            goal_digest: authority.goal_digest.clone(),
            policy_digest: authority.policy_digest.clone(),
            precondition_digest: authority.precondition_digest.clone(),
        };
        let runner = self
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .ok_or("hosted Wait runner missing")?;
        runner
            .validate_active_continuation_with_authority(&request.agent_subject, &authority, actual)
            .map_err(|error| format!("hosted Wait Harness validation failed: {error}"))?;
        let leases_backup = self.provider_cognition_leases.clone();
        let queue_backup = self.provider_completed_decisions.clone();
        let active_backup = self.provider_active_turns.clone();
        let contexts_backup = self.provider_contexts.clone();
        let held_backup = self.provider_held_decisions.clone();
        let restored_backup = self.hosted_restored_wait.clone();
        let queued_backup = self.hosted_restored_wait_decisions.clone();
        #[cfg(any(test, feature = "test_tier_required"))]
        let test_cleanup = self
            .test_wait_cleanup_root()?
            .map(|root| self.test_wait_cleanup_state().map(|before| (root, before)))
            .transpose()?;
        #[cfg(any(test, feature = "test_tier_required"))]
        let mut test_persist_attempted = false;
        let cleanup = (|| {
            if self
                .provider_completed_decisions
                .front()
                .is_some_and(|front| {
                    front
                        .cognition
                        .as_ref()
                        .is_none_or(|context| &context.request.request_context != request)
                })
            {
                return Err("hosted Wait completed queue identity mismatch".into());
            }
            self.clear_settled_service_lease(
                request,
                cognition
                    .cognition_lease
                    .as_ref()
                    .ok_or("hosted Wait lease missing")?,
            )?;
            self.provider_completed_decisions.pop_front();
            self.provider_active_turns.remove(&request.agent_subject);
            self.provider_contexts.remove(&request.agent_subject);
            self.provider_held_decisions.remove(&request.agent_subject);
            self.hosted_wait = None;
            self.hosted_restored_wait = None;
            self.hosted_restored_wait_decisions
                .remove(&request.provider_invocation_key().to_string());
            #[cfg(any(test, feature = "test_tier_required"))]
            if let Some((root, before)) = &test_cleanup {
                let staged = self.test_wait_cleanup_state()?;
                Self::test_write_wait_cleanup_file(
                    root,
                    "wait-cleanup-before.json",
                    &serde_json::to_vec(before).map_err(|_| "test Wait cleanup snapshot failed")?,
                )?;
                Self::test_write_wait_cleanup_file(
                    root,
                    "wait-cleanup-staged.json",
                    &serde_json::to_vec(&staged)
                        .map_err(|_| "test Wait cleanup snapshot failed")?,
                )?;
                Self::test_write_wait_cleanup_file(root, "wait-cleanup-before-persist", b"ready")?;
                Self::test_wait_wait_cleanup_file(root, "wait-cleanup-release")?;
                test_persist_attempted = true;
            }
            self.persist_provider_lineage()
        })();
        if let Err(error) = cleanup {
            self.provider_cognition_leases = leases_backup;
            self.provider_completed_decisions = queue_backup;
            self.provider_active_turns = active_backup;
            self.provider_contexts = contexts_backup;
            self.provider_held_decisions = held_backup;
            self.hosted_wait = Some(wait);
            self.hosted_restored_wait = restored_backup;
            self.hosted_restored_wait_decisions = queued_backup;
            #[cfg(any(test, feature = "test_tier_required"))]
            if test_persist_attempted && let Some((root, _)) = &test_cleanup {
                let recorded = self.test_wait_cleanup_state().and_then(|after| {
                    Self::test_write_wait_cleanup_file(
                        root,
                        "wait-cleanup-after.json",
                        &serde_json::to_vec(&after)
                            .map_err(|_| "test Wait cleanup snapshot failed")?,
                    )?;
                    Self::test_write_wait_cleanup_file(
                        root,
                        "wait-cleanup-rollback",
                        b"rolled back",
                    )
                });
                if recorded.is_ok() {
                    eprintln!("hosted_wait_cleanup_persistence_failed");
                    let _ = Self::test_wait_wait_cleanup_file(root, "wait-cleanup-retry-release");
                }
            }
            return Err(error);
        };
        Ok(())
    }
}
impl crate::viewer::ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn prepare_hosted_wait_io(
        &mut self,
    ) -> Result<AgentServiceProgress, String> {
        if self.llm_sidecar.hosted_service_inflight.is_some() {
            return Ok(AgentServiceProgress::Idle);
        }
        let wait = self
            .llm_sidecar
            .hosted_wait
            .clone()
            .ok_or("hosted Wait phase missing")?;
        self.llm_sidecar.validate_restored_hosted_wait(&wait)?;
        if wait
            .decision
            .cognition
            .as_ref()
            .is_some_and(|cognition| cognition.request.turn_context.continuation.is_some())
        {
            self.llm_sidecar.validate_resumed_wait_checkpoint(&wait)?;
        }
        if wait.stage == "rejected_finalize" {
            self.llm_sidecar.complete_rejected_hosted_wait()?;
            self.llm_sidecar.hosted_service_config_binding = None;
            self.defer_next_auto_play_step_after_completion(self.config.play_step_interval);
            return Ok(AgentServiceProgress::Advanced);
        }
        if wait.stage == "finalize" {
            self.llm_sidecar.complete_hosted_wait()?;
            self.llm_sidecar.hosted_service_config_binding = None;
            self.defer_next_auto_play_step_after_completion(self.config.play_step_interval);
            return Ok(AgentServiceProgress::Advanced);
        }
        self.llm_sidecar.persist_provider_lineage()?;
        let cognition = wait
            .decision
            .cognition
            .as_ref()
            .ok_or("hosted Wait cognition missing")?;
        let request = &cognition.request.request_context;
        let client = self
            .world_service_client()
            .map_err(|error| format!("{error:?}"))?
            .ok_or("canonical client missing")?;
        let operation = if let Some(operation) =
            self.prepare_rejected_wait_operation(&wait, &client)?
        {
            operation
        } else if wait.stage.ends_with("_view") {
            AgentServiceIoOperation::View(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: client.config().expected_world.clone(),
                scope_id: client.config().scope_id.clone(),
                min_commit: wait.commit,
                fixed_commit: None,
                deadline_unix_ms: None,
            })
        } else {
            let (phase, operation) = if wait.stage == "admit" {
                (
                    "admit_wait",
                    SchedulerOperationV1::AdmitContinuation(wait.runtime),
                )
            } else if wait.stage == "settle" {
                let lease = cognition
                    .cognition_lease
                    .as_ref()
                    .ok_or("hosted Wait lease missing")?;
                (
                    "settle",
                    SchedulerOperationV1::SettleLease {
                        lease_id: lease.lease_id.clone(),
                        consumed_amount: lease.reserved_amount,
                    },
                )
            } else {
                return Err("hosted Wait stage invalid".into());
            };
            let (checkpoint, existed) = self
                .llm_sidecar
                .prepare_service_scheduler_checkpoint(request, phase, operation, None)?;
            if phase == "admit_wait"
                && let Some((_, signed_original)) = self.llm_sidecar.hosted_restored_wait.as_mut()
            {
                if let Some(original) = signed_original.as_ref() {
                    if serde_json::to_value(original).map_err(|error| error.to_string())?
                        != serde_json::to_value(&checkpoint).map_err(|error| error.to_string())?
                    {
                        return Err("restored queued Wait signed Admit changed; fenced".into());
                    }
                } else {
                    // The checkpoint was just frozen and persisted by the
                    // ordinary signing seam. Pin it before any transport job.
                    *signed_original = Some(checkpoint.clone());
                }
            }
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
            phase_digest: format!("wait:{}:{}", wait.stage, request.provider_invocation_key()),
        };
        self.llm_sidecar.hosted_service_inflight = Some(token.clone());
        Ok(AgentServiceProgress::NeedsIo(Box::new(AgentServiceIoJob {
            token,
            client: Some(client),
            operation,
        })))
    }
    pub(in crate::viewer::runtime_live) fn apply_hosted_wait_io(
        &mut self,
        result: AgentServiceIoResult,
    ) -> Result<AgentServiceProgress, String> {
        if self.llm_sidecar.hosted_service_inflight.as_ref() != Some(&result.token) {
            return Err("hosted Wait completion token mismatch".into());
        };
        self.llm_sidecar.hosted_service_inflight = None;
        if result.token.config_digest != self.hosted_service_config_digest()? {
            return Err("hosted Wait configuration changed; fenced".into());
        };
        let mut wait = self
            .llm_sidecar
            .hosted_wait
            .clone()
            .ok_or("hosted Wait original missing")?;
        let cognition = wait
            .decision
            .cognition
            .as_ref()
            .ok_or("hosted Wait cognition missing")?;
        let request = &cognition.request.request_context;
        if result.token.phase_digest
            != format!("wait:{}:{}", wait.stage, request.provider_invocation_key())
        {
            return Err("hosted Wait phase mismatch".into());
        };
        let response = result.response?;
        if wait.stage.starts_with("rejection_") || wait.stage.starts_with("compensate_") {
            self.apply_rejected_wait_response(&mut wait, response)?;
            self.llm_sidecar.hosted_wait = Some(wait);
            self.llm_sidecar.persist_provider_lineage()?;
            return Ok(AgentServiceProgress::Advanced);
        }
        if wait.stage.ends_with("_view") {
            let AgentServiceIoResponse::View(view) = response else {
                return Err("hosted Wait minimum view missing".into());
            };
            if wait.stage != "resumed_view"
                && !view
                    .version()
                    .commit
                    .satisfies_minimum(wait.commit.as_ref().ok_or("hosted Wait commit missing")?)
                    .map_err(|error| error.to_string())?
            {
                return Err("hosted Wait view precedes commit".into());
            };
            self.apply_hosted_verified_view(*view)?;
            if wait.stage == "admit_view" || wait.stage == "resumed_view" {
                let admitted = wait.admitted.clone().ok_or("hosted Wait receipt missing")?;
                admitted
                    .validate_authoritative()
                    .map_err(|error| error.to_string())?;
                let projection = self
                    .llm_sidecar
                    .provider_service_projection
                    .as_ref()
                    .ok_or("hosted Wait verified projection missing")?;
                let actual = projection
                    .continuations
                    .iter()
                    .find(|entry| entry.continuation_id == admitted.continuation_id)
                    .ok_or("hosted Wait admitted projection missing")?;
                if wait.stage != "resumed_view"
                    && serde_json::to_value(actual).map_err(|error| error.to_string())?
                        != serde_json::to_value(&admitted).map_err(|error| error.to_string())?
                {
                    return Err("hosted Wait admitted receipt projection mismatch".into());
                }
                let authority = projection
                    .continuation_contexts
                    .get(&actual.continuation_id)
                    .ok_or("hosted Wait verified admitted authority missing")?;
                let authority = crate::simulator::ContinuationAuthorityContextV1 {
                    baseline_observation_digest: authority.baseline_observation_digest.clone(),
                    goal_digest: authority.goal_digest.clone(),
                    policy_digest: authority.policy_digest.clone(),
                    precondition_digest: authority.precondition_digest.clone(),
                };
                authority
                    .validate_proposal(&wait.proposal)
                    .map_err(|error| error.to_string())?;
                let restored = self.llm_sidecar.hosted_restored_wait.is_some();
                let runner = self
                    .llm_sidecar
                    .runner
                    .as_mut()
                    .and_then(RuntimeDecisionRunner::async_runner_mut)
                    .ok_or("hosted Wait runner missing")?;
                if restored {
                    if runner.active_continuation_proposal_id(&request.agent_subject)
                        == Some(wait.proposal.continuation_proposal_id.as_str())
                    {
                        runner
                            .validate_active_continuation_with_authority(
                                &request.agent_subject,
                                &authority,
                                actual,
                            )
                            .map_err(|error| error.to_string())?;
                    } else {
                        runner
                            .hydrate_runtime_continuation_with_authority(
                                &request.agent_subject,
                                wait.proposal.clone(),
                                &authority,
                                actual.clone(),
                            )
                            .map_err(|error| error.to_string())?;
                    }
                } else {
                    if wait.stage == "resumed_view" {
                        runner
                            .apply_runtime_continuation_projection_with_current_context(
                                &request.agent_subject,
                                actual.clone(),
                                &wait.current,
                            )
                            .map_err(|error| error.to_string())?;
                    } else {
                        runner
                            .apply_runtime_continuation_projection_with_current_context(
                                &request.agent_subject,
                                admitted,
                                &wait.current,
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
                                        eprintln!("hosted_wait_missing_harness_continuation");
                                    }
                                }
                                error.to_string()
                            })?;
                    }
                    let prior_wait = self.llm_sidecar.hosted_wait.clone();
                    wait.stage = "settle".into();
                    self.llm_sidecar.hosted_wait = Some(wait.clone());
                    if let Err(error) = self.llm_sidecar.with_runtime_turn_release_for_continuation(
                        &request.agent_subject,
                        &request.agent_session_id,
                        &request.agent_turn_id,
                        &request.decision_request_id,
                        request.request_digest.as_str(),
                        |sidecar| sidecar.persist_provider_lineage(),
                    ) {
                        self.llm_sidecar.hosted_wait = prior_wait;
                        #[cfg(any(test, feature = "test_tier_required"))]
                        if error.contains("unknown pending Runtime turn") {
                            static REPORTED: std::sync::atomic::AtomicBool =
                                std::sync::atomic::AtomicBool::new(false);
                            if !REPORTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                                eprintln!("hosted_wait_missing_awaiting_runtime_turn");
                            }
                        }
                        return Err(error);
                    }
                }
                wait.stage = "settle".into();
            } else {
                wait.stage = "finalize".into()
            };
            wait.commit = None;
        } else {
            let Some(response) =
                crate::viewer::runtime_live::agent_service_phase::original_response(response)?
            else {
                return Ok(AgentServiceProgress::Advanced);
            };
            let phase = if wait.stage == "admit" {
                "admit_wait"
            } else {
                "settle"
            };
            let checkpoint = self
                .llm_sidecar
                .provider_scheduler_pending
                .get(&format!("{}:{phase}", request.provider_invocation_key()))
                .ok_or("hosted Wait original checkpoint missing")?;
            response
                .validate(&checkpoint.correlation)
                .map_err(|error| error.to_string())?;
            let original_response = response.clone();
            match response.outcome {
                IntentOutcome::Committed { commit, receipt } => {
                    if wait.stage == "admit" && wait.rejection.is_some() {
                        return Err("Wait original rejection became committed; fenced".into());
                    }
                    if wait.stage == "admit" {
                        let admitted: crate::runtime::AgentContinuation =
                            serde_json::from_value(receipt).map_err(|error| error.to_string())?;
                        if admitted.continuation_proposal_id
                            != wait.proposal.continuation_proposal_id
                            || admitted.origin_request_digest != request.request_digest.to_string()
                            || admitted.agent_id != request.agent_subject
                            || admitted.agent_turn_id != request.agent_turn_id
                            || admitted.agent_session_id != request.agent_session_id
                            || admitted.decision_request_id != request.decision_request_id
                        {
                            return Err("hosted Wait admission receipt mismatch".into());
                        };
                        wait.admitted = Some(admitted);
                    } else {
                        RuntimeLlmSidecar::validate_hosted_settlement_receipt(
                            request,
                            cognition
                                .cognition_lease
                                .as_ref()
                                .ok_or("hosted Wait lease missing")?,
                            receipt,
                        )?
                    };
                    wait.stage = format!("{}_view", wait.stage);
                    wait.commit = Some(*commit);
                }
                IntentOutcome::Received { .. }
                | IntentOutcome::Pending
                | IntentOutcome::Unknown => return Ok(AgentServiceProgress::Advanced),
                IntentOutcome::Rejected { .. } if wait.stage == "admit" => {
                    wait.accept_rejection(original_response, checkpoint)?;
                    wait.stage = "rejection_view".into();
                    wait.commit = None;
                }
                IntentOutcome::Rejected { reason } => {
                    return Err(format!("canonical Wait settlement rejected: {reason:?}"));
                }
                _ => return Err("canonical Wait unsupported outcome".into()),
            }
        }
        self.llm_sidecar.hosted_wait = Some(wait);
        self.llm_sidecar.persist_provider_lineage()?;
        Ok(AgentServiceProgress::Advanced)
    }
}
