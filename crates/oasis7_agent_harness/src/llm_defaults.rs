//! Portable LLM environment variable names and default decision parameters.
//!
//! These defaults are world-agnostic configuration constants for agent
//! harnesses and decision providers; transport and Runtime execution remain
//! outside `oasis7_agent_harness`.

pub const ENV_LLM_MODEL: &str = "OASIS7_LLM_MODEL";
pub const ENV_LLM_BASE_URL: &str = "OASIS7_LLM_BASE_URL";
pub const ENV_LLM_API_KEY: &str = "OASIS7_LLM_API_KEY";
pub const ENV_LLM_TIMEOUT_MS: &str = "OASIS7_LLM_TIMEOUT_MS";
pub const ENV_LLM_SYSTEM_PROMPT: &str = "OASIS7_LLM_SYSTEM_PROMPT";
pub const ENV_LLM_SHORT_TERM_GOAL: &str = "OASIS7_LLM_SHORT_TERM_GOAL";
pub const ENV_LLM_LONG_TERM_GOAL: &str = "OASIS7_LLM_LONG_TERM_GOAL";
pub const ENV_LLM_MAX_MODULE_CALLS: &str = "OASIS7_LLM_MAX_MODULE_CALLS";
pub const ENV_LLM_MAX_DECISION_STEPS: &str = "OASIS7_LLM_MAX_DECISION_STEPS";
pub const ENV_LLM_MAX_REPAIR_ROUNDS: &str = "OASIS7_LLM_MAX_REPAIR_ROUNDS";
pub const ENV_LLM_PROMPT_MAX_HISTORY_ITEMS: &str = "OASIS7_LLM_PROMPT_MAX_HISTORY_ITEMS";
pub const ENV_LLM_PROMPT_PROFILE: &str = "OASIS7_LLM_PROMPT_PROFILE";
pub const ENV_LLM_FORCE_REPLAN_AFTER_SAME_ACTION: &str =
    "OASIS7_LLM_FORCE_REPLAN_AFTER_SAME_ACTION";

pub const DEFAULT_CONFIG_FILE_NAME: &str = "config.toml";
pub use oasis7_agent_api::DEFAULT_LLM_MODEL;
pub const DEFAULT_LLM_SYSTEM_PROMPT: &str = "你是硅基文明发展 Agent。按“读规则/观察 -> 资源稳态 -> 产业建设 -> 治理协作 -> 危机韧性”推进文明进程，每轮仅提交一个可执行 decision。若规则或动作前置条件不明确，先调用 world.rules.guide 与 environment.current_observation，再做决策。";
pub const DEFAULT_LLM_SHORT_TERM_GOAL: &str = "先识别当前阶段最关键瓶颈，并按前置条件逐步推进：能源与数据稳定后再扩产，扩产后推进治理与风险处理。遇到 action_rejected 时根据 reject_reason 切换到补前置动作，避免原样重复失败参数。";
pub const DEFAULT_LLM_LONG_TERM_GOAL: &str = "构建可持续、可治理、具韧性的文明系统，让资源、组织与风险应对形成长期正反馈，并保持阶段推进可解释。";
pub const DEFAULT_LLM_MAX_MODULE_CALLS: usize = 3;
pub const DEFAULT_LLM_MAX_DECISION_STEPS: usize = 4;
pub const DEFAULT_LLM_MAX_REPAIR_ROUNDS: usize = 1;
pub const DEFAULT_LLM_PROMPT_MAX_HISTORY_ITEMS: usize = 4;
pub const DEFAULT_LLM_FORCE_REPLAN_AFTER_SAME_ACTION: usize = 4;
