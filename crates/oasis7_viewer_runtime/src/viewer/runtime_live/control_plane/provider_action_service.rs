use super::*;

impl ViewerRuntimeLiveServer {
    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_prepare_canonical_provider_response_with_memory(
        &mut self,
        agent_id: &str,
        action: oasis7_simulator::Action,
        intents: Vec<crate::simulator::MemoryWriteIntent>,
    ) -> Result<serde_json::Value, String> {
        self.config.ensure_service_agent_lineage_store()?;
        let value = self.test_prepare_canonical_provider_response(agent_id, action)?;
        let mut context: super::llm_sidecar::RuntimeProviderActionContext =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        context.response.base_decision_response.memory_write_intents = intents.clone();
        context.response.response_digest =
            crate::simulator::cognition_response_digest(&context.response.base_decision_response);
        context.memory_write_intents = intents;
        serde_json::to_value(context).map_err(|error| error.to_string())
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_canonical_provider_summary(&self) -> serde_json::Value {
        let mut summary = self.llm_sidecar.service_test_summary();
        summary["hosted_service_inflight"] = self
            .llm_sidecar
            .hosted_service_inflight
            .as_ref()
            .map(|token| {
                serde_json::json!({
                    "generation": token.generation,
                    "phase_digest": token.phase_digest,
                })
            })
            .unwrap_or(serde_json::Value::Null);
        summary["agent_service_pump_error"] = serde_json::json!(self.agent_service_pump_error);
        summary["io_lifecycle"] = self.agent_service_io_diagnostics.snapshot();
        summary
    }
    /// Explicit live diagnostics; unlike canonical summary, active age changes with wall time.
    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_canonical_provider_diagnostic_summary(&self) -> serde_json::Value {
        let mut summary = self.test_canonical_provider_summary();
        summary["io_lifecycle"] = self.agent_service_io_diagnostics.diagnostic_snapshot();
        summary
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_prepare_canonical_provider_response(
        &mut self,
        agent_id: &str,
        action: oasis7_simulator::Action,
    ) -> Result<serde_json::Value, String> {
        self.config.ensure_service_agent_lineage_store()?;
        use crate::world_service::client::WorldServicePort;
        use oasis7_client_api::world_service::*;
        let client = self
            .world_service_client()
            .map_err(|error| format!("{error:?}"))?
            .ok_or("test requires actual service client")?;
        self.verified_world_view = Some(
            client
                .read_view(ReadWorldViewRequest {
                    observation_nonce: None,
                    formal_anchor: None,
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    world: client.config().expected_world.clone(),
                    scope_id: client.config().scope_id.clone(),
                    min_commit: None,
                    fixed_commit: None,
                    deadline_unix_ms: None,
                })
                .map_err(|error| error.to_string())?,
        );
        self.configure_service_provider();
        self.llm_sidecar
            .sync_shadow_kernel(&self.world, &self.snapshot_config)?;
        self.llm_sidecar.make_service_test_context(agent_id, action)
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_queue_canonical_provider_response(
        &mut self,
        cognition: serde_json::Value,
        action: oasis7_simulator::Action,
    ) -> Result<(), String> {
        self.config.ensure_service_agent_lineage_store()?;
        use crate::world_service::client::WorldServicePort;
        use oasis7_client_api::world_service::*;
        if !self.chain_link_enabled() {
            return Err("test requires chain-linked configuration".into());
        }
        let client = self
            .world_service_client()
            .map_err(|error| format!("{error:?}"))?
            .ok_or("test requires actual service client")?;
        let view = client
            .read_view(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: client.config().expected_world.clone(),
                scope_id: client.config().scope_id.clone(),
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            })
            .map_err(|error| error.to_string())?;
        self.verified_world_view = Some(view);
        self.configure_service_provider();
        let cognition: super::llm_sidecar::RuntimeProviderActionContext =
            serde_json::from_value(cognition).map_err(|error| error.to_string())?;
        self.llm_sidecar
            .queue_service_test_response(&mut self.world, cognition, action)?;
        Ok(())
    }

    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_poll_canonical_provider_response(&mut self) -> Result<(), String> {
        self.llm_sidecar.request_decision();
        self.enqueue_llm_action_from_sidecar()
            .map(|_| ())
            .map_err(|trace| {
                trace
                    .llm_error
                    .unwrap_or_else(|| "provider trace failed".into())
            })
    }

    pub(in crate::viewer::runtime_live) fn configure_service_provider(&mut self) {
        if self.config.world_service.is_some() {
            let previous = self
                .llm_sidecar
                .provider_service_projection
                .as_ref()
                .and_then(|view| view.canonical_agent_chat.as_ref())
                .and_then(|chat| chat.goal.as_ref())
                .map(|goal| goal.intent_id.clone());
            if self.llm_sidecar.canonical_owner_goal.as_ref().is_some_and(|chat| {
                self.session_policy.validate_known_session_key(&chat.player_id, &chat.public_key).is_err()
                    || self.verified_world_view.as_ref().is_none_or(|view| {
                        let commit = &view.version().commit;
                        chat.world_id != commit.world.world_id
                            || chat.canonical_authority.branch_id != commit.binding.branch_id
                            || chat.reorg_epoch != commit.binding.reorg_generation
                            || (view.projection().canonical_agent_owner.as_ref().is_none()
                                && self.llm_sidecar.canonical_owner_goal_commit.as_ref().is_none_or(|owner_commit| !owner_commit.satisfies_minimum(commit).unwrap_or(false)))
                            || view.projection().canonical_agent_owner.as_ref().is_some_and(|fence| fence != &oasis7_proto::viewer::CanonicalAgentOwnerFenceV1::from(chat))
                    })
            }) {
                self.llm_sidecar.canonical_owner_goal = None;
                self.llm_sidecar.canonical_owner_goal_commit = None;
            }
            let current = self
                .verified_world_view
                .as_ref()
                .and_then(|view| view.projection().canonical_agent_chat.clone())
                .or_else(|| self.llm_sidecar.canonical_owner_goal.clone());
            // A provider-local profile is a disposable projection of the
            // authenticated goal. Removal/revocation clears it on refresh.
            for profile in self.llm_sidecar.prompt_profiles.values_mut() {
                profile.short_term_goal_override = None;
            }
            if let Some(chat) = current
                && let Some(goal) = chat
                    .goal
                    .filter(|goal| matches!(goal.status.as_str(), "accepted" | "blocked"))
            {
                self.llm_sidecar
                    .prompt_profiles
                    .entry(chat.agent_id.clone())
                    .or_insert_with(|| AgentPromptProfile::for_agent(chat.agent_id))
                    .short_term_goal_override = Some(goal.message);
                if previous.as_deref() != Some(goal.intent_id.as_str()) {
                    self.llm_sidecar.request_decision();
                }
            }
        }
        self.llm_sidecar.provider_service_required = self.chain_link_enabled();
        self.llm_sidecar.provider_service_lineage_store_explicit =
            self.config.world_service.is_some()
                && self.config.ensure_service_agent_lineage_store().is_ok();
        self.llm_sidecar.provider_service_config = self.config.world_service.clone();
        self.llm_sidecar.provider_service_query_state = self.world_service_query_state.clone();
        self.llm_sidecar.provider_service_signer = self.config.world_service_agent_signer.clone();
        self.llm_sidecar.provider_service_projection = self
            .verified_world_view
            .as_ref()
            .map(|view| view.projection().clone());
        if let Some(projection) = self.llm_sidecar.provider_service_projection.as_mut()
            && projection.canonical_agent_chat.is_none()
        {
            projection.canonical_agent_chat = self.llm_sidecar.canonical_owner_goal.clone();
        }
    }
    #[expect(
        clippy::result_large_err,
        reason = "Failure returns the complete decision trace to the existing control-plane recorder without losing action or recovery evidence"
    )]
    pub(super) fn enqueue_service_provider_action(
        &mut self,
    ) -> Result<Option<AgentDecisionTrace>, AgentDecisionTrace> {
        let tick = self
            .llm_sidecar
            .provider_service_projection
            .as_ref()
            .map(|view| view.state.time)
            .unwrap_or(0);
        if self.llm_sidecar.provider_service_projection.is_none() {
            return Err(wake_handoff_error_trace(
                "world_service",
                tick,
                "verified canonical Agent view is unavailable; turn remains pending".into(),
            ));
        }
        if self
            .llm_sidecar
            .recover_service_release_checkpoints(&mut self.world)
            .map_err(|error| wake_handoff_error_trace("world_service", tick, error))?
        {
            return Ok(None);
        }
        if !self.llm_sidecar.provider_service_pending.is_empty() {
            self.retry_committed_provider_action()
                .map_err(|error| wake_handoff_error_trace("world_service", tick, error))?;
            return Ok(None);
        }
        if let Some((agent_id, trace, cognition)) = self.llm_sidecar.pending_admitted_service_wait()
        {
            return self.close_admitted_service_wait(agent_id, trace, cognition, tick);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self
            .llm_sidecar
            .retry_service_wait_admission(&mut self.world, &self.snapshot_config)
            .map_err(|error| wake_handoff_error_trace("world_service", tick, error))?
        {
            return Ok(None);
        }
        let Some(decision) = self.llm_sidecar.next_llm_decision(
            &mut self.world,
            &self.snapshot_config,
            self.config.world_id.as_str(),
        ) else {
            return Ok(None);
        };
        let trace = decision.decision_trace;
        if trace.as_ref().is_some_and(|trace| {
            trace.parse_error.is_some()
                || (trace.llm_error.is_some()
                    && !super::super::decision_trace::is_trace_only_overflow(trace))
        }) {
            return Err(trace.expect("checked provider error trace"));
        }
        match decision.decision {
            AgentDecision::Act(action) => {
                let cognition = decision.cognition.ok_or_else(|| {
                    wake_handoff_error_trace(
                        &decision.agent_id,
                        tick,
                        "canonical Agent action lacks signed cognition context".into(),
                    )
                })?;
                let runtime_action =
                    simulator_action_to_runtime(&action, &self.world).ok_or_else(|| {
                        wake_handoff_error_trace(
                            &decision.agent_id,
                            tick,
                            "canonical Agent action is unsupported by the registered codec".into(),
                        )
                    })?;
                self.commit_provider_runtime_action(&runtime_action, &cognition, action)
                    .map_err(|error| {
                        wake_handoff_error_trace(&decision.agent_id, tick, error.reason())
                    })?;
                Ok(trace)
            }
            AgentDecision::Wait | AgentDecision::WaitTicks(_) if decision.continuation_admitted => {
                self.close_admitted_service_wait(decision.agent_id, trace, decision.cognition, tick)
            }
            _ => Err(wake_handoff_error_trace(
                &decision.agent_id,
                tick,
                "canonical provider decision has no committed registered intent".into(),
            )),
        }
    }

    #[expect(
        clippy::result_large_err,
        reason = "Failure returns the complete decision trace to the existing control-plane recorder without losing action or recovery evidence"
    )]
    fn close_admitted_service_wait(
        &mut self,
        agent_id: String,
        trace: Option<AgentDecisionTrace>,
        cognition: Option<super::llm_sidecar::RuntimeProviderActionContext>,
        tick: u64,
    ) -> Result<Option<AgentDecisionTrace>, AgentDecisionTrace> {
        self.llm_sidecar
            .checkpoint_admitted_service_wait()
            .map_err(|error| wake_handoff_error_trace(&agent_id, tick, error))?;
        let cognition = cognition.as_ref().ok_or_else(|| {
            wake_handoff_error_trace(
                &agent_id,
                tick,
                "canonical admitted Wait lacks original cognition context".into(),
            )
        })?;
        self.settle_provider_cognition_lease_for_request(
            &agent_id,
            cognition.cognition_lease.clone(),
            Some(&cognition.request.request_context),
        )
        .map_err(|error| wake_handoff_error_trace(&agent_id, tick, error))?;
        use crate::world_service::client::WorldServicePort;
        use oasis7_client_api::world_service::*;
        let client = self
            .world_service_client()
            .map_err(|error| wake_handoff_error_trace(&agent_id, tick, format!("{error:?}")))?
            .ok_or_else(|| {
                wake_handoff_error_trace(&agent_id, tick, "canonical Wait service missing".into())
            })?;
        let view = client
            .read_view(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: client.config().expected_world.clone(),
                scope_id: client.config().scope_id.clone(),
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            })
            .map_err(|error| wake_handoff_error_trace(&agent_id, tick, error.to_string()))?;
        self.verified_world_view = Some(view);
        self.configure_service_provider();
        self.llm_sidecar
            .validate_admitted_service_wait(&agent_id)
            .map_err(|error| wake_handoff_error_trace(&agent_id, tick, error))?;
        self.llm_sidecar
            .finish_admitted_service_wait(&agent_id)
            .map_err(|error| wake_handoff_error_trace(&agent_id, tick, error))?;
        Ok(trace)
    }
}
