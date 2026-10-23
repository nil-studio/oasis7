//! Compatibility exports for the shared provider contract.
pub use oasis7_agent_api::{
    ModelProvider, ModelProviderError, ModelProviderHistory, ModelProviderRequest,
    ModelProviderResponse, ModelProviderTool, ModelProviderToolCall, ModelProviderToolResult,
    ModelProviderTrace, ModelProviderTraceEntry, ModelProviderTurn, ModelProviderUsage,
};

#[cfg(test)]
mod identity_tests {
    use crate as harness;
    use oasis7_agent_api as api;

    struct LegacyProvider;
    impl harness::ModelProvider for LegacyProvider {
        fn provider_id(&self) -> &str {
            "identity-test"
        }
        fn complete(
            &mut self,
            _: &harness::ModelProviderRequest,
        ) -> Result<harness::ModelProviderResponse, harness::ModelProviderError> {
            Err(harness::ModelProviderError::new("test", "test", false))
        }
    }

    #[test]
    fn legacy_implementation_satisfies_shared_trait_and_dtos() {
        fn shared_bound<T: api::ModelProvider>(_: &T) {}
        let mut provider = LegacyProvider;
        shared_bound(&provider);
        let legacy: &mut dyn harness::ModelProvider = &mut provider;
        let shared: &mut dyn api::ModelProvider = legacy;
        assert_eq!(shared.provider_id(), "identity-test");
        assert_eq!(
            shared.history_mode(),
            api::ModelProviderHistory::CallerSnapshot
        );
        assert!(shared.check_execution().is_ok());
        assert!(shared.record_tool_results(&[]).is_ok());
        let legacy = harness::ModelProviderToolResult {
            call_id: "tool-1".into(),
            content: serde_json::json!({"ok": true}),
        };
        let shared: api::ModelProviderToolResult = legacy;
        assert_eq!(shared.call_id, "tool-1");
        assert_eq!(api::DEFAULT_LLM_MODEL, harness::DEFAULT_LLM_MODEL);
        let error: api::ModelProviderError =
            harness::ModelProviderError::new("test", "test", false);
        assert_eq!(error.code(), "test");
    }
}
