//! Pure observation contracts; no process, network, sleeps or deadline changes.
use super::*;

#[test]
fn lookup_failures_use_exact_typed_categories_without_retaining_messages() {
    let cases = [
        (
            WorldServiceClientError::Rejected {
                status: 503,
                message: "HistoryUnavailable: no result at the exact qualified lookup cut".into(),
            },
            LookupFailure::ExactCutAbsent,
        ),
        (
            WorldServiceClientError::Rejected {
                status: 503,
                message: "HistoryUnavailable: private historical context".into(),
            },
            LookupFailure::HistoryUnavailable,
        ),
        (
            WorldServiceClientError::Rejected {
                status: 503,
                message: "private error\nexact_cut_absent=999".into(),
            },
            LookupFailure::Other503,
        ),
        (
            WorldServiceClientError::Rejected {
                status: 400,
                message: "HistoryUnavailable: no result at the exact qualified lookup cut".into(),
            },
            LookupFailure::OtherRejected,
        ),
        (
            WorldServiceClientError::Transport("private transport context".into()),
            LookupFailure::Transport,
        ),
        (
            WorldServiceClientError::Assurance("private proof context".into()),
            LookupFailure::Assurance,
        ),
        (
            WorldServiceClientError::Configuration("private config context".into()),
            LookupFailure::Configuration,
        ),
        (
            WorldServiceClientError::Cooldown {
                status: 503,
                retry_after: Duration::from_secs(999),
            },
            LookupFailure::Cooldown,
        ),
    ];
    let mut observations = LookupObservations::default();
    for (error, expected) in cases {
        let actual = LookupFailure::classify(&error);
        assert_eq!(actual, expected);
        observations.record(Duration::ZERO, Some(actual));
    }
    assert_eq!(observations.failures, [1, 1, 1, 1, 1, 1, 1, 1, 0]);
}

#[test]
fn summary_preserves_monotonic_admission_wait_and_lookup_observations() {
    let origin = Instant::now();
    let admission = AdmissionObservation {
        scenario_started_at: origin,
        admission_started_at: origin + Duration::from_millis(100),
        admission_observed_at: origin + Duration::from_millis(125),
        initial_read_observed_at: origin + Duration::from_millis(50),
        initial_read_position: 7,
    };
    let mut observations = LookupObservations::default();
    observations.record(
        Duration::from_millis(6),
        Some(LookupFailure::ExactCutAbsent),
    );
    observations.record(
        Duration::from_millis(9),
        Some(LookupFailure::UnprovedResponse),
    );
    observations.record(Duration::from_millis(3), None);
    // Preserve sub-millisecond durations until the final rendering.
    observations.record(Duration::from_micros(600), None);
    observations.record(Duration::from_micros(600), None);
    let summary = observations.summary(
        &admission,
        origin + Duration::from_millis(130),
        origin + Duration::from_millis(400),
        Some(9),
    );
    for expected in [
        "admission_started_ms=100 admission_observed_ms=125",
        "wait_started_ms=130 wait_finished_ms=400 wait_elapsed_ms=270",
        "attempts=5 lookup_total_ms=19 lookup_max_ms=9",
        "initial_read_ms=50 initial_read_position=7 original_commit_position=Some(9)",
        "wait_tick_samples=unobserved freeze_contains_original=unobserved",
        "first_failure=Some(ExactCutAbsent) last_failure=Some(UnprovedResponse)",
        "exact_cut_absent=1 history_unavailable=0",
        "cooldown=0 unproved_response=1",
    ] {
        assert!(
            summary.contains(expected),
            "missing fixed observation: {expected}"
        );
    }
    assert_eq!(observations.failures.iter().sum::<u32>(), 2);
    assert!(!summary.contains('\n'));
}

#[test]
fn summary_is_bounded_and_does_not_claim_progress_from_missing_evidence() {
    let origin = Instant::now();
    let admission = AdmissionObservation {
        scenario_started_at: origin,
        admission_started_at: origin,
        admission_observed_at: origin,
        initial_read_observed_at: origin,
        initial_read_position: u64::MAX,
    };
    let mut observations = LookupObservations {
        attempts: u32::MAX,
        failures: [u32::MAX; 9],
        lookup_total: Duration::MAX,
        lookup_max: Duration::MAX,
        ..LookupObservations::default()
    };
    let error = WorldServiceClientError::Transport("signature=secret\ncorrelation=secret".into());
    observations.record(Duration::from_secs(1), Some(LookupFailure::classify(&error)));
    assert_eq!(observations.attempts, u32::MAX);
    assert_eq!(observations.failures, [u32::MAX; 9]);
    assert_eq!(observations.lookup_total, Duration::MAX);
    let summary = observations.summary(&admission, origin, origin, None);
    assert!(summary.len() < 1024);
    assert!(summary.contains("original_commit_position=None"));
    assert!(summary.contains("wait_tick_samples=unobserved freeze_contains_original=unobserved"));
    assert!(!summary.contains("signature"));
    assert!(!summary.contains("correlation"));
    assert!(!summary.contains("secret"));
    assert!(!summary.contains('\n'));
}
