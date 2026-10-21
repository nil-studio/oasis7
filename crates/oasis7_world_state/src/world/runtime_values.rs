use serde::{Deserialize, Serialize};
const DEFAULT_MAX_JOURNAL_EVENTS: usize = 65_536;
const DEFAULT_MAX_INFLIGHT_EFFECTS: usize = 8_192;
const DEFAULT_MAX_PENDING_EFFECTS: usize = 8_192;
const DEFAULT_MAX_PENDING_ACTIONS: usize = 8_192;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRuntimeMemoryLimits {
    pub max_pending_actions: usize,
    pub max_pending_effects: usize,
    pub max_inflight_effects: usize,
    pub max_journal_events: usize,
}

impl Default for WorldRuntimeMemoryLimits {
    fn default() -> Self {
        Self {
            max_pending_actions: DEFAULT_MAX_PENDING_ACTIONS,
            max_pending_effects: DEFAULT_MAX_PENDING_EFFECTS,
            max_inflight_effects: DEFAULT_MAX_INFLIGHT_EFFECTS,
            max_journal_events: DEFAULT_MAX_JOURNAL_EVENTS,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRuntimeBackpressureStats {
    pub pending_actions_evicted: u64,
    pub pending_effects_evicted: u64,
    pub inflight_effects_evicted: u64,
    pub inflight_effect_dispatch_blocked: u64,
    pub journal_events_evicted: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleTickRoutingDurationBuckets {
    pub lt_1ms: u64,
    pub ms_1_to_5: u64,
    pub ms_5_to_25: u64,
    pub ms_25_to_100: u64,
    pub ge_100ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleTickRoutingMetricsSnapshot {
    pub schedule_len: usize,
    pub last_due_count: usize,
    pub last_invoked_count: usize,
    pub missing_invocation_count: u64,
    pub last_missing_invocation_count: usize,
    pub oldest_overdue_ticks: Option<u64>,
    pub routing_count: u64,
    pub last_route_duration_ms: u64,
    pub max_route_duration_ms: u64,
    pub cumulative_route_duration_ms: u64,
    pub duration_buckets: ModuleTickRoutingDurationBuckets,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleTickRoutingDeterministicSnapshot {
    pub last_due_count: usize,
    pub last_invoked_count: usize,
    pub missing_invocation_count: u64,
    pub last_missing_invocation_count: usize,
    pub oldest_overdue_ticks: Option<u64>,
    pub routing_count: u64,
}
