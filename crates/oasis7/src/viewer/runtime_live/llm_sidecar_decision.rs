#[cfg(target_arch = "wasm32")]
use super::super::decision_trace::{is_budget_exhausted_wait, is_trace_only_overflow};
use super::*;

impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn next_llm_decision(
        &mut self,
        world: &mut RuntimeWorld,
        config: &WorldConfig,
        world_id: &str,
    ) -> Option<RuntimeLlmDecision> {
        if !self.is_llm_mode() || self.llm_decision_mailbox == 0 {
            return None;
        }
        self.llm_decision_mailbox = self.llm_decision_mailbox.saturating_sub(1);

        if let Err(message) = self.sync_shadow_kernel(world, config) {
            return Some(RuntimeLlmDecision::from_error(world, message));
        }
        if let Err(message) = self.ensure_runner_initialized() {
            return Some(RuntimeLlmDecision::from_error(world, message));
        }
        if self.provider_service_required
            && let Err(message) = self.sync_canonical_goal_prompt()
        {
            return Some(RuntimeLlmDecision::from_error(world, message));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(runner) = self
                .runner
                .as_mut()
                .and_then(RuntimeDecisionRunner::async_runner_mut)
            else {
                return Some(RuntimeLlmDecision::from_error(
                    world,
                    "native provider runner is unavailable for feedback recovery".to_string(),
                ));
            };
            if runner.active_turn_count() == 0 && self.provider_service_required {
                // A redacted local World is not canonical feedback history.
                // Missing history leaves recovery uninitialized; fresh admission
                // fails closed separately so issued cleanup can still progress.
                if let Some(view) = self.provider_service_projection.as_ref()
                    && let Some(authority) = view.agent_context.as_ref()
                    && let Some(history) = view.feedback_history.as_ref()
                    && let Err(message) = crate::world_service_feedback_host::restore_preverified(history, &authority.agent_id, runner) {
                        tracing::warn!(%message, "authenticated feedback recovery fenced");
                    }
            } else if runner.active_turn_count() == 0
                && runner.needs_runtime_feedback_recovery()
                && let Err(message) = crate::viewer::runtime_live::llm_sidecar_feedback_recovery::
                    restore_agent_feedback_history(world, runner)
            {
                return Some(RuntimeLlmDecision::from_error(world, message));
            }
        }
        // Chain-sync can enqueue authoritative completions before this lazy
        // runner is registered.  Flush the durable mailbox immediately after
        // registration so Builtin and ProviderBacked lanes share the same
        // startup delivery guarantee.
        self.flush_pending_provider_world_events();
        let mut kernel = match self.shadow_kernel.take() {
            Some(kernel) => kernel,
            None => {
                return Some(RuntimeLlmDecision::from_error(
                    world,
                    "shadow kernel not initialized".to_string(),
                ));
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        if matches!(
            self.runner,
            Some(RuntimeDecisionRunner::Builtin(_) | RuntimeDecisionRunner::ProviderBacked(_))
        ) {
            let decision = self.next_async_provider_decision(world, &mut kernel, world_id);
            self.shadow_kernel = Some(kernel);
            return decision;
        }
        #[cfg(target_arch = "wasm32")]
        if matches!(self.runner, Some(RuntimeDecisionRunner::ProviderBacked(_))) {
            if let Err(message) =
                self.prepare_provider_request_contexts(world, &mut kernel, world_id)
            {
                self.shadow_kernel = Some(kernel);
                return Some(RuntimeLlmDecision::from_error(world, message));
            }
            // AgentRunner's wasm lane invokes the provider synchronously. Its
            // deterministic scheduler choice is previewed first so Runtime
            // receives TurnStarted/ContextCaptured/RequestDispatched before
            // the behavior performs any provider I/O.
            let selected_agent = match self.runner.as_ref() {
                Some(RuntimeDecisionRunner::ProviderBacked(runner)) => {
                    runner.next_ready_agent_id(&kernel)
                }
                _ => None,
            };
            if let Some(agent_id) = selected_agent {
                if self.provider_transport_exhausted.contains(&agent_id) {
                    // Let the control-plane exhaustion path terminalize this
                    // identity before the WASM runner can redispatch it.
                    self.shadow_kernel = Some(kernel);
                    return None;
                }
                if let Some(context) = self.provider_contexts.get(&agent_id).cloned() {
                    if let Some(existing_lease) = self.provider_cognition_lease(agent_id.as_str()) {
                        if let Err(error) = self.validate_provider_cognition_lease_for_request(
                            world,
                            agent_id.as_str(),
                            &context.request_context,
                            &existing_lease,
                            "dispatch",
                        ) {
                            self.fence_provider_cognition_lease(
                                agent_id.as_str(),
                                &context,
                                error.clone(),
                            );
                            self.shadow_kernel = Some(kernel);
                            return Some(RuntimeLlmDecision::from_agent_error(
                                world, agent_id, error,
                            ));
                        }
                    }
                    let cognition_lease =
                        match self.reserve_provider_lease_at_authority(world, &context) {
                            Ok(lease) => lease,
                            Err(error) => {
                                self.shadow_kernel = Some(kernel);
                                return Some(RuntimeLlmDecision::from_agent_error(
                                    world, agent_id, error,
                                ));
                            }
                        };
                    self.bind_provider_cognition_lease(agent_id.clone(), cognition_lease.clone());
                    if let Err(error) = self.provider_prefix_at_authority(world, &context) {
                        let release_error = self
                            .release_provider_lease_before_io_or_fence(
                                world,
                                agent_id.as_str(),
                                &context,
                                &cognition_lease,
                            )
                            .err()
                            .map(|error| format!("; {error}"));
                        self.shadow_kernel = Some(kernel);
                        return Some(RuntimeLlmDecision::from_agent_error(
                            world,
                            agent_id,
                            format!(
                                "Runtime cognition prefix rejected provider I/O: {error}{}",
                                release_error.unwrap_or_default()
                            ),
                        ));
                    }
                    if let Some(RuntimeDecisionRunner::ProviderBacked(runner)) =
                        self.runner.as_mut()
                    {
                        if let Some(agent) = runner.get_mut(agent_id.as_str()) {
                            agent
                                .behavior
                                .set_continuous_turn_context(Some(&context.turn_context));
                            agent
                                .behavior
                                .set_continuous_request_context(Some(&context.request_context));
                        }
                    }
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.shadow_kernel = Some(kernel);
            None
        }
        #[cfg(target_arch = "wasm32")]
        let runner = match self.runner.as_mut() {
            Some(runner) => runner,
            None => {
                self.shadow_kernel = Some(kernel);
                return Some(RuntimeLlmDecision::from_error(
                    world,
                    "llm runner not initialized".to_string(),
                ));
            }
        };
        #[cfg(target_arch = "wasm32")]
        let result: Option<crate::simulator::AgentTickResult> = match runner {
            RuntimeDecisionRunner::Builtin(runner) => {
                let result = runner.tick_decide_only(&mut kernel);
                sync_llm_runner_long_term_memory(&mut kernel, runner);
                result
            }
            RuntimeDecisionRunner::ProviderBacked(runner) => runner.tick_decide_only(&mut kernel),
        };
        #[cfg(target_arch = "wasm32")]
        {
            self.shadow_kernel = Some(kernel);
            let Some(tick) = result else {
                return None;
            };
            let provider_response =
                if let Some(RuntimeDecisionRunner::ProviderBacked(runner)) = self.runner.as_mut() {
                    runner
                        .get_mut(tick.agent_id.as_str())
                        .and_then(|agent| agent.behavior.take_continuous_response_context())
                } else {
                    None
                };
            let memory_write_intents =
                if let Some(RuntimeDecisionRunner::ProviderBacked(runner)) = self.runner.as_mut() {
                    runner
                        .get_mut(tick.agent_id.as_str())
                        .map(|agent| agent.behavior.take_memory_write_intents())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
            let cognition = self
                .provider_contexts
                .get(tick.agent_id.as_str())
                .cloned()
                .zip(provider_response)
                .map(|(request, response)| RuntimeProviderActionContext {
                    request,
                    response,
                    cognition_lease: self.provider_cognition_lease(tick.agent_id.as_str()),
                    memory_write_intents: memory_write_intents.clone(),
                });
            if let Some(cognition) = cognition.as_ref() {
                if tick.decision_trace.as_ref().is_none_or(|trace| {
                    trace.parse_error.is_none()
                        && (trace.llm_error.is_none()
                            || is_trace_only_overflow(trace)
                            || is_budget_exhausted_wait(trace))
                }) {
                    self.provider_active_turns
                        .insert(tick.agent_id.clone(), cognition.request.clone());
                    match &tick.decision {
                        AgentDecision::Wait => {
                            #[cfg(target_arch = "wasm32")]
                            tracing::warn!(
                                agent_id = tick.agent_id.as_str(),
                                "WASM ProviderBacked Wait uses the compatibility timer; native Runtime durable continuation admission is not available on this lane"
                            );
                            self.schedule_provider_wait(
                                tick.agent_id.as_str(),
                                world.state().time,
                                1,
                            );
                        }
                        AgentDecision::WaitTicks(ticks) => {
                            #[cfg(target_arch = "wasm32")]
                            tracing::warn!(
                                agent_id = tick.agent_id.as_str(),
                                "WASM ProviderBacked WaitTicks uses the compatibility timer; native Runtime durable continuation admission is not available on this lane"
                            );
                            self.schedule_provider_wait(
                                tick.agent_id.as_str(),
                                world.state().time,
                                *ticks,
                            );
                        }
                        AgentDecision::Act(_) => {
                            #[cfg(target_arch = "wasm32")]
                            if let Some(RuntimeDecisionRunner::ProviderBacked(runner)) =
                                self.runner.as_mut()
                            {
                                if let Some(agent) = runner.get_mut(tick.agent_id.as_str()) {
                                    // Runtime receipt/disposition closes an action
                                    // turn; no local retry may re-enter it early.
                                    agent.wait_until = Some(u64::MAX);
                                }
                            }
                        }
                        AgentDecision::Query(_) | AgentDecision::ModuleCommand { .. } => {}
                    }
                }
            } else if tick
                .decision_trace
                .as_ref()
                .is_some_and(provider_trace_retryable)
            {
                if let Some(context) = self.provider_contexts.get(tick.agent_id.as_str()).cloned() {
                    if context.request_context.transport_attempt < MAX_PROVIDER_TRANSPORT_ATTEMPTS {
                        self.provider_retry_contexts
                            .insert(tick.agent_id.clone(), context);
                        self.persist_provider_lineage_best_effort();
                    } else {
                        self.mark_provider_transport_exhausted(tick.agent_id.clone());
                    }
                }
            }
            Some(RuntimeLlmDecision {
                agent_id: tick.agent_id,
                decision: tick.decision,
                decision_trace: tick.decision_trace,
                cognition,
                memory_write_intents,
                continuation_admitted: false,
            })
        }
    }
}
