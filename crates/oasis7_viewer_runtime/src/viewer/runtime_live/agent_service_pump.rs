use super::agent_service_io::{
    AgentServiceIoJob, AgentServiceIoResponse, AgentServiceIoResult, AgentServiceProgress,
};
use super::control_plane::llm_sidecar::lineage_persistence::checkpoint_write::{
    DeferredPublications, PublicationSourceScope,
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

// Completion permits a typed continuation only; it never replaces server state.
#[derive(Clone)]
struct PublicationFence {
    config: Option<String>,
    generation: u64,
    publication_ticket: u64,
    inflight: Option<super::agent_service_io::AgentServiceIoToken>,
    sessions: BTreeMap<String, String>,
    reorg_epoch: u64,
    source_scope: Option<PublicationSourceScope>,
}
impl PublicationFence {
    fn config_identity(server: &ViewerRuntimeLiveServer) -> Option<String> {
        let config = server.config.world_service.as_ref()?;
        let signer = server
            .config
            .world_service_agent_signer
            .as_ref()
            .map(|signer| (&signer.private_key_hex, signer.delegation_generation));
        // Read-only Hosted writers need ordering even without an Agent signer.
        // Credential material stays private; only a digest participates in the fence.
        let material = format!(
            "{}|{}|{:?}|{}|{}|{:?}|{}|{:?}|{:?}|{:?}",
            config.endpoint,
            config.trusted_service_public_key,
            config.expected_world,
            config.scope_id,
            config.read_private_key_hex,
            config.timeout,
            config.max_response_bytes,
            config.formal_read_trust,
            signer,
            server.llm_sidecar.publication_store_path()
        );
        Some(blake3::hash(material.as_bytes()).to_hex().to_string())
    }
    fn capture(server: &ViewerRuntimeLiveServer) -> Self {
        Self {
            config: Self::config_identity(server),
            generation: server.llm_sidecar.hosted_service_generation,
            publication_ticket: server.llm_sidecar.publication_ticket(),
            inflight: server.llm_sidecar.hosted_service_inflight.clone(),
            sessions: server.llm_sidecar.publication_sessions(),
            reorg_epoch: server.reorg_epoch,
            source_scope: server.llm_sidecar.publication_source_scope(),
        }
    }
    fn matches(&self, server: &ViewerRuntimeLiveServer) -> bool {
        self.config.is_some()
            && self.config == Self::config_identity(server)
            && self.generation == server.llm_sidecar.hosted_service_generation
            && self.inflight == server.llm_sidecar.hosted_service_inflight
            && self.sessions == server.llm_sidecar.publication_sessions()
            && self.reorg_epoch == server.reorg_epoch
            && self.source_scope == server.llm_sidecar.publication_source_scope()
            && server.authoritative_recovery_write_fence.is_none()
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
            "publication_pending": self.llm_sidecar.publication_ticket() != 0,
            "publication_error": self.agent_service_pump_error,
            "play_enabled": !self.auto_play_paused,
            "llm_mode": matches!(self.config.decision_mode, ViewerLiveDecisionMode::Llm),
            "fenced": self.authoritative_recovery_write_fence.is_some(),
            "pump_started": self.agent_service_pump_started,
            "eligible": self.agent_service_eligible(),
            "known_service_commit": self.verified_world_view.as_ref().map(|view| &view.version().commit),
        })
    }

    #[cfg(any(test, feature = "test_tier_required"))]
    pub fn test_change_agent_service_scope(&mut self, scope: String) {
        self.config
            .world_service
            .as_mut()
            .expect("configured real service")
            .scope_id = scope;
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
            server.llm_sidecar.enable_deferred_publications();
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
                // Internal publication uses this same executor and capacity-1
                // lane. Existing diagnostic counters describe remote service
                // transport; file phases remain in the bounded numeric ring.
                let publication = matches!(
                    &job.operation,
                    super::agent_service_io::AgentServiceIoOperation::PublishLineage(_)
                );
                #[cfg(any(test, feature = "test_tier_required"))]
                let result = if publication {
                    job.execute()
                } else {
                    executor_diagnostics.execute(job)
                };
                #[cfg(not(any(test, feature = "test_tier_required")))]
                let result = {
                    let _ = publication;
                    job.execute()
                };
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
            let mut pending_dispatch: Option<(Box<AgentServiceIoJob>, PublicationFence)> = None;
            loop {
                let Some(shared) = weak.upgrade() else {
                    break;
                };
                let pump_phase = stream_budget_diagnostics::phase(20);
                let wait_phase = stream_budget_diagnostics::phase(21);
                let Ok(mut server) = shared.lock() else {
                    break;
                };
                drop(wait_phase);
                let deferred = match DeferredPublications::begin(&server.llm_sidecar) {
                    Ok(deferred) => deferred,
                    Err(_) => {
                        server.record_agent_pump_error("Agent original checkpoint remains unconfirmed; further preparation fenced");
                        return;
                    }
                };
                let mut queued_job = None;
                let pending_before =
                    server.llm_sidecar.publication_ticket() != 0 || pending_dispatch.is_some();
                let had_phase = server.llm_sidecar.hosted_service_phase.is_some();
                let mut progress = None;
                let mut retry_without_progress = false;
                if in_flight && !pending_before {
                    match returned_result.take().unwrap_or_else(|| results.try_recv()) {
                        Ok(result) => {
                            in_flight = false;
                            let original_phase = result.token.phase_digest.clone();
                            let unpublished_response = unpublished_response(&result);
                            let apply_phase = stream_budget_diagnostics::phase(22);
                            #[cfg(any(test, feature = "test_tier_required"))]
                            let applied = diagnostics.apply(&mut server, result);
                            #[cfg(not(any(test, feature = "test_tier_required")))]
                            let applied = server.apply_agent_service_io(result);
                            drop(apply_phase);
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
                if !in_flight && !retry_without_progress && !pending_before {
                    let eligible = server.agent_service_eligible();
                    if progress.is_none()
                        || matches!(
                            progress,
                            Some(AgentServiceProgress::Idle | AgentServiceProgress::Advanced)
                        )
                    {
                        let prepare_phase = stream_budget_diagnostics::phase(23);
                        let prepared = server.prepare_agent_service_io(eligible);
                        drop(prepare_phase);
                        match prepared {
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
                        queued_job = Some(job);
                    }
                }
                let advance_without_sleep = continue_local_advance(
                    !retry_without_progress
                        && matches!(progress, Some(AgentServiceProgress::Advanced)),
                    had_phase,
                    server.llm_sidecar.hosted_service_phase.is_some(),
                    local_advances + 1,
                );
                let _queue = deferred.finish();
                let publications = std::sync::Arc::new(std::sync::Mutex::new(
                    server.llm_sidecar.take_queued_publications(),
                ));
                if !publications.lock().unwrap().is_empty() {
                    server
                        .llm_sidecar
                        .retain_inflight_publications(publications.clone());
                }
                let mut publication_fence = PublicationFence::capture(&server);
                // Release between every local transition so Viewer controls and
                // observers retain access. Fresh admission and terminal phase
                // removal stop the burst; it never starts another model turn.
                drop(server);
                drop(pump_phase);
                if !publications.lock().unwrap().is_empty() {
                    loop {
                        let outcome = publication_worker::publish(
                            &jobs,
                            &results,
                            &mut returned_result,
                            publications.clone(),
                            publication_fence.publication_ticket,
                        );
                        let Ok(mut current) = shared.lock() else {
                            return;
                        };
                        if !publication_fence.matches(&current) {
                            current.record_agent_pump_error("Agent checkpoint scope changed; original publication remains retained for recovery");
                            return;
                        }
                        if current.llm_sidecar.publication_capture_failed()
                            || publications
                                .lock()
                                .unwrap()
                                .iter()
                                .any(|publication| publication.permanently_blocked())
                        {
                            current.record_agent_pump_error("Agent checkpoint capture failed; original queued and in-flight batches retained");
                            return;
                        }
                        if publication_fence.publication_ticket
                            != current.llm_sidecar.publication_ticket()
                        {
                            let additional = current.llm_sidecar.take_queued_publications();
                            publication_fence.publication_ticket =
                                current.llm_sidecar.publication_ticket();
                            drop(current);
                            publications.lock().unwrap().extend(additional);
                            continue;
                        }
                        match outcome {
                            Err(publication_worker::Failure::Executor) => {
                                current.record_agent_pump_error(
                                    "Agent publication executor stopped; original batch retained",
                                );
                                return;
                            }
                            Ok(()) => {
                                if current
                                    .llm_sidecar
                                    .confirm_lineage_publication(
                                        publication_fence.publication_ticket,
                                    )
                                    .is_err()
                                {
                                    current.record_agent_pump_error("Agent checkpoint capture or ticket changed; original batch remains pending");
                                    return;
                                }
                                publication_fence.publication_ticket = 0;
                                current.llm_sidecar.flush_pending_provider_world_events();
                                if let Some((_, published)) =
                                    current.llm_sidecar.hosted_native_dispatch.as_mut()
                                {
                                    *published = true;
                                }
                                break;
                            }
                            Err(publication_worker::Failure::Publication) => {
                                current.record_agent_pump_error("Agent checkpoint publication failed; original work remains pending without dispatch");
                                drop(current);
                                thread::sleep(PUMP_INTERVAL);
                            }
                        }
                    }
                }
                if let Some(job) = queued_job {
                    pending_dispatch = Some((job, publication_fence.clone()));
                }
                if let Some((job, original_fence)) = pending_dispatch.take() {
                    let Ok(mut current) = shared.lock() else {
                        return;
                    };
                    if !original_fence.matches(&current) {
                        current.record_agent_pump_error("Agent prepared job scope changed; original work remains pending without dispatch");
                        return;
                    }
                    if current.llm_sidecar.ensure_lineage_published().is_err()
                        || (job.requires_fresh_dispatch_eligibility()
                            && !current.agent_service_eligible())
                    {
                        // Publication and fresh admission have separate gates.
                        // Pause during publication must keep this exact original
                        // Submit until current eligibility returns. Reconciliation
                        // and accepted terminal work are not cancelled by Pause.
                        pending_dispatch = Some((job, original_fence));
                    } else {
                        #[cfg(any(test, feature = "test_tier_required"))]
                        diagnostics.queued(&job);
                        match jobs.try_send(*job) {
                            Ok(()) => {
                                in_flight = true;
                                last_dispatch = Some(std::time::Instant::now());
                            }
                            Err(_) => {
                                current.agent_pump_worker_failed();
                                return;
                            }
                        }
                    }
                }
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

#[path = "agent_service_publication_worker.rs"]
mod publication_worker;
