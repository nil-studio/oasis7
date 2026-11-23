//! Pure simulator runner telemetry types; no model, archive or async host I/O.
//! The host AgentRunner retains scheduling and actual execution ownership.

use crate::{Action, AgentDecision, AgentDecisionTrace, RuntimePerfSnapshot, WorldTime};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ============================================================================
// Metrics and Statistics
// ============================================================================

/// Metrics for the AgentRunner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunnerMetrics {
    /// Total number of ticks executed.
    pub total_ticks: u64,
    /// Total number of registered agents.
    pub total_agents: usize,
    /// Number of agents that are active (not quota-exhausted, not rate-limited).
    pub agents_active: usize,
    /// Number of agents that have exhausted their quota.
    pub agents_quota_exhausted: usize,
    /// Total actions taken across all agents.
    pub total_actions: u64,
    /// Total decisions made across all agents.
    pub total_decisions: u64,
    /// Average actions per tick.
    pub actions_per_tick: f64,
    /// Average decisions per tick.
    pub decisions_per_tick: f64,
    /// Success rate of actions (0.0 to 1.0).
    pub success_rate: f64,
    /// Runtime execution performance snapshot.
    #[serde(default)]
    pub runtime_perf: RuntimePerfSnapshot,
}

impl Default for RunnerMetrics {
    fn default() -> Self {
        Self {
            total_ticks: 0,
            total_agents: 0,
            agents_active: 0,
            agents_quota_exhausted: 0,
            total_actions: 0,
            total_decisions: 0,
            actions_per_tick: 0.0,
            decisions_per_tick: 0.0,
            success_rate: 0.0,
            runtime_perf: RuntimePerfSnapshot::default(),
        }
    }
}

/// Statistics for a single agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStats {
    /// The agent's ID.
    pub agent_id: String,
    /// Total actions taken by this agent.
    pub action_count: u64,
    /// Total decisions made by this agent.
    pub decision_count: u64,
    /// Whether the agent has exhausted its quota.
    pub is_quota_exhausted: bool,
    /// Time until which the agent is waiting (if any).
    pub wait_until: Option<WorldTime>,
}

/// A log entry for runner events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunnerLogEntry {
    /// The tick number when this event occurred.
    pub tick: u64,
    /// The world time when this event occurred.
    pub time: WorldTime,
    /// The event kind.
    pub kind: RunnerLogKind,
}

/// Kinds of runner log events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum RunnerLogKind {
    /// An agent was registered.
    AgentRegistered { agent_id: String },
    /// An agent was unregistered.
    AgentUnregistered { agent_id: String },
    /// An agent made a decision.
    AgentDecision {
        agent_id: String,
        decision: AgentDecision,
    },
    /// An action was executed.
    ActionExecuted {
        agent_id: String,
        action: Action,
        success: bool,
    },
    /// An agent was skipped.
    AgentSkipped {
        agent_id: String,
        reason: SkippedReason,
    },
    /// An agent exhausted its quota.
    QuotaExhausted { agent_id: String },
    /// An agent was rate-limited.
    RateLimited { agent_id: String },
    /// Metrics snapshot.
    MetricsSnapshot { metrics: RunnerMetrics },
}

/// Reason why an agent was skipped during scheduling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkippedReason {
    /// Agent has exhausted its quota.
    QuotaExhausted,
    /// Agent is rate limited.
    RateLimited,
    /// Agent is waiting (wait_until not reached).
    Waiting,
    /// Agent is not registered in the world.
    NotInWorld,
}

/// Derive the previously host-local LLM latency sample from the immutable
/// pure decision trace. Missing diagnostics remain zero, with no clock IO.
pub fn llm_api_duration_from_trace(decision_trace: Option<&AgentDecisionTrace>) -> Duration {
    let llm_api_latency_ms = decision_trace
        .and_then(|trace| trace.llm_diagnostics.as_ref())
        .and_then(|diagnostics| diagnostics.latency_ms)
        .unwrap_or(0);
    Duration::from_millis(llm_api_latency_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_llm_trace_has_zero_sample() {
        assert_eq!(llm_api_duration_from_trace(None), Duration::ZERO);
    }

    #[test]
    fn metrics_default_keeps_finite_legacy_wire_values() {
        let metrics = RunnerMetrics::default();
        let value = serde_json::to_value(&metrics).unwrap();
        assert_eq!(value["total_ticks"], 0);
        assert_eq!(value["total_actions"], 0);
        assert_eq!(value["total_decisions"], 0);
        assert_eq!(value["actions_per_tick"], 0.0);
        assert_eq!(value["decisions_per_tick"], 0.0);
        assert_eq!(value["success_rate"], 0.0);
        assert!(value.get("runtime_perf").is_some());
        let roundtrip: RunnerMetrics = serde_json::from_value(value).unwrap();
        assert_eq!(roundtrip, metrics);
    }

    #[test]
    fn log_variant_keeps_existing_adjacent_tagged_wire() {
        let reason = RunnerLogKind::AgentSkipped {
            agent_id: "agent-1".to_owned(),
            reason: SkippedReason::QuotaExhausted,
        };
        let value = serde_json::to_value(&reason).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "type": "AgentSkipped",
                "data": { "agent_id": "agent-1", "reason": "QuotaExhausted" }
            })
        );
        let restored: RunnerLogKind = serde_json::from_value(value).unwrap();
        assert_eq!(restored, reason);
    }

    #[test]
    fn agent_stats_optional_wait_and_counters_remain_compatible() {
        let stats = AgentStats {
            agent_id: "agent-1".to_owned(),
            action_count: 2,
            decision_count: 3,
            is_quota_exhausted: false,
            wait_until: None,
        };
        let value = serde_json::to_value(&stats).unwrap();
        assert_eq!(value["action_count"], 2);
        assert_eq!(value["decision_count"], 3);
        assert!(value["wait_until"].is_null());
        assert_eq!(serde_json::from_value::<AgentStats>(value).unwrap(), stats);
    }
}
