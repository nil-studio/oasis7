use super::*;

#[derive(Debug, Clone)]
pub(in crate::viewer::runtime_live) struct RuntimeProviderAgentChatFailure {
    pub(in crate::viewer::runtime_live) pending: RuntimePendingProviderAgentChat,
    pub(in crate::viewer::runtime_live) error: AgentChatError,
    pub(in crate::viewer::runtime_live) retryable: bool,
}

#[derive(Debug)]
pub(in crate::viewer::runtime_live) struct RuntimeProviderAgentChatRequestError {
    pub(in crate::viewer::runtime_live) error: AgentChatError,
    pub(in crate::viewer::runtime_live) retryable: bool,
}

fn validate_provider_agent_chat_response(
    requested_agent_id: &str,
    response: crate::simulator::ProviderAgentChatResponse,
) -> Result<String, RuntimeProviderAgentChatRequestError> {
    if response.agent_id.trim() != requested_agent_id {
        return Err(RuntimeProviderAgentChatRequestError {
            retryable: false,
            error: AgentChatError {
                code: "provider_agent_chat_agent_mismatch".to_string(),
                message: "provider returned a response for a different Agent".to_string(),
                agent_id: Some(requested_agent_id.to_string()),
            },
        });
    }

    let reply = response.message.trim();
    if reply.is_empty() {
        return Err(RuntimeProviderAgentChatRequestError {
            retryable: true,
            error: AgentChatError {
                code: "provider_empty_chat_response".to_string(),
                message: "provider returned an empty agent chat response".to_string(),
                agent_id: Some(requested_agent_id.to_string()),
            },
        });
    }

    Ok(reply.to_string())
}

impl RuntimeLlmSidecar {
    /// Check the provider's Agent Chat capability before the control plane
    /// admits an authenticated Intent. This is deliberately separate from
    /// the eventual request: a queued provider call must never be the first
    /// place where an unsupported capability is discovered.
    pub(in crate::viewer::runtime_live) fn ensure_provider_agent_chat_capability(
        &self,
        agent_id: &str,
    ) -> Result<(), AgentChatError> {
        // Builtin/echo Agent Chat does not have a remote capability contract;
        // only provider-backed admission may perform this network preflight.
        if !env_requests_provider_backend() {
            return Ok(());
        }
        self.ensure_provider_agent_chat_capability_with_retry(agent_id)
            .map_err(|failure| failure.error)
    }

    fn ensure_provider_agent_chat_capability_with_retry(
        &self,
        agent_id: &str,
    ) -> Result<(), RuntimeProviderAgentChatRequestError> {
        let (client, _) = self.provider_agent_chat_client(agent_id)?;
        let info =
            client
                .provider_info()
                .map_err(|error| RuntimeProviderAgentChatRequestError {
                    retryable: error.retryable(),
                    error: AgentChatError {
                        code: "provider_unreachable".to_string(),
                        message: error.to_string(),
                        agent_id: Some(agent_id.to_string()),
                    },
                })?;
        if !info.capabilities.iter().any(|capability| {
            capability
                .trim()
                .eq_ignore_ascii_case(PROVIDER_AGENT_CHAT_CAPABILITY)
        }) {
            return Err(RuntimeProviderAgentChatRequestError {
                retryable: false,
                error: AgentChatError {
                    code: "agent_provider_chat_unsupported".to_string(),
                    message: "configured provider does not advertise the agent_chat capability"
                        .to_string(),
                    agent_id: Some(agent_id.to_string()),
                },
            });
        }
        Ok(())
    }

    fn provider_agent_chat_client(
        &self,
        agent_id: &str,
    ) -> Result<
        (ProviderLoopbackHttpClient, ProviderDecisionSettings),
        RuntimeProviderAgentChatRequestError,
    > {
        let settings = provider_settings_from_env()
            .map_err(|message| RuntimeProviderAgentChatRequestError {
                error: AgentChatError {
                    code: "provider_config_invalid".to_string(),
                    message,
                    agent_id: Some(agent_id.to_string()),
                },
                retryable: false,
            })?
            .ok_or_else(|| RuntimeProviderAgentChatRequestError {
                error: AgentChatError {
                    code: "provider_config_missing".to_string(),
                    message: "provider-backed agent chat requires provider settings".to_string(),
                    agent_id: Some(agent_id.to_string()),
                },
                retryable: true,
            })?;
        let client = ProviderLoopbackHttpClient::new_with_transport(
            settings.base_url.as_str(),
            settings.auth_token.as_deref(),
            settings.decision_timeout_ms,
            settings.provider_transport.as_str(),
        )
        .map_err(|error| RuntimeProviderAgentChatRequestError {
            retryable: error.retryable(),
            error: AgentChatError {
                code: "provider_unreachable".to_string(),
                message: error.to_string(),
                agent_id: Some(agent_id.to_string()),
            },
        })?;
        Ok((client, settings))
    }

    /// Preserve the provider feedback handoff performed by the legacy
    /// provider behavior before the native async runner queues Agent Chat.
    /// This is an input observation, not a Runtime gameplay effect.
    pub(super) fn push_provider_player_message_feedback(
        &self,
        world: &RuntimeWorld,
        agent_id: &str,
        message: &str,
    ) -> Result<(), AgentChatError> {
        let (client, _) = self
            .provider_agent_chat_client(agent_id)
            .map_err(|failure| failure.error)?;
        let summary = format!("player_message: {}", message.trim());
        let ack = client
            .submit_feedback(&crate::simulator::FeedbackEnvelope {
                action_id: world.state().time,
                success: true,
                reject_reason: None,
                emitted_events: Vec::new(),
                world_delta_summary: Some(summary),
            })
            .map_err(|error| AgentChatError {
                code: if error.retryable() {
                    "provider_unreachable".to_string()
                } else {
                    "provider_feedback_rejected".to_string()
                },
                message: error.to_string(),
                agent_id: Some(agent_id.to_string()),
            })?;
        if ack.ok {
            return Ok(());
        }
        Err(AgentChatError {
            code: ack
                .error_code
                .unwrap_or_else(|| "provider_feedback_rejected".to_string()),
            message: ack
                .error
                .unwrap_or_else(|| "provider feedback endpoint rejected payload".to_string()),
            agent_id: Some(agent_id.to_string()),
        })
    }

    pub(super) fn request_provider_agent_chat(
        &self,
        world: &RuntimeWorld,
        agent_id: &str,
        player_id: &str,
        message: &str,
    ) -> Result<Option<String>, RuntimeProviderAgentChatRequestError> {
        let (client, _) = self.provider_agent_chat_client(agent_id)?;
        let info =
            client
                .provider_info()
                .map_err(|error| RuntimeProviderAgentChatRequestError {
                    retryable: error.retryable(),
                    error: AgentChatError {
                        code: "provider_unreachable".to_string(),
                        message: error.to_string(),
                        agent_id: Some(agent_id.to_string()),
                    },
                })?;
        if !info.capabilities.iter().any(|capability| {
            capability
                .trim()
                .eq_ignore_ascii_case(PROVIDER_AGENT_CHAT_CAPABILITY)
        }) {
            return Err(RuntimeProviderAgentChatRequestError {
                retryable: false,
                error: AgentChatError {
                    code: "agent_provider_chat_unsupported".to_string(),
                    message: "configured provider does not advertise the agent_chat capability"
                        .to_string(),
                    agent_id: Some(agent_id.to_string()),
                },
            });
        }
        let agent = world.state().agents.get(agent_id);
        let location_id = agent.map(|agent| location_id_for_pos(agent.state.pos));
        let resources = agent.and_then(|agent| serde_json::to_value(&agent.state.resources).ok());
        let provider_request = ProviderAgentChatRequest {
            agent_id: agent_id.to_string(),
            player_id: player_id.to_string(),
            message: message.to_string(),
            world_time: world.state().time,
            location_id,
            resources,
            recent_feedback: Vec::new(),
        };
        let chat_request_key = provider_agent_chat_log_key(&provider_request);
        let started = Instant::now();
        tracing::info!(
            chat_request_key = chat_request_key.as_str(),
            agent_id,
            player_id,
            world_time = provider_request.world_time,
            "provider-backed agent chat request started"
        );
        let response = match client.request_agent_chat(&provider_request) {
            Ok(response) => {
                tracing::info!(
                    chat_request_key = chat_request_key.as_str(),
                    agent_id,
                    player_id,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "provider-backed agent chat request succeeded"
                );
                response
            }
            Err(error) => {
                tracing::warn!(
                    chat_request_key = chat_request_key.as_str(),
                    agent_id,
                    player_id,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    error = error.to_string().as_str(),
                    "provider-backed agent chat request failed"
                );
                return Err(RuntimeProviderAgentChatRequestError {
                    retryable: error.retryable(),
                    error: AgentChatError {
                        code: "provider_unreachable".to_string(),
                        message: error.to_string(),
                        agent_id: Some(agent_id.to_string()),
                    },
                });
            }
        };
        let reply = validate_provider_agent_chat_response(agent_id, response)?;
        Ok(Some(reply))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulator::ProviderAgentChatResponse;

    #[test]
    fn provider_response_agent_mismatch_is_non_retryable_and_not_returned() {
        let response = ProviderAgentChatResponse {
            agent_id: "agent-1".to_string(),
            message: "private response for another Agent".to_string(),
            location_id: None,
        };

        let error = validate_provider_agent_chat_response("agent-0", response)
            .expect_err("a response for another Agent must be rejected");

        assert!(!error.retryable);
        assert_eq!(error.error.code, "provider_agent_chat_agent_mismatch");
        assert_eq!(
            error.error.message,
            "provider returned a response for a different Agent"
        );
        assert_eq!(error.error.agent_id.as_deref(), Some("agent-0"));
    }

    #[test]
    fn provider_response_agent_id_is_trimmed_before_exact_match() {
        let response = ProviderAgentChatResponse {
            agent_id: "  agent-0  ".to_string(),
            message: "  hello Agent  ".to_string(),
            location_id: None,
        };

        let reply = validate_provider_agent_chat_response("agent-0", response)
            .expect("matching provider Agent response should be accepted");

        assert_eq!(reply, "hello Agent");
    }
}
