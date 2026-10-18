use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "Durable chat ack construction preserves the stable viewer protocol fields"
)]
fn durable_agent_chat_ack(
    agent_id: &str,
    player_id: &str,
    message_len: usize,
    intent_tick: Option<u64>,
    intent_id: String,
    logical_time: u64,
    event_seq: u64,
    status: String,
    receipt_ref: Option<String>,
    replaced_by: Option<String>,
) -> AgentChatAck {
    AgentChatAck {
        auth_nonce: None,
        agent_id: agent_id.to_string(),
        accepted_at_tick: logical_time,
        message_len,
        player_id: Some(player_id.to_string()),
        intent_tick,
        intent_seq: None,
        idempotent_replay: true,
        intent_id: Some(intent_id),
        accepted_event_seq: Some(event_seq),
        status: Some(status),
        receipt_ref,
        replaced_by,
    }
}

impl ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn handle_agent_chat(
        &mut self,
        request: AgentChatRequest,
    ) -> Result<AgentChatAck, AgentChatError> {
        let agent_id = request.agent_id.clone();
        if self.chain_link_enabled() {
            return self.handle_canonical_agent_chat(request);
        }
        if !self.llm_sidecar.is_llm_mode() {
            return Err(AgentChatError {
                code: "llm_mode_required".to_string(),
                message: "agent chat requires runtime live server running with --llm".to_string(),
                agent_id: Some(agent_id),
            });
        }
        let player_id = request
            .player_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let Some(player_id) = player_id else {
            return Err(AgentChatError {
                code: "player_id_required".to_string(),
                message: "agent_chat requires non-empty player_id".to_string(),
                agent_id: Some(agent_id),
            });
        };
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        let message = request.message.trim().to_string();
        if message.is_empty() {
            return Err(AgentChatError {
                code: "empty_message".to_string(),
                message: "chat message cannot be empty".to_string(),
                agent_id: Some(agent_id),
            });
        }
        let verified = self.verify_agent_chat_auth(&request)?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| AgentChatError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                agent_id: Some(agent_id.clone()),
            })?;
        let intent = resolve_agent_chat_intent(&request, verified.nonce).map_err(|message| {
            AgentChatError {
                code: "intent_seq_invalid".to_string(),
                message,
                agent_id: Some(agent_id.clone()),
            }
        })?;
        let authority = AgentIntentAuthorityContext {
            intent_tick: intent.intent_tick,
            world_id: request.world_id.clone(),
            reorg_epoch: request.reorg_epoch,
            authority_scope: request.authority_scope.clone(),
            replaces_intent_id: request.replaces_intent_id.clone(),
        };
        let durable_replay = self
            .world
            .agent_chat_intent_replay_disposition(
                verified.player_id.as_str(),
                agent_id.as_str(),
                intent.intent_seq,
                message.as_str(),
                authority.clone(),
            )
            .map_err(|error| AgentChatError {
                code: "intent_seq_conflict".to_string(),
                message: format!("durable intent conflicts with this request: {error:?}"),
                agent_id: Some(agent_id.clone()),
            })?;
        let cached_replay = self
            .llm_sidecar
            .find_chat_intent_replay(
                verified.player_id.as_str(),
                agent_id.as_str(),
                intent.intent_seq,
                intent.intent_tick,
                message.as_str(),
                public_key.as_deref(),
            )
            .map_err(|message| AgentChatError {
                code: "intent_seq_conflict".to_string(),
                message,
                agent_id: Some(agent_id.clone()),
            })?;
        if let Some(disposition) = durable_replay {
            let mut ack = durable_agent_chat_ack(
                agent_id.as_str(),
                player_id.as_str(),
                message.chars().count(),
                intent.intent_tick,
                disposition.intent_id,
                disposition.logical_time,
                disposition.event_seq,
                disposition.status,
                disposition.receipt_ref,
                disposition.replaced_by,
            );
            ack.intent_seq = Some(intent.intent_seq);
            return Ok(ack);
        }
        if cached_replay.is_some() {
            return Err(AgentChatError {
                code: "intent_persistence_failed".to_string(),
                message: "cached ack has no durable runtime intent".to_string(),
                agent_id: Some(agent_id),
            });
        }
        // Provider capability is an admission precondition. Do this before
        // nonce consumption, binding, durable acceptance, behavior feedback,
        // or provider queueing so an unsupported provider has no side effect.
        self.llm_sidecar
            .ensure_provider_agent_chat_capability(agent_id.as_str())?;
        if self.world.main_token_liquid_balance(agent_id.as_str()) == 0
            && !self
                .world
                .state()
                .starter_oc_claims
                .contains_key(agent_id.as_str())
        {
            return Err(AgentChatError {
                code: "starter_oc_required".to_string(),
                message: "claim starter OC before using LLM/agent chat".to_string(),
                agent_id: Some(agent_id),
            });
        }
        self.llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
            .map_err(|message| AgentChatError {
                code: "auth_nonce_replay".to_string(),
                message,
                agent_id: Some(agent_id.clone()),
            })?;
        self.bind_agent_player_access_for_chat(
            agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        // Commit the canonical intent before touching the provider or emitting
        // a local echo. A provider timeout must never leave an observable chat
        // side effect whose runtime authority was not persisted.
        let mut candidate = self.world.clone();
        let record_outcome = candidate.record_signed_local_agent_chat(
            &request,
            &super::agent_memory_correction::CurrentOwnerAuthority { server: self },
        );
        self.world = candidate;
        let record_outcome = record_outcome.map_err(|error| AgentChatError {
            code: "intent_persistence_failed".to_string(),
            message: format!("failed to persist canonical agent intent: {error:?}"),
            agent_id: Some(agent_id.clone()),
        })?;
        if record_outcome.is_replay() {
            // A durable replay remains authoritative after sidecar cache loss;
            // suppress duplicate provider and echo effects for the same tuple.
            let disposition = self
                .world
                .agent_chat_intent_replay_disposition(
                    verified.player_id.as_str(),
                    agent_id.as_str(),
                    intent.intent_seq,
                    message.as_str(),
                    authority,
                )
                .map_err(|error| AgentChatError {
                    code: "intent_seq_conflict".to_string(),
                    message: format!("durable replay lookup failed: {error:?}"),
                    agent_id: Some(agent_id.clone()),
                })?
                .ok_or_else(|| AgentChatError {
                    code: "intent_persistence_failed".to_string(),
                    message: "replayed intent is missing from the durable runtime ledger"
                        .to_string(),
                    agent_id: Some(agent_id.clone()),
                })?;
            let mut ack = durable_agent_chat_ack(
                agent_id.as_str(),
                player_id.as_str(),
                message.chars().count(),
                intent.intent_tick,
                disposition.intent_id,
                disposition.logical_time,
                disposition.event_seq,
                disposition.status,
                disposition.receipt_ref,
                disposition.replaced_by,
            );
            ack.intent_seq = Some(intent.intent_seq);
            self.llm_sidecar.record_chat_intent_ack(
                verified.player_id.as_str(),
                agent_id.as_str(),
                intent.intent_seq,
                intent.intent_tick,
                message.as_str(),
                public_key.as_deref(),
                &ack,
            );
            return Ok(ack);
        }
        let accepted_intent = self
            .world
            .state()
            .agents
            .get(agent_id.as_str())
            .and_then(|cell| cell.intent.clone())
            .filter(|recorded| recorded.event_seq == record_outcome.event_seq())
            .ok_or_else(|| AgentChatError {
                code: "intent_persistence_failed".to_string(),
                message: "accepted intent is missing from runtime state".to_string(),
                agent_id: Some(agent_id.clone()),
            })?;
        let chat_echo_enabled = self.config.agent_chat_echo_enabled;
        match self.llm_sidecar.push_chat_message_for_intent(
            &self.world,
            &self.snapshot_config,
            &accepted_intent,
            player_id.as_str(),
            message.as_str(),
        ) {
            Ok(()) => {
                self.llm_sidecar.request_decision();
            }
            // A local echo may stand in for an unavailable builtin runner, but
            // a provider capability refusal is authoritative: do not emit an
            // accepted-looking echo while leaving the durable intent accepted.
            Err(error) if chat_echo_enabled && error.code == "llm_init_failed" => {}
            Err(error) => {
                let disposition = if matches!(
                    error.code.as_str(),
                    "llm_init_failed"
                        | "provider_unreachable"
                        | "provider_config_missing"
                        | "provider_timeout"
                ) {
                    AgentIntentProviderFailureDisposition::Blocked
                } else {
                    AgentIntentProviderFailureDisposition::Rejected
                };
                self.world
                    .record_agent_provider_failure(
                        crate::runtime::AgentIntentReference { agent_id: agent_id.as_str(), intent_id: accepted_intent.intent_id.as_str(), request_digest: accepted_intent.request_digest.as_str() },
                        disposition,
                    )
                    .map_err(|disposition_error| AgentChatError {
                        code: "intent_disposition_failed".to_string(),
                        message: format!(
                            "provider handoff failed ({}) and disposition failed: {disposition_error:?}",
                            error.code
                        ),
                        agent_id: Some(agent_id.clone()),
                    })?;
                return Err(error);
            }
        }
        self.enqueue_agent_chat_echo_event_if_enabled(agent_id.as_str(), message.as_str());
        self.complete_agent_chat_if_receipt_bound(
            agent_id.as_str(),
            accepted_intent.intent_id.as_str(),
            accepted_intent.request_digest.as_str(),
        );
        let primary_intent = apply_accepted_primary_intent(
            self.llm_sidecar.primary_intents.get(agent_id.as_str()),
            message.as_str(),
        );
        self.llm_sidecar
            .primary_intents
            .insert(agent_id.clone(), primary_intent);
        self.set_verified_player_gameplay_feedback(&verified, PlayerGameplayRecentFeedback {
            action: "agent_chat".to_string(),
            stage: "accepted".to_string(),
            effect: format!("queued direct agent instruction for {}", agent_id),
            intent_summary: Some(format!("send direct instruction to {}", agent_id)),
            target_agent_id: Some(agent_id.clone()),
            reason: None,
            hint: Some(
                "continue the world and watch for the agent's reply or the next visible world consequence"
                    .to_string(),
            ),
            delta_logical_time: 0,
            delta_event_seq: 0,
        });
        let ack = AgentChatAck {
            auth_nonce: None,
            agent_id: agent_id.clone(),
            accepted_at_tick: accepted_intent.logical_time,
            message_len: message.chars().count(),
            player_id: Some(player_id),
            intent_tick: intent.intent_tick,
            intent_seq: Some(intent.intent_seq),
            idempotent_replay: false,
            intent_id: Some(accepted_intent.intent_id.clone()),
            accepted_event_seq: Some(accepted_intent.event_seq),
            status: Some(accepted_intent.status.clone()),
            receipt_ref: accepted_intent.receipt_ref.clone(),
            replaced_by: accepted_intent.replaced_by.clone(),
        };
        self.llm_sidecar.record_chat_intent_ack(
            verified.player_id.as_str(),
            agent_id.as_str(),
            intent.intent_seq,
            intent.intent_tick,
            message.as_str(),
            public_key.as_deref(),
            &ack,
        );
        Ok(ack)
    }
}
