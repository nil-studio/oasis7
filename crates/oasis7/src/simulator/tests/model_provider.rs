use crate::simulator::{
    BudgetContractV1, Digest32, LlmCompletionClient, LlmCompletionModelProvider,
    LlmCompletionRequest, LlmCompletionResult, LlmCompletionTurn, ModelProvider,
    ModelProviderError, ModelProviderRequest, ModelProviderResponse, ModelProviderTrace,
    ModelProviderTurn, ModelProviderUsage,
};

struct FixtureProvider;

impl ModelProvider for FixtureProvider {
    fn provider_id(&self) -> &str {
        "fixture"
    }

    fn complete(
        &mut self,
        request: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        assert_eq!(request.budget.max_model_calls, 1);
        Ok(ModelProviderResponse {
            request_digest: request.request_digest.clone(),
            turns: vec![ModelProviderTurn::Decision {
                payload: serde_json::json!({"decision": "wait"}),
            }],
            output: "wait".to_string(),
            model: Some(request.model.clone()),
            usage: ModelProviderUsage {
                prompt_tokens: Some(3),
                completion_tokens: Some(1),
                total_tokens: Some(4),
            },
            trace: ModelProviderTrace {
                provider_id: Some(self.provider_id().to_string()),
                output_summary: Some("decision=wait".to_string()),
                ..ModelProviderTrace::default()
            },
        })
    }
}

fn request() -> ModelProviderRequest {
    ModelProviderRequest {
        request_digest: Digest32(
            "blake3:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        ),
        model: "fixture-model".to_string(),
        system_prompt: "system".to_string(),
        user_prompt: "observe".to_string(),
        tools: Vec::new(),
        budget: BudgetContractV1 {
            max_latency_ms: 1_000,
            max_repair_attempts: 1,
            max_model_calls: 1,
            max_tool_calls: 0,
        },
    }
}

#[test]
fn model_provider_preserves_typed_turn_usage_and_trace() {
    let mut provider = FixtureProvider;
    let response = provider.complete(&request()).expect("fixture completion");
    assert_eq!(provider.provider_id(), "fixture");
    assert!(matches!(
        response.turns.first(),
        Some(ModelProviderTurn::Decision { .. })
    ));
    assert_eq!(response.usage.total_tokens, Some(4));
    assert_eq!(response.trace.provider_id.as_deref(), Some("fixture"));
    assert_eq!(response.request_digest, request().request_digest);
}

#[test]
fn model_provider_rejects_noncanonical_request_identity_before_inference() {
    let mut provider = FixtureProvider;
    let mut request = request();
    request.request_digest = Digest32("legacy-timeout-key".to_string());
    let error = provider
        .complete_checked(&request)
        .expect_err("noncanonical digest must be rejected before provider call");
    assert_eq!(error.code(), "invalid_request_digest");
}

struct LegacyClient;

impl LlmCompletionClient for LegacyClient {
    fn complete(
        &self,
        request: &LlmCompletionRequest,
    ) -> Result<LlmCompletionResult, crate::simulator::LlmClientError> {
        Ok(LlmCompletionResult {
            turns: vec![LlmCompletionTurn::ModuleCall {
                module: "agent_tool".to_string(),
                args: serde_json::json!({"value": 1}),
            }],
            output: request.user_prompt.clone(),
            model: Some(request.model.clone()),
            prompt_tokens: Some(2),
            completion_tokens: Some(2),
            total_tokens: Some(4),
        })
    }
}

#[test]
fn model_provider_adapts_existing_completion_client_without_changing_typed_output() {
    let mut provider: oasis7_model_provider::legacy_model_adapter::LlmCompletionModelProvider<_> =
        LlmCompletionModelProvider::new("legacy-client", LegacyClient);
    let response = provider
        .complete_checked(&request())
        .expect("legacy completion adapter");
    assert_eq!(response.trace.provider_id.as_deref(), Some("legacy-client"));
    assert!(matches!(
        response.turns.first(),
        Some(ModelProviderTurn::ToolCall { name, .. }) if name == "agent_tool"
    ));
    assert_eq!(response.usage.total_tokens, Some(4));
}
