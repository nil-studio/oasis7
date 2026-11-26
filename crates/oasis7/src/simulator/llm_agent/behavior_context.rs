use super::*;

use super::super::continuous_agent_harness::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1, ContinuousAgentTurnContextV1,
};
use super::super::decision_provider::ProviderDecision;

pub(super) use oasis7_runtime_host::llm_request_budget::{
    BudgetExhausted as CognitionBudgetExhausted, BudgetSnapshot as CognitionBudgetSnapshot,
    RequestBudgetLedger as CognitionBudgetLedger,
};

#[derive(Debug, Default)]
pub(super) struct ContinuousAgentContext {
    pub(super) turn_context: Option<ContinuousAgentTurnContextV1>,
    pub(super) request_context: Option<ContinuousAgentRequestContextV1>,
    pub(super) pending_response_context: Option<ContinuousAgentResponseContextV1>,
    pub(super) budget_ledger: CognitionBudgetLedger,
}

impl ContinuousAgentContext {
    pub(super) fn snapshot(&self) -> Option<CognitionBudgetSnapshot> {
        let request_context = self.request_context.as_ref()?;
        Some(
            self.budget_ledger
                .snapshot(&request_context.budget_contract),
        )
    }
}

impl<C: LlmCompletionClient> LlmAgentBehavior<C> {
    pub(super) fn collapse_multi_turn_payloads(
        parsed_turns: Vec<ParsedLlmTurn>,
    ) -> (Vec<ParsedLlmTurn>, Option<String>) {
        if parsed_turns.len() <= 1 {
            return (parsed_turns, None);
        }

        let observed_turns = parsed_turns.len();
        let mut fallback_turn: Option<ParsedLlmTurn> = None;
        let mut selected_turn: Option<ParsedLlmTurn> = None;
        for parsed_turn in parsed_turns.into_iter().rev() {
            if matches!(
                &parsed_turn,
                ParsedLlmTurn::Decision { .. }
                    | ParsedLlmTurn::ExecuteUntil { .. }
                    | ParsedLlmTurn::DecisionDraft { .. }
            ) {
                selected_turn = Some(parsed_turn);
                break;
            }
            if fallback_turn.is_none() {
                fallback_turn = Some(parsed_turn);
            }
        }

        let Some(selected_turn) = selected_turn.or(fallback_turn) else {
            return (
                Vec::new(),
                Some(format!(
                    "multi-turn output collapsed by guardrail: observed_turns={} kept_turn=none",
                    observed_turns
                )),
            );
        };

        let kept_turn_kind = Self::parsed_turn_kind_name(&selected_turn);
        (
            vec![selected_turn],
            Some(format!(
                "multi-turn output collapsed by guardrail: observed_turns={} kept_turn={}",
                observed_turns, kept_turn_kind
            )),
        )
    }

    fn parsed_turn_kind_name(parsed_turn: &ParsedLlmTurn) -> &'static str {
        match parsed_turn {
            ParsedLlmTurn::Plan { .. } => "plan",
            ParsedLlmTurn::DecisionDraft { .. } => "decision_draft",
            ParsedLlmTurn::Decision { .. } => "decision",
            ParsedLlmTurn::ExecuteUntil { .. } => "execute_until",
            ParsedLlmTurn::ModuleCall { .. } => "module_call",
            ParsedLlmTurn::Invalid(_) => "invalid",
        }
    }
}

impl<C: LlmCompletionClient> LlmAgentBehavior<C> {
    /// Keep a deterministic, bounded identity window for completion replays.
    /// ActionIds are allocated globally, so numeric or circular ordering is
    /// not a reliable age signal when unrelated actions make the IDs sparse.
    fn remember_recipe_completion_receipt(&mut self, job_id: u64) -> bool {
        if self
            .recipe_coverage
            .completion_receipt_ids
            .contains(&job_id)
        {
            return false;
        }
        self.recipe_coverage.completion_receipt_ids.insert(job_id);
        self.recipe_coverage
            .completion_receipt_order
            .push_back(job_id);
        while self.recipe_coverage.completion_receipt_order.len() > RECIPE_COMPLETION_REPLAY_WINDOW
        {
            if let Some(job_id) = self.recipe_coverage.completion_receipt_order.pop_front() {
                self.recipe_coverage.completion_receipt_ids.remove(&job_id);
            }
        }
        true
    }

    /// Consume authoritative completion feedback for this agent. Runtime-backed
    /// execution must reach `RecipeCompleted`; the pure simulator applies the
    /// recipe resource transformation atomically, so its successful
    /// `RecipeScheduled` event is also a completion receipt. Event/job identity
    /// keeps replay or duplicate delivery idempotent.
    pub(super) fn consume_recipe_completion_feedback(
        &mut self,
        event: &WorldEvent,
    ) -> Option<bool> {
        let (receipt_id, requester_agent_id, recipe_id) =
            if let Some(runtime_event) = event.runtime_event.as_ref() {
                let RuntimeWorldEventBody::Domain(RuntimeDomainEvent::RecipeCompleted {
                    job_id,
                    requester_agent_id,
                    recipe_id,
                    ..
                }) = &runtime_event.body
                else {
                    return None;
                };
                (*job_id, requester_agent_id.as_str(), recipe_id.as_str())
            } else {
                let WorldEventKind::RecipeScheduled {
                    owner: ResourceOwner::Agent { agent_id },
                    recipe_id,
                    ..
                } = &event.kind
                else {
                    return None;
                };
                (event.id, agent_id.as_str(), recipe_id.as_str())
            };
        if requester_agent_id != self.agent_id || !RecipeCoverageProgress::is_tracked(recipe_id) {
            return None;
        }
        if !self.remember_recipe_completion_receipt(receipt_id) {
            return Some(false);
        }
        self.recipe_coverage.mark_completed(recipe_id);
        Some(true)
    }
}

pub(super) fn builtin_provider_decision(decision: &AgentDecision) -> ProviderDecision {
    match decision {
        AgentDecision::Wait => ProviderDecision::Wait,
        AgentDecision::WaitTicks(ticks) => ProviderDecision::WaitTicks { ticks: *ticks },
        AgentDecision::Act(action) => ProviderDecision::Act {
            action_ref: "builtin_action".to_string(),
            action: action.clone(),
        },
        AgentDecision::Query(query) => ProviderDecision::Query {
            query_ref: "builtin_query".to_string(),
            query: query.clone(),
        },
        AgentDecision::ModuleCommand { response } => ProviderDecision::ModuleCommandResponse {
            response: response.clone(),
        },
    }
}

#[cfg(test)]
mod budget_owner_tests {
    use super::*;
    use oasis7_runtime_host::llm_request_budget::RequestBudgetLedger;

    #[test]
    fn live_context_uses_canonical_request_budget_ledger() {
        let mut context = ContinuousAgentContext::default();
        let ledger: &mut RequestBudgetLedger = &mut context.budget_ledger;
        let budget = super::super::super::continuous_agent_harness::BudgetContractV1 {
            max_latency_ms: 5_000,
            max_repair_attempts: 1,
            max_model_calls: 1,
            max_tool_calls: 1,
        };
        ledger.bind_request(Some("logical-request"));
        ledger.admit_model_call(&budget).unwrap();
        ledger.admit_tool_call(&budget).unwrap();
        ledger.bind_request(Some("logical-request"));
        assert_eq!(ledger.admit_model_call(&budget).unwrap_err().used, 1);
        assert_eq!(ledger.admit_tool_call(&budget).unwrap_err().used, 1);
        ledger.bind_request(Some("new-request"));
        assert_eq!(ledger.snapshot(&budget).model_calls_used, 0);
        assert_eq!(ledger.snapshot(&budget).tool_calls_used, 0);
    }
}
