use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderWaitFault {
    None = 0,
    Projection = 1,
    Release = 2,
    Runtime = 3,
    Harness = 4,
    Actor = 5,
    Persistence = 6,
    PersistenceBlocker = 7,
}

#[cfg(not(test))]
impl ProviderWaitFault {
    const ALL: [Self; 8] = [
        Self::None,
        Self::Projection,
        Self::Release,
        Self::Runtime,
        Self::Harness,
        Self::Actor,
        Self::Persistence,
        Self::PersistenceBlocker,
    ];
}

#[cfg(test)]
fn provider_wait_fault() -> ProviderWaitFault {
    match std::env::var("OASIS7_TEST_PROVIDER_WAIT_FAULT").as_deref() {
        Ok("projection") => ProviderWaitFault::Projection,
        Ok("release") => ProviderWaitFault::Release,
        Ok("runtime") => ProviderWaitFault::Runtime,
        Ok("harness") => ProviderWaitFault::Harness,
        Ok("actor") => ProviderWaitFault::Actor,
        Ok("persistence") => ProviderWaitFault::Persistence,
        Ok("persistence_blocker") => ProviderWaitFault::PersistenceBlocker,
        _ => ProviderWaitFault::None,
    }
}

#[cfg(not(test))]
fn provider_wait_fault() -> ProviderWaitFault {
    // Keep the complete fault taxonomy type-checked in production even though
    // fault injection itself is test-only.
    let _ = ProviderWaitFault::ALL;
    ProviderWaitFault::None
}

#[cfg(not(target_arch = "wasm32"))]
fn compensate_provider_wait_admission(
    sidecar: &mut RuntimeLlmSidecar,
    world: &mut RuntimeWorld,
    request: &crate::simulator::ContinuousAgentRequestContextV1,
    admitted: &crate::runtime::AgentContinuation,
    proposal_id: &str,
    reason: impl Into<String>,
) -> String {
    let reason = reason.into();
    if sidecar.provider_service_required {
        let tick = sidecar
            .provider_service_projection
            .as_ref()
            .map(|view| view.state.time)
            .unwrap_or(request.runtime_binding.base_tick);
        let result = sidecar
            .provider_scheduler_operation(
                request,
                "compensate_wait",
                crate::world_service::wire::SchedulerOperationV1::TransitionContinuation {
                    continuation_id: admitted.continuation_id.clone(),
                    to: crate::runtime::ContinuationStatusV1::Rejected,
                    logical_tick: tick,
                },
            )
            .and_then(|_| {
                if let Some(lease) = sidecar.provider_cognition_lease(&request.agent_subject) {
                    sidecar.settle_provider_lease_at_authority(world, request, &lease)?;
                    sidecar.clear_provider_cognition_lease(&request.agent_subject);
                }
                if let Some(runner) = sidecar
                    .runner
                    .as_mut()
                    .and_then(RuntimeDecisionRunner::async_runner_mut)
                {
                    runner
                        .invalidate_continuation_for_agent(
                            &request.agent_subject,
                            crate::simulator::ContinuationInvalidationReason::Rejected,
                        )
                        .map_err(|error| {
                            format!("canonical Wait harness compensation failed: {error}")
                        })?;
                }
                sidecar.provider_continuation_proposals.remove(proposal_id);
                sidecar
                    .provider_continuation_recovery_pending
                    .remove(&request.agent_subject);
                sidecar.provider_active_turns.remove(&request.agent_subject);
                sidecar.persist_provider_lineage()
            });
        if let Err(error) = result {
            sidecar
                .provider_continuation_recovery_pending
                .insert(request.agent_subject.clone(), proposal_id.into());
            sidecar.persist_provider_lineage_best_effort();
            return format!("{reason}; canonical Wait compensation remains pending: {error}");
        }
        return reason;
    }
    let fault = provider_wait_fault();
    let mut compensation_errors = Vec::new();
    let mut runtime_already_terminal =
        world
            .cognition_continuations()
            .as_array()
            .is_some_and(|continuations| {
                continuations.iter().any(|continuation| {
                    continuation["continuation_id"] == admitted.continuation_id
                        && matches!(
                            continuation["status"].as_str(),
                            Some("rejected")
                                | Some("cancelled")
                                | Some("invalidated")
                                | Some("expired")
                        )
                })
            });
    if fault == ProviderWaitFault::Runtime {
        let fault_continuation_id = format!("{}-fault", admitted.continuation_id);
        match world.transition_cognition_continuation(
            fault_continuation_id.as_str(),
            crate::runtime::ContinuationStatusV1::Rejected,
            world.state().time,
        ) {
            Ok(_) => compensation_errors.push(
                "Runtime rejection returned error injection did not return an error".to_string(),
            ),
            Err(error) => {
                compensation_errors.push(format!("Runtime rejection returned error: {error:?}"))
            }
        }
    } else if !runtime_already_terminal {
        match world.transition_cognition_continuation(
            admitted.continuation_id.as_str(),
            crate::runtime::ContinuationStatusV1::Rejected,
            world.state().time,
        ) {
            Ok(_) => runtime_already_terminal = true,
            Err(error) => {
                // A retry can observe a terminal Runtime continuation after
                // another recovery pass committed it. Treat that exact
                // terminal state as idempotent; any other error remains a
                // recoverable compensation failure.
                let terminal_after_error =
                    world
                        .cognition_continuations()
                        .as_array()
                        .is_some_and(|continuations| {
                            continuations.iter().any(|continuation| {
                                continuation["continuation_id"] == admitted.continuation_id
                                    && matches!(
                                        continuation["status"].as_str(),
                                        Some("rejected")
                                            | Some("cancelled")
                                            | Some("invalidated")
                                            | Some("expired")
                                    )
                            })
                        });
                if terminal_after_error {
                    runtime_already_terminal = true;
                } else {
                    compensation_errors.push(format!("Runtime rejection failed: {error:?}"));
                }
            }
        }
    }
    if let Some(runner) = sidecar
        .runner
        .as_mut()
        .and_then(RuntimeDecisionRunner::async_runner_mut)
    {
        if fault == ProviderWaitFault::Harness {
            let harness_fault_result = runner.validate_active_continuation_with_authority(
                request.agent_subject.as_str(),
                &crate::simulator::ContinuationAuthorityContextV1 {
                    baseline_observation_digest: String::new(),
                    goal_digest: String::new(),
                    policy_digest: String::new(),
                    precondition_digest: String::new(),
                },
                admitted,
            );
            match harness_fault_result {
                Ok(()) => compensation_errors.push(
                    "Harness validation returned error injection did not return an error"
                        .to_string(),
                ),
                Err(error) => {
                    compensation_errors.push(format!("Harness validation returned error: {error}"))
                }
            }
        }
        let harness_result = runner.invalidate_continuation_for_agent(
            request.agent_subject.as_str(),
            crate::simulator::ContinuationInvalidationReason::Rejected,
        );
        if let Err(error) = harness_result {
            compensation_errors.push(format!("Harness invalidation failed: {error}"));
        }
        let actor_turn_id = if fault == ProviderWaitFault::Actor {
            format!("{}-fault", request.agent_turn_id)
        } else {
            request.agent_turn_id.clone()
        };
        let actor_result = runner.expire_runtime_turn(
            request.agent_subject.as_str(),
            request.agent_session_id.as_str(),
            actor_turn_id.as_str(),
            request.decision_request_id.as_str(),
            request.request_digest.to_string().as_str(),
        );
        match actor_result {
            Ok(()) if fault == ProviderWaitFault::Actor => compensation_errors
                .push("actor turn cleanup failed injection did not return an error".to_string()),
            Ok(()) => {}
            Err(error) => {
                if fault == ProviderWaitFault::Actor
                    || !runtime_already_terminal
                    || !error.to_string().contains("unknown pending Runtime turn")
                {
                    compensation_errors.push(format!("actor turn cleanup failed: {error}"));
                }
            }
        }
    }

    // Keep every identity map intact until all authority compensation succeeds.
    // If the final checkpoint write fails, restore these values so the prior
    // durable checkpoint still describes the same recoverable request.
    let proposal_backup = sidecar
        .provider_continuation_proposals
        .get(proposal_id)
        .cloned();
    let active_backup = sidecar
        .provider_active_turns
        .get(request.agent_subject.as_str())
        .cloned();
    let context_backup = sidecar
        .provider_contexts
        .get(request.agent_subject.as_str())
        .cloned();
    let held_backup = sidecar
        .provider_held_decisions
        .get(request.agent_subject.as_str())
        .cloned();
    let wait_backup = sidecar
        .provider_wait_until
        .get(request.agent_subject.as_str())
        .copied();
    let recovery_backup = sidecar
        .provider_continuation_recovery_pending
        .get(request.agent_subject.as_str())
        .cloned();
    let lease_backup = sidecar.provider_cognition_lease(request.agent_subject.as_str());

    if compensation_errors.is_empty() {
        #[cfg(test)]
        if matches!(
            fault,
            ProviderWaitFault::Persistence | ProviderWaitFault::PersistenceBlocker
        ) && let Err(error) = sidecar.install_test_provider_lineage_checkpoint_blocker()
        {
            compensation_errors.push(format!(
                "provider Wait test checkpoint blocker setup failed: {error}"
            ));
        }
    }
    if compensation_errors.is_empty()
        && let Some(lease) = lease_backup.as_ref()
    {
        if let Err(error) = sidecar.validate_provider_cognition_lease_for_request(
            world,
            request.agent_subject.as_str(),
            request,
            lease,
            "settle",
        ) {
            compensation_errors.push(format!(
                "provider Wait cognition lease settlement validation failed: {error}"
            ));
        } else if let Err(error) = sidecar.settle_provider_lease_at_authority(world, request, lease)
        {
            compensation_errors.push(format!(
                "provider Wait cognition lease settlement failed: {error:?}"
            ));
        } else {
            sidecar.clear_provider_cognition_lease(request.agent_subject.as_str());
        }
    }
    if compensation_errors.is_empty() {
        sidecar.provider_continuation_proposals.remove(proposal_id);
        sidecar
            .provider_active_turns
            .remove(request.agent_subject.as_str());
        sidecar
            .provider_contexts
            .remove(request.agent_subject.as_str());
        sidecar
            .provider_held_decisions
            .remove(request.agent_subject.as_str());
        sidecar
            .provider_wait_until
            .remove(request.agent_subject.as_str());
        sidecar
            .provider_continuation_recovery_pending
            .remove(request.agent_subject.as_str());
        let persistence_result = sidecar.persist_provider_lineage();
        if let Err(error) = persistence_result {
            compensation_errors.push(format!(
                "provider Wait lineage cleanup persistence failed: {error}"
            ));
        }
    }
    if !compensation_errors.is_empty() {
        if let Some(proposal) = proposal_backup {
            sidecar
                .provider_continuation_proposals
                .insert(proposal_id.to_string(), proposal);
        }
        if let Some(context) = active_backup.clone() {
            sidecar
                .provider_active_turns
                .insert(request.agent_subject.to_string(), context);
        }
        if let Some(context) = context_backup.clone() {
            sidecar
                .provider_contexts
                .insert(request.agent_subject.to_string(), context);
        }
        if let Some(decision) = held_backup {
            sidecar
                .provider_held_decisions
                .insert(request.agent_subject.to_string(), decision);
        }
        if let Some(wait_until) = wait_backup {
            sidecar
                .provider_wait_until
                .insert(request.agent_subject.to_string(), wait_until);
        }
        if let Some(recovery) = recovery_backup {
            sidecar
                .provider_continuation_recovery_pending
                .insert(request.agent_subject.to_string(), recovery);
        } else {
            sidecar.provider_continuation_recovery_pending.insert(
                request.agent_subject.to_string(),
                format!("{reason}; compensation identity retained"),
            );
        }
        if let Some(lease) = lease_backup {
            sidecar.bind_provider_cognition_lease(request.agent_subject.clone(), lease);
        }
        if let Some(context) = context_backup.as_ref().or(active_backup.as_ref()) {
            sidecar
                .provider_active_turns
                .entry(request.agent_subject.to_string())
                .or_insert_with(|| context.clone());
            sidecar
                .provider_contexts
                .entry(request.agent_subject.to_string())
                .or_insert_with(|| context.clone());
        }
        // A persistence error leaves the previous checkpoint untouched;
        // best-effort logging must not erase the in-memory recovery record.
        sidecar.persist_provider_lineage_best_effort();
    }
    if compensation_errors.is_empty() {
        format!("{reason}; Runtime continuation rejected and provider Wait state compensated")
    } else {
        format!(
            "{reason}; provider Wait compensation incomplete: {}",
            compensation_errors.join("; ")
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn provider_wait_continuation_identity_matches(
    continuation: &crate::runtime::AgentContinuation,
    agent_id: &str,
    agent_session_id: &str,
    agent_turn_id: &str,
    decision_request_id: &str,
    request_digest: &str,
    proposal_id: &str,
) -> bool {
    continuation.agent_id == agent_id
        && continuation.agent_session_id == agent_session_id
        && continuation.agent_turn_id == agent_turn_id
        && continuation.decision_request_id == decision_request_id
        && continuation.origin_request_digest == request_digest
        && continuation.continuation_proposal_id == proposal_id
}

#[cfg(not(target_arch = "wasm32"))]
fn retry_pending_provider_wait_compensation(
    sidecar: &mut RuntimeLlmSidecar,
    world: &mut RuntimeWorld,
) -> Result<bool, String> {
    let Some((agent_id, reason)) = sidecar
        .provider_continuation_recovery_pending
        .iter()
        .next()
        .map(|(agent_id, reason)| (agent_id.clone(), reason.clone()))
    else {
        return Ok(false);
    };
    let Some(context) = sidecar.provider_contexts.get(&agent_id).cloned() else {
        return Ok(false);
    };
    let Some(proposal_id) = sidecar
        .provider_continuation_proposals
        .iter()
        .find(|(_, proposal)| proposal.agent_id == agent_id)
        .map(|(proposal_id, _)| proposal_id.clone())
    else {
        return Ok(false);
    };
    let continuations = serde_json::from_value::<Vec<crate::runtime::AgentContinuation>>(
        world.cognition_continuations(),
    )
    .map_err(|error| format!("provider Wait recovery continuation decode failed: {error}"))?;
    let request = &context.request_context;
    let request_digest = request.request_digest.to_string();
    let Some(admitted) = continuations.into_iter().find(|continuation| {
        provider_wait_continuation_identity_matches(
            continuation,
            agent_id.as_str(),
            request.agent_session_id.as_str(),
            request.agent_turn_id.as_str(),
            request.decision_request_id.as_str(),
            request_digest.as_str(),
            proposal_id.as_str(),
        )
    }) else {
        return Ok(false);
    };
    let _ = compensate_provider_wait_admission(
        sidecar,
        world,
        &context.request_context,
        &admitted,
        proposal_id.as_str(),
        reason,
    );
    Ok(!sidecar
        .provider_continuation_recovery_pending
        .contains_key(&agent_id))
}

/// Admit a provider Wait through the production Harness and Runtime seams.
/// The local Viewer timer is deliberately not a fallback: a wait is a durable
/// continuation only after both authorities accept the exact current
/// observation/goal/policy/precondition snapshot.
#[cfg(not(target_arch = "wasm32"))]
pub(in crate::viewer::runtime_live::control_plane::llm_sidecar) fn admit_provider_wait_continuation(
    sidecar: &mut RuntimeLlmSidecar,
    world: &mut RuntimeWorld,
    kernel: &mut WorldKernel,
    cognition: &RuntimeProviderActionContext,
) -> Result<(), String> {
    let request = &cognition.request.request_context;
    let observation = kernel
        .observe(request.agent_subject.as_str())
        .map_err(|error| format!("provider Wait observation failed: {error:?}"))?;
    let (simulator, runtime, current) =
        super::wait_proposal::build_hosted_wait_proposal(cognition, observation)?;

    let Some(runner) = sidecar
        .runner
        .as_mut()
        .and_then(RuntimeDecisionRunner::async_runner_mut)
    else {
        return Err("provider Wait runner is unavailable".to_string());
    };
    let handle = if sidecar.provider_service_required
        && runner.active_continuation_proposal_id(&request.agent_subject)
            == Some(simulator.continuation_proposal_id.as_str())
    {
        None
    } else {
        Some(
            runner
                .submit_continuation_proposal_with_current_context(
                    request.agent_subject.as_str(),
                    simulator.clone(),
                    &current,
                )
                .map_err(|error| format!("provider Wait Harness admission failed: {error}"))?,
        )
    };
    let proposal_id = simulator.continuation_proposal_id.clone();
    sidecar
        .provider_continuation_proposals
        .insert(proposal_id.clone(), simulator);
    if let Err(error) = sidecar.persist_provider_lineage() {
        if let Some(runner) = sidecar
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
        {
            let _ = runner.invalidate_continuation_for_agent(
                request.agent_subject.as_str(),
                crate::simulator::ContinuationInvalidationReason::Rejected,
            );
        }
        sidecar
            .provider_continuation_proposals
            .remove(proposal_id.as_str());
        return Err(format!(
            "provider Wait Harness lineage persistence failed before Runtime admission: {error}"
        ));
    }
    let admitted = if sidecar.provider_service_required {
        let receipt = sidecar.provider_scheduler_operation(
            request,
            "admit_wait",
            crate::world_service::wire::SchedulerOperationV1::AdmitContinuation(runtime),
        )?;
        serde_json::from_value::<crate::runtime::AgentContinuation>(receipt)
            .map_err(|error| format!("canonical provider Wait receipt invalid: {error}"))?
    } else {
        match world.admit_cognition_continuation(runtime) {
            Ok(admitted) => admitted,
            Err(error) => {
                if let Some(runner) = sidecar
                    .runner
                    .as_mut()
                    .and_then(RuntimeDecisionRunner::async_runner_mut)
                {
                    let _ = runner.invalidate_continuation_for_agent(
                        request.agent_subject.as_str(),
                        crate::simulator::ContinuationInvalidationReason::Rejected,
                    );
                }
                sidecar
                    .provider_continuation_proposals
                    .remove(proposal_id.as_str());
                sidecar.persist_provider_lineage_best_effort();
                return Err(format!("provider Wait Runtime admission failed: {error:?}"));
            }
        }
    };
    let admitted_for_compensation = admitted.clone();
    #[cfg(test)]
    if matches!(
        provider_wait_fault(),
        ProviderWaitFault::Projection
            | ProviderWaitFault::Runtime
            | ProviderWaitFault::Harness
            | ProviderWaitFault::Actor
            | ProviderWaitFault::Persistence
            | ProviderWaitFault::PersistenceBlocker
    ) {
        return Err(compensate_provider_wait_admission(
            sidecar,
            world,
            request,
            &admitted_for_compensation,
            proposal_id.as_str(),
            match provider_wait_fault() {
                ProviderWaitFault::Projection => {
                    "provider Wait Runtime projection compensation failure"
                }
                ProviderWaitFault::Runtime => "Runtime compensation operation failure",
                ProviderWaitFault::Harness => "Harness compensation operation failure",
                ProviderWaitFault::Actor => "actor compensation operation failure",
                ProviderWaitFault::Persistence => "provider Wait checkpoint persistence failure",
                ProviderWaitFault::PersistenceBlocker => {
                    "real provider Wait lineage checkpoint blocker"
                }
                _ => "provider Wait compensation failure",
            },
        ));
    }
    let runner_present = sidecar
        .runner
        .as_mut()
        .and_then(RuntimeDecisionRunner::async_runner_mut)
        .is_some();
    let post_admission_error = if runner_present {
        let projection_result = sidecar
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .expect("runner presence checked")
            .apply_runtime_continuation_projection_with_current_context(
                request.agent_subject.as_str(),
                admitted,
                &current,
            );
        if let Err(error) = projection_result {
            Some(format!(
                "provider Wait Runtime projection failed after admission: {error} (Harness handle {:?})",
                handle.as_ref().map(|handle| &handle.chain_id)
            ))
        } else {
            #[cfg(test)]
            let release_fault = provider_wait_fault() == ProviderWaitFault::Release;
            #[cfg(not(test))]
            let release_fault = false;
            let release_turn_id = if release_fault {
                format!("{}-fault", request.agent_turn_id)
            } else {
                request.agent_turn_id.clone()
            };
            let active_backup = sidecar
                .provider_active_turns
                .get(request.agent_subject.as_str())
                .cloned();
            let context_backup = sidecar
                .provider_contexts
                .get(request.agent_subject.as_str())
                .cloned();
            let wait_until_backup = sidecar
                .provider_wait_until
                .get(request.agent_subject.as_str())
                .copied();
            sidecar
                .provider_active_turns
                .remove(request.agent_subject.as_str());
            sidecar
                .provider_contexts
                .remove(request.agent_subject.as_str());
            sidecar
                .provider_wait_until
                .remove(request.agent_subject.as_str());
            match sidecar.with_runtime_turn_release_for_continuation(
                request.agent_subject.as_str(),
                request.agent_session_id.as_str(),
                release_turn_id.as_str(),
                request.decision_request_id.as_str(),
                request.request_digest.to_string().as_str(),
                |sidecar| sidecar.persist_provider_lineage(),
            ) {
                Ok(()) => None,
                Err(error) => {
                    if let Some(context) = active_backup {
                        sidecar
                            .provider_active_turns
                            .insert(request.agent_subject.clone(), context);
                    }
                    if let Some(context) = context_backup {
                        sidecar
                            .provider_contexts
                            .insert(request.agent_subject.clone(), context);
                    }
                    if let Some(wait_until) = wait_until_backup {
                        sidecar
                            .provider_wait_until
                            .insert(request.agent_subject.clone(), wait_until);
                    }
                    Some(format!("provider Wait actor turn release failed: {error}"))
                }
            }
        }
    } else {
        None
    };
    if let Some(error) = post_admission_error {
        return Err(compensate_provider_wait_admission(
            sidecar,
            world,
            request,
            &admitted_for_compensation,
            proposal_id.as_str(),
            error,
        ));
    }
    if !runner_present {
        sidecar
            .provider_active_turns
            .remove(request.agent_subject.as_str());
        sidecar
            .provider_contexts
            .remove(request.agent_subject.as_str());
        sidecar
            .provider_wait_until
            .remove(request.agent_subject.as_str());
        sidecar.persist_provider_lineage_best_effort();
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
impl RuntimeLlmSidecar {
    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::viewer::runtime_live) fn provider_wait_recovery_agent(&self) -> Option<String> {
        self.provider_continuation_recovery_pending
            .keys()
            .next()
            .cloned()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::viewer::runtime_live) fn provider_wait_recovery_requires_attention(
        &self,
    ) -> Option<String> {
        let agent_id = self.provider_wait_recovery_agent()?;
        let compatible_sibling_exists = self.provider_agent_ids.iter().any(|candidate| {
            candidate != &agent_id
                && !self.provider_recovery_pending.contains_key(candidate)
                && !self
                    .provider_continuation_recovery_pending
                    .contains_key(candidate)
                && !self.provider_transport_exhausted.contains(candidate)
        });
        (!compatible_sibling_exists).then_some(agent_id)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::viewer::runtime_live) fn has_provider_wait_recovery(
        &self,
        agent_id: &str,
    ) -> bool {
        self.provider_continuation_recovery_pending
            .contains_key(agent_id)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::viewer::runtime_live) fn retry_provider_wait_compensation(
        &mut self,
        world: &mut RuntimeWorld,
    ) -> Result<bool, String> {
        retry_pending_provider_wait_compensation(self, world)
    }

    pub(in crate::viewer::runtime_live) fn recover_pending_provider_wait(
        &mut self,
        world: &mut RuntimeWorld,
    ) -> Result<(), String> {
        let Some(agent_id) = self.provider_wait_recovery_agent() else {
            return Ok(());
        };
        match self.retry_provider_wait_compensation(world) {
            Ok(true) => Ok(()),
            Ok(false) => Err(format!(
                "provider Wait continuation recovery remains pending for {agent_id}"
            )),
            Err(error) => Err(error),
        }
    }

    pub(in crate::viewer::runtime_live::control_plane::llm_sidecar) fn admit_provider_wait_continuation(
        &mut self,
        world: &mut RuntimeWorld,
        kernel: &mut WorldKernel,
        cognition: &RuntimeProviderActionContext,
    ) -> Result<(), String> {
        admit_provider_wait_continuation(self, world, kernel, cognition)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn provider_wait_recovery_rejects_digest_only_mismatch() {
        let digest = crate::simulator::h_v1("oasis7.test.wait-recovery-digest.v1", &"request");
        let conflicting_digest =
            crate::simulator::h_v1("oasis7.test.wait-recovery-digest.v1", &"conflicting");
        let continuation = crate::runtime::AgentContinuation {
            schema_version: "continuation.v1".to_string(),
            continuation_id: "continuation-digest-test".to_string(),
            wake_id: "wake-digest-test".to_string(),
            world_id: "world".to_string(),
            branch_id: "main".to_string(),
            finality_epoch: 0,
            finality_block_hash: None,
            finality_status: "pending".to_string(),
            reorg_epoch: 0,
            runtime_manifest_hash: "manifest".to_string(),
            agent_id: "agent-digest-test".to_string(),
            agent_session_id: "session-digest-test".to_string(),
            agent_turn_id: "turn-digest-test".to_string(),
            decision_request_id: "request-digest-test".to_string(),
            origin_turn_id: "turn-digest-test".to_string(),
            origin_request_digest: digest.to_string(),
            continuation_proposal_id: "proposal-digest-test".to_string(),
            proposal_digest: "proposal-digest".to_string(),
            action_or_envelope_digest: None,
            wake_conditions: Vec::new(),
            next_wake_tick: Some(1),
            remaining_budget: crate::runtime::ContinuationBudgetV1 {
                unit: "ticks".to_string(),
                value: 1,
            },
            valid_until_tick: Some(2),
            precondition_digest: "precondition".to_string(),
            wake_seq: 1,
            logical_tick: 0,
            status: crate::runtime::ContinuationStatusV1::Scheduled,
            continuation_status_digest: None,
            terminal_disposition: None,
        };
        assert!(provider_wait_continuation_identity_matches(
            &continuation,
            "agent-digest-test",
            "session-digest-test",
            "turn-digest-test",
            "request-digest-test",
            digest.as_str(),
            "proposal-digest-test",
        ));
        assert!(!provider_wait_continuation_identity_matches(
            &continuation,
            "agent-digest-test",
            "session-digest-test",
            "turn-digest-test",
            "request-digest-test",
            conflicting_digest.as_str(),
            "proposal-digest-test",
        ));
    }
}
