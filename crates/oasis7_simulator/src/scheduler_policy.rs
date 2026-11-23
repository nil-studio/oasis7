//! Tick-based quota and rate-limit policy for deterministic simulator scheduling.
//!
//! No wall-clock time, host I/O, or agent-runner lifecycle is owned here.

use crate::WorldTime;
use serde::{Deserialize, Serialize};

/// Select the first eligible Agent ID after the previous cursor, wrapping to
/// the first eligible ID when necessary. The input order must be deterministic
/// (the Host's BTreeMap is ordered). Readiness is computed lazily so the Host
/// retains its original short-circuit behavior and owns all admission checks.
///
/// This pure selector has no World/AgentRunner/Host state or external authority.
pub fn choose_next_ready_agent_id_v1<'a>(
    ordered: impl IntoIterator<Item = (&'a str, bool)>,
    cursor: Option<&str>,
) -> Option<&'a str> {
    let mut first_ready = None;
    for (id, ready) in ordered {
        if !ready {
            continue;
        }
        first_ready.get_or_insert(id);
        if cursor.is_some_and(|last| id > last) {
            return Some(id);
        }
    }
    first_ready
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotas_preserve_independent_action_and_decision_limits() {
        let quota = AgentQuota::new(Some(2), Some(3));
        assert_eq!(quota.remaining_actions(1), Some(1));
        assert_eq!(quota.remaining_decisions(2), Some(1));
        assert!(!quota.is_exhausted(1, 2));
        assert!(quota.is_exhausted(2, 2));
        assert!(quota.is_exhausted(1, 3));
        assert_eq!(quota.remaining_actions(u64::MAX), Some(0));
        assert!(!AgentQuota::new(None, None).is_exhausted(u64::MAX, u64::MAX));
    }

    #[test]
    fn rate_limits_use_tick_windows_without_wall_clock() {
        let policy = RateLimitPolicy::actions_per_tick(2);
        let mut state = RateLimitState::default();
        assert!(!state.is_limited(0, &policy));
        state.record_action(0, &policy);
        assert!(!state.is_limited(0, &policy));
        state.record_action(0, &policy);
        assert!(state.is_limited(0, &policy));
        assert!(!state.is_limited(1, &policy));
        state.record_action(1, &policy);
        assert_eq!(state.window_start, 1);
        assert_eq!(state.actions_in_window, 1);
        state.reset();
        assert_eq!(state, RateLimitState::default());
    }

    #[test]
    fn zero_tick_window_preserves_unlimited_behavior() {
        let policy = RateLimitPolicy::new(0, 0);
        let mut state = RateLimitState::default();
        state.record_action(100, &policy);
        assert_eq!(state, RateLimitState::default());
        assert!(!state.is_limited(100, &policy));
    }

    #[test]
    fn ready_agent_cursor_rotates_and_wraps_without_selecting_blocked_agents() {
        let agents = [("a", true), ("b", false), ("c", true)];
        for (cursor, expected) in [
            (None, Some("a")),
            (Some("a"), Some("c")),
            (Some("b"), Some("c")),
            (Some("c"), Some("a")),
            (Some("z"), Some("a")),
        ] {
            assert_eq!(choose_next_ready_agent_id_v1(agents, cursor), expected);
        }
        assert_eq!(choose_next_ready_agent_id_v1([("a", false)], None), None);
        assert_eq!(
            choose_next_ready_agent_id_v1([] as [(&str, bool); 0], Some("a")),
            None
        );
    }

    #[test]
    fn selection_does_not_evaluate_later_candidates_after_cursor_match() {
        let mut visited = 0;
        let selected = choose_next_ready_agent_id_v1(
            [("a", true), ("b", true), ("c", true)]
                .into_iter()
                .map(|(id, ready)| {
                    visited += 1;
                    (id, ready)
                }),
            Some("a"),
        );
        assert_eq!(selected, Some("b"));
        assert_eq!(visited, 2);
    }
}
