//! AgentRunner: multi-agent scheduling, quota, rate limiting, metrics.
use super::WorldKernel;
use super::archive::WorldEvent;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::agent::{
    ActionResult, AgentBehavior, AgentDecision, AgentDecisionTrace, AgentQuery, AgentQueryResult,
};
use super::runtime_perf::{RuntimePerfCollector, RuntimePerfSnapshot};
use oasis7_simulator::WorldEventKind;
use oasis7_simulator::{Action, WorldTime};

// ============================================================================
// Quota and Rate Limiting
// ============================================================================

/// Quota limits for an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentQuota {
    /// Maximum number of actions the agent can take.
    pub max_actions: Option<u64>,
    /// Maximum number of decisions the agent can make.
    pub max_decisions: Option<u64>,
}

impl AgentQuota {
    /// Create a quota with a maximum number of actions.
    pub fn max_actions(limit: u64) -> Self {
        Self {
            max_actions: Some(limit),
            max_decisions: None,
        }
    }

    /// Create a quota with a maximum number of decisions.
    pub fn max_decisions(limit: u64) -> Self {
        Self {
            max_actions: None,
            max_decisions: Some(limit),
        }
    }

    /// Create a quota with both action and decision limits.
    pub fn new(max_actions: Option<u64>, max_decisions: Option<u64>) -> Self {
        Self {
            max_actions,
            max_decisions,
        }
    }

    /// Check if the quota is exhausted.
    pub fn is_exhausted(&self, action_count: u64, decision_count: u64) -> bool {
        if let Some(max) = self.max_actions
            && action_count >= max
        {
            return true;
        }
        if let Some(max) = self.max_decisions
            && decision_count >= max
        {
            return true;
        }
        false
    }

    /// Returns remaining actions, or None if unlimited.
    pub fn remaining_actions(&self, action_count: u64) -> Option<u64> {
        self.max_actions.map(|max| max.saturating_sub(action_count))
    }

    /// Returns remaining decisions, or None if unlimited.
    pub fn remaining_decisions(&self, decision_count: u64) -> Option<u64> {
        self.max_decisions
            .map(|max| max.saturating_sub(decision_count))
    }
}

/// Rate limiting policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateLimitPolicy {
    /// Maximum actions per time window.
    pub max_actions_per_window: u64,
    /// Time window size in ticks.
    pub window_size_ticks: u64,
}

impl RateLimitPolicy {
    /// Create a new rate limit policy.
    pub fn new(max_actions_per_window: u64, window_size_ticks: u64) -> Self {
        Self {
            max_actions_per_window,
            window_size_ticks,
        }
    }

    /// Create a policy allowing N actions per tick.
    pub fn actions_per_tick(n: u64) -> Self {
        Self {
            max_actions_per_window: n,
            window_size_ticks: 1,
        }
    }
}

/// Rate limiting state for an agent.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RateLimitState {
    /// Start of the current window.
    pub window_start: WorldTime,
    /// Actions taken in the current window.
    pub actions_in_window: u64,
}

impl RateLimitState {
    /// Check if the agent is rate limited.
    pub fn is_limited(&self, now: WorldTime, policy: &RateLimitPolicy) -> bool {
        if policy.window_size_ticks == 0 {
            return false;
        }
        // Check if we're still in the same window
        let window_end = self.window_start.saturating_add(policy.window_size_ticks);
        if now < window_end {
            // Still in the same window, check action count
            self.actions_in_window >= policy.max_actions_per_window
        } else {
            // New window, not limited
            false
        }
    }

    /// Record an action.
    pub fn record_action(&mut self, now: WorldTime, policy: &RateLimitPolicy) {
        if policy.window_size_ticks == 0 {
            return;
        }
        let window_end = self.window_start.saturating_add(policy.window_size_ticks);
        if now >= window_end {
            // Start a new window
            self.window_start = now;
            self.actions_in_window = 1;
        } else {
            // Same window
            self.actions_in_window += 1;
        }
    }

    /// Reset the rate limit state.
    pub fn reset(&mut self) {
        self.window_start = 0;
        self.actions_in_window = 0;
    }
}

// ============================================================================
// Registered Agent
// ============================================================================

/// An agent registration with behavior and state tracking.
#[derive(Debug)]
pub struct RegisteredAgent<B: AgentBehavior> {
    /// The agent behavior implementation.
    pub behavior: B,
    /// Number of ticks to wait before next decision.
    pub wait_until: Option<WorldTime>,
    /// Total actions taken by this agent.
    pub action_count: u64,
    /// Total decisions made by this agent.
    pub decision_count: u64,
    /// Per-agent quota (overrides runner-level quota).
    pub quota: Option<AgentQuota>,
    /// Rate limiting state.
    pub rate_limit_state: RateLimitState,
}

impl<B: AgentBehavior> RegisteredAgent<B> {
    /// Create a new registered agent.
    pub fn new(behavior: B) -> Self {
        Self {
            behavior,
            wait_until: None,
            action_count: 0,
            decision_count: 0,
            quota: None,
            rate_limit_state: RateLimitState::default(),
        }
    }

    /// Create a new registered agent with quota.
    pub fn with_quota(behavior: B, quota: AgentQuota) -> Self {
        Self {
            behavior,
            wait_until: None,
            action_count: 0,
            decision_count: 0,
            quota: Some(quota),
            rate_limit_state: RateLimitState::default(),
        }
    }

    /// Returns true if the agent is ready to act at the given time.
    pub fn is_ready(&self, now: WorldTime) -> bool {
        match self.wait_until {
            Some(until) => now >= until,
            None => true,
        }
    }

    /// Check if the agent has exhausted its quota.
    pub fn is_quota_exhausted(&self) -> bool {
        if let Some(quota) = &self.quota {
            quota.is_exhausted(self.action_count, self.decision_count)
        } else {
            false
        }
    }

    /// Check if the agent is rate limited at the given time.
    pub fn is_rate_limited(&self, now: WorldTime, policy: Option<&RateLimitPolicy>) -> bool {
        if let Some(policy) = policy {
            self.rate_limit_state.is_limited(now, policy)
        } else {
            false
        }
    }

    /// Record an action for rate limiting purposes.
    pub fn record_action(&mut self, now: WorldTime, policy: Option<&RateLimitPolicy>) {
        if let Some(policy) = policy {
            self.rate_limit_state.record_action(now, policy);
        }
    }
}

// ============================================================================
// Agent Runner
// ============================================================================

/// Runs the observe → decide → act loop for multiple agents.
///
/// The AgentRunner manages registered agents and coordinates their
/// interactions with the WorldKernel.
pub struct AgentRunner<B: AgentBehavior> {
    agents: BTreeMap<String, RegisteredAgent<B>>,
    /// Cursor for round-robin scheduling.
    scheduler_cursor: Option<String>,
    /// Total ticks executed.
    total_ticks: u64,
    /// Default quota for all agents (can be overridden per-agent).
    default_quota: Option<AgentQuota>,
    /// Rate limit policy for all agents.
    rate_limit_policy: Option<RateLimitPolicy>,
    /// Runtime execution performance collector.
    runtime_perf: RuntimePerfCollector,
}

fn llm_api_duration_from_trace(decision_trace: Option<&AgentDecisionTrace>) -> Duration {
    let llm_api_latency_ms = decision_trace
        .and_then(|trace| trace.llm_diagnostics.as_ref())
        .and_then(|diagnostics| diagnostics.latency_ms)
        .unwrap_or(0);
    Duration::from_millis(llm_api_latency_ms)
}

impl<B: AgentBehavior> Default for AgentRunner<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: AgentBehavior> AgentRunner<B> {
    /// Create a new agent runner.
    pub fn new() -> Self {
        Self {
            agents: BTreeMap::new(),
            scheduler_cursor: None,
            total_ticks: 0,
            default_quota: None,
            rate_limit_policy: None,
            runtime_perf: RuntimePerfCollector::default(),
        }
    }

    /// Create a new agent runner with a rate limit policy.
    pub fn with_rate_limit(rate_limit: RateLimitPolicy) -> Self {
        Self {
            agents: BTreeMap::new(),
            scheduler_cursor: None,
            total_ticks: 0,
            default_quota: None,
            rate_limit_policy: Some(rate_limit),
            runtime_perf: RuntimePerfCollector::default(),
        }
    }

    /// Create a new agent runner with a default quota.
    pub fn with_quota(quota: AgentQuota) -> Self {
        Self {
            agents: BTreeMap::new(),
            scheduler_cursor: None,
            total_ticks: 0,
            default_quota: Some(quota),
            rate_limit_policy: None,
            runtime_perf: RuntimePerfCollector::default(),
        }
    }

    /// Set the default quota for all agents.
    pub fn set_default_quota(&mut self, quota: Option<AgentQuota>) {
        self.default_quota = quota;
    }

    /// Set the rate limit policy for all agents.
    pub fn set_rate_limit(&mut self, policy: Option<RateLimitPolicy>) {
        self.rate_limit_policy = policy;
    }

    /// Get the rate limit policy.
    pub fn rate_limit_policy(&self) -> Option<&RateLimitPolicy> {
        self.rate_limit_policy.as_ref()
    }

    /// Get the default quota.
    pub fn default_quota(&self) -> Option<&AgentQuota> {
        self.default_quota.as_ref()
    }

    /// Register an agent with the runner.
    pub fn register(&mut self, behavior: B) {
        let agent_id = behavior.agent_id().to_string();
        self.agents.insert(agent_id, RegisteredAgent::new(behavior));
    }

    /// Register an agent with a specific quota.
    pub fn register_with_quota(&mut self, behavior: B, quota: AgentQuota) {
        let agent_id = behavior.agent_id().to_string();
        self.agents
            .insert(agent_id, RegisteredAgent::with_quota(behavior, quota));
    }

    /// Unregister an agent from the runner.
    pub fn unregister(&mut self, agent_id: &str) -> Option<RegisteredAgent<B>> {
        self.agents.remove(agent_id)
    }

    /// Get a reference to a registered agent.
    pub fn get(&self, agent_id: &str) -> Option<&RegisteredAgent<B>> {
        self.agents.get(agent_id)
    }

    /// Get a mutable reference to a registered agent.
    pub fn get_mut(&mut self, agent_id: &str) -> Option<&mut RegisteredAgent<B>> {
        self.agents.get_mut(agent_id)
    }

    /// Returns the number of registered agents.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Returns the IDs of all registered agents.
    pub fn agent_ids(&self) -> Vec<String> {
        self.agents.keys().cloned().collect()
    }

    fn is_agent_ready_for_tick(
        &self,
        kernel: &WorldKernel,
        now: WorldTime,
        id: &str,
        agent: &RegisteredAgent<B>,
    ) -> bool {
        if !kernel.model().agents.contains_key(id) {
            return false;
        }
        if !agent.is_ready(now) {
            return false;
        }
        let quota = agent.quota.as_ref().or(self.default_quota.as_ref());
        if let Some(q) = quota
            && q.is_exhausted(agent.action_count, agent.decision_count)
        {
            return false;
        }
        !agent.is_rate_limited(now, self.rate_limit_policy.as_ref())
    }

    fn select_next_ready_agent_id(&self, kernel: &WorldKernel, now: WorldTime) -> Option<String> {
        let cursor = self.scheduler_cursor.as_deref();
        let mut first_ready = None;
        let mut first_after_cursor = None;

        for (id, agent) in &self.agents {
            if !self.is_agent_ready_for_tick(kernel, now, id, agent) {
                continue;
            }
            let id = id.as_str();
            first_ready.get_or_insert(id);
            if cursor.is_some_and(|cursor| id > cursor) {
                first_after_cursor = Some(id);
                break;
            }
        }

        first_after_cursor.or(first_ready).map(str::to_owned)
    }

    /// Preview the same deterministic scheduler choice used by
    /// `tick_decide_only` without mutating cursor, quota, or Agent state.
    /// Runtime adapters use this hook to persist their lifecycle prefix before
    /// a ProviderBacked behavior performs synchronous I/O (the wasm lane).
    pub fn next_ready_agent_id(&self, kernel: &WorldKernel) -> Option<String> {
        self.select_next_ready_agent_id(kernel, kernel.time())
    }

    /// Returns the total ticks executed.
    pub fn total_ticks(&self) -> u64 {
        self.total_ticks
    }

    /// Run one tick of the agent loop.
    ///
    /// This method:
    /// 1. Selects the next ready agent (round-robin), respecting quota and rate limits
    /// 2. Gets an observation for that agent
    /// 3. Calls the agent's decide method
    /// 4. Submits the action to the kernel if the agent decides to act
    /// 5. Executes one step and notifies the agent of the result
    ///
    /// Returns the action result if an action was taken, or None if
    /// no agent was ready or all agents chose to wait.
    pub fn tick(&mut self, kernel: &mut WorldKernel) -> Option<AgentTickResult> {
        let tick_started_at = Instant::now();
        self.total_ticks += 1;
        let now = kernel.time();

        // Find the next ready agent using round-robin, respecting world
        // registration, wait state, quota, and rate limits.
        let Some(agent_id) = self.select_next_ready_agent_id(kernel, now) else {
            self.runtime_perf
                .record_tick_duration(tick_started_at.elapsed());
            return None;
        };

        self.scheduler_cursor = Some(agent_id.clone());

        // Get observation for the selected agent
        let observation = match kernel.observe(&agent_id) {
            Ok(obs) => obs,
            Err(_) => {
                self.runtime_perf
                    .record_tick_duration(tick_started_at.elapsed());
                return None;
            }
        };

        // Get decision from the agent
        let (decision, decision_trace, decision_duration) = {
            let agent = match self.agents.get_mut(&agent_id) {
                Some(agent) => agent,
                None => {
                    self.runtime_perf
                        .record_tick_duration(tick_started_at.elapsed());
                    return None;
                }
            };
            agent.decision_count += 1;

            let decision_started_at = Instant::now();
            let decision = agent.behavior.decide(&observation);
            let decision_trace = agent.behavior.take_decision_trace();
            (decision, decision_trace, decision_started_at.elapsed())
        };
        let llm_api_duration =
            self.record_decision_runtime_samples(decision_duration, decision_trace.as_ref());
        if matches!(&decision, AgentDecision::Act(_)) {
            Self::record_decision_trace_effects(kernel, decision_trace.as_ref());
        }

        let result = match decision {
            AgentDecision::Wait => Some(AgentTickResult {
                agent_id,
                decision: AgentDecision::Wait,
                action_result: None,
                query_result: None,
                skipped_reason: None,
                decision_trace,
            }),
            AgentDecision::WaitTicks(ticks) => {
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.wait_until = Some(now.saturating_add(ticks));
                }
                Some(AgentTickResult {
                    agent_id,
                    decision: AgentDecision::WaitTicks(ticks),
                    action_result: None,
                    query_result: None,
                    skipped_reason: None,
                    decision_trace,
                })
            }
            AgentDecision::Query(query) => {
                let query_result = Self::evaluate_query(kernel, query.clone());
                self.notify_query_result(agent_id.as_str(), &query_result);
                Some(AgentTickResult {
                    agent_id,
                    decision: AgentDecision::Query(query),
                    action_result: None,
                    query_result: Some(query_result),
                    skipped_reason: None,
                    decision_trace,
                })
            }
            AgentDecision::ModuleCommand { response } => Some(AgentTickResult {
                agent_id,
                decision: AgentDecision::ModuleCommand { response },
                action_result: None,
                query_result: None,
                skipped_reason: None,
                decision_trace,
            }),
            AgentDecision::Act(action) => {
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.action_count += 1;
                    // Record action for rate limiting.
                    let rate_policy = self.rate_limit_policy.as_ref();
                    agent.record_action(now, rate_policy);
                }

                let action_execution_started_at = Instant::now();
                let action_id = kernel.submit_action_from_agent(agent_id.clone(), action.clone());
                let event = kernel.step();
                let action_result = event.map(|event| {
                    let success = !matches!(event.kind, WorldEventKind::ActionRejected { .. });
                    ActionResult {
                        action: action.clone(),
                        action_id,
                        success,
                        event,
                    }
                });
                self.runtime_perf
                    .record_action_execution_duration(action_execution_started_at.elapsed());

                // Notify agent of the result
                if let Some(ref result) = action_result {
                    let callback_started_at = Instant::now();
                    if let Some(agent) = self.agents.get_mut(&agent_id) {
                        agent.behavior.on_action_result(result);
                    }
                    self.runtime_perf
                        .record_callback_duration(callback_started_at.elapsed());
                }

                Some(AgentTickResult {
                    agent_id,
                    decision: AgentDecision::Act(action),
                    action_result,
                    query_result: None,
                    skipped_reason: None,
                    decision_trace,
                })
            }
        };
        self.record_tick_duration_without_llm_api(tick_started_at.elapsed(), llm_api_duration);
        result
    }

    /// Run one decision tick without executing actions in the kernel.
    ///
    /// This keeps agent scheduling/quota/rate-limit semantics, but defers world
    /// state transition until external consensus commit and replay.
    pub fn tick_decide_only(&mut self, kernel: &mut WorldKernel) -> Option<AgentTickResult> {
        let tick_started_at = Instant::now();
        self.total_ticks += 1;
        let now = kernel.time();

        let Some(agent_id) = self.select_next_ready_agent_id(kernel, now) else {
            self.runtime_perf
                .record_tick_duration(tick_started_at.elapsed());
            return None;
        };
        self.scheduler_cursor = Some(agent_id.clone());

        let observation = match kernel.observe(&agent_id) {
            Ok(obs) => obs,
            Err(_) => {
                self.runtime_perf
                    .record_tick_duration(tick_started_at.elapsed());
                return None;
            }
        };

        let (decision, decision_trace, decision_duration) = {
            let agent = match self.agents.get_mut(&agent_id) {
                Some(agent) => agent,
                None => {
                    self.runtime_perf
                        .record_tick_duration(tick_started_at.elapsed());
                    return None;
                }
            };
            agent.decision_count += 1;
            let decision_started_at = Instant::now();
            let decision = agent.behavior.decide(&observation);
            let decision_trace = agent.behavior.take_decision_trace();
            (decision, decision_trace, decision_started_at.elapsed())
        };
        let llm_api_duration =
            self.record_decision_runtime_samples(decision_duration, decision_trace.as_ref());
        if matches!(&decision, AgentDecision::Act(_)) {
            Self::record_decision_trace_effects(kernel, decision_trace.as_ref());
        }

        let result = match decision {
            AgentDecision::Wait => Some(AgentTickResult {
                agent_id,
                decision: AgentDecision::Wait,
                action_result: None,
                query_result: None,
                skipped_reason: None,
                decision_trace,
            }),
            AgentDecision::WaitTicks(ticks) => {
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.wait_until = Some(now.saturating_add(ticks));
                }
                Some(AgentTickResult {
                    agent_id,
                    decision: AgentDecision::WaitTicks(ticks),
                    action_result: None,
                    query_result: None,
                    skipped_reason: None,
                    decision_trace,
                })
            }
            AgentDecision::Query(query) => {
                let query_result = Self::evaluate_query(kernel, query.clone());
                self.notify_query_result(agent_id.as_str(), &query_result);
                Some(AgentTickResult {
                    agent_id,
                    decision: AgentDecision::Query(query),
                    action_result: None,
                    query_result: Some(query_result),
                    skipped_reason: None,
                    decision_trace,
                })
            }
            AgentDecision::ModuleCommand { response } => Some(AgentTickResult {
                agent_id,
                decision: AgentDecision::ModuleCommand { response },
                action_result: None,
                query_result: None,
                skipped_reason: None,
                decision_trace,
            }),
            AgentDecision::Act(action) => {
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.action_count += 1;
                    let rate_policy = self.rate_limit_policy.as_ref();
                    agent.record_action(now, rate_policy);
                }
                Some(AgentTickResult {
                    agent_id,
                    decision: AgentDecision::Act(action),
                    action_result: None,
                    query_result: None,
                    skipped_reason: None,
                    decision_trace,
                })
            }
        };
        self.record_tick_duration_without_llm_api(tick_started_at.elapsed(), llm_api_duration);
        result
    }

    /// Collect multiple agent intents in the same logical tick and commit as one kernel batch.
    ///
    /// `max_agents` controls how many ready agents can contribute intents in this batch.
    /// When `max_agents == 0`, all currently registered agents are eligible.
    pub fn tick_collect_intents_and_commit(
        &mut self,
        kernel: &mut WorldKernel,
        max_agents: usize,
    ) -> Vec<AgentTickResult> {
        let limit = if max_agents == 0 {
            self.agents.len().max(1)
        } else {
            max_agents
        };
        let mut results = Vec::new();
        for _ in 0..limit {
            let Some(result) = self.tick_decide_only(kernel) else {
                break;
            };
            results.push(result);
        }
        if results.is_empty() {
            return results;
        }

        let mut submitted_actions = Vec::new();
        for (index, result) in results.iter().enumerate() {
            if let AgentDecision::Act(action) = &result.decision {
                let action_id =
                    kernel.submit_action_from_agent(result.agent_id.clone(), action.clone());
                submitted_actions.push((index, result.agent_id.clone(), action.clone(), action_id));
            }
        }

        if submitted_actions.is_empty() {
            return results;
        }

        let action_execution_started_at = Instant::now();
        let events = kernel.step_intents_batch();
        self.runtime_perf
            .record_action_execution_duration(action_execution_started_at.elapsed());

        let mut event_by_action_id = BTreeMap::new();
        if let Some(report) = kernel.last_intent_batch_report() {
            let rejected_count = report.rejected_action_ids.len();
            for (offset, action_id) in report.rejected_action_ids.iter().enumerate() {
                if let Some(event) = events.get(offset) {
                    event_by_action_id.insert(*action_id, event.clone());
                }
            }
            for (offset, action_id) in report.accepted_action_ids.iter().enumerate() {
                let index = rejected_count.saturating_add(offset);
                if let Some(event) = events.get(index) {
                    event_by_action_id.insert(*action_id, event.clone());
                }
            }
        }

        for (index, agent_id, action, action_id) in submitted_actions {
            let Some(event) = event_by_action_id.get(&action_id).cloned() else {
                continue;
            };
            let success = !matches!(event.kind, WorldEventKind::ActionRejected { .. });
            let action_result = ActionResult {
                action,
                action_id,
                success,
                event,
            };
            if let Some(result) = results.get_mut(index) {
                result.action_result = Some(action_result.clone());
            }
            let _ = self.notify_action_result(agent_id.as_str(), &action_result);
        }

        results
    }

    pub fn notify_action_result(&mut self, agent_id: &str, result: &ActionResult) -> bool {
        let Some(agent) = self.agents.get_mut(agent_id) else {
            return false;
        };
        let callback_started_at = Instant::now();
        agent.behavior.on_action_result(result);
        self.runtime_perf
            .record_callback_duration(callback_started_at.elapsed());
        true
    }

    fn evaluate_query(kernel: &WorldKernel, query: AgentQuery) -> AgentQueryResult {
        match query {
            AgentQuery::EvaluateMicroDepotQuote(request) => {
                let available_units_by_kind = kernel
                    .model()
                    .regional_infrastructure
                    .get(request.facility_id.as_str())
                    .map(|facility| facility.available_units_by_kind.clone());
                AgentQueryResult::EvaluateMicroDepotQuote {
                    result: kernel.micro_depot_quote(&request),
                    request,
                    available_units_by_kind,
                }
            }
            AgentQuery::QuoteMicroDepotInstall(action) => {
                AgentQueryResult::QuoteMicroDepotInstall {
                    result: kernel.micro_depot_install_quote(&action),
                    action,
                }
            }
        }
    }

    pub fn notify_query_result(&mut self, agent_id: &str, result: &AgentQueryResult) -> bool {
        let Some(agent) = self.agents.get_mut(agent_id) else {
            return false;
        };
        let callback_started_at = Instant::now();
        agent.behavior.on_query_result(result);
        self.runtime_perf
            .record_callback_duration(callback_started_at.elapsed());
        true
    }

    fn record_decision_runtime_samples(
        &mut self,
        decision_total_duration: Duration,
        decision_trace: Option<&AgentDecisionTrace>,
    ) -> Duration {
        let llm_api_duration = llm_api_duration_from_trace(decision_trace);
        let decision_local_duration = decision_total_duration
            .checked_sub(llm_api_duration)
            .unwrap_or(Duration::from_millis(0));
        self.runtime_perf
            .record_decision_duration(decision_local_duration);
        self.runtime_perf.record_llm_api_duration(llm_api_duration);
        llm_api_duration
    }

    fn record_decision_trace_effects(
        kernel: &mut WorldKernel,
        decision_trace: Option<&AgentDecisionTrace>,
    ) {
        let Some(trace) = decision_trace else {
            return;
        };
        for intent in &trace.llm_effect_intents {
            kernel.record_effect_intent(trace.agent_id.clone(), intent.clone());
        }
        for receipt in &trace.llm_effect_receipts {
            kernel.record_effect_receipt(trace.agent_id.clone(), receipt.clone());
        }
    }

    fn record_tick_duration_without_llm_api(
        &mut self,
        tick_total_duration: Duration,
        llm_api_duration: Duration,
    ) {
        let tick_local_duration = tick_total_duration
            .checked_sub(llm_api_duration)
            .unwrap_or(Duration::from_millis(0));
        self.runtime_perf.record_tick_duration(tick_local_duration);
    }

    /// Record action execution duration when action apply happens outside the runner.
    pub fn record_external_action_execution_duration(&mut self, duration: Duration) {
        self.runtime_perf.record_action_execution_duration(duration);
    }

    /// Returns runtime execution performance snapshot.
    pub fn runtime_perf_snapshot(&self) -> RuntimePerfSnapshot {
        self.runtime_perf.snapshot()
    }

    /// Reset runtime execution performance samples.
    pub fn reset_runtime_perf(&mut self) {
        self.runtime_perf.reset();
    }

    /// Check if an agent is quota-exhausted.
    pub fn is_quota_exhausted(&self, agent_id: &str) -> bool {
        if let Some(agent) = self.agents.get(agent_id) {
            let quota = agent.quota.as_ref().or(self.default_quota.as_ref());
            if let Some(q) = quota {
                return q.is_exhausted(agent.action_count, agent.decision_count);
            }
        }
        false
    }

    /// Check if an agent is rate-limited.
    pub fn is_rate_limited(&self, agent_id: &str, now: WorldTime) -> bool {
        if let Some(agent) = self.agents.get(agent_id) {
            return agent.is_rate_limited(now, self.rate_limit_policy.as_ref());
        }
        false
    }

    /// Reset rate limit state for an agent.
    pub fn reset_rate_limit(&mut self, agent_id: &str) {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.rate_limit_state.reset();
        }
    }

    /// Reset rate limit state for all agents.
    pub fn reset_all_rate_limits(&mut self) {
        for agent in self.agents.values_mut() {
            agent.rate_limit_state.reset();
        }
    }

    /// Run the agent loop for a specified number of ticks.
    ///
    /// Returns the results of all ticks where an agent was active.
    pub fn run(&mut self, kernel: &mut WorldKernel, max_ticks: u64) -> Vec<AgentTickResult> {
        let mut results = Vec::new();
        for _ in 0..max_ticks {
            if let Some(result) = self.tick(kernel) {
                results.push(result);
            }
        }
        results
    }

    /// Run the agent loop until all pending actions are processed.
    pub fn run_until_idle(
        &mut self,
        kernel: &mut WorldKernel,
        max_ticks: u64,
    ) -> Vec<AgentTickResult> {
        let mut results = Vec::new();
        let mut consecutive_waits = 0;
        let max_consecutive_waits = self.agents.len().max(1);

        for _ in 0..max_ticks {
            match self.tick(kernel) {
                Some(result) => {
                    if result.action_result.is_some() {
                        consecutive_waits = 0;
                    } else {
                        consecutive_waits += 1;
                    }
                    results.push(result);

                    if consecutive_waits >= max_consecutive_waits {
                        break;
                    }
                }
                None => break,
            }
        }
        results
    }

    /// Broadcast an event to all registered agents.
    pub fn broadcast_event(&mut self, event: &WorldEvent) {
        for agent in self.agents.values_mut() {
            agent.behavior.on_event(event);
        }
    }

    /// Get the current metrics for the runner.
    pub fn metrics(&self) -> RunnerMetrics {
        let mut total_actions = 0u64;
        let mut total_decisions = 0u64;
        let mut agents_active = 0usize;
        let mut agents_quota_exhausted = 0usize;

        for agent in self.agents.values() {
            total_actions += agent.action_count;
            total_decisions += agent.decision_count;
            // Check quota: use agent-specific quota or default quota
            let quota = agent.quota.as_ref().or(self.default_quota.as_ref());
            let is_exhausted = quota
                .map(|q| q.is_exhausted(agent.action_count, agent.decision_count))
                .unwrap_or(false);
            if !is_exhausted {
                agents_active += 1;
            } else {
                agents_quota_exhausted += 1;
            }
        }

        RunnerMetrics {
            total_ticks: self.total_ticks,
            total_agents: self.agents.len(),
            agents_active,
            agents_quota_exhausted,
            total_actions,
            total_decisions,
            actions_per_tick: if self.total_ticks > 0 {
                total_actions as f64 / self.total_ticks as f64
            } else {
                0.0
            },
            decisions_per_tick: if self.total_ticks > 0 {
                total_decisions as f64 / self.total_ticks as f64
            } else {
                0.0
            },
            success_rate: 0.0, // Will be computed in extended metrics
            runtime_perf: self.runtime_perf.snapshot(),
        }
    }

    /// Get detailed metrics with timing information.
    pub fn metrics_with_kernel(&self, kernel: &WorldKernel) -> RunnerMetrics {
        let mut metrics = self.metrics();
        let now = kernel.time();

        // Compute agents that are rate-limited
        let mut agents_rate_limited = 0usize;
        for agent in self.agents.values() {
            if agent.is_rate_limited(now, self.rate_limit_policy.as_ref()) {
                agents_rate_limited += 1;
            }
        }

        // Update active count to exclude rate-limited agents
        metrics.agents_active = metrics.agents_active.saturating_sub(agents_rate_limited);
        metrics
    }

    /// Get per-agent statistics.
    pub fn agent_stats(&self) -> Vec<AgentStats> {
        self.agents
            .iter()
            .map(|(id, agent)| AgentStats {
                agent_id: id.clone(),
                action_count: agent.action_count,
                decision_count: agent.decision_count,
                is_quota_exhausted: agent.is_quota_exhausted(),
                wait_until: agent.wait_until,
            })
            .collect()
    }
}

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

/// Result of a single agent tick.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentTickResult {
    /// The agent that was ticked.
    pub agent_id: String,
    /// The decision made by the agent.
    pub decision: AgentDecision,
    /// The result of the action if one was taken.
    pub action_result: Option<ActionResult>,
    /// The result of a read-only query, if one was made.
    pub query_result: Option<AgentQueryResult>,
    /// Reason why the agent was skipped (if applicable).
    pub skipped_reason: Option<SkippedReason>,
    /// Optional decision trace payload (e.g. LLM prompt/completion).
    pub decision_trace: Option<AgentDecisionTrace>,
}

impl AgentTickResult {
    /// Returns true if an action was taken.
    pub fn has_action(&self) -> bool {
        self.action_result.is_some()
    }

    /// Returns true if the action succeeded (or no action was taken).
    pub fn is_success(&self) -> bool {
        self.action_result
            .as_ref()
            .map(|r| r.success)
            .unwrap_or(true)
    }

    /// Returns true if the agent was skipped.
    pub fn was_skipped(&self) -> bool {
        self.skipped_reason.is_some()
    }
}
