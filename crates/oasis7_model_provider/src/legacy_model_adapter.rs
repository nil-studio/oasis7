//! Legacy single-call completion adapter with one concrete transport owner.
//! API semantics are shared with the original host; no Runtime/World access.
use oasis7_agent_api::{
    ModelProvider, ModelProviderError, ModelProviderRequest, ModelProviderResponse,
    ModelProviderTrace, ModelProviderTurn, ModelProviderUsage,
};

/// Additive bridge for the existing native completion client. This keeps
/// `LlmCompletionClient` source-compatible while giving the next TurnEngine
/// implementation one provider-neutral inference seam.
pub struct LlmCompletionModelProvider<C> {
    provider_id: String,
    client: C,
}

impl<C> LlmCompletionModelProvider<C> {
    pub fn new(provider_id: impl Into<String>, client: C) -> Self {
        Self {
            provider_id: provider_id.into(),
            client,
        }
    }

    pub fn into_inner(self) -> C {
        self.client
    }
}

impl<C: crate::LlmCompletionClient + Send> ModelProvider for LlmCompletionModelProvider<C> {
    fn provider_id(&self) -> &str {
        self.provider_id.as_str()
    }

    fn complete(
        &mut self,
        request: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        request.validate()?;
        let completion_request = crate::LlmCompletionRequest {
            model: request.model.clone(),
            system_prompt: request.system_prompt.clone(),
            user_prompt: request.user_prompt.clone(),
            debug_mode: false,
            max_model_calls: Some(request.budget.max_model_calls),
        };
        let completion = self.client.complete(&completion_request).map_err(|error| {
            let (code, retryable) = match &error {
                crate::LlmClientError::Http { .. } | crate::LlmClientError::HttpStatus { .. } => {
                    ("provider_unavailable", true)
                }
                crate::LlmClientError::BuildClient { .. } => ("provider_unavailable", false),
                crate::LlmClientError::DecodeResponse { .. }
                | crate::LlmClientError::EmptyChoice => ("invalid_model_response", false),
            };
            ModelProviderError::new(code, error.to_string(), retryable)
        })?;
        let turns = completion
            .turns
            .into_iter()
            .map(|turn| match turn {
                crate::LlmCompletionTurn::Decision { payload } => {
                    ModelProviderTurn::Decision { payload }
                }
                crate::LlmCompletionTurn::ModuleCall { module, args } => {
                    ModelProviderTurn::ToolCall {
                        name: module,
                        arguments: args,
                    }
                }
            })
            .collect();
        Ok(ModelProviderResponse {
            request_digest: request.request_digest.clone(),
            turns,
            output: completion.output,
            model: completion.model,
            usage: ModelProviderUsage {
                prompt_tokens: completion.prompt_tokens,
                completion_tokens: completion.completion_tokens,
                total_tokens: completion.total_tokens,
            },
            trace: ModelProviderTrace {
                provider_id: Some(self.provider_id.clone()),
                ..ModelProviderTrace::default()
            },
        })
    }
}
