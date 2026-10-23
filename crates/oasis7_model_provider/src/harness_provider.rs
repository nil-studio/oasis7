use super::*;
use oasis7_agent_api::{
    ModelProvider, ModelProviderError, ModelProviderRequest, ModelProviderResponse,
    ModelProviderTool, ModelProviderTrace, ModelProviderTurn, ModelProviderUsage,
};

/// Explicit tool surface for a physical inference call. The legacy profile
/// preserves the existing game behavior schema; caller tools grant no world authority.
pub enum CompletionToolProfile<'a> {
    LegacySimulator,
    Caller(&'a [ModelProviderTool]),
}
impl OpenAiChatCompletionClient {
    pub fn complete_with_tools(
        &self,
        request: &LlmCompletionRequest,
        tools: &[ModelProviderTool],
    ) -> Result<LlmCompletionResult, LlmClientError> {
        self.complete_with_profile(request, CompletionToolProfile::Caller(tools))
    }
    pub fn complete_with_profile(
        &self,
        request: &LlmCompletionRequest,
        profile: CompletionToolProfile<'_>,
    ) -> Result<LlmCompletionResult, LlmClientError> {
        if request.max_model_calls == Some(0) {
            return Err(LlmClientError::DecodeResponse {
                message: "model call budget exhausted".into(),
            });
        }
        match profile {
            CompletionToolProfile::LegacySimulator => LlmCompletionClient::complete(self, request),
            CompletionToolProfile::Caller(tools) => {
                let mut names = std::collections::BTreeSet::new();
                let mut mapped = Vec::with_capacity(tools.len());
                for tool in tools {
                    if tool.name.is_empty()
                        || tool.name.len() > 128
                        || !tool
                            .name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                        || !names.insert(&tool.name)
                        || !tool.parameters.is_object()
                    {
                        return Err(LlmClientError::DecodeResponse {
                            message: "invalid or duplicate caller tool schema".into(),
                        });
                    }
                    mapped.push(Tool::Function(FunctionTool {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        parameters: Some(tool.parameters.clone()),
                        strict: None,
                        defer_loading: None,
                        r#async: None,
                        output_schema: None,
                        allowed_callers: None,
                    }));
                }
                if mapped.is_empty() {
                    return Err(LlmClientError::DecodeResponse {
                        message: "caller tool profile is empty".into(),
                    });
                }
                let mut payload = build_responses_request_payload(request)?;
                payload.tools = Some(mapped);
                let mut client = self.clone();
                client.legacy_tool_profile = false;
                let result = client.complete_payload(request, payload)?;
                if result.turns.iter().any(|turn| match turn {
                    LlmCompletionTurn::ModuleCall { module, .. } => !names.contains(module),
                    LlmCompletionTurn::Decision { .. } => true,
                }) {
                    return Err(LlmClientError::DecodeResponse {
                        message: "provider returned an undeclared caller tool".into(),
                    });
                }
                Ok(result)
            }
        }
    }
}
impl ModelProvider for OpenAiChatCompletionClient {
    fn provider_id(&self) -> &str {
        "openai-responses"
    }
    fn complete(
        &mut self,
        request: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        request.validate()?;
        if request.budget.max_latency_ms == 0
            || request.budget.max_model_calls == 0
            || request.budget.max_tool_calls == 0
        {
            return Err(ModelProviderError::new(
                "model_budget_exhausted",
                "physical model request budget exhausted",
                false,
            ));
        }
        let mut client = self.clone();
        // complete_payload installs the no-retry HTTP service for every native
        // budgeted call, whether or not this request tightens the timeout.
        client.request_timeout_ms = self.request_timeout_ms.min(request.budget.max_latency_ms);
        let started = std::time::Instant::now();
        let completion = client
            .complete_with_tools(
                &LlmCompletionRequest {
                    model: request.model.clone(),
                    system_prompt: request.system_prompt.clone(),
                    user_prompt: request.user_prompt.clone(),
                    debug_mode: false,
                    max_model_calls: Some(request.budget.max_model_calls),
                },
                &request.tools,
            )
            .map_err(provider_error)?;
        if completion.turns.len() > request.budget.max_tool_calls as usize {
            return Err(ModelProviderError::new(
                "model_tool_budget_exceeded",
                "provider returned more tool calls than admitted",
                false,
            ));
        }
        Ok(ModelProviderResponse {
            request_digest: request.request_digest.clone(),
            turns: completion
                .turns
                .into_iter()
                .map(|turn| match turn {
                    LlmCompletionTurn::Decision { payload } => {
                        ModelProviderTurn::Decision { payload }
                    }
                    LlmCompletionTurn::ModuleCall { module, args } => ModelProviderTurn::ToolCall {
                        name: module,
                        arguments: args,
                    },
                })
                .collect(),
            output: completion.output,
            model: completion.model,
            usage: ModelProviderUsage {
                prompt_tokens: completion.prompt_tokens,
                completion_tokens: completion.completion_tokens,
                total_tokens: completion.total_tokens,
            },
            trace: ModelProviderTrace {
                provider_id: Some(self.provider_id().into()),
                latency_ms: Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
                ..Default::default()
            },
        })
    }
}
fn provider_error(error: LlmClientError) -> ModelProviderError {
    let (code, retryable) = match &error {
        LlmClientError::Http { .. } | LlmClientError::HttpStatus { .. } => {
            ("provider_unavailable", true)
        }
        LlmClientError::BuildClient { .. } => ("provider_unavailable", false),
        LlmClientError::DecodeResponse { .. } | LlmClientError::EmptyChoice => {
            ("invalid_model_response", false)
        }
    };
    ModelProviderError::new(code, error.to_string(), retryable)
}
