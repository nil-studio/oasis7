use super::*;
use oasis7::runtime_host::runner::RuntimeRunnerHostExt;

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn builtin_cognition_settings(
    agent_id: &str,
) -> Result<ProviderDecisionSettings, String> {
    let config = LlmAgentConfig::from_default_sources_for_agent(agent_id)
        .map_err(|error| format!("builtin cognition config failed for {agent_id}: {error}"))?;
    Ok(ProviderDecisionSettings {
        requested_provider_mode: "builtin_llm".to_string(),
        provider_transport: "builtin_openai".to_string(),
        base_url: config.base_url,
        auth_token: None,
        connect_timeout_ms: config.timeout_ms.max(1),
        decision_timeout_ms: config.timeout_ms.max(1),
        agent_profile: config.prompt_profile.as_str().to_string(),
        execution_mode: ProviderExecutionMode::HeadlessAgent,
        fallback_reason: None,
    })
}

pub(super) fn provider_actor_exists(sidecar: &RuntimeLlmSidecar, agent_id: &str) -> bool {
    match sidecar.runner.as_ref() {
        #[cfg(not(target_arch = "wasm32"))]
        Some(RuntimeDecisionRunner::Builtin(runner))
        | Some(RuntimeDecisionRunner::ProviderBacked(runner)) => runner.has_agent(agent_id),
        #[cfg(target_arch = "wasm32")]
        Some(RuntimeDecisionRunner::ProviderBacked(runner)) => runner.get(agent_id).is_some(),
        _ => false,
    }
}

impl RuntimeLlmSidecar {
    /// Keep the detached actor runner unavailable to dispatch while its exact
    /// completed Runtime turn is provisionally released around a host write.
    /// Persistence failure restores the Harness turn before the runner is
    /// made visible again.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn with_runtime_turn_release_for_continuation<F>(
        &mut self,
        agent_id: &str,
        agent_session_id: &str,
        agent_turn_id: &str,
        decision_request_id: &str,
        request_digest: &str,
        persist: F,
    ) -> Result<(), String>
    where
        F: FnOnce(&mut Self) -> Result<(), String>,
    {
        let Some(mut runtime_runner) = self.runner.take() else {
            return Err("continuation release runner is unavailable".into());
        };
        let result = match &mut runtime_runner {
            RuntimeDecisionRunner::Builtin(runner)
            | RuntimeDecisionRunner::ProviderBacked(runner) => runner
                .host_release_turn_for_continuation(
                    agent_id,
                    agent_session_id,
                    agent_turn_id,
                    decision_request_id,
                    request_digest,
                    |_| persist(self),
                )
                .map_err(|error| error.to_string()),
        };
        self.runner = Some(runtime_runner);
        result
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn fence_runtime_continuation(
        &mut self,
        agent_id: &str,
        wake_id: &str,
        reason: impl Into<String>,
    ) -> String {
        let reason = reason.into();
        self.provider_continuation_recovery_pending
            .insert(agent_id.to_string(), reason.clone());
        self.persist_provider_lineage_best_effort();
        format!("Runtime continuation hydration fenced {agent_id} at {wake_id}: {reason}")
    }

    /// Restore the exact Harness proposal before any Runtime wake transition.
    /// A fresh process has no local Harness chain ledger; if the durable
    /// proposal is absent or does not correlate to Runtime, fence only this
    /// Agent and leave Runtime untouched.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn ensure_runtime_harness_continuation(
        &mut self,
        agent_id: &str,
        runtime: &crate::runtime::AgentContinuation,
        wake_id: &str,
        world: &RuntimeWorld,
    ) -> Result<(), String> {
        if let Some(reason) = self
            .provider_continuation_recovery_pending
            .get(agent_id)
            .cloned()
        {
            return Err(self.fence_runtime_continuation(agent_id, wake_id, reason));
        }
        let Some(proposal) = self
            .provider_continuation_proposals
            .get(runtime.continuation_proposal_id.as_str())
            .cloned()
        else {
            return Err(self.fence_runtime_continuation(
                agent_id,
                wake_id,
                format!(
                    "authoritative Harness proposal is missing for {}",
                    runtime.continuation_proposal_id
                ),
            ));
        };
        if proposal.agent_id != agent_id || runtime.agent_id != agent_id {
            return Err(self.fence_runtime_continuation(
                agent_id,
                wake_id,
                "Runtime and Harness continuation Agent identities differ",
            ));
        }
        if let Err(error) = proposal.validate() {
            return Err(self.fence_runtime_continuation(
                agent_id,
                wake_id,
                format!("persisted Harness continuation proposal is invalid: {error}"),
            ));
        }
        let runtime_context =
            match self.authority_continuation_context(world, runtime.continuation_id.as_str()) {
                Ok(context) => context,
                Err(error) => {
                    return Err(self.fence_runtime_continuation(agent_id, wake_id, error));
                }
            };
        let authority = crate::simulator::ContinuationAuthorityContextV1 {
            baseline_observation_digest: runtime_context.baseline_observation_digest,
            goal_digest: runtime_context.goal_digest,
            policy_digest: runtime_context.policy_digest,
            precondition_digest: runtime_context.precondition_digest,
        };
        let active_proposal_id = self
            .runner
            .as_ref()
            .and_then(|runner| match runner {
                RuntimeDecisionRunner::ProviderBacked(runner)
                | RuntimeDecisionRunner::Builtin(runner) => {
                    runner.active_continuation_proposal_id(agent_id)
                }
                #[cfg(target_arch = "wasm32")]
                _ => None,
            })
            .map(ToString::to_string);
        if let Some(active_proposal_id) = active_proposal_id {
            if active_proposal_id == proposal.continuation_proposal_id {
                let Some(validation) = self.runner.as_ref().map(|runner| match runner {
                    RuntimeDecisionRunner::ProviderBacked(runner)
                    | RuntimeDecisionRunner::Builtin(runner) => runner
                        .validate_active_continuation_with_authority(agent_id, &authority, runtime),
                }) else {
                    return Err(self.fence_runtime_continuation(
                        agent_id,
                        wake_id,
                        "Runtime continuation Harness runner is unavailable",
                    ));
                };
                return validation.map_err(|error| {
                    self.fence_runtime_continuation(agent_id, wake_id, error.to_string())
                });
            }
            return Err(self.fence_runtime_continuation(
                agent_id,
                wake_id,
                format!(
                    "active Harness proposal {} differs from Runtime {}",
                    active_proposal_id, proposal.continuation_proposal_id
                ),
            ));
        }
        let Some(result) = self.runner.as_mut().map(|runner| match runner {
            RuntimeDecisionRunner::ProviderBacked(runner)
            | RuntimeDecisionRunner::Builtin(runner) => runner
                .hydrate_runtime_continuation_with_authority(
                    agent_id,
                    proposal,
                    &authority,
                    runtime.clone(),
                ),
        }) else {
            return Err(self.fence_runtime_continuation(
                agent_id,
                wake_id,
                "Runtime continuation Harness runner is unavailable",
            ));
        };
        match result {
            Ok(_) => {
                self.provider_continuation_recovery_pending.remove(agent_id);
                Ok(())
            }
            Err(error) => {
                Err(self.fence_runtime_continuation(agent_id, wake_id, error.to_string()))
            }
        }
    }
}
