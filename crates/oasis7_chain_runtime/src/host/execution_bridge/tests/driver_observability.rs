use std::time::Duration;

use super::super::driver_observability::{
    CommitObservation, SimulatorMirrorCommitObservation, snapshot_commit_observations_for_tests,
};

#[test]
fn execution_bridge_commit_timing_snapshot_records_recent_and_stage_totals() {
    let observations = [
        CommitObservation {
            world_id: "world",
            height: 1,
            action_count: 2,
            runtime_action_count: 1,
            simulator: SimulatorMirrorCommitObservation::default(),
            snapshot_bytes: 10,
            journal_bytes: 20,
            decode_ms: Duration::from_millis(10),
            runtime_step_ms: Duration::from_millis(550),
            simulator_step_ms: Duration::from_millis(5),
            serialize_ms: Duration::from_millis(6),
            cas_put_ms: Duration::from_millis(7),
            simulator_persist_ms: Duration::from_millis(8),
            world_head_proof_ms: Duration::from_millis(9),
            record_persist_ms: Duration::from_millis(11),
            persist_world_ms: Duration::from_millis(12),
            checkpoint_ms: Duration::from_millis(13),
            retention_ms: Duration::from_millis(14),
            total_ms: Duration::from_millis(1_200),
        },
        CommitObservation {
            world_id: "world",
            height: 2,
            action_count: 1,
            runtime_action_count: 1,
            simulator: SimulatorMirrorCommitObservation::default(),
            snapshot_bytes: 10,
            journal_bytes: 20,
            decode_ms: Duration::from_millis(10),
            runtime_step_ms: Duration::from_millis(25),
            simulator_step_ms: Duration::from_millis(5),
            serialize_ms: Duration::from_millis(6),
            cas_put_ms: Duration::from_millis(7),
            simulator_persist_ms: Duration::from_millis(8),
            world_head_proof_ms: Duration::from_millis(9),
            record_persist_ms: Duration::from_millis(11),
            persist_world_ms: Duration::from_millis(12),
            checkpoint_ms: Duration::from_millis(13),
            retention_ms: Duration::from_millis(14),
            total_ms: Duration::from_millis(120),
        },
    ];

    let snapshot = snapshot_commit_observations_for_tests(&observations);
    assert_eq!(snapshot.recent_commit_count, 2);
    assert_eq!(snapshot.p50_total_ms, Some(120));
    assert_eq!(snapshot.p95_total_ms, Some(1_200));
    assert_eq!(snapshot.latest_total_ms, Some(120));
    assert_eq!(snapshot.max_total_ms, Some(1_200));
    assert_eq!(snapshot.recent_over_budget_count, 1);
    assert_eq!(snapshot.recent_over_budget_ratio_ppm, 500_000);
    assert_eq!(snapshot.slow_count, 1);
    assert_eq!(snapshot.last_slow_stage.as_deref(), Some("runtime_step"));
    assert_eq!(snapshot.stages["runtime_step"].count, 2);
    assert_eq!(snapshot.stages["runtime_step"].cumulative_ms, 575);
}
