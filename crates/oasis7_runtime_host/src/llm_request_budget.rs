//! Inactive native Host-owned LLM request-budget ledger, extracted from H2.
//!
//! Logical request digest, not HTTP transport attempt, is the accounting key.
//! This module is independently testable before the paid/live LLM cutover.
use oasis7_agent_api::BudgetContractV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetKind {
    ModelCall,
    ToolCall,
}

impl BudgetKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ModelCall => "max_model_calls",
            Self::ToolCall => "max_tool_calls",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetExhausted {
    pub kind: BudgetKind,
    pub used: u32,
    pub limit: u32,
}

impl BudgetExhausted {
    pub fn message(self) -> String {
        format!(
            "budget_exhausted: {} used={} max={}",
            self.kind.name(),
            self.used,
            self.limit
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetSnapshot {
    pub max_model_calls: u32,
    pub model_calls_used: u32,
    pub max_tool_calls: u32,
    pub tool_calls_used: u32,
}

/// Original H2 ledger policy: a retry of the same logical request must not
/// silently obtain another set of model/tool calls. New request resets counters.
#[derive(Debug, Default)]
pub struct RequestBudgetLedger {
    request_digest: Option<String>,
    model_calls: u32,
    tool_calls: u32,
}

impl RequestBudgetLedger {
    pub fn bind_request(&mut self, request_digest: Option<&str>) {
        let request_digest = request_digest.map(str::to_owned);
        if self.request_digest != request_digest {
            self.request_digest = request_digest;
            self.model_calls = 0;
            self.tool_calls = 0;
        }
    }

    pub fn admit_model_call(&mut self, budget: &BudgetContractV1) -> Result<(), BudgetExhausted> {
        Self::admit(
            BudgetKind::ModelCall,
            &mut self.model_calls,
            budget.max_model_calls,
        )
    }

    pub fn admit_tool_call(&mut self, budget: &BudgetContractV1) -> Result<(), BudgetExhausted> {
        Self::admit(
            BudgetKind::ToolCall,
            &mut self.tool_calls,
            budget.max_tool_calls,
        )
    }

    pub fn snapshot(&self, budget: &BudgetContractV1) -> BudgetSnapshot {
        BudgetSnapshot {
            max_model_calls: budget.max_model_calls,
            model_calls_used: self.model_calls,
            max_tool_calls: budget.max_tool_calls,
            tool_calls_used: self.tool_calls,
        }
    }

    fn admit(kind: BudgetKind, used: &mut u32, limit: u32) -> Result<(), BudgetExhausted> {
        if *used >= limit {
            return Err(BudgetExhausted {
                kind,
                used: *used,
                limit,
            });
        }
        *used = (*used).saturating_add(1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget(model: u32, tool: u32) -> BudgetContractV1 {
        BudgetContractV1 {
            max_latency_ms: 5_000,
            max_repair_attempts: 1,
            max_model_calls: model,
            max_tool_calls: tool,
        }
    }

    #[test]
    fn transport_retry_does_not_reset_model_or_tool_call_accounting() {
        let mut ledger = RequestBudgetLedger::default();
        let allowed = budget(1, 1);
        ledger.bind_request(Some("logical-request-a"));
        ledger.admit_model_call(&allowed).unwrap();
        ledger.admit_tool_call(&allowed).unwrap();
        ledger.bind_request(Some("logical-request-a")); // New HTTP attempt, same digest.
        let model_error = ledger.admit_model_call(&allowed).unwrap_err();
        let tool_error = ledger.admit_tool_call(&allowed).unwrap_err();
        assert_eq!(model_error.kind, BudgetKind::ModelCall);
        assert_eq!(tool_error.kind, BudgetKind::ToolCall);
        assert_eq!(
            model_error.message(),
            "budget_exhausted: max_model_calls used=1 max=1"
        );
        assert_eq!(ledger.snapshot(&allowed).model_calls_used, 1);
        assert_eq!(ledger.snapshot(&allowed).tool_calls_used, 1);
    }

    #[test]
    fn new_logical_request_resets_both_budgets_not_the_limits() {
        let mut ledger = RequestBudgetLedger::default();
        let allowed = budget(2, 1);
        ledger.bind_request(Some("logical-request-a"));
        ledger.admit_model_call(&allowed).unwrap();
        ledger.admit_tool_call(&allowed).unwrap();
        ledger.bind_request(Some("logical-request-b"));
        assert_eq!(ledger.snapshot(&allowed).model_calls_used, 0);
        assert_eq!(ledger.snapshot(&allowed).tool_calls_used, 0);
        ledger.admit_model_call(&allowed).unwrap();
        assert_eq!(ledger.snapshot(&allowed).model_calls_used, 1);
        assert_eq!(ledger.snapshot(&allowed).tool_calls_used, 0);
    }

    #[test]
    fn denied_calls_do_not_charge_and_zero_limit_fails_closed() {
        let mut ledger = RequestBudgetLedger::default();
        ledger.bind_request(Some("zero-budget"));
        let no_calls = budget(0, 0);
        let denied = ledger.admit_model_call(&no_calls).unwrap_err();
        assert_eq!((denied.used, denied.limit), (0, 0));
        assert_eq!(ledger.admit_tool_call(&no_calls).unwrap_err().used, 0);
        assert_eq!(ledger.snapshot(&no_calls).model_calls_used, 0);
        assert_eq!(ledger.snapshot(&no_calls).tool_calls_used, 0);
        ledger.bind_request(None);
        assert_eq!(ledger.snapshot(&no_calls).model_calls_used, 0);
    }
}
