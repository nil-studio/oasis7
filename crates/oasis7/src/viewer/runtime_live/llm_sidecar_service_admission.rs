//! Durable fresh admission. No transport or actor callback is performed by apply.
use super::*;
use crate::viewer::runtime_live::agent_service_io::*;
use oasis7_client_api::world_service::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::viewer::runtime_live) struct HostedAdmission {
    context: ProviderContextState,
    observation: Observation,
    metadata_identity: String,
    stage: String,
    commit: Option<CommitRef>,
    lease: Option<crate::runtime::CognitionLeaseV1>,
}
impl RuntimeLlmSidecar {
    /// A canonical rejection of an exact Reserve is pre-effect. Retire only
    /// this prepared turn; never repair its signature or reuse its correlation.
    fn retire_rejected_reserve(&mut self, response: &IntentResponse<Value>) -> Result<(), String> {
        use crate::world_service::wire::{SchedulerOperationV1, WorldServicePayloadV1};
        let admission = self
            .hosted_admission
            .clone()
            .ok_or("rejected Reserve admission missing")?;
        let request = &admission.context.request_context;
        let agent = request.agent_subject.as_str();
        request.validate().map_err(|error| error.to_string())?;
        Self::validate_resumed_turn_identity(&admission.context)?;
        if admission.context.turn_context.agent_id != request.agent_subject
            || admission.context.turn_context.agent_session_id != request.agent_session_id
            || request.continuation_digest
                != crate::simulator::h_v1(
                    "oasis7.cognition.continuation.v1",
                    &serde_json::Value::Null,
                )
        {
            return Err(
                "rejected Reserve is not an authenticated fresh turn; original retained".into(),
            );
        }
        if admission.stage != "reserve"
            || admission.context.turn_context.continuation.is_some()
            || admission.lease.is_some()
            || admission.commit.is_some()
            || self.provider_active_turns.contains_key(agent)
            || self.provider_recovery_pending.contains_key(agent)
            || self.provider_held_decisions.contains_key(agent)
            || self.provider_cognition_leases.contains_key(agent)
            || !self.provider_service_pending.is_empty()
        {
            return Err("rejected Reserve is not proven pre-effect; original retained".into());
        }
        if self.provider_lineage_store.is_none() {
            return Err("rejected Reserve retirement requires durable private lineage".into());
        }
        let id = format!("{}:reserve", request.provider_invocation_key());
        let pending = self
            .provider_scheduler_pending
            .get(&id)
            .ok_or("rejected Reserve original checkpoint missing")?;
        let config = self
            .provider_service_config
            .as_ref()
            .ok_or("rejected Reserve configuration missing")?;
        Self::validate_scheduler_checkpoint_integrity(&config.expected_world, &id, pending)?;
        response
            .validate(&pending.correlation)
            .map_err(|error| error.to_string())?;
        let IntentOutcome::Rejected { reason } = &response.outcome else {
            return Err("Reserve retirement requires authenticated canonical Rejected".into());
        };
        let WorldServicePayloadV1::Scheduler(signed) = &pending.payload else {
            unreachable!()
        };
        let SchedulerOperationV1::ReserveLease(lease) = &signed.request.operation else {
            return Err("rejected Reserve original operation mismatch".into());
        };
        let signer = self
            .provider_service_signer
            .as_ref()
            .ok_or("rejected Reserve Agent signer missing")?;
        let public =
            crate::world_service::sign_read_request("signer", (), &signer.private_key_hex)?
                .subject_public_key;
        if signed.subject_public_key != public
            || signed.request.delegation_generation != signer.delegation_generation
            || signed.request.agent_id != agent
            || lease.agent_id != agent
            || lease.agent_session_id != request.agent_session_id
            || lease.agent_turn_id != request.agent_turn_id
            || lease.decision_request_id != request.decision_request_id
            || lease.request_digest != request.request_digest.to_string()
            || self.provider_scheduler_pending.keys().any(|key| {
                key.starts_with(&format!("{}:", request.provider_invocation_key())) && key != &id
            })
        {
            return Err("rejected Reserve original identity or effects conflict".into());
        }
        let old_terminal = self.provider_terminal_states.get(agent).cloned();
        let old_ready = self.hosted_fresh_view_ready;
        self.provider_terminal_states.insert(
            agent.into(),
            lineage_persistence::ProviderTerminalState {
                agent_id: agent.into(),
                agent_session_id: request.agent_session_id.clone(),
                agent_turn_id: request.agent_turn_id.clone(),
                decision_request_id: request.decision_request_id.clone(),
                request_digest: request.request_digest.to_string(),
                status: "rejected".into(),
                reject_reason: Some(format!(
                    "canonical Reserve rejected: {reason:?}; original_key={}",
                    pending.correlation.key.request_id_or_nonce
                )),
                feedback_id: None,
                feedback: None,
            },
        );
        self.hosted_admission = None;
        self.hosted_fresh_view_ready = false;
        if let Err(error) = self.persist_provider_lineage() {
            self.hosted_admission = Some(admission.clone());
            self.hosted_fresh_view_ready = old_ready;
            if let Some(terminal) = old_terminal {
                self.provider_terminal_states.insert(agent.into(), terminal);
            } else {
                self.provider_terminal_states.remove(agent);
            }
            return Err(error);
        }
        Ok(())
    }
    fn check_scoped_feedback_fresh_admission(
        &mut self,
        agent: &str,
        session: &str,
    ) -> Result<(), String> {
        let view = self
            .provider_service_projection
            .as_ref()
            .ok_or("authenticated feedback View missing")?;
        let authority = view
            .agent_context
            .as_ref()
            .ok_or("feedback Agent authority missing")?;
        let canonical_session = authority
            .capability_invocation_context
            .presenter
            .session_id
            .as_deref()
            .ok_or("feedback View canonical session missing")?;
        if authority.agent_id != agent {
            return Err("feedback View Agent/session authority mismatch".into());
        }
        if canonical_session != session {
            self.validate_resumed_feedback_session(agent, session, canonical_session)?;
        }
        let history = view
            .feedback_history
            .as_ref()
            .ok_or("authenticated feedback history missing")?;
        // A resumed partition cannot evade an outstanding fence in its canonical
        // source session. Both checks use the authenticated scoped history.
        crate::world_service_feedback_host::check_fresh_session(history, agent, canonical_session)?;
        crate::world_service_feedback_host::check_fresh_session(history, agent, session)?;
        if let Some(runner) = self
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
        {
            crate::world_service_feedback_host::restore_preverified(history, agent, runner)?;
            if runner.feedback_recovery_blocked(agent, canonical_session)
                || runner.feedback_recovery_blocked(agent, session)
            {
                return Err("live feedback verifier conflicts with canonical history".into());
            }
        }
        Ok(())
    }

    fn validate_resumed_turn_identity(context: &ProviderContextState) -> Result<(), String> {
        let request = &context.request_context;
        let turn = &context.turn_context;
        if turn.request_digest != request.request_digest
            || turn.agent_turn_id != request.agent_turn_id
            || turn.decision_request_id != request.decision_request_id
        {
            return Err("resumed feedback prepared turn/request identity mismatch".into());
        }
        Ok(())
    }

    /// A Resume has a new model session, while the canonical capability
    /// presenter keeps its original session. Authenticate the durable handoff,
    /// rather than treating a caller-selected suffix as a new authority.
    fn validate_resumed_feedback_session(
        &self,
        agent: &str,
        session: &str,
        canonical_session: &str,
    ) -> Result<(), String> {
        use crate::world_service::wire::{SchedulerOperationV1, WorldServicePayloadV1};
        let context = &self
            .hosted_admission
            .as_ref()
            .ok_or("derived feedback session lacks original admission")?
            .context;
        Self::validate_resumed_turn_identity(context)?;
        let request = &context.request_context;
        request
            .validate_production_lane()
            .map_err(|e| e.to_string())?;
        context
            .turn_context
            .validate_for_agent(agent)
            .map_err(|e| e.to_string())?;
        if request.agent_subject != agent
            || request.agent_session_id != session
            || context.turn_context.agent_session_id != session
            || request
                .base_decision_request
                .capability_invocation_context
                .as_ref()
                .and_then(|invocation| invocation.presenter.session_id.as_deref())
                != Some(canonical_session)
        {
            return Err("resumed feedback original capability/session mismatch".into());
        }
        let signer = self
            .provider_service_signer
            .as_ref()
            .ok_or("resumed feedback signer missing")?;
        let key: [u8; 32] = hex::decode(&signer.private_key_hex)
            .map_err(|_| "resumed feedback signer invalid")?
            .try_into()
            .map_err(|_| "resumed feedback signer invalid")?;
        let public_key = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&key)
                .verifying_key()
                .to_bytes(),
        );
        let view = self
            .provider_service_projection
            .as_ref()
            .ok_or("resumed feedback view missing")?;
        for (id, pending) in &self.provider_scheduler_pending {
            let WorldServicePayloadV1::Scheduler(signed) = &pending.payload else {
                continue;
            };
            let SchedulerOperationV1::ResumeWake {
                resume,
                proposal,
                wake_id,
                ..
            } = &signed.request.operation
            else {
                continue;
            };
            if pending.resume_context.as_ref().is_none_or(|original| {
                serde_json::to_value(original).ok() != serde_json::to_value(context).ok()
            }) {
                continue;
            }
            crate::world_service::verify_read_request("scheduler", signed)?;
            proposal.validate().map_err(|e| e.to_string())?;
            let expected_world = &self
                .provider_service_config
                .as_ref()
                .ok_or("resumed feedback service configuration missing")?
                .expected_world;
            if pending.correlation.key.world != *expected_world
                || signed.subject_public_key != public_key
                || signed.request.delegation_generation != signer.delegation_generation
                || signed.request.agent_id != agent
                || signed.request.request_id != *id
                || *id != format!("{}:resume:{wake_id}", request.provider_invocation_key())
                || crate::world_service::derive_correlation(
                    pending.correlation.key.world.clone(),
                    &pending.payload,
                )? != pending.correlation
                || resume.agent_session_id != session
                || resume.agent_turn_id != request.agent_turn_id
                || resume.decision_request_id != request.decision_request_id
                || resume.request_digest != request.request_digest.to_string()
                || resume.context_digest != async_support::runtime_provider_context_digest(request)
            {
                return Err("resumed feedback signed handoff identity mismatch".into());
            }
            let actual = view
                .continuations
                .iter()
                .find(|actual| actual.continuation_proposal_id == proposal.continuation_proposal_id)
                .ok_or("resumed feedback canonical successor missing")?;
            if actual.world_id != proposal.world_id
                || actual.world_id != expected_world.world_id
                || actual.branch_id != proposal.branch_id
                || actual.finality_epoch != proposal.finality_epoch
                || actual.finality_block_hash != proposal.finality_block_hash
                || actual.finality_status != proposal.finality_status
                || actual.reorg_epoch != proposal.reorg_epoch
                || actual.runtime_manifest_hash != proposal.runtime_manifest_hash
                || actual.origin_turn_id != proposal.origin_turn_id
                || actual.precondition_digest != proposal.precondition_digest
                || actual.agent_id != agent
                || actual.agent_session_id != session
                || actual.agent_turn_id != request.agent_turn_id
                || actual.decision_request_id != request.decision_request_id
                || actual.origin_request_digest != proposal.origin_request_digest
                || actual.proposal_digest != proposal.proposal_digest
            {
                return Err("resumed feedback canonical successor identity mismatch".into());
            }
            return Ok(());
        }
        Err("derived feedback session lacks authenticated Resume handoff".into())
    }

    pub(in crate::viewer::runtime_live) fn install_resumed_hosted_admission(
        &mut self,
        context: ProviderContextState,
        observation: Observation,
        metadata_identity: String,
    ) -> Result<(), String> {
        if self.hosted_admission.is_some() {
            return Err("resumed admission conflicts with existing original".into());
        }
        self.hosted_admission = Some(HostedAdmission {
            context,
            observation,
            metadata_identity,
            stage: "reserve".into(),
            commit: None,
            lease: None,
        });
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn restore_hosted_admission_original(
        &mut self,
    ) -> Result<(), String> {
        let Some(mut admission) = self.hosted_admission.clone() else {
            return Ok(());
        };
        let request = &admission.context.request_context;
        request
            .validate_production_lane()
            .map_err(|error| error.to_string())?;
        admission
            .context
            .turn_context
            .validate_for_agent(&request.agent_subject)
            .map_err(|error| error.to_string())?;
        if admission.context.turn_context.request_digest != request.request_digest
            || admission.observation.agent_id != request.agent_subject
            || admission.observation.time != request.runtime_binding.base_tick
        {
            return Err("restored fresh admission context identity mismatch".into());
        }
        if !matches!(
            admission.stage.as_str(),
            "reserve" | "reserve_view" | "prefix" | "prefix_view" | "dispatch"
        ) {
            return Err("restored fresh admission stage invalid".into());
        }
        // A private phase label is not canonical completion proof. Reconcile
        // the original signed Reserve and Prefix through Lookup after restart.
        admission.stage = "reserve".into();
        admission.commit = None;
        self.hosted_admission = Some(admission);
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn capture_hosted_admission(
        &mut self,
    ) -> Result<(), String> {
        self.ensure_canonical_agent_durable_admission()?;
        let view = self
            .provider_service_projection
            .clone()
            .ok_or("fresh signed view missing")?;
        let authority = view
            .agent_context
            .as_ref()
            .ok_or("fresh Agent authority missing")?;
        let agent = authority.agent_id.clone();
        if self.provider_active_turns.contains_key(&agent) {
            return Ok(());
        }
        let binding = view
            .runtime_binding
            .clone()
            .ok_or("fresh canonical binding missing")?;
        let session = authority
            .capability_invocation_context
            .presenter
            .session_id
            .clone()
            .ok_or("fresh canonical session missing")?;
        self.check_scoped_feedback_fresh_admission(&agent, &session)?;
        let metadata_identity = self
            .fresh_provider_metadata_identity()?
            .ok_or("fresh provider metadata is not ready")?;
        let settings = provider_settings_from_env()?
            .ok_or("fresh service admission requires configured provider")?;
        let capability = ProviderCapabilityContext {
            catalog: authority.capability_catalog.clone(),
            invocation: authority.capability_invocation_context.clone(),
            session_id: session.clone(),
        };
        let observation = self
            .shadow_kernel
            .as_mut()
            .ok_or("fresh observation kernel missing")?
            .observe(&agent)
            .map_err(|error| format!("{error:?}"))?;
        let sequence = self
            .provider_context_seq
            .get(&agent)
            .copied()
            .unwrap_or(1)
            .max(1);
        let (turn_context, request_context) = build_provider_context(ProviderContextInput {
            session_id: &session,
            sequence,
            agent_id: &agent,
            observation: observation.clone(),
            settings: &settings,
            runtime_binding: binding,
            recent_event_summary: &[],
            capability_context: capability,
            replan_cause: None,
            continuation: None,
            memory_store: &self.provider_memory_store,
            goal_snapshot: trusted_provider_goal_snapshot(self.prompt_profiles.get(&agent))?,
        })?;
        self.hosted_admission = Some(HostedAdmission {
            context: ProviderContextState {
                turn_context,
                request_context,
            },
            observation,
            metadata_identity,
            stage: "reserve".into(),
            commit: None,
            lease: None,
        });
        self.provider_context_seq
            .insert(agent, sequence.saturating_add(1));
        self.persist_provider_lineage()?;
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn hosted_admission_job(
        &mut self,
        world: &RuntimeWorld,
    ) -> Result<Option<AgentServiceIoOperation>, String> {
        let Some(admission) = self.hosted_admission.clone() else {
            return Ok(None);
        };
        self.persist_provider_lineage()?;
        let request = &admission.context.request_context;
        request
            .validate_production_lane()
            .map_err(|error| error.to_string())?;
        admission
            .context
            .turn_context
            .validate_for_agent(&request.agent_subject)
            .map_err(|error| error.to_string())?;
        if admission.stage.ends_with("_view") {
            let config = self
                .provider_service_config
                .as_ref()
                .ok_or("canonical configuration missing")?;
            return Ok(Some(AgentServiceIoOperation::View(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: config.expected_world.clone(),
                scope_id: config.scope_id.clone(),
                min_commit: admission.commit,
                fixed_commit: None,
                deadline_unix_ms: None,
            })));
        }
        if admission.stage == "dispatch" {
            return Ok(None);
        };
        let (phase, operation) = if admission.stage == "reserve" {
            (
                "reserve".into(),
                crate::world_service::wire::SchedulerOperationV1::ReserveLease(
                    async_support::provider_cognition_lease_request(
                        world,
                        &admission.context,
                        true,
                    )?,
                ),
            )
        } else if admission.stage == "prefix" {
            (
                format!("prefix:{}", request.transport_attempt),
                crate::world_service::wire::SchedulerOperationV1::ProviderPrefix {
                    request: crate::simulator::agent_api_compat::request_context_to_api(request),
                    context_digest: async_support::runtime_provider_context_digest(request),
                },
            )
        } else {
            return Err("unknown durable admission stage".into());
        };
        let (pending, existed) =
            self.prepare_service_scheduler_checkpoint(request, &phase, operation, None)?;
        Ok(Some(if existed {
            AgentServiceIoOperation::Lookup {
                request: LookupIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    key: pending.correlation.key,
                },
                original: pending.payload,
            }
        } else {
            AgentServiceIoOperation::Submit(SubmitIntentRequest {
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                correlation: pending.correlation,
                deadline_unix_ms: None,
                signed_payload: pending.payload,
            })
        }))
    }
    pub(in crate::viewer::runtime_live) fn dispatch_hosted_admission(
        &mut self,
    ) -> Result<bool, String> {
        let Some(admission) = self
            .hosted_admission
            .clone()
            .filter(|admission| admission.stage == "dispatch")
        else {
            return Ok(false);
        };
        let context = admission.context;
        let agent = context.request_context.agent_subject.clone();
        self.check_scoped_feedback_fresh_admission(
            &agent,
            &context.request_context.agent_session_id,
        )?;
        let Some(identity) = self.fresh_provider_metadata_identity()? else {
            return Ok(false);
        };
        if identity != admission.metadata_identity {
            return Err("fresh provider configuration changed; original admission fenced".into());
        };
        let lease = admission.lease.ok_or("fresh reserved lease missing")?;
        lineage_generation_recovery::validate_provider_lease_identity(
            &agent,
            &context.request_context,
            &lease,
        )?;
        if self.provider_active_turns.contains_key(&agent) {
            return Err("fresh active marker requires recovery; redispatch fenced".into());
        };
        self.provider_contexts
            .insert(agent.clone(), context.clone());
        self.provider_active_turns
            .insert(agent.clone(), context.clone());
        self.bind_provider_cognition_lease(agent.clone(), lease.clone());
        self.persist_provider_lineage()?;
        let observation = admission.observation;
        let runner = self
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .ok_or("fresh native runner missing")?;
        runner.sync_logical_tick(context.request_context.runtime_binding.base_tick);
        runner
            .start_turn_with_request_context_and_observation_and_lease(
                &agent,
                observation,
                context.turn_context,
                context.request_context,
                lease,
            )
            .map_err(|error| format!("{error:?}"))?;
        self.hosted_admission = None;
        self.persist_provider_lineage()?;
        Ok(true)
    }
}
impl crate::viewer::ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn prepare_fresh_service_io(
        &mut self,
        eligible: bool,
    ) -> Result<AgentServiceProgress, String> {
        if self.config.world_service.is_none() || self.llm_sidecar.hosted_service_inflight.is_some()
        {
            return Ok(AgentServiceProgress::Idle);
        }
        if self.llm_sidecar.hosted_admission.is_none() {
            if !eligible
                || self
                    .llm_sidecar
                    .fresh_provider_metadata_identity()?
                    .is_none()
            {
                return Ok(AgentServiceProgress::Idle);
            }
            if !self.llm_sidecar.provider_active_turns.is_empty()
                || !self.llm_sidecar.pending_runtime_wakes.is_empty()
                || !self.llm_sidecar.provider_continuation_proposals.is_empty()
                || !self.llm_sidecar.provider_recovery_pending.is_empty()
                || !self
                    .llm_sidecar
                    .provider_continuation_recovery_pending
                    .is_empty()
                || !self.llm_sidecar.provider_retry_contexts.is_empty()
                || self
                    .llm_sidecar
                    .provider_service_projection
                    .as_ref()
                    .is_some_and(|view| {
                        self.llm_sidecar
                            .provider_wait_until
                            .values()
                            .any(|until| *until > view.state.time)
                    })
            {
                return Ok(AgentServiceProgress::Idle);
            }
            if !self.llm_sidecar.hosted_fresh_view_ready && !self.should_advance_auto_play_step() {
                return Ok(AgentServiceProgress::Idle);
            }
            self.config.ensure_service_agent_lineage_store()?;
            if self.llm_sidecar.hosted_fresh_view_ready {
                self.llm_sidecar.capture_hosted_admission()?;
                self.llm_sidecar.hosted_fresh_view_ready = false;
            }
        } else if self
            .llm_sidecar
            .hosted_admission
            .as_ref()
            .is_some_and(|admission| admission.stage == "dispatch")
        {
            if !eligible {
                return Ok(AgentServiceProgress::Idle);
            }
            return Ok(if self.llm_sidecar.dispatch_hosted_admission()? {
                AgentServiceProgress::Advanced
            } else {
                AgentServiceProgress::Idle
            });
        }
        if let Some(admission) = self.llm_sidecar.hosted_admission.clone()
            && matches!(admission.stage.as_str(), "reserve" | "prefix")
        {
            let request = &admission.context.request_context;
            let phase = if admission.stage == "reserve" {
                "reserve".into()
            } else {
                format!("prefix:{}", request.transport_attempt)
            };
            let issued = self
                .llm_sidecar
                .provider_scheduler_pending
                .contains_key(&format!("{}:{phase}", request.provider_invocation_key()));
            if !issued {
                self.llm_sidecar.check_scoped_feedback_fresh_admission(
                    &request.agent_subject,
                    &request.agent_session_id,
                )?;
                if !eligible {
                    return Ok(AgentServiceProgress::Idle);
                }
                let Some(identity) = self.llm_sidecar.fresh_provider_metadata_identity()? else {
                    return Ok(AgentServiceProgress::Idle);
                };
                if identity != admission.metadata_identity {
                    return Err(
                        "fresh provider configuration changed; original admission fenced".into(),
                    );
                }
            }
        }
        let config_digest = self.hosted_service_config_digest()?;
        if self
            .llm_sidecar
            .hosted_service_config_binding
            .as_ref()
            .is_some_and(|original| original != &config_digest)
        {
            return Err("fresh service original configuration changed; fenced".into());
        }
        let client = self
            .world_service_client()
            .map_err(|error| format!("{error:?}"))?
            .ok_or("canonical service missing")?;
        let operation = if self.llm_sidecar.hosted_admission.is_some() {
            self.llm_sidecar
                .hosted_admission_job(&self.world)?
                .ok_or("fresh admission stage has no operation")?
        } else {
            AgentServiceIoOperation::View(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: client.config().expected_world.clone(),
                scope_id: client.config().scope_id.clone(),
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            })
        };
        let phase_digest = format!(
            "admission:{}",
            self.llm_sidecar
                .hosted_admission
                .as_ref()
                .map(|admission| admission.stage.as_str())
                .unwrap_or("view")
        );
        self.llm_sidecar.hosted_service_generation =
            self.llm_sidecar.hosted_service_generation.saturating_add(1);
        let token = AgentServiceIoToken {
            generation: self.llm_sidecar.hosted_service_generation,
            config_digest: config_digest.clone(),
            phase_digest,
        };
        self.llm_sidecar.hosted_service_inflight = Some(token.clone());
        self.llm_sidecar
            .hosted_service_config_binding
            .get_or_insert(config_digest);
        Ok(AgentServiceProgress::NeedsIo(Box::new(AgentServiceIoJob {
            token,
            client: Some(client),
            operation,
        })))
    }
    pub(in crate::viewer::runtime_live) fn apply_fresh_service_io(
        &mut self,
        result: AgentServiceIoResult,
    ) -> Result<AgentServiceProgress, String> {
        if self.llm_sidecar.hosted_service_inflight.as_ref() != Some(&result.token) {
            return Err("fresh admission completion token mismatch".into());
        }
        self.llm_sidecar.hosted_service_inflight = None;
        if result.token.config_digest != self.hosted_service_config_digest()? {
            return Err("fresh service configuration changed; fenced".into());
        }
        let stage = self
            .llm_sidecar
            .hosted_admission
            .as_ref()
            .map(|admission| admission.stage.clone())
            .unwrap_or("view".into());
        if result.token.phase_digest != format!("admission:{stage}") {
            return Err("fresh admission phase mismatch".into());
        }
        let response = result.response?;
        if stage == "view" || stage.ends_with("_view") {
            let AgentServiceIoResponse::View(view) = response else {
                return Err("fresh admission expected verified view".into());
            };
            if let Some(commit) = self
                .llm_sidecar
                .hosted_admission
                .as_ref()
                .and_then(|admission| admission.commit.as_ref())
                && !view
                    .version()
                    .commit
                    .satisfies_minimum(commit)
                    .map_err(|error| error.to_string())?
            {
                return Err("fresh admission view precedes original commit".into());
            }
            self.apply_hosted_verified_view(*view)?;
            if stage == "view" {
                // Context observation belongs to a later eligible prepare, never an apply callback.
                self.llm_sidecar.hosted_fresh_view_ready = true;
            } else {
                let admission = self
                    .llm_sidecar
                    .hosted_admission
                    .as_mut()
                    .ok_or("fresh admission missing")?;
                admission.stage = if stage == "reserve_view" {
                    "prefix"
                } else {
                    "dispatch"
                }
                .into();
                admission.commit = None;
                self.llm_sidecar.persist_provider_lineage()?;
            }
            return Ok(AgentServiceProgress::Advanced);
        }
        let Some(response) =
            crate::viewer::runtime_live::agent_service_phase::original_response(response)?
        else {
            return Ok(AgentServiceProgress::Advanced);
        };
        let admission = self
            .llm_sidecar
            .hosted_admission
            .clone()
            .ok_or("original admission missing")?;
        let request = &admission.context.request_context;
        let phase = if stage == "reserve" {
            "reserve".into()
        } else {
            format!("prefix:{}", request.transport_attempt)
        };
        let checkpoint = self
            .llm_sidecar
            .provider_scheduler_pending
            .get(&format!("{}:{phase}", request.provider_invocation_key()))
            .ok_or("original admission checkpoint missing")?;
        response
            .validate(&checkpoint.correlation)
            .map_err(|error| error.to_string())?;
        match response.outcome {
            IntentOutcome::Committed { commit, receipt } => {
                let lease = if stage == "reserve" {
                    let lease: crate::runtime::CognitionLeaseV1 =
                        serde_json::from_value(receipt).map_err(|error| error.to_string())?;
                    lineage_generation_recovery::validate_provider_lease_identity(
                        &request.agent_subject,
                        request,
                        &lease,
                    )?;
                    Some(lease)
                } else {
                    if receipt.get("dispatched").and_then(Value::as_bool) != Some(true)
                        || receipt.get("request_digest").and_then(Value::as_str)
                            != Some(request.request_digest.as_str())
                    {
                        return Err("canonical prefix receipt identity mismatch".into());
                    }
                    admission.lease.clone()
                };
                let mut next = admission;
                next.stage = format!("{stage}_view");
                next.commit = Some(*commit);
                next.lease = lease;
                self.llm_sidecar.hosted_admission = Some(next);
                self.llm_sidecar.persist_provider_lineage()?;
            }
            IntentOutcome::Received { .. } | IntentOutcome::Pending | IntentOutcome::Unknown => {}
            IntentOutcome::Rejected { reason } => {
                if stage == "reserve" {
                    self.llm_sidecar.retire_rejected_reserve(&IntentResponse {
                        formal_profile: None,
                        execution_evidence_scope: None,
                        contract_version: response.contract_version,
                        correlation: response.correlation,
                        outcome: IntentOutcome::Rejected { reason },
                    })?;
                } else {
                    return Err(format!(
                        "canonical admission phase={stage} original_key={:?} rejected: {reason:?}",
                        checkpoint.correlation.key
                    ));
                }
            }
            _ => return Err("canonical admission cannot complete original checkpoint".into()),
        }
        Ok(AgentServiceProgress::Advanced)
    }
}

#[cfg(test)]
#[path = "llm_sidecar_feedback_admission_tests.rs"]
mod feedback_admission_tests;

#[cfg(test)]
#[path = "llm_sidecar_reserve_rejection_tests.rs"]
mod reserve_rejection_tests;
