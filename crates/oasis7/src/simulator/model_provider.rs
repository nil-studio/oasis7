//! Compatibility adapter for the shared Harness inference contract.
pub use oasis7_agent_harness::{
    ModelProvider, ModelProviderError, ModelProviderRequest, ModelProviderResponse,
    ModelProviderTool, ModelProviderTrace, ModelProviderTraceEntry, ModelProviderTurn,
    ModelProviderUsage,
};

// Concrete legacy completion adapter is now owned by oasis7_model_provider.
#[cfg(not(target_arch = "wasm32"))]
pub use oasis7_model_provider::legacy_model_adapter::LlmCompletionModelProvider;
