//! Provider-neutral single-call inference boundary.
//!
//! This is the former `oasis7` model-provider contract without its application
//!-specific `LlmCompletionClient` adapter. It is deliberately unable to commit
//! a decision or mutate Runtime state.

use std::error::Error;
use std::fmt;

use crate::{BudgetContractV1, Digest32};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelProviderTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

/// Protocol identity retained outside world DTOs. Tool execution still requires
/// the host's current game authority; a provider call ID is not an action ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelProviderToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelProviderToolResult {
    pub call_id: String,
    pub content: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelProviderRequest {
    pub request_digest: Digest32,
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    #[serde(default)]
    pub tools: Vec<ModelProviderTool>,
    pub budget: BudgetContractV1,
}

impl ModelProviderRequest {
    pub fn validate(&self) -> Result<(), ModelProviderError> {
        if !self.request_digest.is_canonical_blake3() {
            return Err(ModelProviderError::new(
                "invalid_request_digest",
                "model provider request digest must be a canonical BLAKE3-256 value",
                false,
            ));
        }
        if self.model.trim().is_empty() || self.model.len() > 256 {
            return Err(ModelProviderError::new(
                "invalid_model",
                "model identifier must be non-empty and bounded",
                false,
            ));
        }
        for tool in &self.tools {
            if tool.name.trim().is_empty() || tool.name.len() > 128 {
                return Err(ModelProviderError::new(
                    "invalid_tool_schema",
                    "tool name must be non-empty and bounded",
                    false,
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModelProviderTurn {
    Decision { payload: Value },
    ToolCall { name: String, arguments: Value },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModelProviderUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelProviderTrace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub transcript: Vec<ModelProviderTraceEntry>,
    #[serde(default)]
    pub tool_trace: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_trace: Option<Value>,
    #[serde(default)]
    pub repair_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelProviderTraceEntry {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelProviderResponse {
    pub request_digest: Digest32,
    pub turns: Vec<ModelProviderTurn>,
    #[serde(default)]
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: ModelProviderUsage,
    #[serde(default)]
    pub trace: ModelProviderTrace,
}

impl ModelProviderResponse {
    pub fn validate_for(&self, request: &ModelProviderRequest) -> Result<(), ModelProviderError> {
        if self.request_digest != request.request_digest {
            return Err(ModelProviderError::new(
                "response_identity_mismatch",
                "model provider response digest does not match the request",
                false,
            ));
        }
        if self.turns.is_empty() {
            return Err(ModelProviderError::new(
                "empty_model_response",
                "model provider response contains no typed turns",
                false,
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelProviderError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_trace: Option<Value>,
}

impl ModelProviderError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self::with_upstream_trace(code, message, retryable, None)
    }

    pub fn with_upstream_trace(
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        upstream_trace: Option<Value>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            upstream_trace,
        }
    }

    pub fn code(&self) -> &str {
        self.code.as_str()
    }
}

impl fmt::Display for ModelProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for ModelProviderError {}

/// Input ownership is explicit; supplier IDs alone do not imply history support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelProviderHistory {
    /// Every request contains the complete current snapshot and tool transcript.
    #[default]
    CallerSnapshot,
    /// The provider retains ordered protocol history and accepts incremental input.
    ProviderConversation,
}

pub trait ModelProvider: Send {
    fn history_mode(&self) -> ModelProviderHistory {
        ModelProviderHistory::CallerSnapshot
    }

    /// Pure local capacity/shape checks, with no reservation or network send.
    /// Stateful providers check the full candidate including retained history.
    fn preflight(&self, request: &ModelProviderRequest) -> Result<(), ModelProviderError> {
        request.validate()
    }

    fn provider_id(&self) -> &str;

    /// Recheck the trusted executor lease/run before a tool may execute.
    /// Ordinary providers have no additional execution authority.
    fn check_execution(&self) -> Result<(), ModelProviderError> {
        Ok(())
    }

    fn complete(
        &mut self,
        request: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError>;

    /// Structured tool causality for adapters which maintain protocol history.
    /// Legacy single-call providers expose no pending protocol calls.
    fn pending_tool_calls(&self) -> Vec<ModelProviderToolCall> {
        Vec::new()
    }

    fn record_tool_results(
        &mut self,
        results: &[ModelProviderToolResult],
    ) -> Result<(), ModelProviderError> {
        if results.is_empty() {
            return Ok(());
        }
        Err(ModelProviderError::new(
            "tool_history_unsupported",
            "provider does not accept structured tool history",
            false,
        ))
    }

    fn complete_checked(
        &mut self,
        request: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        request.validate()?;
        let response = self.complete(request)?;
        response.validate_for(request)?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REQUEST_DIGEST: &str =
        "blake3:0000000000000000000000000000000000000000000000000000000000000000";

    fn request() -> ModelProviderRequest {
        ModelProviderRequest {
            request_digest: Digest32::from(REQUEST_DIGEST),
            model: "test-model".into(),
            system_prompt: "system".into(),
            user_prompt: "user".into(),
            tools: vec![ModelProviderTool {
                name: "inspect".into(),
                description: None,
                parameters: json!({"type":"object"}),
            }],
            budget: BudgetContractV1 {
                max_latency_ms: 100,
                max_repair_attempts: 1,
                max_model_calls: 2,
                max_tool_calls: 3,
            },
        }
    }

    struct EchoProvider;

    impl ModelProvider for EchoProvider {
        fn provider_id(&self) -> &str {
            "echo"
        }

        fn complete(
            &mut self,
            request: &ModelProviderRequest,
        ) -> Result<ModelProviderResponse, ModelProviderError> {
            Ok(ModelProviderResponse {
                request_digest: request.request_digest.clone(),
                turns: vec![ModelProviderTurn::Decision {
                    payload: json!({"wait":true}),
                }],
                output: String::new(),
                model: None,
                usage: ModelProviderUsage::default(),
                trace: ModelProviderTrace::default(),
            })
        }
    }

    #[test]
    fn checked_model_provider_preserves_request_identity_and_serde_shape() {
        let request = request();
        let response = EchoProvider.complete_checked(&request).unwrap();
        response.validate_for(&request).unwrap();
        assert_eq!(response.request_digest, request.request_digest);
        assert_eq!(
            serde_json::to_value(&response).unwrap()["turns"][0],
            json!({"Decision":{"payload":{"wait":true}}})
        );
    }

    #[test]
    fn checked_model_provider_rejects_invalid_identity_before_acceptance() {
        let mut request = request();
        request.request_digest = Digest32::from("legacy-timeout-key");
        assert_eq!(
            EchoProvider.complete_checked(&request).unwrap_err().code(),
            "invalid_request_digest"
        );
    }

    #[test]
    fn model_provider_rejects_response_for_another_request() {
        let request = request();
        let mut response = EchoProvider.complete_checked(&request).unwrap();
        response.request_digest = Digest32::from(
            "blake3:1111111111111111111111111111111111111111111111111111111111111111",
        );
        assert_eq!(
            response.validate_for(&request).unwrap_err().code(),
            "response_identity_mismatch"
        );
    }
}
