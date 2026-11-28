use super::behavior_context::{CognitionBudgetExhausted, CognitionBudgetSnapshot};
use super::*;

pub(super) struct BudgetTraceState {
    pub(super) model: Option<String>,
    pub(super) latency_total_ms: u64,
    pub(super) prompt_tokens_total: u64,
    pub(super) completion_tokens_total: u64,
    pub(super) total_tokens_total: u64,
    pub(super) has_prompt_tokens: bool,
    pub(super) has_completion_tokens: bool,
    pub(super) has_total_tokens: bool,
    pub(super) repair_rounds_used: u32,
    pub(super) trace_inputs: Vec<String>,
    pub(super) trace_outputs: Vec<String>,
    pub(super) llm_step_trace: Vec<LlmStepTrace>,
    pub(super) llm_prompt_section_trace: Vec<LlmPromptSectionTrace>,
}

pub(super) fn budget_diagnostics(
    snapshot: Option<CognitionBudgetSnapshot>,
) -> LlmDecisionDiagnostics {
    let Some(snapshot) = snapshot else {
        return LlmDecisionDiagnostics::default();
    };
    LlmDecisionDiagnostics {
        max_model_calls: Some(snapshot.max_model_calls),
        model_calls_used: Some(snapshot.model_calls_used),
        max_tool_calls: Some(snapshot.max_tool_calls),
        tool_calls_used: Some(snapshot.tool_calls_used),
        ..LlmDecisionDiagnostics::default()
    }
}

impl<C: LlmCompletionClient> LlmAgentBehavior<C> {
    pub(super) fn budget_exhausted_decision(
        &mut self,
        observation: &Observation,
        trace_chat_start: usize,
        exhausted: CognitionBudgetExhausted,
        state: BudgetTraceState,
    ) -> AgentDecision {
        let decision = AgentDecision::Wait;
        let message = exhausted.message();
        let trace_chat_messages = self.conversation_history
            [trace_chat_start.min(self.conversation_history.len())..]
            .to_vec();
        let mut llm_step_trace = state.llm_step_trace;
        llm_step_trace.push(LlmStepTrace {
            step_index: llm_step_trace.len(),
            step_type: "budget_admission".to_string(),
            input_summary: exhausted.kind.name().to_string(),
            output_summary: message.clone(),
            status: "denied".to_string(),
        });
        let mut trace_outputs = state.trace_outputs;
        trace_outputs.push(message.clone());
        self.pending_trace = Some(AgentDecisionTrace {
            agent_id: self.agent_id.clone(),
            time: observation.time,
            decision: decision.clone(),
            llm_input: (!state.trace_inputs.is_empty())
                .then(|| state.trace_inputs.join("\n\n---\n\n")),
            llm_output: (!trace_outputs.is_empty()).then(|| trace_outputs.join("\n\n---\n\n")),
            llm_error: Some(message),
            parse_error: None,
            llm_diagnostics: Some(LlmDecisionDiagnostics {
                model: state.model,
                latency_ms: Some(state.latency_total_ms),
                prompt_tokens: state.has_prompt_tokens.then_some(state.prompt_tokens_total),
                completion_tokens: state
                    .has_completion_tokens
                    .then_some(state.completion_tokens_total),
                total_tokens: state.has_total_tokens.then_some(state.total_tokens_total),
                retry_count: state.repair_rounds_used,
                ..budget_diagnostics(self.continuous_context.snapshot())
            }),
            // Exhaustion must not create a new projected effect.  The prior
            // calls remain auditable in input/output and step traces.
            llm_effect_intents: Vec::new(),
            llm_effect_receipts: Vec::new(),
            llm_step_trace,
            llm_prompt_section_trace: state.llm_prompt_section_trace,
            llm_chat_messages: trace_chat_messages,
        });
        self.set_builtin_response_context(&decision);
        decision
    }
}
