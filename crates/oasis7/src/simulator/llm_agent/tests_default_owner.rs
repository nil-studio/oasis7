#[test]
fn llm_config_blank_prompt_uses_canonical_harness_default() {
    for blank in ["", "  \t  "] {
        let mut vars = BTreeMap::new();
        vars.insert(
            ENV_LLM_BASE_URL.to_string(),
            "https://api.example.com/v1".to_string(),
        );
        vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());
        vars.insert(ENV_LLM_SYSTEM_PROMPT.to_string(), blank.to_string());
        let config = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap();
        assert_eq!(
            config.system_prompt,
            oasis7_agent_harness::llm_defaults::DEFAULT_LLM_SYSTEM_PROMPT
        );
    }
}
