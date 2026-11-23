use oasis7_agent_api::{
    BudgetContractV1, Digest32, ModelProvider, ModelProviderRequest, ModelProviderTurn,
};
use oasis7_model_provider::{
    LlmClientError, LlmCompletionClient, LlmCompletionRequest, LlmCompletionResult,
    LlmCompletionTurn, legacy_model_adapter::LlmCompletionModelProvider,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct ProbeClient {
    calls: Arc<AtomicUsize>,
    fail_transport: bool,
}

impl LlmCompletionClient for ProbeClient {
    fn complete(
        &self,
        request: &LlmCompletionRequest,
    ) -> Result<LlmCompletionResult, LlmClientError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.model, "fixture-model");
        assert_eq!(request.system_prompt, "system");
        assert_eq!(request.user_prompt, "user");
        assert!(!request.debug_mode);
        assert_eq!(request.max_model_calls, Some(1));
        if self.fail_transport {
            return Err(LlmClientError::Http {
                message: "service unavailable".into(),
            });
        }
        Ok(LlmCompletionResult {
            turns: vec![
                LlmCompletionTurn::Decision {
                    payload: serde_json::json!({"action":"wait"}),
                },
                LlmCompletionTurn::ModuleCall {
                    module: "world.rules.guide".into(),
                    args: serde_json::json!({"topic":"fees"}),
                },
            ],
            output: "ok".into(),
            model: Some("fixture-model".into()),
            prompt_tokens: Some(2),
            completion_tokens: Some(3),
            total_tokens: Some(5),
        })
    }
}

fn request() -> ModelProviderRequest {
    ModelProviderRequest {
        request_digest: Digest32(format!("blake3:{}", "a".repeat(64))),
        model: "fixture-model".into(),
        system_prompt: "system".into(),
        user_prompt: "user".into(),
        tools: Vec::new(),
        budget: BudgetContractV1 {
            max_latency_ms: 1_000,
            max_repair_attempts: 0,
            max_model_calls: 1,
            max_tool_calls: 1,
        },
    }
}

#[test]
fn direct_owner_preserves_original_single_call_identity_turns_usage_and_trace() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut adapter = LlmCompletionModelProvider::new(
        "native-legacy",
        ProbeClient {
            calls: calls.clone(),
            fail_transport: false,
        },
    );
    let response = adapter
        .complete_checked(&request())
        .expect("valid legacy response");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(response.request_digest, request().request_digest);
    assert_eq!(response.usage.total_tokens, Some(5));
    assert_eq!(response.trace.provider_id.as_deref(), Some("native-legacy"));
    assert_eq!(response.turns.len(), 2);
    assert!(matches!(
        &response.turns[0],
        ModelProviderTurn::Decision { payload } if payload["action"] == "wait"
    ));
    assert!(matches!(
        &response.turns[1],
        ModelProviderTurn::ToolCall { name, arguments }
            if name == "world.rules.guide" && arguments["topic"] == "fees"
    ));
}

#[test]
fn direct_owner_rejects_bad_request_identity_without_invoking_client() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut adapter = LlmCompletionModelProvider::new(
        "native-legacy",
        ProbeClient {
            calls: calls.clone(),
            fail_transport: false,
        },
    );
    let mut invalid = request();
    invalid.request_digest = Digest32("not-canonical".into());
    let error = adapter.complete_checked(&invalid).unwrap_err();
    assert_eq!(error.code(), "invalid_request_digest");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn direct_owner_transport_failure_keeps_original_retryable_classification() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut adapter = LlmCompletionModelProvider::new(
        "native-legacy",
        ProbeClient {
            calls: calls.clone(),
            fail_transport: true,
        },
    );
    let error = adapter.complete_checked(&request()).unwrap_err();
    assert_eq!(error.code(), "provider_unavailable");
    assert!(error.retryable);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
