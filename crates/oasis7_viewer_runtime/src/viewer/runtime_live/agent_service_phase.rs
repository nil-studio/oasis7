//! Hosted canonical Act transitions. Original durable maps survive worker loss.
use super::agent_service_io::*;
use super::control_plane::llm_sidecar::lineage_persistence::PendingProviderServiceIntent;
use super::control_plane::provider_action_commit::{
    ProviderServiceCognitionReceipt, decode_provider_service_receipt,
    validate_provider_service_receipt,
};
use super::control_plane::simulator_action_to_runtime;
use crate::world_service::{verified_view::VerifiedWorldView, wire::SchedulerOperationV1};
use oasis7_client_api::world_service::*;

#[path = "agent_service_phase_direct.rs"]
mod direct;

#[derive(Clone)]
pub(super) enum HostedServicePhase {
    Act {
        pending: PendingProviderServiceIntent,
        submit: bool,
    },
    ActView {
        pending: PendingProviderServiceIntent,
        receipt: ProviderServiceCognitionReceipt,
        commit: CommitRef,
        settled: bool,
    },
    ActSettle {
        pending: PendingProviderServiceIntent,
        receipt: ProviderServiceCognitionReceipt,
        submit: bool,
        replay_authorized: bool,
    },
    HandoffWake {
        pending: PendingProviderServiceIntent,
        receipt: ProviderServiceCognitionReceipt,
        target_wake: Box<crate::runtime::SchedulerWakeV1>,
        submit: bool,
        replay_authorized: bool,
    },
    HandoffView {
        pending: PendingProviderServiceIntent,
        receipt: ProviderServiceCognitionReceipt,
        target_wake: Box<crate::runtime::SchedulerWakeV1>,
        commit: CommitRef,
    },
    Feedback {
        pending: PendingProviderServiceIntent,
        receipt: ProviderServiceCognitionReceipt,
    },
    FeedbackAck {
        pending: PendingProviderServiceIntent,
        receipt: ProviderServiceCognitionReceipt,
        submit: bool,
    },
    FeedbackAckView {
        pending: PendingProviderServiceIntent,
        receipt: ProviderServiceCognitionReceipt,
        commit: CommitRef,
    },
    Finalize {
        pending: PendingProviderServiceIntent,
        receipt: ProviderServiceCognitionReceipt,
        feedback_acked: bool,
    },
}

impl super::control_plane::llm_sidecar::RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn persist_feedback_ack_issued(
        &mut self,
        agent: &str,
        phase_pending: &mut PendingProviderServiceIntent,
    ) -> Result<(), String> {
        let durable = self
            .provider_service_pending
            .get(agent)
            .ok_or("ACK original missing")?;
        if serde_json::to_value(durable).map_err(|error| error.to_string())?
            != serde_json::to_value(&*phase_pending).map_err(|error| error.to_string())?
        {
            return Err("ACK phase differs from durable original action; fenced".into());
        }
        let previous = self.provider_service_pending.clone();
        self.provider_service_pending
            .get_mut(agent)
            .and_then(|pending| pending.feedback_ack.as_mut())
            .ok_or("ACK checkpoint missing")?
            .issued = true;
        if let Err(error) = self.persist_provider_lineage() {
            self.provider_service_pending = previous;
            return Err(error);
        }

        phase_pending
            .feedback_ack
            .as_mut()
            .ok_or("ACK checkpoint missing")?
            .issued = true;
        Ok(())
    }
}

impl crate::viewer::ViewerRuntimeLiveServer {
    pub(super) fn hosted_service_config_digest(&self) -> Result<String, String> {
        let config = self
            .config
            .world_service
            .as_ref()
            .ok_or("canonical service configuration missing")?;
        let signer = self
            .config
            .world_service_agent_signer
            .as_ref()
            .ok_or("canonical Agent signer missing")?;
        // Private fence: never expose the credential-bearing source or this token.
        let material = format!(
            "{}|{}|{:?}|{}|{}|{:?}|{}|{}|{}",
            config.endpoint,
            config.trusted_service_public_key,
            config.expected_world,
            config.scope_id,
            config.read_private_key_hex,
            config.timeout,
            config.max_response_bytes,
            signer.private_key_hex,
            signer.delegation_generation
        );
        Ok(blake3::hash(material.as_bytes()).to_hex().to_string())
    }

    pub(super) fn prepare_hosted_service_io(
        &mut self,
        _eligible: bool,
    ) -> Result<AgentServiceProgress, String> {
        if self.config.world_service.is_none() {
            return Ok(AgentServiceProgress::Idle);
        }
        self.configure_service_provider();
        if self.config.world_service_agent_signer.is_none()
            && self.llm_sidecar.provider_service_pending.is_empty()
            && self.llm_sidecar.hosted_service_phase.is_none()
        {
            return Ok(AgentServiceProgress::Idle);
        }
        let current_config = self.hosted_service_config_digest()?;
        if self
            .llm_sidecar
            .hosted_service_config_binding
            .as_ref()
            .is_some_and(|original| original != &current_config)
        {
            return Err("hosted service original configuration changed; checkpoint fenced".into());
        }
        if self.llm_sidecar.hosted_wait.is_some() {
            return self.prepare_hosted_wait_io();
        }
        if self.llm_sidecar.hosted_resume.is_some() {
            return self.prepare_hosted_resume_io(_eligible);
        }
        if self.llm_sidecar.hosted_service_phase.is_none()
            && self.llm_sidecar.provider_service_pending.is_empty()
            && self.llm_sidecar.hosted_admission.is_none()
        {
            let resume = self.prepare_hosted_resume_io(_eligible)?;
            if !matches!(resume, AgentServiceProgress::Idle) {
                return Ok(resume);
            }
        }
        if self.llm_sidecar.hosted_service_inflight.is_some() {
            return Ok(AgentServiceProgress::Idle);
        }
        if self.llm_sidecar.hosted_service_phase.is_none() {
            if let Some(pending) = self
                .llm_sidecar
                .provider_service_pending
                .values()
                .next()
                .cloned()
            {
                self.llm_sidecar
                    .validate_hosted_restored_original(&self.world, &pending)?;
                let crate::world_service::wire::WorldServicePayloadV1::Cognition(signed) =
                    &pending.payload
                else {
                    return Err("hosted recovery operation mismatch".into());
                };
                self.prepare_provider_service_action_checkpoint(
                    &signed.request.action,
                    &pending.cognition,
                    pending.action.clone(),
                )
                .map_err(|error| format!("{error:?}"))?;
                // Existing durable intent is always recovered by original Lookup.
                self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Act {
                    pending,
                    submit: false,
                });
            } else {
                if let Some(decision) = self
                    .llm_sidecar
                    .poll_hosted_provider_decision(&self.world)?
                {
                    if matches!(
                        decision.decision,
                        crate::simulator::AgentDecision::Wait
                            | crate::simulator::AgentDecision::WaitTicks(_)
                    ) {
                        self.llm_sidecar.capture_hosted_wait(decision)?;
                        return Ok(AgentServiceProgress::Advanced);
                    }
                    let crate::simulator::AgentDecision::Act(action) = decision.decision else {
                        return Err("hosted provider decision has no supported phase".into());
                    };
                    let cognition = decision.cognition.ok_or("hosted Act cognition missing")?;
                    let runtime_action = simulator_action_to_runtime(&action, &self.world)
                        .ok_or("hosted Agent action has no registered codec")?;
                    let (pending, existed) = self
                        .prepare_provider_service_action_checkpoint(
                            &runtime_action,
                            &cognition,
                            action,
                        )
                        .map_err(|error| format!("{error:?}"))?;
                    self.llm_sidecar
                        .retire_hosted_provider_outcome(&cognition.request.request_context)?;
                    self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Act {
                        pending,
                        submit: !existed,
                    });
                }
            }
        }
        let Some(mut phase) = self.llm_sidecar.hosted_service_phase.clone() else {
            return Ok(AgentServiceProgress::Idle);
        };
        self.llm_sidecar
            .hosted_service_config_binding
            .get_or_insert(current_config);
        if let HostedServicePhase::Finalize {
            pending,
            receipt,
            feedback_acked,
        } = phase
        {
            if pending.feedback_ack.is_none() {
                self.llm_sidecar.checkpoint_service_feedback_consumption(
                    &self.world,
                    &pending,
                    &receipt,
                )?;
            }
            let pending = self
                .llm_sidecar
                .provider_service_pending
                .get(&pending.cognition.request.request_context.agent_subject)
                .cloned()
                .ok_or("feedback pending missing")?;
            self.llm_sidecar
                .validate_pending_feedback_consumption(&pending)?;
            if feedback_acked {
                self.finalize_hosted_service_act(pending, receipt)?;
                self.llm_sidecar.hosted_service_phase = None;
                self.llm_sidecar.hosted_service_config_binding = None;
            } else {
                // Delivery is repeated idempotently after recovery; a persisted
                // unsigned phase bit is not proof of provider acceptance.
                self.llm_sidecar.hosted_service_phase =
                    Some(HostedServicePhase::Feedback { pending, receipt });
            }
            return Ok(AgentServiceProgress::Advanced);
        }
        let mut client = self
            .world_service_client()
            .map_err(|error| format!("{error:?}"))?
            .ok_or("canonical service transport missing")?;
        if let HostedServicePhase::FeedbackAckView { pending, .. } = &phase {
            self.llm_sidecar
                .validate_hosted_restored_original(&self.world, pending)?;
            let crate::world_service::WorldServicePayloadV1::Cognition(signed) = &pending.payload
            else {
                return Err("ACK View original Cognition missing".into());
            };
            let agent = &signed.request.request.agent_id;
            if agent != &pending.cognition.request.request_context.agent_subject {
                return Err("ACK View original Agent mismatch".into());
            }
            let mut config = client.config().clone();
            config.scope_id = format!("agent:{agent}");
            client = crate::world_service::client::RemoteWorldServiceClient::new(config)
                .map_err(|error| error.to_string())?
                .with_query_state(self.world_service_query_state.clone());
        }
        let operation = match &phase {
            HostedServicePhase::Act { pending, submit } => {
                original_intent_io(&pending.correlation, &pending.payload, *submit)
            }
            HostedServicePhase::ActView {
                pending, commit, ..
            } => AgentServiceIoOperation::View(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: pending.correlation.key.world.clone(),
                scope_id: client.config().scope_id.clone(),
                min_commit: Some(commit.clone()),
                fixed_commit: None,
                deadline_unix_ms: None,
            }),
            HostedServicePhase::ActSettle {
                pending,
                submit,
                replay_authorized,
                ..
            } => {
                let request = &pending.cognition.request.request_context;
                let lease = pending
                    .cognition
                    .cognition_lease
                    .as_ref()
                    .ok_or("hosted settled Act missing lease")?;
                let (checkpoint, existed) = self.llm_sidecar.prepare_service_scheduler_checkpoint(
                    request,
                    "settle",
                    SchedulerOperationV1::SettleLease {
                        lease_id: lease.lease_id.clone(),
                        consumed_amount: lease.reserved_amount,
                    },
                    None,
                )?;
                original_settlement_io(
                    &checkpoint.correlation,
                    &checkpoint.payload,
                    *submit,
                    existed,
                    *replay_authorized,
                )
            }
            HostedServicePhase::HandoffWake {
                pending,
                target_wake,
                submit,
                ..
            } => {
                let request = &pending.cognition.request.request_context;
                let (_, operation) = self
                    .llm_sidecar
                    .committed_service_wake_handoff(&pending.cognition)?
                    .filter(|(current_wake, _)| current_wake == target_wake.as_ref())
                    .ok_or("canonical wake disappeared before terminal handoff; fenced")?;
                let wake_id = &target_wake.wake_id;
                let (checkpoint, _) = self.llm_sidecar.prepare_service_scheduler_checkpoint(
                    request,
                    &format!("handoff:{wake_id}"),
                    operation,
                    None,
                )?;
                original_intent_io(&checkpoint.correlation, &checkpoint.payload, *submit)
            }
            HostedServicePhase::HandoffView {
                pending, commit, ..
            } => AgentServiceIoOperation::View(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: pending.correlation.key.world.clone(),
                scope_id: client.config().scope_id.clone(),
                min_commit: Some(commit.clone()),
                fixed_commit: None,
                deadline_unix_ms: None,
            }),
            HostedServicePhase::Feedback {
                pending,
                receipt: _,
            } => {
                let ack = pending
                    .feedback_ack
                    .as_ref()
                    .ok_or("feedback consumption missing")?;
                if self.llm_sidecar.service_feedback_is_builtin() {
                    self.mark_hosted_feedback_delivered(pending.clone())?;
                    return Ok(AgentServiceProgress::Advanced);
                }
                AgentServiceIoOperation::Feedback {
                    client: self.llm_sidecar.service_feedback_transport()?,
                    payload: ack.original_feedback.clone(),
                }
            }
            HostedServicePhase::FeedbackAck {
                pending, submit, ..
            } => {
                let ack = pending
                    .feedback_ack
                    .as_ref()
                    .ok_or("feedback ACK checkpoint missing")?;
                original_intent_io(&ack.correlation, &ack.payload, *submit)
            }
            HostedServicePhase::FeedbackAckView {
                pending, commit, ..
            } => AgentServiceIoOperation::View(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: pending.correlation.key.world.clone(),
                scope_id: client.config().scope_id.clone(),
                min_commit: Some(commit.clone()),
                fixed_commit: None,
                deadline_unix_ms: None,
            }),
            HostedServicePhase::Finalize { .. } => unreachable!(),
        };
        if let HostedServicePhase::FeedbackAck {
            pending,
            submit: true,
            ..
        } = &phase
        {
            let agent = pending
                .cognition
                .request
                .request_context
                .agent_subject
                .clone();
            // The durable ACK checkpoint is the same owned phase, with its
            // issuance bit advanced before network I/O. Keep the in-memory
            // phase identity in lockstep so direct recovery does not compare
            // stale unsigned bookkeeping against the persisted signed intent.
            let HostedServicePhase::FeedbackAck { pending, .. } = &mut phase else {
                unreachable!("the guarded phase is FeedbackAck")
            };
            self.llm_sidecar
                .persist_feedback_ack_issued(&agent, pending)?;
            self.llm_sidecar.hosted_service_phase = Some(phase.clone());
        }
        self.llm_sidecar.hosted_service_generation =
            self.llm_sidecar.hosted_service_generation.saturating_add(1);
        let token = AgentServiceIoToken {
            generation: self.llm_sidecar.hosted_service_generation,
            config_digest: self.hosted_service_config_digest()?,
            phase_digest: phase.identity()?,
        };
        self.llm_sidecar.hosted_service_inflight = Some(token.clone());
        // After dispatch reservation any worker failure must recover by Lookup.
        match self.llm_sidecar.hosted_service_phase.as_mut() {
            Some(HostedServicePhase::Act { submit, .. })
            | Some(HostedServicePhase::ActSettle { submit, .. })
            | Some(HostedServicePhase::HandoffWake { submit, .. })
            | Some(HostedServicePhase::FeedbackAck { submit, .. }) => *submit = false,
            _ => {}
        }
        Ok(AgentServiceProgress::NeedsIo(Box::new(AgentServiceIoJob {
            token,
            client: Some(client),
            operation,
        })))
    }
    pub(super) fn apply_hosted_service_io(
        &mut self,
        result: AgentServiceIoResult,
    ) -> Result<AgentServiceProgress, String> {
        if result.token.phase_digest.starts_with("wait:") {
            return self.apply_hosted_wait_io(result);
        }
        if result.token.phase_digest.starts_with("resume:") {
            return self.apply_hosted_resume_io(result);
        }
        if self.llm_sidecar.hosted_service_inflight.as_ref() != Some(&result.token) {
            return Err(
                "hosted service completion token mismatch; original intent retained".into(),
            );
        }
        self.llm_sidecar.hosted_service_inflight = None;
        if result.token.config_digest != self.hosted_service_config_digest()? {
            return Err("hosted service configuration changed; original intent fenced".into());
        }
        let phase = self
            .llm_sidecar
            .hosted_service_phase
            .clone()
            .ok_or("hosted service completion lacks original phase")?;
        if result.token.phase_digest != phase.identity()? {
            return Err("hosted service phase identity mismatch".into());
        }
        let response = result.response?;
        match phase {
            HostedServicePhase::Act { pending, .. } => {
                let Some(response) = original_response(response)? else {
                    return Ok(AgentServiceProgress::Advanced);
                };
                if original_act_requires_replay(&response, &pending.correlation)? {
                    self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Act {
                        pending,
                        submit: true,
                    });
                    return Ok(AgentServiceProgress::Advanced);
                }
                match response.outcome {
                    IntentOutcome::Committed { commit, receipt } => {
                        let receipt: ProviderServiceCognitionReceipt =
                            decode_provider_service_receipt(receipt)?;
                        validate_provider_service_receipt(&pending, &receipt)?;
                        self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::ActView {
                            pending,
                            receipt,
                            commit: *commit,
                            settled: false,
                        });
                    }
                    IntentOutcome::Received { .. } | IntentOutcome::Pending => {}
                    IntentOutcome::Rejected { reason } => {
                        return Err(format!("canonical Agent rejected: {reason:?}"));
                    }
                    state => {
                        return Err(format!(
                            "canonical Agent outcome cannot complete original intent: {state:?}; original_key={:?}",
                            pending.correlation.key
                        ));
                    }
                }
            }
            HostedServicePhase::ActView {
                pending,
                receipt,
                commit,
                settled,
            } => {
                let AgentServiceIoResponse::View(view) = response else {
                    return Err("hosted service expected verified minimum view".into());
                };
                if !view
                    .version()
                    .commit
                    .satisfies_minimum(&commit)
                    .map_err(|error| error.to_string())?
                {
                    return Err("hosted service view precedes committed Act".into());
                }
                self.apply_hosted_verified_view(*view)?;
                self.llm_sidecar.hosted_service_phase = if settled {
                    if let Some(handoff) =
                        self.prepare_hosted_service_wake_handoff(pending.clone(), receipt.clone())?
                    {
                        Some(handoff)
                    } else {
                        Some(HostedServicePhase::Finalize {
                            pending,
                            receipt,
                            feedback_acked: false,
                        })
                    }
                } else {
                    Some(HostedServicePhase::ActSettle {
                        pending,
                        receipt,
                        submit: true,
                        replay_authorized: false,
                    })
                };
            }
            HostedServicePhase::ActSettle {
                pending, receipt, ..
            } => {
                let request = &pending.cognition.request.request_context;
                let checkpoint = self
                    .llm_sidecar
                    .provider_scheduler_pending
                    .get(&format!("{}:settle", request.provider_invocation_key()))
                    .ok_or("hosted settlement checkpoint missing")?;
                let Some(response) = original_response(response)? else {
                    return Ok(AgentServiceProgress::Advanced);
                };
                if original_settlement_requires_replay(&response, &checkpoint.correlation)? {
                    self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::ActSettle {
                        pending,
                        receipt,
                        submit: true,
                        replay_authorized: true,
                    });
                    return Ok(AgentServiceProgress::Advanced);
                }
                match response.outcome {
                    IntentOutcome::Committed {
                        receipt: settled,
                        commit,
                    } => {
                        super::control_plane::RuntimeLlmSidecar::validate_hosted_settlement_receipt(request,pending.cognition.cognition_lease.as_ref().ok_or("hosted settlement lease missing")?,settled)?;
                        self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::ActView {
                            pending,
                            receipt,
                            commit: *commit,
                            settled: true,
                        });
                    }
                    IntentOutcome::Received { .. } | IntentOutcome::Pending => {}
                    IntentOutcome::Rejected { reason } => {
                        return Err(format!("canonical settlement rejected: {reason:?}"));
                    }
                    state => {
                        return Err(format!(
                            "canonical settlement cannot complete original intent: {state:?}; original_key={:?}",
                            checkpoint.correlation.key
                        ));
                    }
                }
            }
            HostedServicePhase::HandoffWake {
                pending,
                receipt,
                target_wake,
                replay_authorized,
                ..
            } => {
                let Some(response) = original_response(response)? else {
                    return Ok(AgentServiceProgress::Advanced);
                };
                response
                    .validate(
                        &self
                            .llm_sidecar
                            .provider_scheduler_pending
                            .get(&format!(
                                "{}:handoff:{}",
                                pending
                                    .cognition
                                    .request
                                    .request_context
                                    .provider_invocation_key(),
                                target_wake.wake_id
                            ))
                            .ok_or("wake handoff checkpoint missing")?
                            .correlation,
                    )
                    .map_err(|error| error.to_string())?;
                match response.outcome {
                    IntentOutcome::Committed {
                        commit,
                        receipt: value,
                    } => {
                        let handoff: crate::runtime::CognitionWakeHandoffResultV1 =
                            serde_json::from_value(value).map_err(|error| {
                                format!("wake handoff receipt invalid: {error}")
                            })?;
                        let request = &pending.cognition.request.request_context;
                        if handoff.wake != *target_wake
                            || handoff.continuation.wake_id != target_wake.wake_id
                            || handoff.continuation.continuation_id != target_wake.continuation_id
                            || handoff.continuation.world_id != target_wake.world_id
                            || handoff.continuation.branch_id != target_wake.branch_id
                            || handoff.continuation.finality_epoch != target_wake.finality_epoch
                            || handoff.continuation.finality_block_hash
                                != target_wake.finality_block_hash
                            || handoff.continuation.finality_status != target_wake.finality_status
                            || handoff.continuation.reorg_epoch != target_wake.reorg_epoch
                            || handoff.continuation.runtime_manifest_hash
                                != target_wake.runtime_manifest_hash
                            || handoff.continuation.agent_id != request.agent_subject
                            || handoff.continuation.agent_session_id != request.agent_session_id
                            || handoff.continuation.agent_turn_id != request.agent_turn_id
                            || handoff.continuation.decision_request_id
                                != request.decision_request_id
                            || handoff.continuation.origin_request_digest
                                != request.request_digest.to_string()
                            || handoff.replanned_continuation.is_some()
                            || handoff.continuation.status
                                != crate::runtime::ContinuationStatusV1::Completed
                            || handoff.continuation.terminal_disposition.as_deref()
                                != Some("provider_action_committed")
                        {
                            return Err("canonical wake handoff receipt identity mismatch".into());
                        }
                        self.llm_sidecar.hosted_service_phase =
                            Some(HostedServicePhase::HandoffView {
                                pending,
                                receipt,
                                target_wake,
                                commit: *commit,
                            });
                    }
                    IntentOutcome::Unknown if !replay_authorized => {
                        self.llm_sidecar.hosted_service_phase =
                            Some(HostedServicePhase::HandoffWake {
                                pending,
                                receipt,
                                target_wake,
                                submit: true,
                                replay_authorized: true,
                            });
                    }
                    IntentOutcome::Received { .. }
                    | IntentOutcome::Pending
                    | IntentOutcome::Unknown => {}
                    IntentOutcome::Expired | IntentOutcome::HistoryUnavailable => {
                        return Err(
                            "canonical wake handoff history unavailable; exact checkpoint retained"
                                .into(),
                        );
                    }
                    IntentOutcome::Rejected { reason } => {
                        return Err(format!("canonical wake handoff rejected: {reason:?}"));
                    }
                }
            }
            HostedServicePhase::HandoffView {
                pending,
                receipt,
                target_wake,
                commit,
            } => {
                let AgentServiceIoResponse::View(view) = response else {
                    return Err("hosted wake handoff minimum View missing".into());
                };
                if !view
                    .version()
                    .commit
                    .satisfies_minimum(&commit)
                    .map_err(|error| error.to_string())?
                {
                    return Err("hosted wake handoff View precedes committed handoff".into());
                }
                if view
                    .projection()
                    .scheduler_wakes
                    .iter()
                    .any(|wake| wake.wake_id == target_wake.wake_id)
                {
                    return Err(
                        "hosted wake handoff remains visible after committed readback".into(),
                    );
                }
                self.apply_hosted_verified_view(*view)?;
                self.llm_sidecar.complete_committed_service_wake_handoff(
                    &pending.cognition,
                    &target_wake.wake_id,
                )?;
                self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Finalize {
                    pending,
                    receipt,
                    feedback_acked: false,
                });
            }
            HostedServicePhase::Feedback { pending, .. } => {
                if !matches!(response, AgentServiceIoResponse::Feedback) {
                    return Err("feedback transport response mismatch".into());
                }
                self.mark_hosted_feedback_delivered(pending)?;
            }
            HostedServicePhase::FeedbackAck {
                pending, receipt, ..
            } => {
                let Some(response) = original_response(response)? else {
                    return Ok(AgentServiceProgress::Advanced);
                };
                let ack = pending
                    .feedback_ack
                    .as_ref()
                    .ok_or("ACK checkpoint missing")?;
                response
                    .validate(&ack.correlation)
                    .map_err(|e| e.to_string())?;
                match response.outcome {
                    IntentOutcome::Committed {
                        commit,
                        receipt: result,
                    } => {
                        let crate::world_service::WorldServicePayloadV1::FeedbackAck(signed) =
                            &ack.payload
                        else {
                            return Err("ACK codec mismatch".into());
                        };
                        if result["delivery_state"] != "acked"
                            || result["acknowledged"]
                                != serde_json::to_value(&signed.request)
                                    .map_err(|e| e.to_string())?
                        {
                            return Err("ACK receipt mismatch".into());
                        }
                        self.llm_sidecar.hosted_service_phase =
                            Some(HostedServicePhase::FeedbackAckView {
                                pending,
                                receipt,
                                commit: *commit,
                            });
                    }
                    IntentOutcome::Received { .. } | IntentOutcome::Pending => {}
                    IntentOutcome::Unknown => {
                        // Explicit absence permits replaying the exact signed
                        // key/payload after a pre-Submit crash; no new nonce.
                        self.llm_sidecar.hosted_service_phase =
                            Some(HostedServicePhase::FeedbackAck {
                                pending,
                                receipt,
                                submit: true,
                            });
                    }
                    other => return Err(format!("ACK original not completed: {other:?}")),
                }
            }
            HostedServicePhase::FeedbackAckView {
                pending,
                receipt,
                commit,
            } => {
                let AgentServiceIoResponse::View(view) = response else {
                    return Err("ACK minimum View missing".into());
                };
                if !view
                    .version()
                    .commit
                    .satisfies_minimum(&commit)
                    .map_err(|e| e.to_string())?
                {
                    return Err("ACK View too old".into());
                }
                let ack = pending
                    .feedback_ack
                    .as_ref()
                    .ok_or("ACK checkpoint missing")?;
                let crate::world_service::WorldServicePayloadV1::FeedbackAck(signed) = &ack.payload
                else {
                    return Err("ACK codec mismatch".into());
                };
                let agent = &pending.cognition.request.request_context.agent_subject;
                if signed.request.agent_id != *agent
                    || view.version().visibility_scope != format!("agent:{agent}")
                {
                    return Err("ACK View Agent scope mismatch".into());
                }
                let history = view
                    .projection()
                    .feedback_history
                    .as_ref()
                    .ok_or("ACK scoped history missing")?;
                if history.agent_id != *agent {
                    return Err("ACK history Agent mismatch".into());
                }
                history.metadata(agent)?;
                let record = history
                    .records
                    .iter()
                    .find(|record| record.feedback.feedback_id == signed.request.feedback_id)
                    .ok_or("ACK feedback missing")?;
                let feedback = &record.feedback;
                if record.delivery_state != "acked"
                    || record.original_envelope_digest != signed.request.original_envelope_digest
                    || feedback.agent_subject != signed.request.agent_id
                    || feedback.agent_session_id != signed.request.agent_session_id
                    || feedback.agent_turn_id != signed.request.agent_turn_id
                    || feedback.decision_request_id != signed.request.decision_request_id
                    || feedback.request_digest.to_string() != signed.request.request_digest
                    || feedback.feedback_seq != signed.request.feedback_seq
                {
                    return Err("ACK readback mismatch".into());
                }
                if let Some(current) = self.verified_world_view.as_ref()
                    && !view
                        .version()
                        .commit
                        .satisfies_minimum(&current.version().commit)
                        .map_err(|error| error.to_string())?
                {
                    return Err("ACK private View precedes current Viewer View".into());
                }
                if self
                    .config
                    .world_service
                    .as_ref()
                    .is_some_and(|config| config.scope_id == view.version().visibility_scope)
                {
                    self.apply_hosted_verified_view(*view)?;
                } else {
                    // The Agent history is private provider input, not a public
                    // Viewer cursor or a replacement for its verified snapshot.
                    self.llm_sidecar.provider_service_projection = Some(view.projection().clone());
                    self.llm_sidecar
                        .sync_shadow_kernel(&self.world, &self.snapshot_config)?;
                }
                self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Finalize {
                    pending,
                    receipt,
                    feedback_acked: true,
                });
            }
            HostedServicePhase::Finalize { .. } => {
                return Err("hosted finalization has no transport result".into());
            }
        }
        Ok(AgentServiceProgress::Advanced)
    }
    pub(super) fn apply_hosted_verified_view(
        &mut self,
        view: VerifiedWorldView,
    ) -> Result<(), String> {
        if let Some(current) = self.verified_world_view.as_ref()
            && !view
                .version()
                .commit
                .satisfies_minimum(&current.version().commit)
                .map_err(|error| error.to_string())?
        {
            return Err("hosted service old view cannot replace newer view".into());
        }
        self.llm_sidecar.provider_service_projection = Some(view.projection().clone());
        self.verified_world_view = Some(view);
        self.llm_sidecar
            .sync_shadow_kernel(&self.world, &self.snapshot_config)
    }
    fn mark_hosted_feedback_delivered(
        &mut self,
        pending: PendingProviderServiceIntent,
    ) -> Result<(), String> {
        let agent = &pending.cognition.request.request_context.agent_subject;
        let previous = self.llm_sidecar.provider_service_pending.clone();
        self.llm_sidecar
            .provider_service_pending
            .get_mut(agent)
            .ok_or("feedback pending missing")?
            .feedback_ack
            .as_mut()
            .ok_or("consumption checkpoint missing")?
            .delivered = true;
        if let Err(error) = self.llm_sidecar.persist_provider_lineage() {
            self.llm_sidecar.provider_service_pending = previous;
            return Err(error);
        }
        let updated = self
            .llm_sidecar
            .provider_service_pending
            .get(agent)
            .cloned()
            .unwrap();
        let Some(HostedServicePhase::Feedback { receipt, .. }) =
            self.llm_sidecar.hosted_service_phase.clone()
        else {
            return Err("feedback phase changed".into());
        };
        let submit = !updated
            .feedback_ack
            .as_ref()
            .ok_or("ACK checkpoint missing")?
            .issued;
        self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::FeedbackAck {
            pending: updated,
            receipt,
            submit,
        });
        Ok(())
    }
    fn finalize_hosted_service_act(
        &mut self,
        pending: PendingProviderServiceIntent,
        receipt: ProviderServiceCognitionReceipt,
    ) -> Result<(), String> {
        let request = &pending.cognition.request.request_context;
        if self
            .llm_sidecar
            .provider_service_projection
            .as_ref()
            .is_some_and(|projection| {
                projection
                    .scheduler_wakes
                    .iter()
                    .any(|wake| wake.agent_id == request.agent_subject)
            })
        {
            return Err(
                "hosted wake terminal handoff remains pending; original Act retained".into(),
            );
        }
        let action_id = receipt
            .commit_record
            .action_id
            .strip_prefix("action:")
            .and_then(|id| id.parse::<u64>().ok())
            .ok_or("canonical action ID is not numeric")?;
        self.llm_sidecar.track_action(
            action_id,
            request.agent_subject.clone(),
            pending.action.clone(),
            Some(pending.cognition.clone()),
        );
        self.llm_sidecar.clear_settled_service_lease(
            request,
            pending
                .cognition
                .cognition_lease
                .as_ref()
                .ok_or("hosted Act lease missing")?,
        )?;
        self.llm_sidecar
            .finalize_provider_action_with_feedback_checked(action_id, receipt.feedback)?;
        self.llm_sidecar
            .provider_service_pending
            .remove(&request.agent_subject);
        if let Err(error) = self.llm_sidecar.persist_provider_lineage() {
            self.llm_sidecar
                .provider_service_pending
                .insert(request.agent_subject.clone(), pending);
            return Err(error);
        }
        self.llm_sidecar
            .provider_restored_service_checkpoints
            .remove(&request.agent_subject);
        self.defer_next_auto_play_step_after_completion(self.config.play_step_interval);
        Ok(())
    }
}
pub(super) fn original_intent_io(
    correlation: &RequestCorrelation,
    payload: &crate::world_service::WorldServicePayloadV1,
    submit: bool,
) -> AgentServiceIoOperation {
    if submit {
        AgentServiceIoOperation::Submit(SubmitIntentRequest {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            correlation: correlation.clone(),
            deadline_unix_ms: None,
            signed_payload: payload.clone(),
        })
    } else {
        AgentServiceIoOperation::Lookup {
            request: LookupIntentRequest {
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                key: correlation.key.clone(),
            },
            original: payload.clone(),
        }
    }
}
pub(super) fn original_response(
    response: AgentServiceIoResponse,
) -> Result<Option<IntentResponse<serde_json::Value>>, String> {
    match response {
        AgentServiceIoResponse::Intent(response) => Ok(Some(response)),
        AgentServiceIoResponse::Submit(SubmitObservation::Response(response)) => {
            Ok(Some(*response))
        }
        AgentServiceIoResponse::Submit(SubmitObservation::OutcomeUnknown(_)) => Ok(None),
        _ => Err("hosted service response operation mismatch".into()),
    }
}

/// An authenticated Unknown reports no published receipt, not an empty Node
/// admission queue. Replay only the original key and signed bytes; canonical
/// admission/execution deduplication owns any already received effect.
pub(super) fn original_act_requires_replay(
    response: &IntentResponse<serde_json::Value>,
    original: &RequestCorrelation,
) -> Result<bool, String> {
    response
        .validate(original)
        .map_err(|error| error.to_string())?;
    Ok(matches!(response.outcome, IntentOutcome::Unknown))
}

#[cfg(any(test, feature = "test_tier_required"))]
impl crate::viewer::ViewerRuntimeLiveServer {
    /// Required-tier drive of the production Act reconciliation decision and
    /// original operation builder with genuine signed service responses.
    pub fn test_replay_original_hosted_act(
        client: crate::world_service::client::RemoteWorldServiceClient,
        original: SubmitIntentRequest<crate::world_service::WorldServicePayloadV1>,
    ) -> Result<SubmitObservation<serde_json::Value>, String> {
        use crate::world_service::client::WorldServicePort;
        original.validate().map_err(|error| error.to_string())?;
        let response = client
            .lookup(
                LookupIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    key: original.correlation.key.clone(),
                },
                original.signed_payload.clone(),
            )
            .map_err(|error| error.to_string())?;
        if !original_act_requires_replay(&response, &original.correlation)? {
            return Err("test drive requires genuine unpublished original outcome".into());
        }
        let AgentServiceIoOperation::Submit(replay) =
            original_intent_io(&original.correlation, &original.signed_payload, true)
        else {
            unreachable!()
        };
        assert_eq!(replay.correlation, original.correlation);
        assert_eq!(replay.signed_payload, original.signed_payload);
        client.submit(replay).map_err(|error| error.to_string())
    }
}

fn original_settlement_requires_replay(
    response: &IntentResponse<serde_json::Value>,
    original: &RequestCorrelation,
) -> Result<bool, String> {
    response
        .validate(original)
        .map_err(|error| error.to_string())?;
    Ok(matches!(response.outcome, IntentOutcome::Unknown))
}
fn original_settlement_io(
    correlation: &RequestCorrelation,
    payload: &crate::world_service::WorldServicePayloadV1,
    submit: bool,
    existed: bool,
    replay_authorized: bool,
) -> AgentServiceIoOperation {
    // Existing durable requests first reconcile by Lookup. Only a verified
    // Unknown authorizes replay; an ActView transition is not that evidence.
    // Dispatch reservation consumes submit before any completion is applied.
    original_intent_io(
        correlation,
        payload,
        submit && (!existed || replay_authorized),
    )
}
#[cfg(any(test, feature = "test_tier_required"))]
impl crate::viewer::ViewerRuntimeLiveServer {
    pub fn test_replay_original_hosted_settlement(
        client: crate::world_service::client::RemoteWorldServiceClient,
        original: SubmitIntentRequest<crate::world_service::WorldServicePayloadV1>,
    ) -> Result<SubmitObservation<serde_json::Value>, String> {
        use crate::world_service::client::WorldServicePort;
        original.validate().map_err(|error| error.to_string())?;
        if !matches!(&original.signed_payload, crate::world_service::WorldServicePayloadV1::Scheduler(s) if matches!(s.request.operation, SchedulerOperationV1::SettleLease {..}))
        {
            return Err("settlement operation required".into());
        }
        assert!(
            matches!(
                original_settlement_io(
                    &original.correlation,
                    &original.signed_payload,
                    true,
                    true,
                    false
                ),
                AgentServiceIoOperation::Lookup { .. }
            ),
            "existing settlement first reconciles by Lookup"
        );
        assert!(
            matches!(
                original_settlement_io(
                    &original.correlation,
                    &original.signed_payload,
                    true,
                    false,
                    false
                ),
                AgentServiceIoOperation::Submit(_)
            ),
            "new settlement may first Submit"
        );
        assert!(
            matches!(
                original_settlement_io(
                    &original.correlation,
                    &original.signed_payload,
                    false,
                    true,
                    true
                ),
                AgentServiceIoOperation::Lookup { .. }
            ),
            "dispatch consumes an authorized Submit"
        );
        let response = client
            .lookup(
                LookupIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    key: original.correlation.key.clone(),
                },
                original.signed_payload.clone(),
            )
            .map_err(|e| e.to_string())?;
        if !original_settlement_requires_replay(&response, &original.correlation)? {
            return Err("unpublished settlement required".into());
        }
        let AgentServiceIoOperation::Submit(replay) = original_settlement_io(
            &original.correlation,
            &original.signed_payload,
            true,
            true,
            true,
        ) else {
            return Err("retained settlement was not submitted".into());
        };
        assert_eq!(replay.correlation, original.correlation);
        assert_eq!(replay.signed_payload, original.signed_payload);
        client.submit(replay).map_err(|e| e.to_string())
    }
}
