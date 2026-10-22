//! Harness adapter keeps typed supplier history private to the model session.
use crate::{
    connection::{ConnectionManager, Principal},
    protocol::{Conversation, Message, Tool, ToolCall},
    transport::{ModelTransport, validate_input},
};
use oasis7_agent_api::{
    ModelProvider, ModelProviderError, ModelProviderHistory, ModelProviderRequest,
    ModelProviderResponse, ModelProviderToolCall, ModelProviderToolResult, ModelProviderTrace,
    ModelProviderTurn, ModelProviderUsage,
};

pub struct SessionModelProvider {
    manager: ConnectionManager,
    principal: Principal,
    connection_id: String,
    version: u64,
    generation: u64,
    conversation: Option<Conversation>,
    pending: Vec<ToolCall>,
    provider_id: String,
}
fn blocked(code: &str) -> ModelProviderError {
    ModelProviderError::new(code, "model session request blocked", false)
}
impl SessionModelProvider {
    pub fn new(
        manager: ConnectionManager,
        principal: Principal,
        connection_id: String,
        version: u64,
        generation: u64,
    ) -> Result<Self, ModelProviderError> {
        let snapshot = manager
            .snapshot(&principal, &connection_id)
            .map_err(|_| blocked("session_expired"))?;
        if snapshot.active_version != Some(version)
            || snapshot.generation != generation
            || snapshot.run_state != crate::connection::RunState::Active
        {
            return Err(blocked("game_executor_unavailable"));
        }
        Ok(Self {
            manager,
            principal,
            connection_id,
            version,
            generation,
            conversation: None,
            pending: vec![],
            provider_id: "session_byok".into(),
        })
    }
}
impl ModelProvider for SessionModelProvider {
    fn history_mode(&self) -> ModelProviderHistory {
        ModelProviderHistory::ProviderConversation
    }
    fn preflight(&self, request: &ModelProviderRequest) -> Result<(), ModelProviderError> {
        self.candidate(request).map(|_| ())
    }
    fn check_execution(&self) -> Result<(), ModelProviderError> {
        self.manager
            .check_execution(
                &self.principal,
                &self.connection_id,
                self.version,
                self.generation,
            )
            .map_err(|_| blocked("model_session_unavailable"))
    }
    fn provider_id(&self) -> &str {
        &self.provider_id
    }
    fn pending_tool_calls(&self) -> Vec<ModelProviderToolCall> {
        self.pending
            .iter()
            .map(|call| ModelProviderToolCall {
                call_id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            })
            .collect()
    }
    fn record_tool_results(
        &mut self,
        results: &[ModelProviderToolResult],
    ) -> Result<(), ModelProviderError> {
        if self.pending.is_empty()
            || results.len() != self.pending.len()
            || self
                .pending
                .iter()
                .zip(results)
                .any(|(call, result)| call.id != result.call_id)
        {
            return Err(blocked("tool_result_identity_mismatch"));
        }
        let snapshot = self
            .manager
            .snapshot(&self.principal, &self.connection_id)
            .map_err(|_| blocked("session_expired"))?;
        if snapshot.active_version != Some(self.version)
            || snapshot.generation != self.generation
            || snapshot.run_state != crate::connection::RunState::Active
        {
            return Err(blocked("session_expired"));
        }
        let conversation = self
            .conversation
            .as_mut()
            .ok_or_else(|| blocked("tool_history_missing"))?;
        for result in results {
            conversation.messages.push(Message::ToolResult {
                call_id: result.call_id.clone(),
                content: result.content.to_string(),
            });
        }
        self.pending.clear();
        Ok(())
    }
    fn complete(
        &mut self,
        request: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        self.complete_using(request, |permit, conversation| {
            ModelTransport.send_with_timeout(permit, conversation, request.budget.max_latency_ms)
        })
    }
}
impl SessionModelProvider {
    fn candidate(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<Conversation, ModelProviderError> {
        request.validate()?;
        if request.budget.max_model_calls == 0
            || request.budget.max_latency_ms == 0
            || request.budget.max_tool_calls == 0
        {
            return Err(blocked("budget_exhausted"));
        }
        if !self.pending.is_empty() {
            return Err(blocked("tool_results_required"));
        }
        let tools = request
            .tools
            .iter()
            .map(|t| Tool {
                name: t.name.clone(),
                description: t.description.clone().unwrap_or_default(),
                parameters: t.parameters.clone(),
            })
            .collect::<Vec<_>>();
        let output = u32::try_from(self.manager.limits().max_output_tokens)
            .map_err(|_| blocked("budget_exhausted"))?;
        let mut conversation = self.conversation.clone().unwrap_or_else(|| Conversation {
            system_prompt: request.system_prompt.clone(),
            messages: vec![],
            tools: tools.clone(),
            max_output_tokens: output,
        });
        if conversation.system_prompt != request.system_prompt || conversation.tools != tools {
            return Err(blocked("model_context_changed"));
        }
        conversation.messages.push(Message::User {
            content: request.user_prompt.clone(),
        });
        let preset = self
            .manager
            .active_preset(
                &self.principal,
                &self.connection_id,
                self.version,
                self.generation,
            )
            .map_err(|_| blocked("model_session_unavailable"))?;
        if preset.model != request.model {
            return Err(blocked("model_identity_mismatch"));
        }
        validate_input(&preset, self.manager.limits(), &conversation)
            .map_err(|e| blocked(&e.to_string()))?;
        Ok(conversation)
    }
    fn complete_using(
        &mut self,
        request: &ModelProviderRequest,
        send: impl FnOnce(
            &crate::connection::RequestPermit,
            &Conversation,
        )
            -> Result<crate::protocol::ProviderResponse, crate::protocol::ProviderFailure>,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        let mut conversation = self.candidate(request)?;
        let reservation = self
            .manager
            .limits()
            .max_input_tokens
            .saturating_add(self.manager.limits().max_output_tokens);
        let permit = self
            .manager
            .reserve_run(
                &self.principal,
                &self.connection_id,
                self.version,
                self.generation,
                reservation,
            )
            .map_err(|_| blocked("model_session_unavailable"))?;
        if permit.preset().model != request.model {
            let _ = permit.finish(None, false);
            return Err(blocked("model_identity_mismatch"));
        }
        // Admission failures leave the durable history untouched. Once reserved,
        // an uncertain physical request remains conservatively budgeted.
        self.conversation = Some(conversation.clone());
        let response = match send(&permit, &conversation) {
            Ok(response) => response,
            Err(error) => {
                let _ = permit.finish(None, false);
                return Err(blocked(&error.to_string()));
            }
        };
        // Completion rechecks owner/session, connection version, generation,
        // lease and Runtime binding before any response reaches the runner.
        permit
            .finish(response.usage.total_tokens, true)
            .map_err(|_| blocked("model_result_expired"))?;
        if response.tool_calls.len() > request.budget.max_tool_calls as usize {
            return Err(blocked("tool_budget_exhausted"));
        }
        if response.tool_calls.is_empty() {
            return Err(blocked("typed_tool_call_required"));
        }
        self.pending = response.tool_calls;
        conversation.messages.extend(response.replay);
        self.conversation = Some(conversation);
        Ok(ModelProviderResponse {
            request_digest: request.request_digest.clone(),
            turns: self
                .pending
                .iter()
                .map(|c| ModelProviderTurn::ToolCall {
                    name: c.name.clone(),
                    arguments: c.arguments.clone(),
                })
                .collect(),
            output: String::new(),
            model: Some(request.model.clone()),
            usage: ModelProviderUsage {
                prompt_tokens: response.usage.input_tokens,
                completion_tokens: response.usage.output_tokens,
                total_tokens: response.usage.total_tokens,
            },
            trace: ModelProviderTrace {
                provider_id: Some(self.provider_id.clone()),
                ..Default::default()
            },
        })
    }
}

#[cfg(test)]
#[path = "harness_adapter_tests.rs"]
mod tests;
