use super::agent_service_io::{
    AgentServiceIoJob, AgentServiceIoResponse, AgentServiceIoResult, AgentServiceProgress,
};
use super::*;
use oasis7_client_api::world_service::{IntentOutcome, SubmitObservation};
use std::sync::{Weak, mpsc};

const PUMP_INTERVAL: Duration = Duration::from_millis(50);
const MAX_LOCAL_ADVANCES: usize = 8;

pub(super) fn wait_for_result(
    results: &mpsc::Receiver<AgentServiceIoResult>,
    interval: Duration,
) -> Option<Result<AgentServiceIoResult, mpsc::TryRecvError>> {
    match results.recv_timeout(interval) {
        Ok(result) => Some(Ok(result)),
        Err(mpsc::RecvTimeoutError::Timeout) => None,
        Err(mpsc::RecvTimeoutError::Disconnected) => Some(Err(mpsc::TryRecvError::Disconnected)),
    }
}

fn continue_local_advance(advanced: bool, had_phase: bool, has_phase: bool, count: usize) -> bool {
    advanced && had_phase && has_phase && count < MAX_LOCAL_ADVANCES
}

fn phase_identity_requires_retry(
    identity: Option<Result<String, String>>,
    original_phase: &str,
) -> bool {
    match identity {
        Some(Ok(identity)) => identity == original_phase,
        Some(Err(_)) => true,
        None => false,
    }
}

fn unpublished_response(result: &AgentServiceIoResult) -> bool {
    let outcome = match &result.response {
        Err(_) | Ok(AgentServiceIoResponse::Submit(SubmitObservation::OutcomeUnknown(_))) => {
            return true;
        }
        Ok(AgentServiceIoResponse::Intent(response)) => &response.outcome,
        Ok(AgentServiceIoResponse::Submit(SubmitObservation::Response(response))) => {
            &response.outcome
        }
        _ => return false,
    };
    matches!(
        outcome,
        IntentOutcome::Pending | IntentOutcome::Received { .. } | IntentOutcome::Unknown
    )
}

/// Connection presence is independent of playback: Pause is shared server policy.
pub(super) struct AgentServicePresence {
    server: Weak<Mutex<ViewerRuntimeLiveServer>>,
    registered: bool,
}

impl AgentServicePresence {
    pub(super) fn new(server: &Arc<Mutex<ViewerRuntimeLiveServer>>) -> Self {
        Self {
            server: Arc::downgrade(server),
            registered: false,
        }
    }

    pub(super) fn observe(
        &mut self,
        server: &mut ViewerRuntimeLiveServer,
        session: &RuntimeLiveSession,
    ) {
        if !self.registered && session.initial_snapshot_sent {
            server.agent_service_session_count =
                server.agent_service_session_count.saturating_add(1);
            self.registered = true;
        }
    }
}

impl Drop for AgentServicePresence {
    fn drop(&mut self) {
        if self.registered
            && let Some(shared) = self.server.upgrade()
            && let Ok(mut server) = shared.lock()
        {
            server.agent_service_session_count =
                server.agent_service_session_count.saturating_sub(1);
        }
    }
}

impl ViewerRuntimeLiveServer {
    fn agent_service_eligible(&self) -> bool {
        self.agent_service_session_count > 0
            && !self.auto_play_paused
            && self.authoritative_recovery_write_fence.is_none()
            && matches!(self.config.decision_mode, ViewerLiveDecisionMode::Llm)
    }

    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_agent_service_pump_status(&self) -> serde_json::Value {
        serde_json::json!({
            "session_count": self.agent_service_session_count,
            "play_enabled": !self.auto_play_paused,
            "llm_mode": matches!(self.config.decision_mode, ViewerLiveDecisionMode::Llm),
            "fenced": self.authoritative_recovery_write_fence.is_some(),
            "pump_started": self.agent_service_pump_started,
            "eligible": self.agent_service_eligible(),
            "known_service_commit": self.verified_world_view.as_ref().map(|view| &view.version().commit),
        })
    }

    fn record_agent_pump_error(&mut self, reason: &'static str) {
        if self.agent_service_pump_error == Some(reason) {
            return;
        }
        self.agent_service_pump_error = Some(reason);
        self.set_latest_player_gameplay_feedback(Self::make_player_gameplay_feedback(
            "agent_service", "blocked", "canonical Agent work remains pending",
            None, None, Some(reason.into()),
            Some("restore provider/service readiness; original requests remain queued for reconciliation".into()),
            0, 0,
        ));
    }

    fn observe_agent_pump_progress(&mut self, progress: &AgentServiceProgress) {
        if matches!(progress, AgentServiceProgress::Advanced) {
            self.agent_service_pump_error = None;
            if self
                .latest_player_gameplay_feedback
                .as_ref()
                .is_some_and(|feedback| feedback.action == "agent_service")
            {
                self.latest_player_gameplay_feedback = None;
            }
        }
    }

    fn agent_pump_worker_failed(&mut self) {
        self.agent_service_io_worker_failed();
        self.agent_service_pump_started = false;
        self.record_agent_pump_error(
            "Agent service executor stopped; pending work requires reconciliation",
        );
    }

    pub(super) fn start_agent_service_pump(
        shared: &Arc<Mutex<Self>>,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        #[cfg(any(test, feature = "test_tier_required"))]
        let diagnostics;
        {
            let mut server = lock_shared_server(shared)?;
            if server.agent_service_pump_started || server.config.world_service.is_none() {
                return Ok(());
            }
            server.agent_service_pump_started = true;
            #[cfg(any(test, feature = "test_tier_required"))]
            {
                diagnostics = server.agent_service_io_diagnostics.clone();
            }
        }
        let (jobs, incoming) = mpsc::sync_channel::<AgentServiceIoJob>(1);
        let (completed, results) = mpsc::sync_channel::<AgentServiceIoResult>(1);
        #[cfg(any(test, feature = "test_tier_required"))]
        let executor_diagnostics = diagnostics.clone();
        // The executor owns only immutable jobs. It never accesses the Viewer mutex.
        thread::spawn(move || {
            while let Ok(job) = incoming.recv() {
                #[cfg(any(test, feature = "test_tier_required"))]
                let result = executor_diagnostics.execute(job);
                #[cfg(not(any(test, feature = "test_tier_required")))]
                let result = job.execute();
                if completed.send(result).is_err() {
                    break;
                }
            }
        });
        let weak = Arc::downgrade(shared);
        thread::spawn(move || {
            let mut in_flight = false;
            let mut local_advances = 0;
            let mut returned_result = None;
            let mut last_dispatch = None;
            loop {
                let Some(shared) = weak.upgrade() else {
                    break;
                };
                let Ok(mut server) = shared.lock() else {
                    break;
                };
                let had_phase = server.llm_sidecar.hosted_service_phase.is_some();
                let mut progress = None;
                let mut retry_without_progress = false;
                if in_flight {
                    match returned_result.take().unwrap_or_else(|| results.try_recv()) {
                        Ok(result) => {
                            in_flight = false;
                            let original_phase = result.token.phase_digest.clone();
                            let unpublished_response = unpublished_response(&result);
                            #[cfg(any(test, feature = "test_tier_required"))]
                            let applied = diagnostics.apply(&mut server, result);
                            #[cfg(not(any(test, feature = "test_tier_required")))]
                            let applied = server.apply_agent_service_io(result);
                            let phase_identity = if applied.is_err() {
                                None
                            } else {
                                server
                                    .llm_sidecar
                                    .hosted_service_phase
                                    .as_ref()
                                    .map(|phase| phase.identity())
                            };
                            let phase_identity_unavailable =
                                phase_identity.as_ref().is_some_and(Result::is_err);
                            retry_without_progress = unpublished_response
                                || applied.is_err()
                                || phase_identity_requires_retry(phase_identity, &original_phase);
                            match applied {
                                Ok(value) => {
                                    server.observe_agent_pump_progress(&value);
                                    progress = Some(value);
                                }
                                Err(_error) => {
                                    #[cfg(any(test, feature = "test_tier_required"))]
                                    if std::env::var("PRE2_RESUME_REJECTION_RECOVERY_TRACE")
                                        .as_deref()
                                        == Ok("1")
                                    {
                                        eprintln!("pre2_private_recovery_apply_error={_error}");
                                    }
                                    server.record_agent_pump_error("Agent service result could not be applied; original work remains pending");
                                }
                            }
                            if phase_identity_unavailable {
                                server.record_agent_pump_error(
                                    "Agent service phase identity is unavailable; original work remains pending",
                                );
                            }
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            server.agent_pump_worker_failed();
                            break;
                        }
                        Err(mpsc::TryRecvError::Empty) => {}
                    }
                }
                if !in_flight && !retry_without_progress {
                    let eligible = server.agent_service_eligible();
                    if progress.is_none()
                        || matches!(
                            progress,
                            Some(AgentServiceProgress::Idle | AgentServiceProgress::Advanced)
                        )
                    {
                        match server.prepare_agent_service_io(eligible) {
                            Ok(value) => {
                                server.observe_agent_pump_progress(&value);
                                progress = Some(value);
                            }
                            Err(_error) => {
                                #[cfg(any(test, feature = "test_tier_required"))]
                                if std::env::var("PRE2_RESUME_REJECTION_RECOVERY_TRACE").as_deref()
                                    == Ok("1")
                                {
                                    eprintln!("pre2_private_recovery_prepare_error={_error}");
                                }
                                progress = None;
                                server.record_agent_pump_error("Agent service preparation is blocked; original work remains pending");
                            }
                        }
                    }
                    if matches!(progress, Some(AgentServiceProgress::NeedsIo(_))) {
                        let Some(AgentServiceProgress::NeedsIo(job)) = progress.take() else {
                            unreachable!()
                        };
                        // One owned in-flight job guarantees capacity; never wait under the mutex.
                        #[cfg(any(test, feature = "test_tier_required"))]
                        diagnostics.queued(&job);
                        match jobs.try_send(*job) {
                            Ok(()) => {
                                in_flight = true;
                                last_dispatch = Some(std::time::Instant::now());
                            }
                            Err(_) => {
                                server.agent_pump_worker_failed();
                                break;
                            }
                        }
                    }
                }
                let advance_without_sleep = continue_local_advance(
                    !retry_without_progress
                        && matches!(progress, Some(AgentServiceProgress::Advanced)),
                    had_phase,
                    server.llm_sidecar.hosted_service_phase.is_some(),
                    local_advances + 1,
                );
                // Release between every local transition so Viewer controls and
                // observers retain access. Fresh admission and terminal phase
                // removal stop the burst; it never starts another model turn.
                drop(server);
                drop(shared);
                if advance_without_sleep {
                    local_advances += 1;
                    continue;
                }
                local_advances = 0;
                if retry_without_progress {
                    // Preserve the dispatch cadence without adding another full
                    // interval after a slow transport or delayed result collection.
                    thread::sleep(last_dispatch.map_or(PUMP_INTERVAL, |started| {
                        PUMP_INTERVAL.saturating_sub(started.elapsed())
                    }));
                } else if in_flight {
                    // Wake on completion without holding the Viewer mutex. Timeout
                    // preserves periodic policy checks and weak-reference shutdown.
                    returned_result = wait_for_result(&results, PUMP_INTERVAL);
                } else {
                    thread::sleep(PUMP_INTERVAL);
                }
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn result_wait_preserves_periodic_timeout_and_disconnect() {
        let (sender, receiver) = mpsc::channel();
        assert!(wait_for_result(&receiver, Duration::from_millis(1)).is_none());
        drop(sender);
        assert!(matches!(
            wait_for_result(&receiver, Duration::from_secs(30)),
            Some(Err(mpsc::TryRecvError::Disconnected))
        ));
    }
    #[test]
    fn local_advance_burst_requires_existing_phase_and_is_bounded() {
        assert!(continue_local_advance(true, true, true, 0));
        assert!(continue_local_advance(
            true,
            true,
            true,
            MAX_LOCAL_ADVANCES - 1
        ));
        assert!(!continue_local_advance(
            true,
            true,
            true,
            MAX_LOCAL_ADVANCES
        ));
        assert!(!continue_local_advance(false, true, true, 0)); // Idle or NeedsIo
        assert!(!continue_local_advance(true, false, true, 0)); // Fresh turn
        assert!(!continue_local_advance(true, true, false, 0)); // Terminal
    }

    #[test]
    fn phase_identity_encoding_failure_fails_closed_for_retry() {
        assert!(phase_identity_requires_retry(
            Some(Err("encoding failed".into())),
            "expected-phase"
        ));
        assert!(phase_identity_requires_retry(
            Some(Ok("expected-phase".into())),
            "expected-phase"
        ));
        assert!(!phase_identity_requires_retry(
            Some(Ok("next-phase".into())),
            "expected-phase"
        ));
        assert!(!phase_identity_requires_retry(None, "expected-phase"));
    }
}
