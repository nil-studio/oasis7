//! Observe the existing pre-arm wait without adding RPCs, retries or deadlines.
use super::{
    RemoteWorldServiceClient, SubmitIntentRequest, WorldServicePayloadV1, cli_support,
    original_commit_from_lookup,
};
use oasis7::world_service::client::{WorldServiceClientConfig, WorldServiceClientError};
use oasis7_client_api::world_service::CommitRef;
use std::{
    thread,
    time::{Duration, Instant},
};

pub(super) struct AdmissionObservation {
    pub(super) scenario_started_at: Instant,
    pub(super) admission_started_at: Instant,
    // Observed only after the original volatile-admission assertion succeeds.
    pub(super) admission_observed_at: Instant,
    // This is the earlier verified read, not a read taken at admission time.
    pub(super) initial_read_observed_at: Instant,
    pub(super) initial_read_position: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
enum LookupFailure {
    ExactCutAbsent,
    HistoryUnavailable,
    Other503,
    OtherRejected,
    Transport,
    Assurance,
    Configuration,
    Cooldown,
    UnprovedResponse,
}

impl LookupFailure {
    fn classify(error: &WorldServiceClientError) -> Self {
        match error {
            WorldServiceClientError::Rejected {
                status: 503,
                message,
            } if message == "HistoryUnavailable: no result at the exact qualified lookup cut" => {
                Self::ExactCutAbsent
            }
            WorldServiceClientError::Rejected {
                status: 503,
                message,
            } if message.starts_with("HistoryUnavailable:") => {
                Self::HistoryUnavailable
            }
            WorldServiceClientError::Rejected { status: 503, .. } => Self::Other503,
            WorldServiceClientError::Rejected { .. } => Self::OtherRejected,
            WorldServiceClientError::Transport(_) => Self::Transport,
            WorldServiceClientError::Assurance(_) => Self::Assurance,
            WorldServiceClientError::Configuration(_) => Self::Configuration,
            WorldServiceClientError::Cooldown { .. } => Self::Cooldown,
        }
    }
}

#[derive(Default)]
struct LookupObservations {
    attempts: u32,
    failures: [u32; 9],
    first_failure: Option<LookupFailure>,
    last_failure: Option<LookupFailure>,
    lookup_total: Duration,
    lookup_max: Duration,
}

impl LookupObservations {
    fn record(&mut self, elapsed: Duration, failure: Option<LookupFailure>) {
        self.attempts = self.attempts.saturating_add(1);
        self.lookup_total = self.lookup_total.saturating_add(elapsed);
        self.lookup_max = self.lookup_max.max(elapsed);
        if let Some(failure) = failure {
            let count = &mut self.failures[failure as usize];
            *count = count.saturating_add(1);
            self.first_failure.get_or_insert(failure);
            self.last_failure = Some(failure);
        }
    }

    fn summary(
        &self,
        admission: &AdmissionObservation,
        wait_started_at: Instant,
        wait_finished_at: Instant,
        committed: Option<u64>,
    ) -> String {
        let relative_ms = |at: Instant| {
            at.saturating_duration_since(admission.scenario_started_at)
                .as_millis()
        };
        // Fixed fields only. In particular, no error message, request, identity,
        // signature or correlation is retained by the observation accumulator.
        // A missing result is not evidence that the request missed a freeze.
        format!(
            "original_commit_wait admission_started_ms={} admission_observed_ms={} wait_started_ms={} wait_finished_ms={} wait_elapsed_ms={} attempts={} lookup_total_ms={} lookup_max_ms={} initial_read_ms={} initial_read_position={} original_commit_position={committed:?} wait_tick_samples=unobserved freeze_contains_original=unobserved first_failure={:?} last_failure={:?} exact_cut_absent={} history_unavailable={} other_503={} other_rejected={} transport={} assurance={} configuration={} cooldown={} unproved_response={}",
            relative_ms(admission.admission_started_at),
            relative_ms(admission.admission_observed_at),
            relative_ms(wait_started_at),
            relative_ms(wait_finished_at),
            wait_finished_at
                .saturating_duration_since(wait_started_at)
                .as_millis(),
            self.attempts,
            self.lookup_total.as_millis(),
            self.lookup_max.as_millis(),
            relative_ms(admission.initial_read_observed_at),
            admission.initial_read_position,
            self.first_failure,
            self.last_failure,
            self.failures[LookupFailure::ExactCutAbsent as usize],
            self.failures[LookupFailure::HistoryUnavailable as usize],
            self.failures[LookupFailure::Other503 as usize],
            self.failures[LookupFailure::OtherRejected as usize],
            self.failures[LookupFailure::Transport as usize],
            self.failures[LookupFailure::Assurance as usize],
            self.failures[LookupFailure::Configuration as usize],
            self.failures[LookupFailure::Cooldown as usize],
            self.failures[LookupFailure::UnprovedResponse as usize],
        )
    }
}

pub(super) fn wait(
    owner_client: &RemoteWorldServiceClient,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    owner_config: &WorldServiceClientConfig,
    admission: AdmissionObservation,
) -> Result<(CommitRef, String), String> {
    let wait_started_at = Instant::now();
    let original_deadline = wait_started_at + Duration::from_secs(20);
    let mut observations = LookupObservations::default();
    loop {
        let lookup_started_at = Instant::now();
        let response = owner_client.lookup_formal_original(request.clone());
        let failure = response.as_ref().err().map(LookupFailure::classify);
        // Use the unchanged shared correlation/profile/committed-outcome checks.
        let result = original_commit_from_lookup(response, request);
        let lookup_finished_at = Instant::now();
        observations.record(
            lookup_finished_at.saturating_duration_since(lookup_started_at),
            result
                .is_err()
                .then_some(failure.unwrap_or(LookupFailure::UnprovedResponse)),
        );
        match result {
            Ok(commit) => {
                let summary = observations.summary(
                    &admission,
                    wait_started_at,
                    lookup_finished_at,
                    Some(commit.position),
                );
                return Ok((commit, summary));
            }
            Err(error) if Instant::now() >= original_deadline => {
                // Capture the wait's end before the existing failure-only status
                // request. No new status I/O consumes the original wait budget.
                let summary =
                    observations.summary(&admission, wait_started_at, Instant::now(), None);
                let status = cli_support::original_commit_failure_status(owner_config);
                let status_observation_finished_ms =
                    admission.scenario_started_at.elapsed().as_millis();
                return Err(format!(
                    "original request was not committed before arm: {error}; {status}; {summary} status_observation_finished_ms={status_observation_finished_ms}"
                ));
            }
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}

#[path = "formal_live_healthy_original_commit_wait_tests.rs"]
mod tests;
