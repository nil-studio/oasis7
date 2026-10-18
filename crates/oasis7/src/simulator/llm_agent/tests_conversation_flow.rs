use super::*;
use crate::geometry::GeoPos;
use crate::simulator::{LlmChatRole, WorldEvent};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_agent_api::world_resource::ResourceStock;
use oasis7_simulator::{
    Action, Observation, ObservedAgent, ObservedLocation, PowerOrderSide, RejectReason,
    ResourceOwner, SocialAdjudicationDecision, SocialStake, WorldEventKind,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Clone)]
struct MockClient {
    output: Option<String>,
    err: Option<LlmClientError>,
}

struct EnvVarGuard {
    key: String,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn capture(key: impl Into<String>) -> Self {
        let key = key.into();
        Self {
            previous: std::env::var(key.as_str()).ok(),
            key,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    oasis7::env_mut::set_var(self.key.as_str(), value);
                }
            }
            None => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    oasis7::env_mut::remove_var(self.key.as_str());
                }
            }
        }
    }
}

fn llm_env_lock() -> &'static std::sync::Mutex<()> {
    crate::viewer::canonical_runtime_provider_env_lock()
}

fn completion_turn_from_value(value: serde_json::Value) -> LlmCompletionTurn {
    if value
        .get("type")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("module_call"))
    {
        return LlmCompletionTurn::ModuleCall {
            module: value
                .get("module")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            args: value
                .get("args")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        };
    }

    LlmCompletionTurn::Decision { payload: value }
}

fn next_json_start(raw: &str, from: usize) -> Option<usize> {
    raw.get(from..)?
        .char_indices()
        .find_map(|(offset, ch)| match ch {
            '{' | '[' => Some(from + offset),
            _ => None,
        })
}

fn extract_json_block_from(raw: &str, start: usize) -> Option<(usize, usize)> {
    let open_char = raw.get(start..)?.chars().next()?;
    if open_char != '{' && open_char != '[' {
        return None;
    }
    let close_char = if open_char == '{' { '}' } else { ']' };

    let mut depth: u32 = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in raw[start..].char_indices() {
        let index = start + offset;
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            c if c == open_char => depth = depth.saturating_add(1),
            c if c == close_char => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((start, index));
                }
            }
            _ => {}
        }
    }

    None
}

fn extract_json_blocks(raw: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut cursor = 0_usize;

    while let Some(start) = next_json_start(raw, cursor) {
        let Some((_, end)) = extract_json_block_from(raw, start) else {
            break;
        };
        if let Some(block) = raw.get(start..=end) {
            blocks.push(block);
        }
        cursor = end.saturating_add(1);
    }

    blocks
}

pub(super) fn completion_turns_from_output(output: &str) -> Vec<LlmCompletionTurn> {
    let blocks = extract_json_blocks(output);
    if blocks.is_empty() {
        return Vec::new();
    }
    blocks
        .into_iter()
        .filter_map(|block| serde_json::from_str::<serde_json::Value>(block).ok())
        .map(completion_turn_from_value)
        .collect()
}

impl LlmCompletionClient for MockClient {
    fn complete(
        &self,
        request: &LlmCompletionRequest,
    ) -> Result<LlmCompletionResult, LlmClientError> {
        if let Some(err) = &self.err {
            return Err(err.clone());
        }
        let output = self
            .output
            .clone()
            .unwrap_or_else(|| "{\"decision\":\"wait\"}".to_string());
        Ok(LlmCompletionResult {
            turns: completion_turns_from_output(output.as_str()),
            output,
            model: Some(request.model.clone()),
            prompt_tokens: Some(12),
            completion_tokens: Some(4),
            total_tokens: Some(16),
        })
    }
}

#[derive(Debug, Clone)]
struct SequenceMockClient {
    outputs: RefCell<VecDeque<String>>,
    model: String,
}

impl SequenceMockClient {
    fn new(outputs: Vec<String>) -> Self {
        Self {
            outputs: RefCell::new(outputs.into()),
            model: "gpt-test".to_string(),
        }
    }
}

impl LlmCompletionClient for SequenceMockClient {
    fn complete(
        &self,
        _request: &LlmCompletionRequest,
    ) -> Result<LlmCompletionResult, LlmClientError> {
        let output = self
            .outputs
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| "{\"decision\":\"wait\"}".to_string());
        Ok(LlmCompletionResult {
            turns: completion_turns_from_output(output.as_str()),
            output,
            model: Some(self.model.clone()),
            prompt_tokens: Some(12),
            completion_tokens: Some(4),
            total_tokens: Some(16),
        })
    }
}

#[derive(Debug, Clone)]
struct StressMockClient {
    calls: Arc<AtomicUsize>,
}

impl StressMockClient {
    fn new(calls: Arc<AtomicUsize>) -> Self {
        Self { calls }
    }
}

impl LlmCompletionClient for StressMockClient {
    fn complete(
        &self,
        _request: &LlmCompletionRequest,
    ) -> Result<LlmCompletionResult, LlmClientError> {
        let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
        let output = match call_index % 5 {
            0 => r#"{"type":"plan","missing":["memory"],"next":"module_call"}"#,
            1 => r#"{"type":"module_call","module":"memory.short_term.recent","args":{"limit":4}}"#,
            2 => r#"{"type":"decision_draft","decision":{"decision":"move_agent","to":"loc-2"},"confidence":0.64,"need_verify":true}"#,
            3 => "not-json",
            _ => r#"{"decision":"move_agent","to":"loc-2"}"#,
        }
        .to_string();

        Ok(LlmCompletionResult {
            turns: completion_turns_from_output(output.as_str()),
            output,
            model: Some("gpt-stress".to_string()),
            prompt_tokens: Some(16),
            completion_tokens: Some(6),
            total_tokens: Some(22),
        })
    }
}

#[derive(Debug, Clone)]
struct CountingSequenceMockClient {
    outputs: RefCell<VecDeque<String>>,
    calls: Arc<AtomicUsize>,
}

impl CountingSequenceMockClient {
    fn new(outputs: Vec<String>, calls: Arc<AtomicUsize>) -> Self {
        Self {
            outputs: RefCell::new(outputs.into()),
            calls,
        }
    }
}

impl LlmCompletionClient for CountingSequenceMockClient {
    fn complete(
        &self,
        _request: &LlmCompletionRequest,
    ) -> Result<LlmCompletionResult, LlmClientError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let output = self
            .outputs
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| "{\"decision\":\"wait\"}".to_string());
        Ok(LlmCompletionResult {
            turns: completion_turns_from_output(output.as_str()),
            output,
            model: Some("gpt-count".to_string()),
            prompt_tokens: Some(12),
            completion_tokens: Some(4),
            total_tokens: Some(16),
        })
    }
}

fn make_observation() -> Observation {
    let mut self_resources = ResourceStock::default();
    self_resources
        .add(ResourceKind::Electricity, 30)
        .expect("seed electricity");
    self_resources
        .add(ResourceKind::Data, 0)
        .expect("seed data");
    self_resources
        .add(ResourceKind::Data, 12)
        .expect("seed data");

    Observation {
        time: 7,
        agent_id: "agent-1".to_string(),
        pos: GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
        self_resources,
        visibility_range_cm: 100,
        visible_agents: vec![ObservedAgent {
            agent_id: "agent-2".to_string(),
            location_id: "loc-2".to_string(),
            pos: GeoPos {
                x_cm: 1,
                y_cm: 0,
                z_cm: 0,
            },
            distance_cm: 1,
        }],
        visible_locations: vec![ObservedLocation {
            location_id: "loc-2".to_string(),
            name: "outpost".to_string(),
            pos: GeoPos {
                x_cm: 1,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 1,
        }],
        module_lifecycle: Default::default(),
        module_market: Default::default(),
        power_market: Default::default(),
        social_state: Default::default(),
    }
}

fn make_dense_observation(time: u64, extra: usize) -> Observation {
    let mut observation = make_observation();
    observation.time = time;

    for index in 0..extra {
        observation.visible_agents.push(ObservedAgent {
            agent_id: format!("agent-extra-{index}"),
            location_id: format!("loc-extra-{index}"),
            pos: GeoPos {
                x_cm: 100 + index as i64,
                y_cm: (index as i64) / 2,
                z_cm: 0,
            },
            distance_cm: 100 + index as i64,
        });
        observation.visible_locations.push(ObservedLocation {
            location_id: format!("loc-extra-{index}"),
            name: format!("outpost-extra-{index}"),
            pos: GeoPos {
                x_cm: 100 + index as i64,
                y_cm: (index as i64) / 2,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 100 + index as i64,
        });
    }

    observation
}

fn base_config() -> LlmAgentConfig {
    LlmAgentConfig {
        model: "gpt-test".to_string(),
        base_url: "https://example.invalid/v1".to_string(),
        api_key: "test-key".to_string(),
        timeout_ms: 1000,
        system_prompt: "prompt".to_string(),
        short_term_goal: "short-goal".to_string(),
        long_term_goal: "long-goal".to_string(),
        max_module_calls: 3,
        max_decision_steps: 4,
        max_repair_rounds: 1,
        prompt_max_history_items: 4,
        prompt_profile: LlmPromptProfile::Balanced,
        force_replan_after_same_action: DEFAULT_LLM_FORCE_REPLAN_AFTER_SAME_ACTION,
        harvest_max_amount_cap: DEFAULT_LLM_HARVEST_MAX_AMOUNT_CAP,
        execute_until_auto_reenter_ticks: DEFAULT_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS,
        llm_debug_mode: DEFAULT_LLM_DEBUG_MODE,
    }
}

#[test]
fn llm_prompt_profile_uses_relaxed_token_budget_for_stability() {
    let compact = LlmPromptProfile::Compact.prompt_budget();
    assert_eq!(compact.context_window_tokens, 4_096);
    assert_eq!(compact.reserved_output_tokens, 768);
    assert_eq!(compact.safety_margin_tokens, 352);
    assert_eq!(compact.effective_input_budget_tokens(), 2_976);

    let balanced = LlmPromptProfile::Balanced.prompt_budget();
    assert_eq!(balanced.context_window_tokens, 4_608);
    assert_eq!(balanced.reserved_output_tokens, 896);
    assert_eq!(balanced.safety_margin_tokens, 480);
    assert_eq!(balanced.effective_input_budget_tokens(), 3_232);
}

#[test]
fn llm_config_uses_default_system_prompt() {
    let mut vars = BTreeMap::new();
    vars.insert(ENV_LLM_MODEL.to_string(), "gpt-4o-mini".to_string());
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());

    let config = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap();
    assert_eq!(config.system_prompt, DEFAULT_LLM_SYSTEM_PROMPT);
    assert_eq!(config.timeout_ms, DEFAULT_LLM_TIMEOUT_MS);
    assert_eq!(config.short_term_goal, DEFAULT_LLM_SHORT_TERM_GOAL);
    assert_eq!(config.long_term_goal, DEFAULT_LLM_LONG_TERM_GOAL);
    assert_eq!(config.max_module_calls, DEFAULT_LLM_MAX_MODULE_CALLS);
    assert_eq!(config.max_decision_steps, DEFAULT_LLM_MAX_DECISION_STEPS);
    assert_eq!(config.max_repair_rounds, DEFAULT_LLM_MAX_REPAIR_ROUNDS);
    assert_eq!(
        config.prompt_max_history_items,
        DEFAULT_LLM_PROMPT_MAX_HISTORY_ITEMS
    );
    assert_eq!(config.prompt_profile, DEFAULT_LLM_PROMPT_PROFILE);
    assert_eq!(
        config.force_replan_after_same_action,
        DEFAULT_LLM_FORCE_REPLAN_AFTER_SAME_ACTION
    );
    assert_eq!(
        config.harvest_max_amount_cap,
        DEFAULT_LLM_HARVEST_MAX_AMOUNT_CAP
    );
    assert_eq!(
        config.execute_until_auto_reenter_ticks,
        DEFAULT_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS
    );
    assert_eq!(config.llm_debug_mode, DEFAULT_LLM_DEBUG_MODE);
}

#[test]
fn llm_config_uses_default_model_when_unset() {
    let mut vars = BTreeMap::new();
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());

    let config = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap();
    assert_eq!(config.model, DEFAULT_LLM_MODEL);
}

#[test]
fn llm_config_reads_system_prompt_from_env() {
    let mut vars = BTreeMap::new();
    vars.insert(ENV_LLM_MODEL.to_string(), "gpt-4o-mini".to_string());
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());
    vars.insert(
        ENV_LLM_SYSTEM_PROMPT.to_string(),
        "自定义 system prompt".to_string(),
    );
    vars.insert(ENV_LLM_TIMEOUT_MS.to_string(), "2000".to_string());

    let config = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap();
    assert_eq!(config.system_prompt, "自定义 system prompt");
    assert_eq!(config.timeout_ms, 2000);
}

#[test]
fn llm_config_reads_goal_and_module_limits_from_env() {
    let mut vars = BTreeMap::new();
    vars.insert(ENV_LLM_MODEL.to_string(), "gpt-4o-mini".to_string());
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());
    vars.insert(
        ENV_LLM_SHORT_TERM_GOAL.to_string(),
        "保持本轮高效".to_string(),
    );
    vars.insert(
        ENV_LLM_LONG_TERM_GOAL.to_string(),
        "建立长期资源优势".to_string(),
    );
    vars.insert(ENV_LLM_MAX_MODULE_CALLS.to_string(), "5".to_string());

    let config = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap();
    assert_eq!(config.short_term_goal, "保持本轮高效");
    assert_eq!(config.long_term_goal, "建立长期资源优势");
    assert_eq!(config.max_module_calls, 5);
}

#[test]
fn llm_config_reads_multistep_and_prompt_fields_from_env() {
    let mut vars = BTreeMap::new();
    vars.insert(ENV_LLM_MODEL.to_string(), "gpt-4o-mini".to_string());
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());
    vars.insert(ENV_LLM_MAX_DECISION_STEPS.to_string(), "6".to_string());
    vars.insert(ENV_LLM_MAX_REPAIR_ROUNDS.to_string(), "2".to_string());
    vars.insert(
        ENV_LLM_PROMPT_MAX_HISTORY_ITEMS.to_string(),
        "7".to_string(),
    );
    vars.insert(ENV_LLM_PROMPT_PROFILE.to_string(), "compact".to_string());
    vars.insert(
        ENV_LLM_FORCE_REPLAN_AFTER_SAME_ACTION.to_string(),
        "9".to_string(),
    );
    vars.insert(ENV_LLM_HARVEST_MAX_AMOUNT_CAP.to_string(), "88".to_string());
    vars.insert(
        ENV_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS.to_string(),
        "5".to_string(),
    );

    let config = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap();
    assert_eq!(config.max_decision_steps, 6);
    assert_eq!(config.max_repair_rounds, 2);
    assert_eq!(config.prompt_max_history_items, 7);
    assert_eq!(config.prompt_profile, LlmPromptProfile::Compact);
    assert_eq!(config.force_replan_after_same_action, 9);
    assert_eq!(config.harvest_max_amount_cap, 88);
    assert_eq!(config.execute_until_auto_reenter_ticks, 5);
}

#[test]
fn llm_config_reads_debug_mode_from_env() {
    let mut vars = BTreeMap::new();
    vars.insert(ENV_LLM_MODEL.to_string(), "gpt-4o-mini".to_string());
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());
    vars.insert(ENV_LLM_DEBUG_MODE.to_string(), "true".to_string());

    let config = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap();
    assert!(config.llm_debug_mode);
}

#[test]
fn llm_config_rejects_invalid_debug_mode() {
    let mut vars = BTreeMap::new();
    vars.insert(ENV_LLM_MODEL.to_string(), "gpt-4o-mini".to_string());
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());
    vars.insert(ENV_LLM_DEBUG_MODE.to_string(), "maybe".to_string());

    let err = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap_err();
    assert!(matches!(err, LlmConfigError::InvalidDebugMode { .. }));
}

#[test]
fn llm_config_rejects_invalid_harvest_max_amount_cap() {
    let mut vars = BTreeMap::new();
    vars.insert(ENV_LLM_MODEL.to_string(), "gpt-4o-mini".to_string());
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());
    vars.insert(ENV_LLM_HARVEST_MAX_AMOUNT_CAP.to_string(), "0".to_string());

    let err = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap_err();
    assert!(matches!(
        err,
        LlmConfigError::InvalidHarvestMaxAmountCap { .. }
    ));
}

#[test]
fn llm_config_rejects_invalid_execute_until_auto_reenter_ticks() {
    let mut vars = BTreeMap::new();
    vars.insert(ENV_LLM_MODEL.to_string(), "gpt-4o-mini".to_string());
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());
    vars.insert(
        ENV_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS.to_string(),
        "-1".to_string(),
    );

    let err = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap_err();
    assert!(matches!(
        err,
        LlmConfigError::InvalidExecuteUntilAutoReenterTicks { .. }
    ));
}

#[test]
fn llm_config_rejects_invalid_prompt_profile() {
    let mut vars = BTreeMap::new();
    vars.insert(ENV_LLM_MODEL.to_string(), "gpt-4o-mini".to_string());
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());
    vars.insert(
        ENV_LLM_PROMPT_PROFILE.to_string(),
        "invalid-profile".to_string(),
    );

    let err = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "").unwrap_err();
    assert!(matches!(err, LlmConfigError::InvalidPromptProfile { .. }));
}

#[test]
fn llm_config_agent_scoped_goal_overrides_global_value() {
    let mut vars = BTreeMap::new();
    vars.insert(ENV_LLM_MODEL.to_string(), "gpt-4o-mini".to_string());
    vars.insert(
        ENV_LLM_BASE_URL.to_string(),
        "https://api.example.com/v1".to_string(),
    );
    vars.insert(ENV_LLM_API_KEY.to_string(), "secret".to_string());
    vars.insert(
        ENV_LLM_SHORT_TERM_GOAL.to_string(),
        "global-short".to_string(),
    );
    vars.insert(
        "OASIS7_LLM_SHORT_TERM_GOAL_AGENT_1".to_string(),
        "agent-short".to_string(),
    );

    let config = LlmAgentConfig::from_env_with(|key| vars.get(key).cloned(), "agent-1").unwrap();
    assert_eq!(config.short_term_goal, "agent-short");
    assert_eq!(config.long_term_goal, DEFAULT_LLM_LONG_TERM_GOAL);
}

#[test]
fn llm_env_var_reads_oasis7_prefix() {
    let _env_lock = llm_env_lock().lock().expect("env lock");
    let _primary_guard = EnvVarGuard::capture(ENV_LLM_MODEL);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(ENV_LLM_MODEL, "oasis7-model");
    }

    assert_eq!(llm_env_var(ENV_LLM_MODEL).as_deref(), Some("oasis7-model"));
}

#[test]
fn llm_env_var_rejects_removed_old_brand_prefix() {
    let _env_lock = llm_env_lock().lock().expect("env lock");
    let _primary_guard = EnvVarGuard::capture(ENV_LLM_MODEL);
    let removed_old_brand_model = removed_old_brand_llm_env("MODEL");
    let _removed_old_brand_guard = EnvVarGuard::capture(removed_old_brand_model.as_str());
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(ENV_LLM_MODEL);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(removed_old_brand_model.as_str(), "removed-old-brand-model");
    }

    assert!(llm_env_var(ENV_LLM_MODEL).is_none());
}

#[test]
fn llm_config_from_env_for_agent_rejects_removed_old_brand_prefix() {
    let _env_lock = llm_env_lock().lock().expect("env lock");
    let _model_guard = EnvVarGuard::capture(ENV_LLM_MODEL);
    let _base_url_guard = EnvVarGuard::capture(ENV_LLM_BASE_URL);
    let _api_key_guard = EnvVarGuard::capture(ENV_LLM_API_KEY);
    let removed_old_brand_goal = removed_old_brand_llm_env("SHORT_TERM_GOAL_AGENT_1");
    let removed_old_brand_model = removed_old_brand_llm_env("MODEL");
    let removed_old_brand_base_url = removed_old_brand_llm_env("BASE_URL");
    let removed_old_brand_api_key = removed_old_brand_llm_env("API_KEY");
    let _removed_old_brand_goal_guard = EnvVarGuard::capture(removed_old_brand_goal.as_str());
    let _removed_old_brand_model_guard = EnvVarGuard::capture(removed_old_brand_model.as_str());
    let _removed_old_brand_base_url_guard =
        EnvVarGuard::capture(removed_old_brand_base_url.as_str());
    let _removed_old_brand_api_key_guard = EnvVarGuard::capture(removed_old_brand_api_key.as_str());
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(ENV_LLM_MODEL);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(ENV_LLM_BASE_URL);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(ENV_LLM_API_KEY);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(removed_old_brand_model.as_str(), "removed-old-brand-model");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            removed_old_brand_base_url.as_str(),
            "https://removed-old-brand.example.com/v1",
        );
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            removed_old_brand_api_key.as_str(),
            "removed-old-brand-secret",
        );
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            removed_old_brand_goal.as_str(),
            "removed-old-brand-agent-short",
        );
    }

    let error = LlmAgentConfig::from_env_for_agent("agent-1").expect_err("missing oasis7 env");
    assert!(matches!(
        error,
        LlmConfigError::MissingEnv { key } if key == ENV_LLM_BASE_URL
    ));
}

fn removed_old_brand_llm_env(suffix: &str) -> String {
    ["AGENT", "WORLD", "LLM", suffix].join("_")
}

#[test]
fn llm_config_reads_from_config_file() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path_buf = std::env::temp_dir().join(format!("oasis7-llm-config-{unique}.toml"));
    let path = Path::new(&path_buf);
    let content = r#"
model = "fallback-model"
model_provider = "ark"
profile = "default"

[model_providers.ark]
base_url = "https://api.example.com/v1"
auth_token = "secret"

[profiles.default]
model = "gpt-4o-mini"
model_provider = "ark"

[llm]
timeout_ms = 4567
"#;
    std::fs::write(path, content).unwrap();

    let config = LlmAgentConfig::from_config_file(path).unwrap();

    std::fs::remove_file(path).ok();

    assert_eq!(config.model, "gpt-4o-mini");
    assert_eq!(config.base_url, "https://api.example.com/v1");
    assert_eq!(config.api_key, "secret");
    assert_eq!(config.timeout_ms, 4567);
    assert_eq!(config.system_prompt, DEFAULT_LLM_SYSTEM_PROMPT);
    assert_eq!(config.max_decision_steps, DEFAULT_LLM_MAX_DECISION_STEPS);
    assert_eq!(config.max_repair_rounds, DEFAULT_LLM_MAX_REPAIR_ROUNDS);
    assert_eq!(
        config.prompt_max_history_items,
        DEFAULT_LLM_PROMPT_MAX_HISTORY_ITEMS
    );
    assert_eq!(config.prompt_profile, DEFAULT_LLM_PROMPT_PROFILE);
    assert_eq!(
        config.force_replan_after_same_action,
        DEFAULT_LLM_FORCE_REPLAN_AFTER_SAME_ACTION
    );
    assert_eq!(
        config.harvest_max_amount_cap,
        DEFAULT_LLM_HARVEST_MAX_AMOUNT_CAP
    );
    assert_eq!(
        config.execute_until_auto_reenter_ticks,
        DEFAULT_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS
    );
    assert_eq!(config.llm_debug_mode, DEFAULT_LLM_DEBUG_MODE);
}

#[test]
fn llm_config_prefers_llm_table_core_fields_over_profile_defaults() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path_buf =
        std::env::temp_dir().join(format!("oasis7-llm-config-llm-core-priority-{unique}.toml"));
    let path = Path::new(&path_buf);
    let content = r#"
model_provider = "ark"
profile = "default"

[model_providers.ark]
base_url = "https://provider.example.com/v1"
auth_token = "provider-secret"

[profiles.default]
model = "profile-model"
model_provider = "ark"

[llm]
model = "llm-model"
base_url = "https://llm.example.com/v1"
api_key = "llm-secret"
"#;
    std::fs::write(path, content).unwrap();

    let config = LlmAgentConfig::from_config_file(path).unwrap();

    std::fs::remove_file(path).ok();

    assert_eq!(config.model, "llm-model");
    assert_eq!(config.base_url, "https://llm.example.com/v1");
    assert_eq!(config.api_key, "llm-secret");
}

#[test]
fn llm_config_reads_agent_scoped_goal_overrides_from_llm_agent_overrides_table() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path_buf = std::env::temp_dir().join(format!(
        "oasis7-llm-config-agent-goal-override-{unique}.toml"
    ));
    let path = Path::new(&path_buf);
    let content = r#"
model_provider = "ark"
profile = "default"

[model_providers.ark]
base_url = "https://api.example.com/v1"
auth_token = "secret"

[profiles.default]
model = "gpt-4o-mini"
model_provider = "ark"

[llm]
short_term_goal = "global-short"

[llm.agent_overrides.AGENT_1]
short_term_goal = "agent-short"
"#;
    std::fs::write(path, content).unwrap();

    let config = LlmAgentConfig::from_config_file_for_agent(path, "agent-1").unwrap();

    std::fs::remove_file(path).ok();

    assert_eq!(config.short_term_goal, "agent-short");
    assert_eq!(config.long_term_goal, DEFAULT_LLM_LONG_TERM_GOAL);
}

#[test]
fn normalize_openai_api_base_url_handles_suffix_variants() {
    assert_eq!(
        normalize_openai_api_base_url("https://api.example.com/v1"),
        "https://api.example.com/v1"
    );
    assert_eq!(
        normalize_openai_api_base_url("https://api.example.com/v1/"),
        "https://api.example.com/v1"
    );
    assert_eq!(
        normalize_openai_api_base_url("https://api.example.com/v1/chat/completions"),
        "https://api.example.com/v1"
    );
    assert_eq!(
        normalize_openai_api_base_url("https://api.example.com/v1/responses"),
        "https://api.example.com/v1"
    );
}

#[test]
fn openai_client_respects_configured_timeout_without_hidden_retry() {
    let _env_lock = llm_env_lock().lock().expect("env lock");
    let _diagnostic_guard = EnvVarGuard::capture(ENV_LLM_STREAM_DIAGNOSTICS);
    // SAFETY: The shared environment lock isolates the ordinary SDK contract
    // from the parallel opt-in diagnostic transport test.
    unsafe {
        oasis7::env_mut::set_var(ENV_LLM_STREAM_DIAGNOSTICS, "0");
    }

    let mut config = base_config();
    config.timeout_ms = 200;
    config.base_url = spawn_slow_openai_like_server(Duration::from_millis(600));
    let client = OpenAiChatCompletionClient::from_config(&config).expect("client");
    let request = LlmCompletionRequest {
        model: config.model.clone(),
        system_prompt: config.system_prompt.clone(),
        user_prompt: "return wait".to_string(),
        debug_mode: false,
        max_model_calls: None,
    };
    let started_at = Instant::now();
    let error = client
        .complete(&request)
        .expect_err("request should time out");

    assert_eq!(client.request_timeout_ms(), 200);
    assert!(
        started_at.elapsed() < Duration::from_secs(2),
        "client should fail fast near the configured timeout instead of retrying with a long fallback"
    );
    match error {
        LlmClientError::Http { message } => {
            assert!(
                message.contains("request timed out after 200ms"),
                "unexpected timeout message: {message}"
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn openai_client_retries_single_concurrency_limit_decode_failure() {
    let _env_lock = llm_env_lock().lock().expect("env lock");
    let _diagnostic_guard = EnvVarGuard::capture(ENV_LLM_STREAM_DIAGNOSTICS);
    // SAFETY: The shared environment lock isolates the ordinary SDK contract
    // from the parallel opt-in diagnostic transport test.
    unsafe {
        oasis7::env_mut::set_var(ENV_LLM_STREAM_DIAGNOSTICS, "0");
    }

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind flaky openai server");
    let bind = listener.local_addr().expect("listener addr");
    thread::spawn(move || {
        for attempt in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept request");
            let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer);
            let event_payload = if attempt == 0 {
                serde_json::json!({
                    "error": {
                        "type": "rate_limit_error",
                        "message": "Concurrency limit exceeded for account, please retry later"
                    }
                })
                .to_string()
            } else {
                serde_json::json!({
                    "type": "response.completed",
                    "sequence_number": 1,
                    "response": {
                        "id": "resp_retry_ok",
                        "object": "response",
                        "created_at": 1,
                        "completed_at": 2,
                        "model": "gpt-test",
                        "output": [{
                            "type": "function_call",
                            "call_id": "call_decision",
                            "name": "agent_submit_decision",
                            "arguments": "{\"decision\":\"wait\"}"
                        }],
                        "status": "completed",
                        "parallel_tool_calls": false
                    }
                })
                .to_string()
            };
            let body = format!("data: {event_payload}\n\n");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    let mut config = base_config();
    config.timeout_ms = 1_000;
    config.base_url = format!("http://{bind}/v1");
    let client = OpenAiChatCompletionClient::from_config(&config).expect("client");
    let request = LlmCompletionRequest {
        model: config.model.clone(),
        system_prompt: config.system_prompt.clone(),
        user_prompt: "return wait".to_string(),
        debug_mode: false,
        max_model_calls: None,
    };

    let result = client.complete(&request).expect("client should retry once");
    assert!(result.output.contains("\"decision\":\"wait\""));
}

#[test]
fn stream_transport_diagnostic_is_structured_and_redacted() {
    let metadata = StreamTransportMetadata {
        http_status: Some(200),
        request_timeout_ms: 30_000,
        response_version: Some("HTTP/1.1".to_string()),
        content_type: Some("text/event-stream; charset=utf-8".to_string()),
        content_encoding: None,
        transfer_encoding: Some("chunked".to_string()),
        content_length: None,
        connection: Some("keep-alive".to_string()),
        frame_count: 7,
        decoded_frame_count: 6,
        elapsed_ms: 21_158,
        nested_transport_cause: "Transport error: error decoding response body Bearer secret-token"
            .to_string(),
    };

    let rendered = format_stream_transport_diagnostics(&metadata);

    assert!(rendered.contains("http_status=200"));
    assert!(rendered.contains("request_timeout_ms=30000"));
    assert!(rendered.contains("response_version=HTTP/1.1"));
    assert!(rendered.contains("content_type=text/event-stream; charset=utf-8"));
    assert!(rendered.contains("content_encoding=absent"));
    assert!(rendered.contains("transfer_encoding=chunked"));
    assert!(rendered.contains("content_length=absent"));
    assert!(rendered.contains("connection=keep-alive"));
    assert!(rendered.contains("frames=7"));
    assert!(rendered.contains("decoded_frames=6"));
    assert!(rendered.contains("elapsed_ms=21158"));
    assert!(rendered.contains("cause=Transport error: error decoding response body"));
    assert!(!rendered.contains("secret-token"));
}

#[test]
fn instrumented_stream_transport_error_reports_response_metadata() {
    let _env_lock = llm_env_lock().lock().expect("env lock");
    let _diagnostic_guard = EnvVarGuard::capture(ENV_LLM_STREAM_DIAGNOSTICS);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(ENV_LLM_STREAM_DIAGNOSTICS, "1");
    }

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind truncated stream server");
    let bind = listener.local_addr().expect("truncated stream address");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept truncated stream request");
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        let event = serde_json::json!({
            "type": "response.completed",
            "sequence_number": 1,
            "response": {
                "id": "resp_transport_metadata",
                "object": "response",
                "created_at": 1,
                "completed_at": 2,
                "model": "gpt-test",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_decision",
                    "name": "agent_submit_decision",
                    "arguments": "{\"decision\":\"wait\"}"
                }],
                "status": "completed",
                "parallel_tool_calls": false
            }
        })
        .to_string();
        let body = format!("data: {event}\n\n");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len() + 32,
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.shutdown(Shutdown::Both);
    });

    let mut config = base_config();
    config.timeout_ms = 1_000;
    config.base_url = format!("http://{bind}/v1");
    let client = OpenAiChatCompletionClient::from_config(&config).expect("client");
    let request = LlmCompletionRequest {
        model: config.model.clone(),
        system_prompt: config.system_prompt.clone(),
        user_prompt: "return wait".to_string(),
        debug_mode: false,
        max_model_calls: Some(1),
    };

    let error = client
        .complete(&request)
        .expect_err("truncated stream should fail");
    match error {
        LlmClientError::Http { message } => {
            assert!(
                message.contains("http_status=200"),
                "unexpected error: {message}"
            );
            assert!(
                message.contains("request_timeout_ms=1000"),
                "unexpected error: {message}"
            );
            assert!(
                message.contains("response_version=HTTP/1.1"),
                "unexpected error: {message}"
            );
            assert!(
                message.contains("content_type=text/event-stream"),
                "unexpected error: {message}"
            );
            assert!(
                message.contains("content_length="),
                "unexpected error: {message}"
            );
            assert!(message.contains("frames=1"), "unexpected error: {message}");
            assert!(
                message.contains("decoded_frames=1"),
                "unexpected error: {message}"
            );
            assert!(
                message.contains("cause=failure_kind=transport reqwest_kind=decode"),
                "unexpected error: {message}"
            );
            assert!(
                message.contains("source_chain=request or response body error"),
                "unexpected error: {message}"
            );
            assert!(
                message.contains("end of file before message length reached"),
                "unexpected error: {message}"
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

fn spawn_slow_openai_like_server(response_delay: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind slow openai server");
    let bind = listener.local_addr().expect("listener addr");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer);
        thread::sleep(response_delay);
        let body = serde_json::json!({
            "id": "resp_test",
            "object": "response",
            "model": "gpt-test",
            "output": [],
            "usage": {
                "input_tokens": 1,
                "output_tokens": 1,
                "total_tokens": 2
            }
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
    });
    format!("http://127.0.0.1:{}/v1", bind.port())
}

include!("tests_response_tools.rs");
