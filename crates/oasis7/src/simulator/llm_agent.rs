//! LLM-powered agent behavior and OpenAI-compatible completion client.

use super::archive::WorldEvent;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;

use crate::runtime::{DomainEvent as RuntimeDomainEvent, WorldEventBody as RuntimeWorldEventBody};

use super::agent::{
    ActionResult, AgentBehavior, AgentDecision, AgentDecisionTrace, AgentQuery, AgentQueryResult,
    LlmChatMessageTrace, LlmChatRole, LlmDecisionDiagnostics, LlmEffectIntentTrace,
    LlmEffectReceiptTrace, LlmPromptSectionTrace, LlmStepTrace,
};
use super::llm_defaults::DEFAULT_LLM_MODEL;
use super::memory::{AgentMemory, LongTermMemoryEntry, MemoryEntry};
use oasis7_simulator::{
    Action, CM_PER_KM, DEFAULT_MOVE_COST_PER_KM_ELECTRICITY, ModuleInstallTarget, ResourceKind,
    ResourceOwner,
};
use oasis7_simulator::{Observation, RejectReason, WorldEventKind};

mod behavior_budget;
mod behavior_context;
mod behavior_guardrails;
mod behavior_loop;
mod behavior_prompt;
mod behavior_prompt_modules;
mod behavior_runtime_helpers;
mod config_helpers;
mod decision_flow;
mod execution_controls;
mod memory_selector;
#[cfg(test)]
mod openai_payload;
mod prompt_assembly;
mod recipe_coverage;
use recipe_coverage::RecipeCoverageProgress;

pub use memory_selector::{MemorySelector, MemorySelectorConfig};
pub use prompt_assembly::{
    PromptAssembler, PromptAssemblyInput, PromptAssemblyOutput, PromptBudget, PromptStepContext,
};

use decision_flow::{
    DecisionRewriteReceipt, ExecuteUntilDirective, LlmModuleCallRequest, ModuleCallExchange,
    ParsedLlmTurn, parse_limit_arg, parse_llm_turn_payloads_with_debug_mode,
    prompt_section_kind_name, prompt_section_priority_name, summarize_execute_until_conditions,
    summarize_trace_text,
};
use execution_controls::{
    ActionReplanGuardState, ActiveExecuteUntil, default_execute_until_conditions_for_action,
};

#[cfg(test)]
use async_openai::types::responses::{OutputItem, Tool};
use config_helpers::{
    goal_value, parse_non_negative_usize, parse_positive_i64, parse_positive_usize, required_env,
};
#[cfg(test)]
use oasis7_model_provider::legacy_tool_names::*;
#[cfg(test)]
use oasis7_model_provider::openai_payload::normalize_openai_api_base_url;
#[cfg(test)]
pub(super) use oasis7_model_provider::openai_stream_diagnostics::StreamTransportMetadata;
#[cfg(test)]
use oasis7_model_provider::openai_stream_diagnostics::format_stream_transport_diagnostics;
#[cfg(test)]
use openai_payload::{
    build_responses_request_payload, completion_result_from_sdk_stream_events,
    output_item_to_completion_turn, responses_tools, responses_tools_with_debug_mode,
};

pub use oasis7_model_provider::config_constants::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmPromptProfile {
    Compact,
    Balanced,
}

impl LlmPromptProfile {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "compact" => Some(Self::Compact),
            "balanced" => Some(Self::Balanced),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Balanced => "balanced",
        }
    }

    pub fn prompt_budget(self) -> PromptBudget {
        match self {
            Self::Compact => PromptBudget {
                context_window_tokens: 4_096,
                reserved_output_tokens: 768,
                safety_margin_tokens: 352,
            },
            Self::Balanced => PromptBudget {
                context_window_tokens: 4_608,
                reserved_output_tokens: 896,
                safety_margin_tokens: 480,
            },
        }
    }

    pub fn memory_selector_config(self) -> MemorySelectorConfig {
        match self {
            Self::Compact => MemorySelectorConfig {
                short_term_candidate_limit: 8,
                long_term_candidate_limit: 12,
                short_term_top_k: 3,
                long_term_top_k: 4,
            },
            Self::Balanced => MemorySelectorConfig {
                short_term_candidate_limit: 8,
                long_term_candidate_limit: 12,
                short_term_top_k: 2,
                long_term_top_k: 3,
            },
        }
    }
}

pub const DEFAULT_CONFIG_FILE_NAME: &str = "config.toml";
pub const DEFAULT_LLM_TIMEOUT_MS: u64 = 180_000;
pub const DEFAULT_LLM_SYSTEM_PROMPT: &str = "你是硅基文明发展 Agent。按“读规则/观察 -> 资源稳态 -> 产业建设 -> 治理协作 -> 危机韧性”推进文明进程，每轮仅提交一个可执行 decision。若规则或动作前置条件不明确，先调用 world.rules.guide 与 environment.current_observation，再做决策。";
pub const DEFAULT_LLM_SHORT_TERM_GOAL: &str = "先识别当前阶段最关键瓶颈，并按前置条件逐步推进：能源与数据稳定后再扩产，扩产后推进治理与风险处理。遇到 action_rejected 时根据 reject_reason 切换到补前置动作，避免原样重复失败参数。";
pub const DEFAULT_LLM_LONG_TERM_GOAL: &str = "构建可持续、可治理、具韧性的文明系统，让资源、组织与风险应对形成长期正反馈，并保持阶段推进可解释。";
pub const DEFAULT_LLM_MAX_MODULE_CALLS: usize = 3;
pub const DEFAULT_LLM_MAX_DECISION_STEPS: usize = 4;
pub const DEFAULT_LLM_MAX_REPAIR_ROUNDS: usize = 1;
pub const DEFAULT_LLM_PROMPT_MAX_HISTORY_ITEMS: usize = 4;
pub const DEFAULT_LLM_PROMPT_PROFILE: LlmPromptProfile = LlmPromptProfile::Balanced;
pub const DEFAULT_LLM_FORCE_REPLAN_AFTER_SAME_ACTION: usize = 4;
pub const DEFAULT_LLM_HARVEST_MAX_AMOUNT_CAP: i64 = 100;
pub const DEFAULT_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS: usize = 4;
pub const DEFAULT_LLM_DEBUG_MODE: bool = false;
pub const DEFAULT_LLM_HARVEST_EXECUTE_UNTIL_MAX_TICKS: u64 = 3;
const DEFAULT_RECIPE_HARDWARE_COST_PER_BATCH: i64 = 2;
const DEFAULT_RECIPE_ELECTRICITY_COST_PER_BATCH: i64 = 6;
const DEFAULT_REFINE_RECOVERY_MASS_G_PER_HARDWARE: i64 = 1_000;
const DEFAULT_REFINE_ELECTRICITY_COST_PER_KG: i64 = 2;
const DEFAULT_MINE_COMPOUND_MAX_PER_ACTION_G: i64 = 5_000;
const DEFAULT_MINE_ELECTRICITY_COST_PER_KG: i64 = 1;
const DEFAULT_MINE_DEPLETED_LOCATION_COOLDOWN_TICKS: u64 = 6;
const DEFAULT_MINE_FAILURE_STREAK_WINDOW_TICKS: u64 = 24;
const DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK: i64 = 1_000_000;
const TRACKED_RECIPE_IDS: [&str; 11] = [
    "recipe.smelter.iron_ingot",
    "recipe.smelter.copper_wire",
    "recipe.smelter.polymer_resin",
    "recipe.smelter.alloy_plate",
    "recipe.assembler.gear",
    "recipe.assembler.control_chip",
    "recipe.assembler.motor_mk1",
    "recipe.assembler.logistics_drone",
    "recipe.assembler.sensor_pack",
    "recipe.assembler.module_rack",
    "recipe.assembler.factory_core",
];

const DEFAULT_SHORT_TERM_MEMORY_CAPACITY: usize = 128;
const DEFAULT_LONG_TERM_MEMORY_CAPACITY: usize = 256;
const LLM_PROMPT_MODULE_CALL_KIND: &str = "llm.prompt.module_call";
const LLM_PROMPT_MODULE_CALL_CAP_REF: &str = "llm.prompt.module_access";
const LLM_PROMPT_MODULE_CALL_ORIGIN: &str = "llm_agent";
const PROMPT_MODULE_RESULT_MAX_CHARS: usize = 520;
const PROMPT_MODULE_ARGS_MAX_CHARS: usize = 192;
const PROMPT_MEMORY_DIGEST_MAX_CHARS: usize = 360;
const PROMPT_CONVERSATION_ITEM_MAX_CHARS: usize = 320;
const PROMPT_CONVERSATION_MAX_ITEMS: usize = 12;
const PROMPT_OBSERVATION_VISIBLE_AGENTS_MAX: usize = 5;
const PROMPT_OBSERVATION_VISIBLE_LOCATIONS_MAX: usize = 5;
const CONVERSATION_HISTORY_MAX_ITEMS: usize = 64;
const RECIPE_COMPLETION_REPLAY_WINDOW: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptLastActionSummary {
    kind: String,
    success: bool,
    reject_reason: Option<String>,
    decision_rewrite: Option<DecisionRewriteReceipt>,
}

impl RecipeCoverageProgress {
    fn tracked_factory_kind(recipe_id: &str) -> Option<&'static str> {
        match recipe_id.trim() {
            recipe_id if recipe_id.starts_with("recipe.smelter.") => Some("factory.smelter.mk1"),
            recipe_id if recipe_id.starts_with("recipe.assembler.") => {
                Some("factory.assembler.mk1")
            }
            _ => None,
        }
    }

    fn is_tracked(recipe_id: &str) -> bool {
        TRACKED_RECIPE_IDS
            .iter()
            .any(|candidate| candidate == &recipe_id.trim())
    }

    fn mark_completed(&mut self, recipe_id: &str) -> bool {
        let normalized = recipe_id.trim();
        if Self::is_tracked(normalized) {
            return self.completed.insert(normalized.to_string());
        }
        false
    }

    fn is_completed(&self, recipe_id: &str) -> bool {
        self.completed.contains(recipe_id.trim())
    }

    fn missing_recipe_ids(&self) -> Vec<String> {
        TRACKED_RECIPE_IDS
            .iter()
            .filter(|recipe_id| !self.completed.contains(**recipe_id))
            .map(|recipe_id| (*recipe_id).to_string())
            .collect()
    }

    fn next_uncovered_recipe_for_factory_kind_excluding(
        &self,
        factory_kind: &str,
        current_recipe_id: &str,
    ) -> Option<String> {
        let current_recipe_id = current_recipe_id.trim();
        self.missing_recipe_ids().into_iter().find(|recipe_id| {
            recipe_id.as_str() != current_recipe_id
                && Self::tracked_factory_kind(recipe_id.as_str()) == Some(factory_kind)
        })
    }

    fn summary_json(&self) -> serde_json::Value {
        let completed = TRACKED_RECIPE_IDS
            .iter()
            .filter(|recipe_id| self.completed.contains(**recipe_id))
            .map(|recipe_id| (*recipe_id).to_string())
            .collect::<Vec<_>>();
        let missing = self.missing_recipe_ids();
        serde_json::json!({
            "tracked_total": TRACKED_RECIPE_IDS.len(),
            "completed": completed,
            "missing": missing,
        })
    }

    fn is_fully_covered(&self) -> bool {
        TRACKED_RECIPE_IDS
            .iter()
            .all(|recipe_id| self.completed.contains(*recipe_id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MineFailureStreak {
    count: u32,
    last_time: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct KnownModuleArtifactRecord {
    pub wasm_hash: String,
    pub publisher_agent_id: String,
    pub bytes_len: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_id_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct KnownInstalledModuleRecord {
    pub module_id: String,
    pub module_version: String,
    pub wasm_hash: String,
    pub installer_agent_id: String,
    pub install_target: ModuleInstallTarget,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LlmPromptOverrides {
    pub system_prompt: Option<String>,
    pub short_term_goal: Option<String>,
    pub long_term_goal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmAgentConfig {
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub timeout_ms: u64,
    pub system_prompt: String,
    pub short_term_goal: String,
    pub long_term_goal: String,
    pub max_module_calls: usize,
    pub max_decision_steps: usize,
    pub max_repair_rounds: usize,
    pub prompt_max_history_items: usize,
    pub prompt_profile: LlmPromptProfile,
    pub force_replan_after_same_action: usize,
    pub harvest_max_amount_cap: i64,
    pub execute_until_auto_reenter_ticks: usize,
    pub llm_debug_mode: bool,
}

use oasis7_model_provider::parse_debug_mode_flag;

impl LlmAgentConfig {
    pub fn from_default_sources() -> Result<Self, LlmConfigError> {
        Self::from_default_sources_for_agent("")
    }

    pub fn from_default_sources_for_agent(agent_id: &str) -> Result<Self, LlmConfigError> {
        let config_path = Path::new(DEFAULT_CONFIG_FILE_NAME);
        if config_path.exists() {
            return Self::from_config_file_for_agent(config_path, agent_id);
        }
        Self::from_env_for_agent(agent_id)
    }

    pub fn from_config_file(path: &Path) -> Result<Self, LlmConfigError> {
        Self::from_config_file_for_agent(path, "")
    }

    pub fn from_config_file_for_agent(path: &Path, agent_id: &str) -> Result<Self, LlmConfigError> {
        let content = fs::read_to_string(path).map_err(|err| LlmConfigError::ReadConfigFile {
            path: path.display().to_string(),
            message: err.to_string(),
        })?;
        let value: toml::Value =
            toml::from_str(&content).map_err(|err| LlmConfigError::ParseConfigFile {
                path: path.display().to_string(),
                message: err.to_string(),
            })?;
        let table = value
            .as_table()
            .ok_or_else(|| LlmConfigError::ParseConfigFile {
                path: path.display().to_string(),
                message: "root is not a TOML table".to_string(),
            })?;

        Self::from_env_with(
            |key| config_value_for_env_key(table, key).or_else(|| llm_env_var(key)),
            agent_id,
        )
    }

    pub fn from_env() -> Result<Self, LlmConfigError> {
        Self::from_env_for_agent("")
    }

    pub fn from_env_for_agent(agent_id: &str) -> Result<Self, LlmConfigError> {
        Self::from_env_with(llm_env_var, agent_id)
    }

    fn from_env_with<F>(mut getter: F, agent_id: &str) -> Result<Self, LlmConfigError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let model = getter(ENV_LLM_MODEL)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_LLM_MODEL.to_string());
        let base_url = required_env(&mut getter, ENV_LLM_BASE_URL)?;
        let api_key = required_env(&mut getter, ENV_LLM_API_KEY)?;
        let timeout_ms = match getter(ENV_LLM_TIMEOUT_MS) {
            Some(value) => value
                .parse::<u64>()
                .map_err(|_| LlmConfigError::InvalidTimeout { value })?,
            None => DEFAULT_LLM_TIMEOUT_MS,
        };
        let system_prompt = getter(ENV_LLM_SYSTEM_PROMPT)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_LLM_SYSTEM_PROMPT.to_string());
        let short_term_goal = goal_value(&mut getter, ENV_LLM_SHORT_TERM_GOAL, agent_id)
            .unwrap_or_else(|| DEFAULT_LLM_SHORT_TERM_GOAL.to_string());
        let long_term_goal = goal_value(&mut getter, ENV_LLM_LONG_TERM_GOAL, agent_id)
            .unwrap_or_else(|| DEFAULT_LLM_LONG_TERM_GOAL.to_string());
        let max_module_calls = parse_positive_usize(
            &mut getter,
            ENV_LLM_MAX_MODULE_CALLS,
            DEFAULT_LLM_MAX_MODULE_CALLS,
            |value| LlmConfigError::InvalidMaxModuleCalls { value },
        )?;
        let max_decision_steps = parse_positive_usize(
            &mut getter,
            ENV_LLM_MAX_DECISION_STEPS,
            DEFAULT_LLM_MAX_DECISION_STEPS,
            |value| LlmConfigError::InvalidMaxDecisionSteps { value },
        )?;
        let max_repair_rounds = parse_positive_usize(
            &mut getter,
            ENV_LLM_MAX_REPAIR_ROUNDS,
            DEFAULT_LLM_MAX_REPAIR_ROUNDS,
            |value| LlmConfigError::InvalidMaxRepairRounds { value },
        )?;
        let prompt_max_history_items = parse_positive_usize(
            &mut getter,
            ENV_LLM_PROMPT_MAX_HISTORY_ITEMS,
            DEFAULT_LLM_PROMPT_MAX_HISTORY_ITEMS,
            |value| LlmConfigError::InvalidPromptMaxHistoryItems { value },
        )?;
        let prompt_profile = match getter(ENV_LLM_PROMPT_PROFILE) {
            Some(value) => LlmPromptProfile::parse(value.as_str())
                .ok_or(LlmConfigError::InvalidPromptProfile { value })?,
            None => DEFAULT_LLM_PROMPT_PROFILE,
        };
        let force_replan_after_same_action = parse_non_negative_usize(
            &mut getter,
            ENV_LLM_FORCE_REPLAN_AFTER_SAME_ACTION,
            DEFAULT_LLM_FORCE_REPLAN_AFTER_SAME_ACTION,
            |value| LlmConfigError::InvalidForceReplanAfterSameAction { value },
        )?;
        let harvest_max_amount_cap = parse_positive_i64(
            &mut getter,
            ENV_LLM_HARVEST_MAX_AMOUNT_CAP,
            DEFAULT_LLM_HARVEST_MAX_AMOUNT_CAP,
            |value| LlmConfigError::InvalidHarvestMaxAmountCap { value },
        )?;
        let execute_until_auto_reenter_ticks = parse_non_negative_usize(
            &mut getter,
            ENV_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS,
            DEFAULT_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS,
            |value| LlmConfigError::InvalidExecuteUntilAutoReenterTicks { value },
        )?;
        let llm_debug_mode = match getter(ENV_LLM_DEBUG_MODE) {
            Some(value) => parse_debug_mode_flag(value.as_str())
                .ok_or(LlmConfigError::InvalidDebugMode { value })?,
            None => DEFAULT_LLM_DEBUG_MODE,
        };

        Ok(Self {
            model,
            base_url,
            api_key,
            timeout_ms,
            system_prompt,
            short_term_goal,
            long_term_goal,
            max_module_calls,
            max_decision_steps,
            max_repair_rounds,
            prompt_max_history_items,
            prompt_profile,
            force_replan_after_same_action,
            harvest_max_amount_cap,
            execute_until_auto_reenter_ticks,
            llm_debug_mode,
        })
    }

    fn prompt_budget(&self) -> PromptBudget {
        self.prompt_profile.prompt_budget()
    }

    fn memory_selector_config(&self) -> MemorySelectorConfig {
        self.prompt_profile.memory_selector_config()
    }
}

fn llm_env_var(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

use oasis7_model_provider::config_value_for_env_key;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmConfigError {
    MissingEnv { key: &'static str },
    EmptyEnv { key: &'static str },
    InvalidTimeout { value: String },
    InvalidMaxModuleCalls { value: String },
    InvalidMaxDecisionSteps { value: String },
    InvalidMaxRepairRounds { value: String },
    InvalidPromptMaxHistoryItems { value: String },
    InvalidPromptProfile { value: String },
    InvalidForceReplanAfterSameAction { value: String },
    InvalidHarvestMaxAmountCap { value: String },
    InvalidExecuteUntilAutoReenterTicks { value: String },
    InvalidDebugMode { value: String },
    ReadConfigFile { path: String, message: String },
    ParseConfigFile { path: String, message: String },
}

impl fmt::Display for LlmConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmConfigError::MissingEnv { key } => write!(f, "missing env variable: {key}"),
            LlmConfigError::EmptyEnv { key } => write!(f, "empty env variable: {key}"),
            LlmConfigError::InvalidTimeout { value } => {
                write!(f, "invalid timeout value: {value}")
            }
            LlmConfigError::InvalidMaxModuleCalls { value } => {
                write!(f, "invalid max module calls value: {value}")
            }
            LlmConfigError::InvalidMaxDecisionSteps { value } => {
                write!(f, "invalid max decision steps value: {value}")
            }
            LlmConfigError::InvalidMaxRepairRounds { value } => {
                write!(f, "invalid max repair rounds value: {value}")
            }
            LlmConfigError::InvalidPromptMaxHistoryItems { value } => {
                write!(f, "invalid prompt max history items value: {value}")
            }
            LlmConfigError::InvalidPromptProfile { value } => {
                write!(f, "invalid prompt profile value: {value}")
            }
            LlmConfigError::InvalidForceReplanAfterSameAction { value } => {
                write!(f, "invalid force replan after same action value: {value}")
            }
            LlmConfigError::InvalidHarvestMaxAmountCap { value } => {
                write!(f, "invalid harvest max amount cap value: {value}")
            }
            LlmConfigError::InvalidExecuteUntilAutoReenterTicks { value } => {
                write!(f, "invalid execute_until auto reenter ticks value: {value}")
            }
            LlmConfigError::InvalidDebugMode { value } => {
                write!(f, "invalid debug mode value: {value}")
            }
            LlmConfigError::ReadConfigFile { path, message } => {
                write!(f, "read config file failed ({path}): {message}")
            }
            LlmConfigError::ParseConfigFile { path, message } => {
                write!(f, "parse config file failed ({path}): {message}")
            }
        }
    }
}

impl Error for LlmConfigError {}

#[cfg(test)]
pub use oasis7_model_provider::LlmCompletionResult;
pub use oasis7_model_provider::{
    LlmClientError, LlmCompletionClient, LlmCompletionRequest, LlmCompletionTurn,
    OpenAiChatCompletionClient,
};
impl oasis7_model_provider::CompletionClientConfig for LlmAgentConfig {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn api_key(&self) -> &str {
        &self.api_key
    }
    fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

fn sanitize_prompt_override(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[derive(Debug)]
pub struct LlmAgentBehavior<C: LlmCompletionClient> {
    agent_id: String,
    config: LlmAgentConfig,
    prompt_overrides: LlmPromptOverrides,
    client: C,
    memory: AgentMemory,
    next_effect_intent_id: u64,
    pending_trace: Option<AgentDecisionTrace>,
    replan_guard_state: ActionReplanGuardState,
    active_execute_until: Option<ActiveExecuteUntil>,
    conversation_history: Vec<LlmChatMessageTrace>,
    conversation_trace_cursor: usize,
    last_action_summary: Option<PromptLastActionSummary>,
    last_query_result: Option<AgentQueryResult>,
    pending_decision_rewrite: Option<DecisionRewriteReceipt>,
    known_factory_locations: BTreeMap<String, String>,
    known_factory_kinds_by_id: BTreeMap<String, String>,
    known_factory_kind_aliases: BTreeMap<String, String>,
    known_module_artifacts: BTreeMap<String, KnownModuleArtifactRecord>,
    known_installed_modules: BTreeMap<String, KnownInstalledModuleRecord>,
    move_distance_exceeded_targets: BTreeSet<String>,
    known_compound_availability_by_location: BTreeMap<String, i64>,
    depleted_mine_location_cooldowns: BTreeMap<String, u64>,
    mine_failure_streaks_by_location: BTreeMap<String, MineFailureStreak>,
    recipe_coverage: RecipeCoverageProgress,
    continuous_context: behavior_context::ContinuousAgentContext,
}

impl LlmAgentBehavior<OpenAiChatCompletionClient> {
    pub fn from_env(agent_id: impl Into<String>) -> Result<Self, LlmAgentBuildError> {
        let agent_id = agent_id.into();
        let config = LlmAgentConfig::from_default_sources_for_agent(agent_id.as_str())
            .map_err(LlmAgentBuildError::Config)?;
        let client =
            OpenAiChatCompletionClient::from_config(&config).map_err(LlmAgentBuildError::Client)?;
        Ok(Self::new(agent_id, config, client))
    }
}

impl<C: LlmCompletionClient> LlmAgentBehavior<C> {
    pub fn new(agent_id: impl Into<String>, config: LlmAgentConfig, client: C) -> Self {
        Self::new_with_memory(
            agent_id,
            config,
            client,
            AgentMemory::with_capacities(
                DEFAULT_SHORT_TERM_MEMORY_CAPACITY,
                DEFAULT_LONG_TERM_MEMORY_CAPACITY,
            ),
        )
    }

    pub fn new_with_memory(
        agent_id: impl Into<String>,
        config: LlmAgentConfig,
        client: C,
        memory: AgentMemory,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            config,
            prompt_overrides: LlmPromptOverrides::default(),
            client,
            memory,
            next_effect_intent_id: 0,
            pending_trace: None,
            replan_guard_state: ActionReplanGuardState::default(),
            active_execute_until: None,
            conversation_history: Vec::new(),
            conversation_trace_cursor: 0,
            last_action_summary: None,
            last_query_result: None,
            pending_decision_rewrite: None,
            known_factory_locations: BTreeMap::new(),
            known_factory_kinds_by_id: BTreeMap::new(),
            known_factory_kind_aliases: BTreeMap::new(),
            known_module_artifacts: BTreeMap::new(),
            known_installed_modules: BTreeMap::new(),
            move_distance_exceeded_targets: BTreeSet::new(),
            known_compound_availability_by_location: BTreeMap::new(),
            depleted_mine_location_cooldowns: BTreeMap::new(),
            mine_failure_streaks_by_location: BTreeMap::new(),
            recipe_coverage: RecipeCoverageProgress::default(),
            continuous_context: behavior_context::ContinuousAgentContext::default(),
        }
    }

    pub fn apply_prompt_overrides(
        &mut self,
        system_prompt: Option<String>,
        short_term_goal: Option<String>,
        long_term_goal: Option<String>,
    ) {
        self.prompt_overrides.system_prompt = sanitize_prompt_override(system_prompt);
        self.prompt_overrides.short_term_goal = sanitize_prompt_override(short_term_goal);
        self.prompt_overrides.long_term_goal = sanitize_prompt_override(long_term_goal);
    }

    pub fn prompt_overrides(&self) -> LlmPromptOverrides {
        self.prompt_overrides.clone()
    }

    pub fn export_long_term_memory_entries(&self) -> Vec<LongTermMemoryEntry> {
        self.memory.export_long_term_entries()
    }

    pub fn restore_long_term_memory_entries(&mut self, entries: &[LongTermMemoryEntry]) {
        self.memory.restore_long_term_entries(entries.to_vec());
    }

    pub fn push_player_message(&mut self, time: u64, message: impl AsRef<str>) -> bool {
        self.append_conversation_message(time, LlmChatRole::Player, message.as_ref())
            .is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmAgentBuildError {
    Config(LlmConfigError),
    Client(LlmClientError),
}

impl fmt::Display for LlmAgentBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmAgentBuildError::Config(err) => write!(f, "llm config error: {err}"),
            LlmAgentBuildError::Client(err) => write!(f, "llm client error: {err}"),
        }
    }
}

impl Error for LlmAgentBuildError {}

#[cfg(test)]
mod tests;
