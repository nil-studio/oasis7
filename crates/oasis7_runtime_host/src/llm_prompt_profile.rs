//! Native L4 owner of LLM prompt and memory-selection profile policy.
use oasis7_agent_harness::llm_prompt_assembly::PromptBudget;
use oasis7_simulator::agent_memory_selector::MemorySelectorConfig;

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

pub const DEFAULT_LLM_PROMPT_PROFILE: LlmPromptProfile = LlmPromptProfile::Balanced;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_profile_preserves_original_budgets_and_selection() {
        assert_eq!(LlmPromptProfile::parse(" Compact "), Some(LlmPromptProfile::Compact));
        assert_eq!(LlmPromptProfile::parse("unknown"), None);
        assert_eq!(LlmPromptProfile::Balanced.as_str(), "balanced");
        assert_eq!(LlmPromptProfile::Compact.prompt_budget().context_window_tokens, 4_096);
        assert_eq!(LlmPromptProfile::Balanced.prompt_budget().reserved_output_tokens, 896);
        assert_eq!(LlmPromptProfile::Compact.memory_selector_config().short_term_top_k, 3);
        assert_eq!(LlmPromptProfile::Balanced.memory_selector_config().long_term_top_k, 3);
    }
}
