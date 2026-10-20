use super::{build_minimal_status_payload, build_minimal_status_payload_with_runtime_perf};
use oasis7::simulator::{
    RuntimePerfBottleneck, RuntimePerfHealth, RuntimePerfSeriesSnapshot, RuntimePerfSnapshot,
};
use std::collections::BTreeMap;

fn runtime_perf_series(p95_ms: f64, over_budget_ratio_ppm: u64) -> RuntimePerfSeriesSnapshot {
    RuntimePerfSeriesSnapshot {
        samples_total: 10,
        samples_window: 10,
        budget_ms: 20.0,
        last_ms: p95_ms,
        avg_ms: p95_ms,
        min_ms: p95_ms,
        max_ms: p95_ms,
        p50_ms: p95_ms,
        p95_ms,
        p99_ms: p95_ms,
        over_budget_total: 1,
        over_budget_ratio_ppm,
    }
}

#[test]
fn build_chain_status_payload_surfaces_runtime_perf_snapshot() {
    let runtime_perf = RuntimePerfSnapshot {
        sample_window: 512,
        tick: runtime_perf_series(31.5, 0),
        decision: runtime_perf_series(24.2, 125_000),
        action_execution: runtime_perf_series(14.8, 0),
        callback: runtime_perf_series(3.1, 0),
        llm_api: runtime_perf_series(980.0, 0),
        health: RuntimePerfHealth::Warn,
        bottleneck: RuntimePerfBottleneck::Decision,
    };

    let payload = build_minimal_status_payload_with_runtime_perf(runtime_perf);

    let runtime_perf = payload.runtime_perf.expect("runtime perf snapshot");
    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(runtime_perf.bottleneck, RuntimePerfBottleneck::Decision);
    assert_eq!(runtime_perf.decision.p95_ms, 24.2);
    assert_eq!(runtime_perf.decision.over_budget_ratio_ppm, 125_000);
}

#[test]
fn build_chain_status_payload_warns_observability_for_runtime_perf_degradation() {
    let runtime_perf = RuntimePerfSnapshot {
        sample_window: 512,
        tick: runtime_perf_series(31.5, 0),
        decision: runtime_perf_series(24.2, 125_000),
        action_execution: runtime_perf_series(14.8, 0),
        callback: runtime_perf_series(3.1, 0),
        llm_api: runtime_perf_series(980.0, 0),
        health: RuntimePerfHealth::Warn,
        bottleneck: RuntimePerfBottleneck::Decision,
    };

    let payload = build_minimal_status_payload_with_runtime_perf(runtime_perf);

    assert_eq!(payload.observability.status, "warn");
    assert!(payload.observability.ready);
    let alert = payload
        .observability
        .alerts
        .iter()
        .find(|alert| alert.code == "runtime_perf_degraded")
        .expect("runtime perf observability alert");
    assert_eq!(alert.severity, "warn");
    assert!(alert.summary.contains("health=warn"));
    assert!(alert.summary.contains("bottleneck=decision"));
}

#[test]
fn build_chain_status_payload_warns_when_only_llm_api_performance_is_over_budget() {
    let runtime_perf = RuntimePerfSnapshot {
        sample_window: 512,
        tick: runtime_perf_series(10.0, 0),
        decision: runtime_perf_series(10.0, 0),
        action_execution: runtime_perf_series(10.0, 0),
        callback: runtime_perf_series(10.0, 0),
        llm_api: runtime_perf_series(980.0, 125_000),
        health: RuntimePerfHealth::Healthy,
        bottleneck: RuntimePerfBottleneck::None,
    };

    let payload = build_minimal_status_payload_with_runtime_perf(runtime_perf);

    assert_eq!(payload.observability.status, "warn");
    let alert = payload
        .observability
        .alerts
        .iter()
        .find(|alert| alert.code == "llm_api_perf_degraded")
        .expect("llm API performance observability alert");
    assert_eq!(alert.severity, "warn");
    assert!(alert.summary.contains("llm_api_p95_ms=980.00"));
    assert!(
        alert
            .summary
            .contains("llm_api_over_budget_ratio_ppm=125000")
    );
}

#[test]
fn build_chain_status_payload_marks_runtime_perf_critical_not_ready() {
    let runtime_perf = RuntimePerfSnapshot {
        sample_window: 512,
        tick: runtime_perf_series(80.0, 250_000),
        decision: runtime_perf_series(24.2, 0),
        action_execution: runtime_perf_series(14.8, 0),
        callback: runtime_perf_series(3.1, 0),
        llm_api: runtime_perf_series(980.0, 0),
        health: RuntimePerfHealth::Critical,
        bottleneck: RuntimePerfBottleneck::Tick,
    };

    let payload = build_minimal_status_payload_with_runtime_perf(runtime_perf);

    assert_eq!(payload.observability.status, "critical");
    assert!(!payload.observability.ready);
    assert!(payload.observability.runtime_perf_available);
    assert_eq!(payload.observability.runtime_perf_health, "critical");
    assert_eq!(payload.observability.runtime_perf_bottleneck, "tick");
    assert!(payload.observability.runtime_perf_degraded);
    let alert = payload
        .observability
        .alerts
        .iter()
        .find(|alert| alert.code == "runtime_perf_degraded")
        .expect("runtime perf observability alert");
    assert_eq!(alert.severity, "critical");
    assert!(alert.summary.contains("health=critical"));
    assert!(alert.summary.contains("bottleneck=tick"));
}

#[test]
fn build_chain_status_payload_marks_runtime_perf_unavailable_without_source() {
    let payload = build_minimal_status_payload(None);

    assert!(payload.runtime_perf.is_none());
    assert!(!payload.observability.runtime_perf_available);
    assert_eq!(payload.observability.runtime_perf_health, "unavailable");
    assert_eq!(payload.observability.runtime_perf_bottleneck, "none");
    assert!(!payload.observability.runtime_perf_degraded);
    assert!(
        payload
            .observability
            .alerts
            .iter()
            .all(|alert| alert.code != "runtime_perf_degraded")
    );
}

fn strict_runtime_perf_from_timing(
    timing: &super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot,
) -> Option<RuntimePerfSnapshot> {
    super::super::status_payload::build_runtime_perf_snapshot_from_execution_bridge_timing(
        timing,
        super::super::status_payload::RuntimePerfGateTier::Strict,
    )
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_requires_commit_samples() {
    let timing = super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot::default();

    let runtime_perf = strict_runtime_perf_from_timing(&timing);

    assert!(runtime_perf.is_none());
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_warns_for_slow_commits() {
    let timing = super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot {
        window_capacity: 128,
        recent_commit_count: 4,
        recent_over_budget_count: 1,
        recent_over_budget_ratio_ppm: 250_000,
        p50_total_ms: Some(780),
        p95_total_ms: Some(1_250),
        latest_total_ms: Some(1_250),
        max_total_ms: Some(1_250),
        slow_count: 1,
        last_slow_stage: Some("runtime_step".to_string()),
        stages: BTreeMap::new(),
    };

    let runtime_perf = strict_runtime_perf_from_timing(&timing).expect("runtime perf snapshot");

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
    assert_eq!(runtime_perf.action_execution.samples_total, 4);
    assert_eq!(runtime_perf.action_execution.budget_ms, 1_000.0);
    assert_eq!(runtime_perf.action_execution.p50_ms, 780.0);
    assert_eq!(runtime_perf.action_execution.p95_ms, 1_250.0);
    assert_eq!(runtime_perf.action_execution.over_budget_total, 1);
    assert_eq!(runtime_perf.action_execution.over_budget_ratio_ppm, 250_000);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_keeps_low_sample_slow_commits_warn() {
    let timing = super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot {
        window_capacity: 128,
        recent_commit_count: 3,
        recent_over_budget_count: 1,
        recent_over_budget_ratio_ppm: 333_333,
        p50_total_ms: Some(544),
        p95_total_ms: Some(3_388),
        latest_total_ms: Some(3_388),
        max_total_ms: Some(3_388),
        slow_count: 1,
        last_slow_stage: Some("retention".to_string()),
        stages: BTreeMap::new(),
    };

    let runtime_perf = strict_runtime_perf_from_timing(&timing).expect("runtime perf snapshot");

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
    assert_eq!(runtime_perf.action_execution.samples_total, 3);
    assert_eq!(runtime_perf.action_execution.p95_ms, 3_388.0);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_keeps_cold_start_outliers_warn() {
    let timing = super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot {
        window_capacity: 128,
        recent_commit_count: 21,
        recent_over_budget_count: 2,
        recent_over_budget_ratio_ppm: 95_238,
        p50_total_ms: Some(544),
        p95_total_ms: Some(2_890),
        latest_total_ms: Some(4_161),
        max_total_ms: Some(4_161),
        slow_count: 2,
        last_slow_stage: Some("cas_put".to_string()),
        stages: BTreeMap::new(),
    };

    let runtime_perf = strict_runtime_perf_from_timing(&timing).expect("runtime perf snapshot");

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
    assert_eq!(runtime_perf.action_execution.samples_total, 21);
    assert_eq!(runtime_perf.action_execution.p95_ms, 2_890.0);
    assert_eq!(runtime_perf.action_execution.max_ms, 4_161.0);

    let payload = build_minimal_status_payload_with_runtime_perf(runtime_perf);
    assert_eq!(payload.observability.status, "warn");
    assert!(payload.observability.ready);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_keeps_31_sustained_slow_commits_warn() {
    let timing = super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot {
        window_capacity: 128,
        recent_commit_count: 31,
        recent_over_budget_count: 31,
        recent_over_budget_ratio_ppm: 1_000_000,
        p50_total_ms: Some(2_500),
        p95_total_ms: Some(2_500),
        latest_total_ms: Some(2_500),
        max_total_ms: Some(2_500),
        slow_count: 31,
        last_slow_stage: Some("runtime_step".to_string()),
        stages: BTreeMap::new(),
    };

    let runtime_perf = strict_runtime_perf_from_timing(&timing).expect("runtime perf snapshot");

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    let payload = build_minimal_status_payload_with_runtime_perf(runtime_perf);
    assert_eq!(payload.observability.status, "warn");
    assert!(payload.observability.ready);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_keeps_two_mature_outliers_warn() {
    let timing = super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot {
        window_capacity: 128,
        recent_commit_count: 32,
        recent_over_budget_count: 2,
        recent_over_budget_ratio_ppm: 62_500,
        p50_total_ms: Some(544),
        p95_total_ms: Some(2_890),
        latest_total_ms: Some(4_161),
        max_total_ms: Some(4_161),
        slow_count: 2,
        last_slow_stage: Some("cas_put".to_string()),
        stages: BTreeMap::new(),
    };

    let runtime_perf = strict_runtime_perf_from_timing(&timing).expect("runtime perf snapshot");

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    let payload = build_minimal_status_payload_with_runtime_perf(runtime_perf);
    assert_eq!(payload.observability.status, "warn");
    assert!(payload.observability.ready);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_keeps_moderate_sustained_latency_warn() {
    let timing = super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot {
        window_capacity: 128,
        recent_commit_count: 32,
        recent_over_budget_count: 8,
        recent_over_budget_ratio_ppm: 250_000,
        p50_total_ms: Some(780),
        p95_total_ms: Some(1_250),
        latest_total_ms: Some(1_250),
        max_total_ms: Some(1_250),
        slow_count: 8,
        last_slow_stage: Some("runtime_step".to_string()),
        stages: BTreeMap::new(),
    };

    let runtime_perf = strict_runtime_perf_from_timing(&timing).expect("runtime perf snapshot");

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    let payload = build_minimal_status_payload_with_runtime_perf(runtime_perf);
    assert_eq!(payload.observability.status, "warn");
    assert!(payload.observability.ready);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_marks_sustained_slow_commits_critical() {
    let timing = super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot {
        window_capacity: 128,
        recent_commit_count: 32,
        recent_over_budget_count: 32,
        recent_over_budget_ratio_ppm: 1_000_000,
        p50_total_ms: Some(1_500),
        p95_total_ms: Some(2_500),
        latest_total_ms: Some(2_500),
        max_total_ms: Some(2_500),
        slow_count: 32,
        last_slow_stage: Some("runtime_step".to_string()),
        stages: BTreeMap::new(),
    };

    let runtime_perf = strict_runtime_perf_from_timing(&timing).expect("runtime perf snapshot");

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Critical);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
    assert_eq!(runtime_perf.action_execution.p95_ms, 2_500.0);
    let payload = build_minimal_status_payload_with_runtime_perf(runtime_perf);
    assert_eq!(payload.observability.status, "critical");
    assert!(!payload.observability.ready);
}
