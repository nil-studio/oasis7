//! Pure performance-series aggregation from caller-provided `Duration` inputs.
//!
//! This code never reads wall-clock time, performs I/O or drives a host loop.
//! The host `AgentRunner` owns `Instant::now()` and supplies measured samples.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;

const DEFAULT_SAMPLE_WINDOW: usize = 512;
const DEFAULT_TICK_BUDGET_MS: f64 = 33.0;
const DEFAULT_DECISION_BUDGET_MS: f64 = 20.0;
const DEFAULT_ACTION_EXECUTION_BUDGET_MS: f64 = 20.0;
const DEFAULT_CALLBACK_BUDGET_MS: f64 = 10.0;
const DEFAULT_LLM_API_BUDGET_MS: f64 = 1000.0;

const WARN_OVER_BUDGET_RATIO_PPM: u64 = 50_000;
const CRITICAL_OVER_BUDGET_RATIO_PPM: u64 = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePerfHealth {
    #[default]
    Unknown,
    Healthy,
    Warn,
    Critical,
}

impl RuntimePerfHealth {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Healthy => "healthy",
            Self::Warn => "warn",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePerfBottleneck {
    #[default]
    None,
    Tick,
    Decision,
    ActionExecution,
    Callback,
}

impl RuntimePerfBottleneck {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Tick => "tick",
            Self::Decision => "decision",
            Self::ActionExecution => "action_execution",
            Self::Callback => "callback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RuntimePerfSeriesSnapshot {
    pub samples_total: u64,
    pub samples_window: usize,
    pub budget_ms: f64,
    pub last_ms: f64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub over_budget_total: u64,
    pub over_budget_ratio_ppm: u64,
}

impl RuntimePerfSeriesSnapshot {
    pub fn has_samples(&self) -> bool {
        self.samples_total > 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RuntimePerfSnapshot {
    pub sample_window: usize,
    pub tick: RuntimePerfSeriesSnapshot,
    pub decision: RuntimePerfSeriesSnapshot,
    pub action_execution: RuntimePerfSeriesSnapshot,
    pub callback: RuntimePerfSeriesSnapshot,
    #[serde(default)]
    pub llm_api: RuntimePerfSeriesSnapshot,
    pub health: RuntimePerfHealth,
    pub bottleneck: RuntimePerfBottleneck,
}

/// Constructs the metrics snapshot used when runtime performance data is unavailable.
///
/// This intentionally preserves the default snapshot shape while making `Unknown`
/// health explicit at the boundary that reports unsupported metrics.
#[allow(dead_code)]
pub fn unsupported_runtime_perf_snapshot() -> RuntimePerfSnapshot {
    let snapshot = RuntimePerfSnapshot::default();
    debug_assert_eq!(snapshot.health, RuntimePerfHealth::Unknown);
    snapshot
}

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct RuntimePerfCollector {
    sample_window: usize,
    tick: PerfSeriesState,
    decision: PerfSeriesState,
    action_execution: PerfSeriesState,
    callback: PerfSeriesState,
    llm_api: PerfSeriesState,
}

impl Default for RuntimePerfCollector {
    fn default() -> Self {
        Self::with_sample_window(DEFAULT_SAMPLE_WINDOW)
    }
}

impl RuntimePerfCollector {
    pub fn with_sample_window(sample_window: usize) -> Self {
        let sample_window = sample_window.max(1);
        Self {
            sample_window,
            tick: PerfSeriesState::new(DEFAULT_TICK_BUDGET_MS),
            decision: PerfSeriesState::new(DEFAULT_DECISION_BUDGET_MS),
            action_execution: PerfSeriesState::new(DEFAULT_ACTION_EXECUTION_BUDGET_MS),
            callback: PerfSeriesState::new(DEFAULT_CALLBACK_BUDGET_MS),
            llm_api: PerfSeriesState::new(DEFAULT_LLM_API_BUDGET_MS),
        }
    }

    pub fn record_tick_duration(&mut self, duration: Duration) {
        self.tick
            .record(duration_to_ms(duration), self.sample_window);
    }

    pub fn record_decision_duration(&mut self, duration: Duration) {
        self.decision
            .record(duration_to_ms(duration), self.sample_window);
    }

    pub fn record_action_execution_duration(&mut self, duration: Duration) {
        self.action_execution
            .record(duration_to_ms(duration), self.sample_window);
    }

    pub fn record_callback_duration(&mut self, duration: Duration) {
        self.callback
            .record(duration_to_ms(duration), self.sample_window);
    }

    pub fn record_llm_api_duration(&mut self, duration: Duration) {
        self.llm_api
            .record(duration_to_ms(duration), self.sample_window);
    }

    pub fn snapshot(&self) -> RuntimePerfSnapshot {
        let mut percentile_scratch = Vec::new();
        let tick = self.tick.snapshot(&mut percentile_scratch);
        let decision = self.decision.snapshot(&mut percentile_scratch);
        let action_execution = self.action_execution.snapshot(&mut percentile_scratch);
        let callback = self.callback.snapshot(&mut percentile_scratch);
        let llm_api = self.llm_api.snapshot(&mut percentile_scratch);
        let health = derive_health([&tick, &decision, &action_execution, &callback]);
        let bottleneck = derive_bottleneck(&tick, &decision, &action_execution, &callback);
        RuntimePerfSnapshot {
            sample_window: self.sample_window,
            tick,
            decision,
            action_execution,
            callback,
            llm_api,
            health,
            bottleneck,
        }
    }

    pub fn reset(&mut self) {
        self.tick.reset();
        self.decision.reset();
        self.action_execution.reset();
        self.callback.reset();
        self.llm_api.reset();
    }
}

#[derive(Debug, Clone)]
struct PerfSeriesState {
    budget_ms: f64,
    window_samples: VecDeque<f64>,
    samples_total: u64,
    over_budget_total: u64,
    total_ms: f64,
    last_ms: f64,
    min_ms: f64,
    max_ms: f64,
}

impl PerfSeriesState {
    fn new(budget_ms: f64) -> Self {
        Self {
            budget_ms,
            window_samples: VecDeque::new(),
            samples_total: 0,
            over_budget_total: 0,
            total_ms: 0.0,
            last_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
        }
    }

    fn record(&mut self, sample_ms: f64, sample_window: usize) {
        if !sample_ms.is_finite() || sample_ms <= 0.0 {
            return;
        }
        self.samples_total = self.samples_total.saturating_add(1);
        self.total_ms += sample_ms;
        self.last_ms = sample_ms;
        if self.samples_total == 1 {
            self.min_ms = sample_ms;
            self.max_ms = sample_ms;
        } else {
            self.min_ms = self.min_ms.min(sample_ms);
            self.max_ms = self.max_ms.max(sample_ms);
        }
        if sample_ms > self.budget_ms {
            self.over_budget_total = self.over_budget_total.saturating_add(1);
        }

        self.window_samples.push_back(sample_ms);
        while self.window_samples.len() > sample_window {
            self.window_samples.pop_front();
        }
    }

    fn snapshot(&self, percentile_scratch: &mut Vec<f64>) -> RuntimePerfSeriesSnapshot {
        let window = WindowSampleStats::from_samples(&self.window_samples, percentile_scratch);
        let avg_ms = if self.samples_total > 0 {
            self.total_ms / self.samples_total as f64
        } else {
            0.0
        };
        let over_budget_ratio_ppm = self
            .over_budget_total
            .saturating_mul(1_000_000)
            .checked_div(self.samples_total)
            .unwrap_or_default();

        RuntimePerfSeriesSnapshot {
            samples_total: self.samples_total,
            samples_window: window.len,
            budget_ms: self.budget_ms,
            last_ms: self.last_ms,
            avg_ms,
            min_ms: self.min_ms,
            max_ms: self.max_ms,
            p50_ms: window.p50_ms,
            p95_ms: window.p95_ms,
            p99_ms: window.p99_ms,
            over_budget_total: self.over_budget_total,
            over_budget_ratio_ppm,
        }
    }

    fn reset(&mut self) {
        self.window_samples.clear();
        self.samples_total = 0;
        self.over_budget_total = 0;
        self.total_ms = 0.0;
        self.last_ms = 0.0;
        self.min_ms = 0.0;
        self.max_ms = 0.0;
    }
}

fn duration_to_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[derive(Debug, Clone, Copy)]
struct WindowSampleStats {
    len: usize,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
}

impl WindowSampleStats {
    fn from_samples(samples: &VecDeque<f64>, values: &mut Vec<f64>) -> Self {
        let len = samples.len();
        if len == 0 {
            return Self {
                len,
                p50_ms: 0.0,
                p95_ms: 0.0,
                p99_ms: 0.0,
            };
        }

        values.clear();
        values.extend(samples.iter().copied());
        let p50_index = percentile_index(len, 0.50);
        let p95_index = percentile_index(len, 0.95);
        let p99_index = percentile_index(len, 0.99);

        Self {
            len,
            p50_ms: select_percentile_value(values, p50_index),
            p95_ms: select_percentile_value(values, p95_index),
            p99_ms: select_percentile_value(values, p99_index),
        }
    }
}

fn percentile_index(len: usize, percentile: f64) -> usize {
    ((len - 1) as f64 * percentile.clamp(0.0, 1.0)).round() as usize
}

fn select_percentile_value(values: &mut [f64], index: usize) -> f64 {
    let (_, value, _) = values.select_nth_unstable_by(index, |left, right| left.total_cmp(right));
    *value
}

fn is_critical_stage(stage: &RuntimePerfSeriesSnapshot) -> bool {
    stage.has_samples()
        && ((stage.budget_ms > 0.0 && stage.p95_ms > stage.budget_ms * 2.0)
            || stage.over_budget_ratio_ppm >= CRITICAL_OVER_BUDGET_RATIO_PPM)
}

fn is_warn_stage(stage: &RuntimePerfSeriesSnapshot) -> bool {
    stage.has_samples()
        && ((stage.budget_ms > 0.0 && stage.p95_ms > stage.budget_ms)
            || stage.over_budget_ratio_ppm >= WARN_OVER_BUDGET_RATIO_PPM)
}

fn derive_health(stages: [&RuntimePerfSeriesSnapshot; 4]) -> RuntimePerfHealth {
    if stages.iter().all(|stage| !stage.has_samples()) {
        return RuntimePerfHealth::Unknown;
    }
    if stages.iter().any(|stage| is_critical_stage(stage)) {
        return RuntimePerfHealth::Critical;
    }
    if stages.iter().any(|stage| is_warn_stage(stage)) {
        return RuntimePerfHealth::Warn;
    }
    RuntimePerfHealth::Healthy
}

fn derive_bottleneck(
    tick: &RuntimePerfSeriesSnapshot,
    decision: &RuntimePerfSeriesSnapshot,
    action_execution: &RuntimePerfSeriesSnapshot,
    callback: &RuntimePerfSeriesSnapshot,
) -> RuntimePerfBottleneck {
    let candidates = [
        (RuntimePerfBottleneck::Tick, tick),
        (RuntimePerfBottleneck::Decision, decision),
        (RuntimePerfBottleneck::ActionExecution, action_execution),
        (RuntimePerfBottleneck::Callback, callback),
    ];

    let mut current = RuntimePerfBottleneck::None;
    let mut current_p95 = 0.0;
    for (bottleneck, stage) in candidates {
        if !stage.has_samples() {
            continue;
        }
        if stage.p95_ms > current_p95 {
            current = bottleneck;
            current_p95 = stage.p95_ms;
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_snapshots_window_percentiles_and_budget_ratio() {
        let mut collector = RuntimePerfCollector::with_sample_window(3);
        collector.record_tick_duration(Duration::from_micros(10_000));
        collector.record_tick_duration(Duration::from_micros(20_000));
        collector.record_tick_duration(Duration::from_micros(30_000));
        collector.record_tick_duration(Duration::from_micros(40_000));

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.tick.samples_total, 4);
        assert_eq!(snapshot.tick.samples_window, 3);
        assert!((snapshot.tick.p50_ms - 30.0).abs() < f64::EPSILON);
        assert!((snapshot.tick.p95_ms - 40.0).abs() < f64::EPSILON);
        assert!(snapshot.tick.over_budget_ratio_ppm >= 250_000);
    }

    #[test]
    fn collector_health_and_bottleneck_are_derived_from_series() {
        let mut collector = RuntimePerfCollector::default();
        assert_eq!(collector.snapshot().health, RuntimePerfHealth::Unknown);
        assert_eq!(collector.snapshot().bottleneck, RuntimePerfBottleneck::None);

        for _ in 0..10 {
            collector.record_tick_duration(Duration::from_micros(8_000));
            collector.record_decision_duration(Duration::from_micros(9_000));
            collector.record_action_execution_duration(Duration::from_micros(11_000));
            collector.record_callback_duration(Duration::from_micros(2_000));
        }
        let healthy = collector.snapshot();
        assert_eq!(healthy.health, RuntimePerfHealth::Healthy);
        assert_eq!(healthy.bottleneck, RuntimePerfBottleneck::ActionExecution);

        collector.record_decision_duration(Duration::from_micros(25_000));
        let warn = collector.snapshot();
        assert_eq!(warn.health, RuntimePerfHealth::Warn);

        collector.record_action_execution_duration(Duration::from_micros(50_000));
        let critical = collector.snapshot();
        assert_eq!(critical.health, RuntimePerfHealth::Critical);
        assert_eq!(critical.bottleneck, RuntimePerfBottleneck::ActionExecution);
    }

    #[test]
    fn reset_clears_all_series() {
        let mut collector = RuntimePerfCollector::default();
        collector.record_tick_duration(Duration::from_micros(9_000));
        collector.record_decision_duration(Duration::from_micros(7_000));
        collector.record_action_execution_duration(Duration::from_micros(11_000));
        collector.record_callback_duration(Duration::from_micros(2_000));
        collector.record_llm_api_duration(Duration::from_micros(120_000));
        assert_eq!(collector.snapshot().health, RuntimePerfHealth::Healthy);

        collector.reset();
        let snapshot = collector.snapshot();
        assert_eq!(snapshot.tick.samples_total, 0);
        assert_eq!(snapshot.decision.samples_total, 0);
        assert_eq!(snapshot.action_execution.samples_total, 0);
        assert_eq!(snapshot.callback.samples_total, 0);
        assert_eq!(snapshot.llm_api.samples_total, 0);
        assert_eq!(snapshot.health, RuntimePerfHealth::Unknown);
        assert_eq!(snapshot.bottleneck, RuntimePerfBottleneck::None);
    }

    #[test]
    fn collector_health_ignores_llm_api_stage() {
        let mut collector = RuntimePerfCollector::default();
        for _ in 0..8 {
            collector.record_tick_duration(Duration::from_micros(8_000));
            collector.record_decision_duration(Duration::from_micros(9_000));
            collector.record_action_execution_duration(Duration::from_micros(11_000));
            collector.record_callback_duration(Duration::from_micros(2_000));
        }
        let baseline = collector.snapshot();
        assert_eq!(baseline.health, RuntimePerfHealth::Healthy);

        collector.record_llm_api_duration(Duration::from_millis(2_500));
        let snapshot = collector.snapshot();
        assert_eq!(snapshot.health, RuntimePerfHealth::Healthy);
        assert_eq!(snapshot.bottleneck, RuntimePerfBottleneck::ActionExecution);
        assert!(snapshot.llm_api.p95_ms >= 2500.0);
    }

    #[test]
    fn default_window_preserves_percentiles_health_and_bottleneck() {
        let mut collector = RuntimePerfCollector::default();
        for sample_ms in 1..=DEFAULT_SAMPLE_WINDOW {
            collector.record_tick_duration(Duration::from_millis(sample_ms as u64));
        }

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.sample_window, DEFAULT_SAMPLE_WINDOW);
        assert_eq!(snapshot.tick.samples_total, DEFAULT_SAMPLE_WINDOW as u64);
        assert_eq!(snapshot.tick.samples_window, DEFAULT_SAMPLE_WINDOW);
        assert_eq!(snapshot.tick.p50_ms, 257.0);
        assert_eq!(snapshot.tick.p95_ms, 486.0);
        assert_eq!(snapshot.tick.p99_ms, 507.0);
        assert_eq!(snapshot.health, RuntimePerfHealth::Critical);
        assert_eq!(snapshot.bottleneck, RuntimePerfBottleneck::Tick);
    }

    #[test]
    fn unsupported_snapshot_keeps_default_unavailable_semantics() {
        let snapshot = unsupported_runtime_perf_snapshot();
        assert_eq!(snapshot.sample_window, 0);
        assert_eq!(snapshot.health, RuntimePerfHealth::Unknown);
        assert_eq!(snapshot.bottleneck, RuntimePerfBottleneck::None);
        assert!(!snapshot.tick.has_samples());
        assert!(!snapshot.llm_api.has_samples());
    }
}
