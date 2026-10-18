use super::*;

pub(super) fn runtime_continuation_for_wake_with_identity(
    world: &RuntimeWorld,
    projection: Option<&crate::world_service::projection::WorldServiceProjection>,
    wake: &crate::runtime::SchedulerWakeV1,
    session_id: &str,
    sequence: u64,
) -> Result<
    (
        SimulatorContinuationProposalV1,
        crate::runtime::CognitionContinuationProposalV1,
    ),
    String,
> {
    let continuation = if let Some(view) = projection {
        view.continuations
            .iter()
            .find(|continuation| {
                continuation.continuation_id == wake.continuation_id
                    && continuation.agent_id == wake.agent_id
            })
            .cloned()
            .ok_or("authorized continuation missing")?
    } else {
        active_runtime_continuation_for_wake(world, wake)?
    };
    if continuation.remaining_budget.value <= 1 {
        return Err(format!(
            "Runtime continuation {} has no budget for a resumed request",
            continuation.continuation_id
        ));
    }
    let mut proposal = serde_json::to_value(continuation).map_err(|error| {
        format!(
            "Runtime continuation {} cannot cross provider boundary: {error}",
            wake.continuation_id
        )
    })?;
    let projected_context = projection
        .and_then(|view| view.continuation_contexts.get(&wake.continuation_id))
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| format!("canonical continuation context invalid: {error}"))?;
    if projection.is_some() && projected_context.is_none() {
        return Err("authorized canonical continuation context missing".into());
    }
    if let Some(context) = projected_context
        .as_ref()
        .and_then(Value::as_object)
        .or_else(|| {
            world
                .cognition()
                .get("continuation_contexts")
                .and_then(Value::as_object)
                .and_then(|contexts| contexts.get(&wake.continuation_id))
                .and_then(Value::as_object)
        })
    {
        for field in [
            "baseline_observation_digest",
            "goal_digest",
            "policy_digest",
            "policy_revision",
            "precondition_summary",
            "precondition_digest",
        ] {
            if let Some(value) = context.get(field) {
                proposal[field] = value.clone();
            }
        }
    }
    proposal["action_or_plan_kind"] = serde_json::json!("continuation_resume");
    // AgentContinuation is the Runtime-owned durable projection and does not
    // retain the adapter source label. Reintroduce the bounded paired-schema
    // field before decoding the resume proposal; Runtime still owns and
    // verifies every identity/digest below.
    proposal["source"] = serde_json::json!("runtime-resume");
    let continuation_proposal_id = format!(
        "{}:resume:{}",
        proposal["continuation_proposal_id"]
            .as_str()
            .unwrap_or("continuation"),
        sequence
    );
    proposal["continuation_proposal_id"] = serde_json::json!(continuation_proposal_id);
    proposal["agent_session_id"] = serde_json::json!(session_id);
    proposal["agent_turn_id"] = serde_json::json!(format!("{session_id}-turn-{sequence}"));
    proposal["decision_request_id"] = serde_json::json!(format!("{session_id}-request-{sequence}"));
    let remaining = proposal
        .get("remaining_budget")
        .and_then(Value::as_object)
        .and_then(|budget| budget.get("value"))
        .and_then(Value::as_u64)
        .ok_or_else(|| "Runtime continuation budget projection is invalid".to_string())?;
    let remaining = remaining
        .checked_sub(1)
        .ok_or_else(|| "Runtime continuation budget is exhausted".to_string())?;
    proposal["remaining_budget"]["value"] = serde_json::json!(remaining);
    proposal["schema_version"] = serde_json::json!(1);
    // This is Runtime-derived admission metadata, not the original causal
    // wake condition. Let Runtime derive it at the new committed tick.
    proposal["next_wake_tick"] = Value::Null;
    let mut simulator = serde_json::from_value::<SimulatorContinuationProposalV1>(proposal.clone())
        .map_err(|error| {
            format!(
                "Runtime continuation {} cannot cross provider boundary: {error}",
                wake.continuation_id
            )
        })?;
    simulator.proposal_digest = simulator
        .proposal_digest()
        .map_err(|error| format!("simulator continuation digest failed: {error}"))?
        .to_string();
    let mut runtime =
        serde_json::from_value::<crate::runtime::CognitionContinuationProposalV1>(proposal)
            .map_err(|error| {
                format!(
                    "Runtime continuation {} cannot produce admission proposal: {error}",
                    wake.continuation_id
                )
            })?;
    runtime.proposal_digest = runtime.proposal_digest();
    Ok((simulator, runtime))
}

impl crate::viewer::ViewerRuntimeLiveServer {
    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_reserve_canonical_provider_context(
        &mut self,
        value: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.config.ensure_service_agent_lineage_store()?;
        if !self.llm_sidecar.provider_service_required || self.config.world_service.is_none() {
            return Err("test requires actual canonical provider service".into());
        }
        let mut cognition: RuntimeProviderActionContext =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        let lease = self
            .llm_sidecar
            .reserve_provider_lease_at_authority(&mut self.world, &cognition.request)?;
        self.llm_sidecar.bind_provider_cognition_lease(
            cognition.request.request_context.agent_subject.clone(),
            lease.clone(),
        );
        self.llm_sidecar.provider_contexts.insert(
            cognition.request.request_context.agent_subject.clone(),
            cognition.request.clone(),
        );
        cognition.cognition_lease = Some(lease);
        self.llm_sidecar.persist_provider_lineage()?;
        serde_json::to_value(cognition).map_err(|error| error.to_string())
    }

    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_release_canonical_provider_context(
        &mut self,
        value: serde_json::Value,
    ) -> Result<(), String> {
        if !self.llm_sidecar.provider_service_required || self.config.world_service.is_none() {
            return Err("test requires actual canonical provider service".into());
        }
        let cognition: RuntimeProviderActionContext =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        let request = &cognition.request.request_context;
        let lease = cognition
            .cognition_lease
            .as_ref()
            .ok_or("test requires actually reserved canonical lease")?;
        self.llm_sidecar
            .validate_provider_cognition_lease_for_request(
                &self.world,
                &request.agent_subject,
                request,
                lease,
                "release",
            )?;
        self.llm_sidecar.release_provider_lease_before_io_or_fence(
            &mut self.world,
            &request.agent_subject,
            &cognition.request,
            lease,
        )
    }
    /// Builds a production-bound Wait response; the regular queue/poll helpers
    /// run the actor and perform actual canonical continuation admission.
    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_prepare_canonical_wait_provider_response(
        &mut self,
        agent_id: &str,
        observation_action: oasis7_simulator::Action,
        ticks: u64,
    ) -> Result<serde_json::Value, String> {
        if ticks == 0 {
            return Err("canonical Wait fixture requires positive ticks".into());
        }
        let value = self.test_prepare_canonical_provider_response(agent_id, observation_action)?;
        let mut cognition: RuntimeProviderActionContext =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        cognition.response.base_decision_response.decision =
            crate::simulator::ProviderDecision::WaitTicks { ticks };
        cognition.response.response_digest =
            crate::simulator::cognition_response_digest(&cognition.response.base_decision_response);
        serde_json::to_value(cognition).map_err(|error| error.to_string())
    }

    /// Returns the original checkpoint produced by the real provider Wait
    /// handler. Its existence alone does not attest canonical admission.
    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_canonical_provider_wait_proposal(
        &self,
        agent_id: &str,
    ) -> Result<crate::simulator::ContinuationProposalV1, String> {
        let mut proposals = self
            .llm_sidecar
            .provider_continuation_proposals
            .values()
            .filter(|proposal| proposal.agent_id == agent_id && proposal.source == "provider_wait");
        let proposal = proposals
            .next()
            .cloned()
            .ok_or("real provider Wait has not produced an original proposal")?;
        if proposals.next().is_some() {
            return Err("multiple provider Wait proposals require explicit selection".into());
        }
        proposal.validate().map_err(|error| error.to_string())?;
        Ok(proposal)
    }

    /// Uses a caller's original admitted proposal only as a recovery checkpoint;
    /// the production wake path validates it against the signed canonical view.
    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_prepare_canonical_wake_provider_response(
        &mut self,
        agent_id: &str,
        action: oasis7_simulator::Action,
        original_proposal: SimulatorContinuationProposalV1,
    ) -> Result<serde_json::Value, String> {
        if self.llm_sidecar.pending_admitted_service_wait().is_some() {
            return Err("canonical Wait closure remains pending before wake preparation".into());
        }
        let value = self.test_prepare_canonical_provider_response(agent_id, action)?;
        let mut cognition: RuntimeProviderActionContext =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        original_proposal
            .validate()
            .map_err(|error| error.to_string())?;
        let provider = crate::simulator::MockDecisionProvider::with_scripted_responses(
            "canonical-test-provider",
            vec![Ok(cognition.response.base_decision_response.clone())],
        );
        let behavior = crate::simulator::ProviderBackedAgentBehavior::new(
            agent_id.to_string(),
            provider,
            provider_phase1_action_catalog(),
        )
        .require_continuous_request_context();
        let wake_actor_key = format!("{agent_id}:wake");
        if !self
            .llm_sidecar
            .service_test_actor_agents
            .contains(&wake_actor_key)
        {
            let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
            runner
                .register(behavior)
                .map_err(|error| format!("{error:?}"))?;
            self.llm_sidecar.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
            self.llm_sidecar
                .service_test_actor_agents
                .insert(agent_id.to_string());
            self.llm_sidecar
                .service_test_actor_agents
                .insert(wake_actor_key);
        }
        self.llm_sidecar
            .provider_agent_ids
            .insert(agent_id.to_string());
        self.llm_sidecar.provider_continuation_proposals.insert(
            original_proposal.continuation_proposal_id.clone(),
            original_proposal,
        );
        self.llm_sidecar.sync_runtime_wakes(&self.world)?;
        if !self
            .llm_sidecar
            .has_pending_runtime_wake_for_agent(agent_id)
            && !self
                .llm_sidecar
                .has_pending_service_resume_for_agent(agent_id)
        {
            return Err("test requires a real canonical selected wake".into());
        }
        let world_id = self
            .llm_sidecar
            .provider_service_projection
            .as_ref()
            .and_then(|view| view.runtime_binding.as_ref())
            .ok_or("canonical binding missing")?
            .world_id
            .clone();
        let mut kernel = self
            .llm_sidecar
            .shadow_kernel
            .take()
            .ok_or("authenticated observation kernel missing")?;
        let prepared = self.llm_sidecar.prepare_provider_request_contexts(
            &mut self.world,
            &mut kernel,
            &world_id,
        );
        self.llm_sidecar.shadow_kernel = Some(kernel);
        prepared?;
        cognition.request = self
            .llm_sidecar
            .provider_contexts
            .get(agent_id)
            .cloned()
            .ok_or("canonical wake resume did not produce a request")?;
        let request = &cognition.request.request_context;
        cognition.response.agent_session_id = request.agent_session_id.clone();
        cognition.response.agent_turn_id = request.agent_turn_id.clone();
        cognition.response.decision_request_id = request.decision_request_id.clone();
        cognition.response.retry_seq = request.retry_seq;
        cognition.response.transport_attempt = request.transport_attempt;
        cognition.response.request_digest = request.request_digest.clone();
        serde_json::to_value(cognition).map_err(|error| error.to_string())
    }
    pub fn canonical_agent_service_context(
        world: &RuntimeWorld,
        agent_id: &str,
    ) -> Result<oasis7_agent_api::world_context::WorldServiceAgentContext, String> {
        world.canonical_agent_service_context(agent_id)
    }
}

impl RuntimeLlmSidecar {
    /// Admission already delivered the canonical projection and released the
    /// actor turn. Validate that active Harness identity before durable cleanup.
    pub(in crate::viewer::runtime_live) fn validate_admitted_service_wait(
        &self,
        agent_id: &str,
    ) -> Result<(), String> {
        let runner = self
            .runner
            .as_ref()
            .ok_or("canonical Wait Harness runner missing")?;
        let held = self
            .provider_held_decisions
            .get(agent_id)
            .filter(|held| held.continuation_admitted)
            .and_then(|held| held.cognition.as_ref())
            .ok_or("canonical Wait original held request missing")?;
        let request = &held.request.request_context;
        let proposal = self
            .provider_continuation_proposals
            .values()
            .find(|proposal| {
                proposal.source == "provider_wait"
                    && proposal.agent_id == agent_id
                    && proposal.agent_session_id == request.agent_session_id
                    && proposal.agent_turn_id == request.agent_turn_id
                    && proposal.decision_request_id == request.decision_request_id
                    && proposal.origin_turn_id == request.agent_turn_id
                    && proposal.origin_request_digest == request.request_digest.to_string()
            })
            .ok_or("canonical Wait original proposal identity mismatch")?;
        let view = self
            .provider_service_projection
            .as_ref()
            .ok_or("canonical Wait verified projection missing")?;
        let runtime = view
            .continuations
            .iter()
            .find(|runtime| {
                runtime.continuation_proposal_id == proposal.continuation_proposal_id
                    && runtime.agent_id == agent_id
                    && runtime.agent_session_id == request.agent_session_id
                    && runtime.agent_turn_id == request.agent_turn_id
                    && runtime.decision_request_id == request.decision_request_id
                    && runtime.origin_turn_id == request.agent_turn_id
                    && runtime.origin_request_digest == request.request_digest.to_string()
            })
            .ok_or("canonical Wait admitted projection identity mismatch")?;
        let context = view
            .continuation_contexts
            .get(&runtime.continuation_id)
            .ok_or("canonical Wait verified continuation authority missing")?;
        let authority = crate::simulator::ContinuationAuthorityContextV1 {
            baseline_observation_digest: context.baseline_observation_digest.clone(),
            goal_digest: context.goal_digest.clone(),
            policy_digest: context.policy_digest.clone(),
            precondition_digest: context.precondition_digest.clone(),
        };
        #[cfg(not(target_arch = "wasm32"))]
        match runner {
            RuntimeDecisionRunner::ProviderBacked(runner)
            | RuntimeDecisionRunner::Builtin(runner) => runner
                .validate_active_continuation_with_authority(agent_id, &authority, runtime)
                .map_err(|error| {
                    format!("canonical Wait Harness admission validation failed: {error}")
                }),
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (runner, authority);
            runtime
                .validate_authoritative()
                .map_err(|error| error.to_string())
        }
    }

    pub(super) fn pending_service_resume_proposal(
        pending: &lineage_persistence::PendingProviderSchedulerIntent,
    ) -> Result<crate::runtime::CognitionContinuationProposalV1, String> {
        let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) = &pending.payload
        else {
            return Err("pending canonical ResumeWake operation mismatch".into());
        };
        let crate::world_service::wire::SchedulerOperationV1::ResumeWake { proposal, .. } =
            &signed.request.operation
        else {
            return Err("pending canonical ResumeWake operation mismatch".into());
        };
        Ok(proposal.clone())
    }

    pub(super) fn pending_service_resume_sequence(
        &self,
        agent_id: &str,
        wake: Option<&crate::runtime::SchedulerWakeV1>,
    ) -> Option<u64> {
        let wake = wake?;
        self.provider_scheduler_pending
            .values()
            .find_map(|pending| {
                let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
                    &pending.payload
                else {
                    return None;
                };
                let crate::world_service::wire::SchedulerOperationV1::ResumeWake {
                    wake_id,
                    resume,
                    ..
                } = &signed.request.operation
                else {
                    return None;
                };
                (wake_id == &wake.wake_id && signed.request.agent_id == agent_id)
                    .then(|| {
                        resume
                            .agent_turn_id
                            .rsplit("-turn-")
                            .next()?
                            .parse::<u64>()
                            .ok()
                    })
                    .flatten()
            })
    }

    pub(super) fn validated_pending_service_resume(
        &self,
        agent_id: &str,
        wake: Option<&crate::runtime::SchedulerWakeV1>,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
    ) -> Result<Option<lineage_persistence::PendingProviderSchedulerIntent>, String> {
        let pending = wake.and_then(|wake| {
            self.provider_scheduler_pending
                .values()
                .find(|pending| {
                    matches!(&pending.payload,
                crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed)
                if signed.request.agent_id == agent_id && matches!(&signed.request.operation,
                    crate::world_service::wire::SchedulerOperationV1::ResumeWake { wake_id, .. }
                    if wake_id == &wake.wake_id))
                })
                .cloned()
        });
        if let Some(pending) = pending.as_ref() {
            let original = pending
                .resume_context
                .as_ref()
                .ok_or("pending canonical ResumeWake lacks complete original context; fenced")?;
            original
                .request_context
                .validate_production_lane()
                .map_err(|error| {
                    format!("pending canonical ResumeWake context invalid: {error}")
                })?;
            let old = &original.request_context.runtime_binding;
            let current = &request.runtime_binding;
            if old.world_id != current.world_id
                || old.branch_id != current.branch_id
                || old.reorg_epoch != current.reorg_epoch
                || old.finality_epoch != current.finality_epoch
                || lineage_generation_recovery::provider_request_capability_identity(
                    &original.request_context,
                ) != lineage_generation_recovery::provider_request_capability_identity(request)
            {
                return Err(
                    "pending canonical ResumeWake world or delegation changed; fenced".into(),
                );
            }
        }
        Ok(pending)
    }

    pub(in crate::viewer::runtime_live) fn clear_settled_service_lease(
        &mut self,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        original: &crate::runtime::CognitionLeaseV1,
    ) -> Result<(), String> {
        let agent_id = &request.agent_subject;
        lineage_generation_recovery::validate_provider_lease_identity(agent_id, request, original)?;
        if let Some(current) = self.provider_cognition_leases.get(agent_id) {
            if current != original {
                return Err(
                    "canonical settled lease mirror identity conflict; current mirror retained"
                        .into(),
                );
            }
            self.provider_cognition_leases.remove(agent_id);
        }
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn pending_admitted_service_wait(
        &self,
    ) -> Option<(
        String,
        Option<AgentDecisionTrace>,
        Option<RuntimeProviderActionContext>,
    )> {
        self.provider_held_decisions
            .values()
            .find(|decision| {
                decision.continuation_admitted
                    && matches!(
                        decision.decision,
                        AgentDecision::Wait | AgentDecision::WaitTicks(_)
                    )
            })
            .map(|decision| {
                (
                    decision.agent_id.clone(),
                    decision.decision_trace.clone(),
                    decision.cognition.clone(),
                )
            })
    }

    pub(in crate::viewer::runtime_live) fn finish_admitted_service_wait(
        &mut self,
        agent_id: &str,
    ) -> Result<(), String> {
        let lease = self.provider_cognition_leases.remove(agent_id);
        let decision = self.provider_held_decisions.remove(agent_id);
        if let Err(error) = self.persist_provider_lineage() {
            if let Some(lease) = lease {
                self.provider_cognition_leases
                    .insert(agent_id.into(), lease);
            }
            if let Some(decision) = decision {
                self.provider_held_decisions
                    .insert(agent_id.into(), decision);
            }
            return Err(error);
        }
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn checkpoint_admitted_service_wait(
        &self,
    ) -> Result<(), String> {
        self.persist_provider_lineage()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::viewer::runtime_live) fn retry_service_wait_admission(
        &mut self,
        world: &mut RuntimeWorld,
        config: &WorldConfig,
    ) -> Result<bool, String> {
        let pending = self.provider_held_decisions.values().find_map(|decision| {
            if !matches!(
                decision.decision,
                AgentDecision::Wait | AgentDecision::WaitTicks(_)
            ) || !self.provider_active_turns.contains_key(&decision.agent_id)
            {
                return None;
            }
            let cognition = decision.cognition.as_ref()?;
            let key = format!(
                "{}:admit_wait",
                cognition.request.request_context.provider_invocation_key()
            );
            self.provider_scheduler_pending
                .contains_key(&key)
                .then(|| cognition.clone())
        });
        let Some(cognition) = pending else {
            return Ok(false);
        };
        self.sync_shadow_kernel(world, config)?;
        let mut kernel = self
            .shadow_kernel
            .take()
            .ok_or("canonical Wait recovery observation missing")?;
        let result = self.admit_provider_wait_continuation(world, &mut kernel, &cognition);
        self.shadow_kernel = Some(kernel);
        result?;
        let agent_id = &cognition.request.request_context.agent_subject;
        let held = self
            .provider_held_decisions
            .get_mut(agent_id)
            .ok_or("canonical admitted Wait original held decision missing")?;
        let original = held
            .cognition
            .as_ref()
            .ok_or("canonical admitted Wait original cognition missing")?;
        if !lineage_generation_recovery::provider_context_identity_matches(
            &original.request,
            &cognition.request,
        ) {
            return Err("canonical admitted Wait held request identity changed".into());
        }
        held.continuation_admitted = true;
        // Retain the original held request even if this checkpoint fails.
        // The next poll retries closure; canonical admission is idempotent.
        self.persist_provider_lineage()?;
        Ok(true)
    }
    #[cfg(any(test, feature = "test_tier_required"))]
    pub(in crate::viewer::runtime_live) fn make_service_test_context(
        &mut self,
        agent_id: &str,
        action: oasis7_simulator::Action,
    ) -> Result<serde_json::Value, String> {
        self.ensure_canonical_agent_durable_admission()?;
        let view = self
            .provider_service_projection
            .as_ref()
            .ok_or("test requires signed canonical view")?;
        let authority = view
            .agent_context
            .as_ref()
            .filter(|context| context.agent_id == agent_id)
            .ok_or("test requires authorized Agent capability context")?;
        let binding = view
            .runtime_binding
            .clone()
            .ok_or("test requires canonical binding")?;
        let observation = self
            .shadow_kernel
            .as_mut()
            .ok_or("test requires projection observation kernel")?
            .observe(agent_id)
            .map_err(|error| format!("{error:?}"))?;
        let session = authority
            .capability_invocation_context
            .presenter
            .session_id
            .clone()
            .ok_or("canonical provider session missing")?;
        let settings = match provider_settings_from_env()? {
            Some(settings) => settings,
            None => continuation_support::builtin_cognition_settings(agent_id)?,
        };
        let (turn_context, request_context) = build_provider_context(ProviderContextInput {
            session_id: &session,
            sequence: 1,
            agent_id,
            observation,
            settings: &settings,
            runtime_binding: binding,
            recent_event_summary: &[],
            capability_context: ProviderCapabilityContext {
                catalog: authority.capability_catalog.clone(),
                invocation: authority.capability_invocation_context.clone(),
                session_id: session.clone(),
            },
            replan_cause: None,
            continuation: None,
            memory_store: &self.provider_memory_store,
            goal_snapshot: trusted_provider_goal_snapshot(self.prompt_profiles.get(agent_id))?,
        })?;
        let mut response = crate::simulator::DecisionResponse::wait("canonical-test-provider");
        if !matches!(&action, oasis7_simulator::Action::MoveAgent { .. }) {
            return Err("deterministic canonical provider fixture requires MoveAgent".into());
        }
        response.decision = crate::simulator::ProviderDecision::Act {
            action_ref: "move_agent".into(),
            action,
        };
        let response_context = crate::simulator::ContinuousAgentResponseContextV1 {
            response_digest: crate::simulator::cognition_response_digest(&response),
            base_decision_response: response,
            context_discriminator: CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.into(),
            context_version: CONTINUOUS_AGENT_CONTEXT_VERSION,
            agent_session_id: request_context.agent_session_id.clone(),
            agent_turn_id: request_context.agent_turn_id.clone(),
            decision_request_id: request_context.decision_request_id.clone(),
            retry_seq: request_context.retry_seq,
            transport_attempt: request_context.transport_attempt,
            request_digest: request_context.request_digest.clone(),
        };
        serde_json::to_value(RuntimeProviderActionContext {
            request: ProviderContextState {
                turn_context,
                request_context,
            },
            response: response_context,
            cognition_lease: None,
            memory_write_intents: Vec::new(),
        })
        .map_err(|error| error.to_string())
    }
    pub(super) fn authority_continuation_for_wake(
        &self,
        world: &RuntimeWorld,
        wake: &crate::runtime::SchedulerWakeV1,
    ) -> Result<crate::runtime::AgentContinuation, String> {
        if !self.provider_service_required {
            return active_runtime_continuation_for_wake(world, wake);
        }
        self.provider_service_projection
            .as_ref()
            .and_then(|view| {
                view.continuations.iter().find(|continuation| {
                    continuation.continuation_id == wake.continuation_id
                        && continuation.agent_id == wake.agent_id
                })
            })
            .cloned()
            .ok_or_else(|| "authorized canonical continuation missing".into())
    }

    pub(in crate::viewer::runtime_live) fn authority_continuation_context(
        &self,
        world: &RuntimeWorld,
        id: &str,
    ) -> Result<crate::runtime::CognitionContextDigestsV1, String> {
        if !self.provider_service_required {
            return runtime_context_digests_for_continuation(world, id);
        }
        let context = self
            .provider_service_projection
            .as_ref()
            .and_then(|view| view.continuation_contexts.get(id))
            .ok_or("authorized canonical continuation context missing")?;
        Ok(crate::runtime::CognitionContextDigestsV1 {
            baseline_observation_digest: context.baseline_observation_digest.clone(),
            goal_digest: context.goal_digest.clone(),
            policy_digest: context.policy_digest.clone(),
            precondition_digest: context.precondition_digest.clone(),
        })
    }

    pub(in crate::viewer::runtime_live) fn committed_service_wake_handoff(
        &mut self,
        cognition: &RuntimeProviderActionContext,
    ) -> Result<
        Option<(
            crate::runtime::SchedulerWakeV1,
            crate::world_service::wire::SchedulerOperationV1,
        )>,
        String,
    > {
        let request = &cognition.request.request_context;
        let binding = &request.runtime_binding;
        let projection = self
            .provider_service_projection
            .as_ref()
            .ok_or("canonical wake projection missing")?;
        let current_binding = projection
            .runtime_binding
            .as_ref()
            .ok_or("canonical wake projection runtime binding missing; fenced")?;
        if !lineage_persistence::runtime_binding_advances_same_lineage(binding, current_binding) {
            return Err(
                "canonical wake projection/runtime binding changed lineage or regressed; fenced"
                    .into(),
            );
        }
        let wake_matches_request = |wake: &&crate::runtime::SchedulerWakeV1| {
            lineage_persistence::provider_context_matches_wake(&cognition.request, wake)
                && wake.world_id == binding.world_id
                && wake.branch_id == binding.branch_id
                && wake.finality_epoch == binding.finality_epoch
                && wake.finality_block_hash
                    == binding
                        .finality_block_hash
                        .as_ref()
                        .map(ToString::to_string)
                && wake.finality_status == binding.finality_status
                && wake.reorg_epoch == binding.reorg_epoch
                && wake.runtime_manifest_hash == binding.runtime_manifest_hash.to_string()
        };
        let projection_wakes = projection
            .scheduler_wakes
            .iter()
            .filter(wake_matches_request)
            .cloned()
            .collect::<Vec<_>>();
        let mirrored_wakes = self
            .pending_runtime_wakes
            .values()
            .filter(wake_matches_request)
            .cloned()
            .collect::<Vec<_>>();
        let mut wakes: Vec<crate::runtime::SchedulerWakeV1> = Vec::new();
        for wake in projection_wakes.iter().chain(mirrored_wakes.iter()) {
            if let Some(existing) = wakes
                .iter()
                .find(|existing| existing.wake_id == wake.wake_id)
            {
                if existing != wake {
                    return Err("canonical wake projection and recovery identity conflict".into());
                }
            } else {
                wakes.push(wake.clone());
            }
        }
        if wakes.is_empty() {
            let checkpoint_prefix = format!("{}:handoff:", request.provider_invocation_key());
            if self
                .provider_scheduler_pending
                .keys()
                .any(|key| key.starts_with(&checkpoint_prefix))
            {
                return Err(
                    "canonical handoff checkpoint lacks its exact wake identity; fenced".into(),
                );
            }
            return Ok(None);
        }
        if wakes.len() != 1 {
            return Err("canonical wake projection is ambiguous for cognition".into());
        }
        self.ensure_canonical_agent_durable_admission()?;
        let wake = &wakes[0];

        let checkpoint_id = format!(
            "{}:handoff:{}",
            request.provider_invocation_key(),
            wake.wake_id
        );
        if let Some(checkpoint) = self.provider_scheduler_pending.get(&checkpoint_id) {
            let expected_world = self
                .provider_service_config
                .as_ref()
                .ok_or("canonical provider service missing")?
                .expected_world
                .clone();
            Self::validate_scheduler_checkpoint_integrity(
                &expected_world,
                &checkpoint_id,
                checkpoint,
            )?;
            let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
                &checkpoint.payload
            else {
                return Err("canonical wake handoff checkpoint codec mismatch".into());
            };
            let crate::world_service::wire::SchedulerOperationV1::HandoffWake {
                wake_id: saved_wake_id,
                disposition: crate::runtime::CognitionWakeDispositionV1::Terminal { status, reason },
                ..
            } = &signed.request.operation
            else {
                return Err("canonical wake handoff checkpoint operation mismatch".into());
            };
            let signer = self
                .provider_service_signer
                .as_ref()
                .ok_or("explicit canonical Agent signer missing")?;
            let signer_key: [u8; 32] = hex::decode(&signer.private_key_hex)
                .map_err(|_| "invalid canonical Agent signer key")?
                .try_into()
                .map_err(|_| "invalid canonical Agent signer key")?;
            let signer_public_key = hex::encode(
                ed25519_dalek::SigningKey::from_bytes(&signer_key)
                    .verifying_key()
                    .to_bytes(),
            );
            if saved_wake_id != &wake.wake_id
                || *status != crate::runtime::ContinuationStatusV1::Completed
                || reason != "provider_action_committed"
                || signed.request.agent_id != request.agent_subject
                || signed.subject_public_key != signer_public_key
                || !lineage_persistence::handoff_checkpoint_matches_verified_binding(
                    binding,
                    wake,
                    &signed.request.captured_base_binding,
                    current_binding,
                )
                || signed.request.delegation_generation != signer.delegation_generation
            {
                return Err("canonical wake handoff checkpoint identity conflict; fenced".into());
            }
            return Ok(Some((wake.clone(), signed.request.operation.clone())));
        }

        if !projection_wakes
            .iter()
            .any(|projected| projected.wake_id == wake.wake_id)
        {
            return Err(
                "durable cognition wake is absent from verified projection without its signed handoff checkpoint; fenced"
                    .into(),
            );
        }

        let observation = self
            .shadow_kernel
            .as_mut()
            .ok_or("canonical observation kernel missing")?
            .observe(&request.agent_subject)
            .map_err(|error| format!("{error:?}"))?;
        let current = crate::simulator::ContinuationCurrentContextV1::from_observation(
            observation.clone(),
            &cognition.request.turn_context.goal_snapshot,
            provider_policy_context_digest(request),
            provider_wait_precondition_digest(&observation),
        );
        Ok(Some((
            wake.clone(),
            crate::world_service::wire::SchedulerOperationV1::HandoffWake {
                wake_id: wake.wake_id.clone(),
                disposition: crate::runtime::CognitionWakeDispositionV1::Terminal {
                    status: crate::runtime::ContinuationStatusV1::Completed,
                    reason: "provider_action_committed".into(),
                },
                current_context: crate::runtime::CognitionContextDigestsV1 {
                    baseline_observation_digest: current.authority.baseline_observation_digest,
                    goal_digest: current.authority.goal_digest,
                    policy_digest: current.authority.policy_digest,
                    precondition_digest: current.authority.precondition_digest,
                },
            },
        )))
    }

    pub(in crate::viewer::runtime_live) fn complete_committed_service_wake_handoff(
        &mut self,
        cognition: &RuntimeProviderActionContext,
        wake_id: &str,
    ) -> Result<(), String> {
        let request = &cognition.request.request_context;
        self.ensure_canonical_agent_durable_admission()?;
        let checkpoint_id = format!("{}:handoff:{wake_id}", request.provider_invocation_key());
        let checkpoint = self
            .provider_scheduler_pending
            .get(&checkpoint_id)
            .ok_or("canonical handoff checkpoint missing before durable cleanup; fenced")?;
        let original_correlation = checkpoint.correlation.clone();
        let expected_world = self
            .provider_service_config
            .as_ref()
            .ok_or("canonical provider service missing")?
            .expected_world
            .clone();
        Self::validate_scheduler_checkpoint_integrity(&expected_world, &checkpoint_id, checkpoint)?;
        let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
            &checkpoint.payload
        else {
            return Err("canonical handoff checkpoint codec mismatch".into());
        };
        let crate::world_service::wire::SchedulerOperationV1::HandoffWake {
            wake_id: saved_wake_id,
            disposition: crate::runtime::CognitionWakeDispositionV1::Terminal { status, reason },
            ..
        } = &signed.request.operation
        else {
            return Err("canonical handoff checkpoint operation mismatch".into());
        };
        let pending_wake = self.pending_runtime_wakes.get(wake_id);
        let current_binding = self
            .provider_service_projection
            .as_ref()
            .and_then(|projection| projection.runtime_binding.as_ref())
            .ok_or("canonical handoff cleanup lacks verified runtime binding; fenced")?;
        let signer = self
            .provider_service_signer
            .as_ref()
            .ok_or("explicit canonical Agent signer missing")?;
        let signer_key: [u8; 32] = hex::decode(&signer.private_key_hex)
            .map_err(|_| "invalid canonical Agent signer key")?
            .try_into()
            .map_err(|_| "invalid canonical Agent signer key")?;
        let signer_public_key = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&signer_key)
                .verifying_key()
                .to_bytes(),
        );
        if saved_wake_id != wake_id
            || *status != crate::runtime::ContinuationStatusV1::Completed
            || reason != "provider_action_committed"
            || signed.request.agent_id != request.agent_subject
            || signed.subject_public_key != signer_public_key
            || !pending_wake.is_some_and(|wake| {
                lineage_persistence::handoff_checkpoint_matches_verified_binding(
                    &request.runtime_binding,
                    wake,
                    &signed.request.captured_base_binding,
                    current_binding,
                )
            })
            || signed.request.delegation_generation != signer.delegation_generation
        {
            return Err(
                "canonical handoff checkpoint identity conflict before cleanup; fenced".into(),
            );
        }
        let operation = signed.request.operation.clone();
        let (checked_checkpoint, existed) = self.prepare_service_scheduler_checkpoint(
            request,
            &format!("handoff:{wake_id}"),
            operation,
            None,
        )?;
        if !existed || checked_checkpoint.correlation != original_correlation {
            return Err(
                "canonical handoff checkpoint changed before durable cleanup; fenced".into(),
            );
        }

        let previous_wake = self.pending_runtime_wakes.remove(wake_id);
        let previous_checkpoint = self.provider_scheduler_pending.remove(&checkpoint_id);
        if let Err(error) = self.persist_provider_lineage() {
            if let Some(wake) = previous_wake {
                self.pending_runtime_wakes.insert(wake_id.to_string(), wake);
            }
            if let Some(checkpoint) = previous_checkpoint {
                self.provider_scheduler_pending
                    .insert(checkpoint_id, checkpoint);
            }
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "llm_sidecar_service_wake_handoff_tests.rs"]
mod service_wake_handoff_tests;

#[cfg(test)]
mod service_wait_checkpoint_tests {
    use super::*;

    #[test]
    fn admitted_wait_is_retained_when_cleanup_checkpoint_fails() {
        let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
        let decision = async_support::RuntimeLlmDecision {
            agent_id: "agent-a".into(),
            decision: AgentDecision::WaitTicks(2),
            decision_trace: None,
            cognition: None,
            memory_write_intents: vec![],
            continuation_admitted: true,
        };
        sidecar
            .provider_held_decisions
            .insert("agent-a".into(), decision);
        let directory = std::env::temp_dir().join(format!(
            "oasis7-wait-checkpoint-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).expect("checkpoint test directory");
        let blocker = directory.join("file-blocker");
        std::fs::write(&blocker, b"blocker").expect("checkpoint path blocker");
        sidecar.provider_lineage_store = Some(blocker.join("checkpoint.json"));
        assert!(sidecar.finish_admitted_service_wait("agent-a").is_err());
        assert!(sidecar.pending_admitted_service_wait().is_some());
        assert!(matches!(
            sidecar.provider_held_decisions["agent-a"].decision,
            AgentDecision::WaitTicks(2)
        ));
        sidecar.provider_lineage_store = None;
        sidecar.finish_admitted_service_wait("agent-a").unwrap();
        assert!(sidecar.pending_admitted_service_wait().is_none());
        std::fs::remove_dir_all(directory).expect("checkpoint test cleanup");
    }
}
