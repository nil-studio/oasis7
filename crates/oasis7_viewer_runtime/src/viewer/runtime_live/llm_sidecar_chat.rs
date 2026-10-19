use super::*;

impl RuntimeLlmSidecar {
    pub(super) fn push_chat_message_with_intent(
        &mut self,
        world: &RuntimeWorld,
        config: &WorldConfig,
        agent_id: &str,
        player_id: &str,
        message: &str,
        intent_identity: Option<(&str, &str)>,
    ) -> Result<(), AgentChatError> {
        if !self.is_llm_mode() {
            return Err(AgentChatError {
                code: "llm_mode_required".to_string(),
                message: "agent chat requires runtime live server running with --llm".to_string(),
                agent_id: Some(agent_id.to_string()),
            });
        }
        if let Err(message) = self.sync_shadow_kernel(world, config) {
            return Err(AgentChatError {
                code: "llm_init_failed".to_string(),
                message,
                agent_id: Some(agent_id.to_string()),
            });
        }
        if let Err(message) = self.ensure_runner_initialized() {
            return Err(AgentChatError {
                code: "llm_init_failed".to_string(),
                message,
                agent_id: Some(agent_id.to_string()),
            });
        }
        let provider_backed = match self.runner.as_ref() {
            Some(RuntimeDecisionRunner::Builtin(_)) => false,
            Some(RuntimeDecisionRunner::ProviderBacked(_)) => {
                if !self.provider_agent_ids.contains(agent_id) {
                    return Err(AgentChatError {
                        code: "agent_not_registered".to_string(),
                        message: format!("agent {} is not registered in provider runner", agent_id),
                        agent_id: Some(agent_id.to_string()),
                    });
                }
                true
            }
            None => {
                return Err(AgentChatError {
                    code: "llm_init_failed".to_string(),
                    message: "llm runner not initialized".to_string(),
                    agent_id: Some(agent_id.to_string()),
                });
            }
        };
        if provider_backed {
            #[cfg(not(target_arch = "wasm32"))]
            {
                // AsyncAgentRunner deliberately exposes no mutable behavior
                // handle on the world thread. Chat is queued for its
                // dedicated provider endpoint; the actor remains isolated
                // from world-thread mutation while that request is drained.
                self.push_provider_player_message_feedback(world, agent_id, message)?;
                self.pending_provider_agent_chats
                    .push_back(RuntimePendingProviderAgentChat {
                        agent_id: agent_id.to_string(),
                        player_id: player_id.to_string(),
                        message: message.to_string(),
                        intent_id: intent_identity.map(|(intent_id, _)| intent_id.to_string()),
                        request_digest: intent_identity
                            .map(|(_, request_digest)| request_digest.to_string()),
                    });
                Ok(())
            }
            #[cfg(target_arch = "wasm32")]
            {
                let Some(RuntimeDecisionRunner::ProviderBacked(runner)) = self.runner.as_mut()
                else {
                    return Err(AgentChatError {
                        code: "llm_init_failed".to_string(),
                        message: "provider runner disappeared while sending agent chat".to_string(),
                        agent_id: Some(agent_id.to_string()),
                    });
                };
                let Some(agent) = runner.get_mut(agent_id) else {
                    return Err(AgentChatError {
                        code: "agent_not_registered".to_string(),
                        message: format!("agent {} is not registered in provider runner", agent_id),
                        agent_id: Some(agent_id.to_string()),
                    });
                };
                agent
                    .behavior
                    .push_player_message_feedback(world.state().time, message)
                    .map_err(|error| AgentChatError {
                        code: error.code,
                        message: error.message,
                        agent_id: Some(agent_id.to_string()),
                    })?;
                self.pending_provider_agent_chats
                    .push_back(RuntimePendingProviderAgentChat {
                        agent_id: agent_id.to_string(),
                        player_id: player_id.to_string(),
                        message: message.to_string(),
                        intent_id: intent_identity.map(|(intent_id, _)| intent_id.to_string()),
                        request_digest: intent_identity
                            .map(|(_, request_digest)| request_digest.to_string()),
                    });
                Ok(())
            }
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if message.trim().is_empty() {
                    return Err(AgentChatError {
                        code: "empty_message".to_string(),
                        message: "chat message cannot be empty".to_string(),
                        agent_id: Some(agent_id.to_string()),
                    });
                }
                let Some(runner) = self
                    .runner
                    .as_mut()
                    .and_then(RuntimeDecisionRunner::async_runner_mut)
                else {
                    return Err(AgentChatError {
                        code: "llm_init_failed".to_string(),
                        message: "builtin llm runner disappeared while sending agent chat"
                            .to_string(),
                        agent_id: Some(agent_id.to_string()),
                    });
                };
                runner
                    .notify_player_message(agent_id, world.state().time, message)
                    .map_err(|error| AgentChatError {
                        code: error.code().to_string(),
                        message: error.to_string(),
                        agent_id: Some(agent_id.to_string()),
                    })?;
                Ok(())
            }
            #[cfg(target_arch = "wasm32")]
            {
                let Some(RuntimeDecisionRunner::Builtin(runner)) = self.runner.as_mut() else {
                    return Err(AgentChatError {
                        code: "llm_init_failed".to_string(),
                        message: "builtin llm runner disappeared while sending agent chat"
                            .to_string(),
                        agent_id: Some(agent_id.to_string()),
                    });
                };
                {
                    let Some(agent) = runner.get_mut(agent_id) else {
                        return Err(AgentChatError {
                            code: "agent_not_registered".to_string(),
                            message: format!("agent {} is not registered in llm runner", agent_id),
                            agent_id: Some(agent_id.to_string()),
                        });
                    };
                    if !agent
                        .behavior
                        .push_player_message(world.state().time, message)
                    {
                        return Err(AgentChatError {
                            code: "empty_message".to_string(),
                            message: "chat message cannot be empty".to_string(),
                            agent_id: Some(agent_id.to_string()),
                        });
                    }
                    Ok(())
                }
            }
        }
    }
}
