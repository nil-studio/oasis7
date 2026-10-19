use super::*;

impl RuntimeLlmSidecar {
    /// Lazily admit the Hosted local-mock provider context for prompt control
    /// after the authoritative chain world has arrived. A headed client may
    /// submit Apply before auto-play ever asks the decision loop to initialize
    /// its runner; this uses the same Runtime-owned preparation as that path.
    pub(in crate::viewer::runtime_live) fn prepare_hosted_local_mock_prompt_context(
        &mut self,
        world: &mut RuntimeWorld,
        config: &WorldConfig,
        world_id: &str,
    ) -> Result<(), String> {
        if !self.hosted_local_mock_test_lane {
            return Ok(());
        }
        self.sync_shadow_kernel(world, config)?;
        self.ensure_runner_initialized()?;
        let mut kernel = self
            .shadow_kernel
            .take()
            .ok_or_else(|| "Hosted local-mock provider shadow kernel missing".to_string())?;
        self.prepare_provider_request_contexts(world, &mut kernel, world_id)
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(in crate::viewer::runtime_live) fn replace_builtin_runner_for_test(
        &mut self,
        runner: AsyncAgentRunner,
    ) {
        self.runner = Some(RuntimeDecisionRunner::Builtin(runner));
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(in crate::viewer::runtime_live) fn replace_provider_backed_runner_for_test(
        &mut self,
        agent_id: &str,
    ) -> Result<(), String> {
        let behavior = ProviderBackedAgentBehavior::new(
            agent_id.to_string(),
            crate::simulator::MockDecisionProvider::new("hosted-local-mock-runtime-test-provider"),
            vec![ActionCatalogEntry::new("wait", "wait")],
        );
        let mut runner = AsyncAgentRunner::with_default_capacity();
        runner
            .register(behavior)
            .map_err(|error| format!("register ProviderBacked test actor: {error:?}"))?;
        self.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
        self.provider_agent_ids.insert(agent_id.to_string());
        Ok(())
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(in crate::viewer::runtime_live) fn prepare_provider_backed_test_context(
        &mut self,
        world: &mut RuntimeWorld,
        config: &WorldConfig,
        world_id: &str,
        agent_id: &str,
    ) -> Result<(), String> {
        self.replace_provider_backed_runner_for_test(agent_id)?;
        self.provider_contexts.clear();
        self.sync_shadow_kernel(world, config)?;
        let mut kernel = self
            .shadow_kernel
            .take()
            .ok_or_else(|| "Hosted local-mock provider shadow kernel missing".to_string())?;
        self.prepare_provider_request_contexts(world, &mut kernel, world_id)
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(in crate::viewer::runtime_live) fn refresh_provider_backed_test_context(
        &mut self,
        world: &mut RuntimeWorld,
        config: &WorldConfig,
        world_id: &str,
    ) -> Result<(), String> {
        self.sync_shadow_kernel(world, config)?;
        let mut kernel = self
            .shadow_kernel
            .take()
            .ok_or_else(|| "Hosted local-mock provider shadow kernel missing".to_string())?;
        self.prepare_provider_request_contexts(world, &mut kernel, world_id)
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(in crate::viewer::runtime_live) fn provider_test_goal_summary(
        &self,
        agent_id: &str,
    ) -> Option<String> {
        self.provider_contexts.get(agent_id).map(|context| {
            context
                .turn_context
                .goal_snapshot
                .short_term_summary
                .clone()
        })
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(in crate::viewer::runtime_live) fn provider_test_context_binding(
        &self,
        agent_id: &str,
    ) -> Option<RuntimeBindingV1> {
        self.provider_contexts
            .get(agent_id)
            .map(|context| context.request_context.runtime_binding.clone())
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(in crate::viewer::runtime_live) fn set_provider_test_binding(
        &mut self,
        binding: RuntimeBindingV1,
    ) {
        self.provider_lineage_binding = Some(binding);
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(in crate::viewer::runtime_live) fn provider_test_binding(
        &self,
    ) -> Option<RuntimeBindingV1> {
        self.provider_lineage_binding.clone()
    }

    /// Deliver only an owner-authenticated committed goal to the existing
    /// runner. Failed delivery blocks the turn and is retried, never cached as
    /// success; local non-chain prompt profiles remain unchanged.
    pub(in crate::viewer::runtime_live) fn sync_canonical_goal_prompt(
        &mut self,
    ) -> Result<(), String> {
        let current = self
            .provider_service_projection
            .as_ref()
            .and_then(|view| view.canonical_agent_chat.as_ref())
            .and_then(|chat| {
                chat.goal
                    .as_ref()
                    .filter(|goal| matches!(goal.status.as_str(), "accepted" | "blocked"))
                    .map(|goal| (chat.agent_id.clone(), Some(goal.intent_id.clone())))
            });
        if current == self.canonical_goal_prompt_applied {
            return Ok(());
        }
        let agent = current
            .as_ref()
            .or(self.canonical_goal_prompt_applied.as_ref())
            .map(|(agent, _)| agent.clone());
        if let Some(agent) = agent {
            let profile = self
                .prompt_profiles
                .get(&agent)
                .cloned()
                .unwrap_or_else(|| AgentPromptProfile::for_agent(agent));
            self.apply_prompt_profile_to_driver(&profile)?;
        }
        self.canonical_goal_prompt_applied = current;
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn apply_prompt_profile_to_driver(
        &mut self,
        profile: &AgentPromptProfile,
    ) -> Result<(), String> {
        #[cfg(not(target_arch = "wasm32"))]
        let hosted_local_mock_provider_context_ready = self.hosted_local_mock_test_lane
            && self
                .provider_contexts
                .get(profile.agent_id.as_str())
                .is_some_and(|context| {
                    context
                        .turn_context
                        .validate_for_agent(profile.agent_id.as_str())
                        .is_ok()
                        && context.request_context.validate_production_lane().is_ok()
                        && context.request_context.agent_subject == profile.agent_id
                        && self
                            .provider_lineage_binding
                            .as_ref()
                            .is_some_and(|binding| {
                                binding == &context.request_context.runtime_binding
                            })
                });
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(RuntimeDecisionRunner::Builtin(runner)) = self.runner.as_mut() {
            runner
                .set_prompt_overrides(
                    profile.agent_id.as_str(),
                    profile.system_prompt_override.clone(),
                    profile.short_term_goal_override.clone(),
                    profile.long_term_goal_override.clone(),
                )
                .map_err(|error| format!("prompt override enqueue failed: {error}"))?;
            return Ok(());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(RuntimeDecisionRunner::ProviderBacked(runner)) = self.runner.as_mut() {
            // ProviderBacked remains denied in production.  The only native
            // exception is the explicitly admitted Hosted local-mock test
            // lane, and only while the sidecar still holds a validated
            // Runtime-installed outer context for this exact Agent.  The
            // context check prevents the lane marker (or backend name) from
            // becoming a standalone prompt-control grant.
            if !hosted_local_mock_provider_context_ready {
                return Err(
                    "prompt control is unsupported for ProviderBacked runtime without an admitted Hosted local-mock Runtime context"
                        .to_string(),
                );
            }
            runner
                .set_prompt_overrides(
                    profile.agent_id.as_str(),
                    profile.system_prompt_override.clone(),
                    profile.short_term_goal_override.clone(),
                    profile.long_term_goal_override.clone(),
                )
                .map_err(|error| format!("prompt override enqueue failed: {error}"))?;
            return Ok(());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.hosted_local_mock_test_lane
            && env_requests_provider_backend()
            && self.runner.is_none()
        {
            return Err("llm runner is not initialized".to_string());
        }
        #[cfg(target_arch = "wasm32")]
        let Some(RuntimeDecisionRunner::Builtin(runner)) = self.runner.as_mut() else {
            return Err("builtin llm runner is not initialized".to_string());
        };
        #[cfg(target_arch = "wasm32")]
        let Some(agent) = runner.get_mut(profile.agent_id.as_str()) else {
            return Err(format!(
                "agent is not registered in llm runner: {}",
                profile.agent_id
            ));
        };
        #[cfg(target_arch = "wasm32")]
        agent.behavior.apply_prompt_overrides(
            profile.system_prompt_override.clone(),
            profile.short_term_goal_override.clone(),
            profile.long_term_goal_override.clone(),
        );
        Ok(())
    }

    pub(super) fn ensure_runner_initialized(&mut self) -> Result<(), String> {
        let kernel = self
            .shadow_kernel
            .as_ref()
            .ok_or_else(|| "shadow kernel not initialized".to_string())?;
        let provider_settings = provider_settings_from_env()?;
        if self.runner.is_none() {
            self.runner = Some(match provider_settings.as_ref() {
                #[cfg(not(target_arch = "wasm32"))]
                Some(_) => {
                    RuntimeDecisionRunner::ProviderBacked(AsyncAgentRunner::with_default_capacity())
                }
                #[cfg(not(target_arch = "wasm32"))]
                None => RuntimeDecisionRunner::Builtin(AsyncAgentRunner::with_default_capacity()),
                #[cfg(target_arch = "wasm32")]
                Some(_) => RuntimeDecisionRunner::ProviderBacked(AgentRunner::new()),
                #[cfg(target_arch = "wasm32")]
                None => RuntimeDecisionRunner::Builtin(AgentRunner::new()),
            });
        }
        let runner = self
            .runner
            .as_mut()
            .ok_or_else(|| "llm runner not initialized".to_string())?;
        let mut agent_ids: Vec<String> = kernel.model().agents.keys().cloned().collect();
        agent_ids.sort();
        for agent_id in agent_ids {
            match runner {
                #[cfg(not(target_arch = "wasm32"))]
                RuntimeDecisionRunner::Builtin(runner) => {
                    if runner.has_agent(agent_id.as_str()) {
                        self.provider_agent_ids.insert(agent_id);
                        continue;
                    }
                    let mut config = LlmAgentConfig::from_default_sources_for_agent(
                        agent_id.as_str(),
                    )
                    .map_err(|err| format!("llm init failed for {}: {:?}", agent_id, err))?;
                    config.timeout_ms = resolve_runtime_live_llm_timeout_ms(config.timeout_ms);
                    let client = OpenAiChatCompletionClient::from_config(&config)
                        .map_err(|err| format!("llm init failed for {}: {:?}", agent_id, err))?;
                    let mut behavior =
                        LlmAgentBehavior::new(agent_id.clone(), config.clone(), client);
                    let profile = self.prompt_profiles.get(agent_id.as_str());
                    behavior.apply_prompt_overrides(
                        profile.and_then(|profile| profile.system_prompt_override.clone()),
                        profile
                            .and_then(|profile| profile.short_term_goal_override.clone())
                            .or_else(|| Some(runtime_live_phase1_short_term_goal())),
                        profile.and_then(|profile| profile.long_term_goal_override.clone()),
                    );
                    restore_behavior_long_term_memory_from_model(
                        &mut behavior,
                        kernel,
                        agent_id.as_str(),
                    );
                    runner
                        .register(behavior)
                        .map_err(|error| format!("builtin agent init failed: {error}"))?;
                    // The async Builtin runner deliberately enters the same
                    // cognition-context preparation/feedback lifecycle as
                    // ProviderBacked. The set is retained under the historic
                    // provider-prefixed field until that storage is renamed.
                    self.provider_agent_ids.insert(agent_id);
                }
                #[cfg(target_arch = "wasm32")]
                RuntimeDecisionRunner::Builtin(runner) => {
                    if runner.get(agent_id.as_str()).is_some() {
                        continue;
                    }
                    let mut config = LlmAgentConfig::from_default_sources_for_agent(
                        agent_id.as_str(),
                    )
                    .map_err(|err| format!("llm init failed for {}: {:?}", agent_id, err))?;
                    config.timeout_ms = resolve_runtime_live_llm_timeout_ms(config.timeout_ms);
                    let client = OpenAiChatCompletionClient::from_config(&config)
                        .map_err(|err| format!("llm init failed for {}: {:?}", agent_id, err))?;
                    let mut behavior = LlmAgentBehavior::new(agent_id.clone(), config, client);
                    let short_term_goal_override = self
                        .prompt_profiles
                        .get(agent_id.as_str())
                        .and_then(|profile| profile.short_term_goal_override.clone())
                        .or_else(|| Some(runtime_live_phase1_short_term_goal()));
                    let system_prompt_override = self
                        .prompt_profiles
                        .get(agent_id.as_str())
                        .and_then(|profile| profile.system_prompt_override.clone());
                    let long_term_goal_override = self
                        .prompt_profiles
                        .get(agent_id.as_str())
                        .and_then(|profile| profile.long_term_goal_override.clone());
                    behavior.apply_prompt_overrides(
                        system_prompt_override,
                        short_term_goal_override,
                        long_term_goal_override,
                    );
                    restore_behavior_long_term_memory_from_model(
                        &mut behavior,
                        kernel,
                        agent_id.as_str(),
                    );
                    runner.register(behavior);
                }
                RuntimeDecisionRunner::ProviderBacked(runner) => {
                    if runner.has_agent(agent_id.as_str()) {
                        self.provider_agent_ids.insert(agent_id);
                        continue;
                    }
                    let settings = provider_settings.as_ref().ok_or_else(|| {
                        "provider runner selected without resolved settings".to_string()
                    })?;
                    let adapter = ProviderLoopbackAdapter::new_with_transport(
                        settings.base_url.as_str(),
                        settings.auth_token.as_deref(),
                        settings.connect_timeout_ms,
                        settings.provider_transport.as_str(),
                    )
                    .map_err(|err| format!("provider init failed for {}: {}", agent_id, err))?;
                    let behavior = ProviderBackedAgentBehavior::new(
                        agent_id.clone(),
                        adapter,
                        provider_phase1_action_catalog(),
                    )
                    .require_continuous_request_context()
                    .with_provider_config_ref(format!(
                        "provider://{}/runtime-live/{}",
                        settings.provider_transport, agent_id
                    ))
                    .with_agent_profile(settings.agent_profile.clone())
                    .with_execution_mode(settings.execution_mode)
                    .with_timeout_budget_ms(settings.decision_timeout_ms)
                    .with_environment_class("runtime_live")
                    .with_memory_summary(provider_phase1_memory_summary());
                    let behavior =
                        if let Some(fallback_reason) = settings.fallback_reason.as_deref() {
                            behavior.with_fallback_reason(fallback_reason)
                        } else {
                            behavior
                        };
                    runner
                        .register(behavior)
                        .map_err(|error| format!("provider agent init failed: {error}"))?;
                    self.provider_agent_ids.insert(agent_id);
                }
            }
        }
        Ok(())
    }
}
