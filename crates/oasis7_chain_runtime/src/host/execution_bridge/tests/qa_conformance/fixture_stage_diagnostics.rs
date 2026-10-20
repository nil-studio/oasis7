//! Failure-only fixture timings. Fixed stage labels and numbers never retain request data.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(super) enum Stage {
    Dispatcher,
    ResponseCaptureRelay,
    ControlledDriverLockWait,
    ControlledCommit,
    ReplayVerification,
}

pub(super) enum AdmissionObservation {
    Fresh,
    ExactReplay,
    RejectedExisting,
}

#[derive(Default)]
struct Measurements {
    count: AtomicU64,
    total_us: AtomicU64,
    max_us: AtomicU64,
}

#[derive(Default)]
pub(super) struct Diagnostics {
    stages: [Measurements; 5],
    fresh_commits: AtomicU64,
    exact_replay_commits: AtomicU64,
    invalid_existing_commits: AtomicU64,
}

impl Diagnostics {
    pub(super) fn observe_admission(&self, outcome: AdmissionObservation) {
        let counter = match outcome {
            AdmissionObservation::Fresh => &self.fresh_commits,
            AdmissionObservation::ExactReplay => &self.exact_replay_commits,
            AdmissionObservation::RejectedExisting => &self.invalid_existing_commits,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn measure<T>(&self, stage: Stage, operation: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let result = operation();
        self.record(stage, start.elapsed());
        result
    }

    pub(super) fn record(&self, stage: Stage, duration: Duration) {
        let value = duration.as_micros().min(u64::MAX as u128) as u64;
        let measurements = &self.stages[stage as usize];
        measurements.count.fetch_add(1, Ordering::Relaxed);
        measurements.total_us.fetch_add(value, Ordering::Relaxed);
        measurements.max_us.fetch_max(value, Ordering::Relaxed);
    }

    /// Concurrent fields are observational, not a transactional snapshot.
    pub(super) fn snapshot(&self) -> serde_json::Value {
        let mut stages = serde_json::Map::new();
        for (index, label) in [
            "dispatcher_including_pin",
            "response_capture_relay",
            "controlled_driver_lock_wait",
            "controlled_commit",
            "replay_verification",
        ]
        .into_iter()
        .enumerate()
        {
            let stage = &self.stages[index];
            stages.insert(
                label.into(),
                serde_json::json!({
                    "count": stage.count.load(Ordering::Relaxed),
                    "total_us": stage.total_us.load(Ordering::Relaxed),
                    "max_us": stage.max_us.load(Ordering::Relaxed),
                }),
            );
        }
        stages.insert(
            "controlled_admission".into(),
            serde_json::json!({
                "fresh_count": self.fresh_commits.load(Ordering::Relaxed),
                "exact_replay_count": self.exact_replay_commits.load(Ordering::Relaxed),
                "rejected_existing_count": self.invalid_existing_commits.load(Ordering::Relaxed),
            }),
        );
        serde_json::Value::Object(stages)
    }
}

#[test]
fn fixture_stage_diagnostics_are_event_updated_numbers_only() {
    let diagnostics = Diagnostics::default();
    diagnostics.record(Stage::ControlledCommit, Duration::from_micros(7));
    diagnostics.record(Stage::ControlledCommit, Duration::from_micros(11));
    let snapshot = diagnostics.snapshot();
    assert_eq!(snapshot, diagnostics.snapshot());
    assert_eq!(snapshot["controlled_commit"]["count"], 2);
    assert_eq!(snapshot["controlled_commit"]["total_us"], 18);
    assert_eq!(snapshot["controlled_commit"]["max_us"], 11);
    assert_eq!(snapshot["dispatcher_including_pin"]["count"], 0);
    for stage in snapshot.as_object().unwrap().values() {
        assert_eq!(stage.as_object().unwrap().len(), 3);
        assert!(
            stage
                .as_object()
                .unwrap()
                .values()
                .all(|value| value.is_u64())
        );
    }
}
