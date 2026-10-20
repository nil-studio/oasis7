//! Bounded real TCP request reader; unused speculative connections carry no request.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::ExitStatus,
    sync::Condvar,
};
#[path = "http_fixture_compensation_settle_gate.rs"]
mod http_fixture_compensation_settle_gate;
#[path = "http_fixture_feedback_ack_gate.rs"]
mod http_fixture_feedback_ack_gate;
#[path = "http_fixture_rejected_admit_gate.rs"]
mod rejected_admit_gate;
#[path = "http_fixture_wait_admit_gate.rs"]
mod wait_admit_gate;

#[path = "http_fixture_rejected_resume_gate.rs"]
mod rejected_resume_gate;
#[path = "http_fixture_world_coherence_gate.rs"]
mod world_coherence_gate;

const RESUME_VIEW_RENDEZVOUS_BUDGET: Duration = Duration::from_millis(1_200);
const RESUME_VIEW_HOLD_BUDGET: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GateDisposition {
    FeedbackAckAbandoned,
    Continue,
    ResumeViewAbandoned,
    WaitAdmitViewAbandoned,
    CompensationSettleViewAbandoned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResumeCandidate {
    world: WorldIdentity,
    agent_id: String,
    correlation_digest: String,
    payload_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResumeViewSelector {
    world: WorldIdentity,
    scope_id: String,
    min_commit: CommitRef,
    resume_correlation_digest: String,
    resume_payload_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResumeViewSelectorEvidence {
    pub(super) commit: CommitRef,
    pub(super) selector_digest: String,
    pub(super) correlation_digest: String,
    pub(super) payload_digest: String,
}

#[derive(Default)]
struct ResumeViewGateState {
    candidate: Option<ResumeCandidate>,
    candidate_conflict: bool,
    selector: Option<ResumeViewSelector>,
    selector_digest: Option<String>,
    selector_installing: bool,
    selector_failed: bool,
    claimed: bool,
    owner: Option<thread::ThreadId>,
    released: bool,
    handler_returned: bool,
    joined: bool,
}

fn parse_resume_candidate(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<ResumeCandidate, String> {
    request.validate().map_err(|error| error.to_string())?;
    let WorldServicePayloadV1::Scheduler(signed) = &request.signed_payload else {
        return Err("ResumeWake candidate is not a scheduler payload".into());
    };
    if !matches!(
        &signed.request.operation,
        SchedulerOperationV1::ResumeWake { .. }
    ) {
        return Err("scheduler candidate is not ResumeWake".into());
    }
    let derived = correlation::derive_correlation(
        request.correlation.key.world.clone(),
        &request.signed_payload,
    )?;
    if derived != request.correlation {
        return Err("ResumeWake candidate correlation mismatch".into());
    }
    if signed.request.agent_id.trim().is_empty() {
        return Err("ResumeWake candidate has no Agent identity".into());
    }
    let world = derived.key.world.clone();
    let correlation_digest = correlation::key_digest(&derived.key)?;
    Ok(ResumeCandidate {
        world,
        agent_id: signed.request.agent_id.clone(),
        correlation_digest,
        payload_digest: derived.payload_digest,
    })
}

fn write_new_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn write_new_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    write_new_file(path, &bytes)
}

/// Fairness uses four real connections; all other fixtures retain serial dispatch.
pub(super) fn serve_listener<F>(
    listener: TcpListener,
    halt: Arc<AtomicBool>,
    concurrent: Arc<AtomicBool>,
    gate: Arc<WorldGate>,
    handler: Arc<F>,
) where
    F: Fn(TcpStream) + Send + Sync + 'static,
{
    let mut workers: Vec<thread::JoinHandle<thread::ThreadId>> = Vec::new();
    let mut failed = false;
    while !halt.load(Ordering::SeqCst) {
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                let result = workers.swap_remove(index).join();
                eprintln!("fixture_connection_worker_ok={}", result.is_ok());
                match result {
                    Ok(worker_id) => gate.connection_worker_joined(worker_id),
                    Err(_) => failed = true,
                }
            } else {
                index += 1;
            }
        }
        if workers.len() == 4 {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        match listener.accept() {
            Ok((stream, _)) if concurrent.load(Ordering::SeqCst) && gate.concurrent_ready() => {
                let handler = handler.clone();
                let worker_gate = gate.clone();
                workers.push(thread::spawn(move || {
                    handler(stream);
                    worker_gate.connection_worker_finished();
                    thread::current().id()
                }));
            }
            Ok((stream, _)) => {
                handler(stream);
                gate.connection_worker_finished();
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("accept: {error}"),
        }
    }
    for worker in workers {
        let result = worker.join();
        eprintln!("fixture_connection_worker_ok={}", result.is_ok());
        match result {
            Ok(worker_id) => gate.connection_worker_joined(worker_id),
            Err(_) => failed = true,
        }
    }
    assert!(!failed, "actual fixture connection worker failed");
}
#[derive(Default)]
pub(super) struct Outage {
    pub(super) active: AtomicBool,
    pub(super) observed: AtomicBool,
}
impl Outage {
    pub(super) fn reject(&self, stream: &TcpStream) -> bool {
        if !self.active.load(Ordering::SeqCst) {
            return false;
        }
        self.observed.store(true, Ordering::SeqCst);
        stream.shutdown(std::net::Shutdown::Both).unwrap();
        true
    }
}
pub(super) fn record_lookup_digest(bytes: &[u8], observed: &Arc<Mutex<Vec<String>>>) {
    let body = crate::feedback_submit_api::extract_http_json_body(bytes).unwrap();
    let request: oasis7::world_service::wire::AuthenticatedLookup =
        serde_json::from_slice(body).unwrap();
    let digest = correlation::key_digest(&request.request.key).unwrap();
    let mut observed = observed.lock().unwrap();
    assert!(
        observed.len() < 4096,
        "bounded conformance Lookup observation exhausted"
    );
    observed.push(digest);
}
pub(super) fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let n = match stream.read(&mut buffer) {
            Ok(0) if bytes.is_empty() => return Vec::new(),
            Ok(n) => n,
            Err(error)
                if bytes.is_empty()
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
            {
                return Vec::new();
            }
            Err(error) => panic!("incomplete HTTP fixture request: {error}"),
        };
        assert!(n > 0, "truncated request");
        bytes.extend_from_slice(&buffer[..n]);
        assert!(bytes.len() <= 262_144 + 8192, "unbounded request");
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let header = std::str::from_utf8(&bytes[..end]).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or_else(|| {
                    let first = header.lines().next().unwrap_or("");
                    let mut parts = first.split_whitespace();
                    let method = parts.next().unwrap_or("");
                    let path = parts.next().unwrap_or("");
                    eprintln!("fixture_no_content_length method={method} path={path}");
                    assert!(
                        matches!(method, "GET" | "HEAD"),
                        "body-bearing fixture request lacks Content-Length"
                    );
                    0
                });
            if bytes.len() >= end + 4 + length {
                return bytes;
            }
        }
    }
}

/// Only a verified Describe response establishes readiness; no write is retried.
pub(super) fn wait_ready(client: &RemoteWorldServiceClient, worker: &thread::JoinHandle<()>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match client.describe() {
            Ok(_) => return,
            Err(error) => {
                eprintln!(
                    "fixture_describe_not_ready error={error:?} worker_finished={}",
                    worker.is_finished()
                );
                // The public client intentionally hides reqwest's source chain. A read-only
                // diagnostic repeats Describe on a separate transport, without reporting body/keys.
                let http = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(1))
                    .build()
                    .unwrap();
                let result = http
                    .post(format!("{}{}", client.config().endpoint, DESCRIBE_PATH))
                    .json(&DescribeWorldRequest {
                        contract_version: 1,
                        expected_world: client.config().expected_world.clone(),
                        trust_config_ref: client.config().trusted_service_public_key.clone(),
                    })
                    .send();
                match result {
                    Ok(response) => eprintln!(
                        "fixture_describe_transport_probe status={}",
                        response.status()
                    ),
                    Err(error) => {
                        use std::error::Error;
                        let mut chain = vec![error.to_string()];
                        let mut source = error.source();
                        while let Some(error) = source {
                            chain.push(error.to_string());
                            source = error.source();
                        }
                        eprintln!("fixture_describe_transport_chain={chain:?}");
                    }
                }
                assert!(
                    !worker.is_finished(),
                    "fixture listener worker ended before verified Describe"
                );
                assert!(
                    Instant::now() < deadline,
                    "fixture Describe readiness timeout: {error:?}"
                );
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Darwin accept inherits O_NONBLOCK from the listening socket. Record the real
/// descriptor flag, then use blocking request reads with a finite read timeout.
pub(super) fn configure_accepted_stream(stream: &TcpStream) {
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        unsafe extern "C" {
            fn fcntl(fd: i32, command: i32) -> i32;
        }
        // F_GETFL=3 and O_NONBLOCK=4 are Darwin descriptor constants.
        let flags = unsafe { fcntl(stream.as_raw_fd(), 3) };
        assert!(flags >= 0, "read accepted descriptor flags failed");
        eprintln!(
            "fixture_accepted_socket_flags={flags} inherited_nonblocking={}",
            flags & 4 != 0
        );
    }
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
}

// Prefix distinguishes Submit admission observations from exact Lookup observations.
pub(super) fn record_submit_digest(bytes: &[u8], trace: &Arc<Mutex<Vec<String>>>) -> bool {
    let body = crate::feedback_submit_api::extract_http_json_body(bytes).unwrap();
    let Ok(request) = serde_json::from_slice::<SubmitIntentRequest<WorldServicePayloadV1>>(body)
    else {
        return false;
    };
    let digest = correlation::key_digest(&request.correlation.key).unwrap();
    let mut observed = trace.lock().unwrap();
    assert!(
        observed.len() < 4096,
        "bounded conformance operation trace exhausted"
    );
    observed.push(format!("submit:{digest}"));
    if matches!(&request.signed_payload, WorldServicePayloadV1::Cognition(_)) {
        request
            .validate()
            .expect("captured Cognition Submit contract");
        // Retain the complete typed request privately, never in stdout. This
        // catches alternate correlations or signed bytes, not only key reuse.
        assert!(
            observed.len() < 4096,
            "bounded typed Submit trace exhausted"
        );
        observed.push(format!(
            "cognition_submit:{}",
            serde_json::to_string(&request).unwrap()
        ));
    }
    matches!(request.signed_payload,WorldServicePayloadV1::Scheduler(signed) if matches!(signed.request.operation,SchedulerOperationV1::ReleaseLease {..}))
}

/// A real parsed HTTP operation can be held without altering its bytes or canonical executor.
#[derive(Default)]
pub(super) struct WorldGate {
    pub(super) root: Mutex<Option<std::path::PathBuf>>,
    feedback_ack_typed_trace: Mutex<Vec<serde_json::Value>>,
    coherence: Mutex<world_coherence_gate::CoherenceGateState>,
    rejected_resume: Mutex<rejected_resume_gate::RejectedResumeGate>,
    periodic_view_claimed: AtomicBool,
    periodic_view_owner: Mutex<Option<thread::ThreadId>>,
    resume_view_armed: AtomicBool,
    resume_view: Mutex<ResumeViewGateState>,
    resume_view_changed: Condvar,
    admit_wait_view: Mutex<wait_admit_gate::AdmitWaitViewGateState>,
    admit_wait_view_changed: Condvar,
    rejected_admit: Mutex<rejected_admit_gate::AdmitRejectedGateState>,
    rejected_admit_changed: Condvar,
    compensation_settle: Mutex<http_fixture_compensation_settle_gate::CompensationGateState>,
    compensation_changed: Condvar,
}
impl WorldGate {
    fn concurrent_ready(&self) -> bool {
        self.root
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|root| root.join("world-concurrent-ready").exists())
    }
    /// Only the dedicated Resume-before-View process-crash oracle participates
    /// in selector rendezvous. Ordinary Resume traffic must never wait for it.
    pub(super) fn arm_resume_view_selector(&self) {
        assert!(
            !self.resume_view_armed.swap(true, Ordering::SeqCst),
            "Resume View selector must be armed exactly once"
        );
    }

    pub(super) fn install_resume_view_selector_from_authenticated_lookup(
        &self,
        client: &RemoteWorldServiceClient,
        scope_id: &str,
        original: SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<ResumeViewSelectorEvidence, String> {
        let root = self
            .root
            .lock()
            .unwrap()
            .clone()
            .ok_or("Resume View gate root is not configured")?;
        let candidate = parse_resume_candidate(&original)?;
        let expected_scope = format!("agent:{}", candidate.agent_id);
        if scope_id != expected_scope {
            return Err("Resume View selector scope differs from candidate Agent".into());
        }

        {
            let mut state = self.resume_view.lock().unwrap();
            if state.selector.is_some() || state.selector_installing || state.selector_failed {
                return Err("Resume View selector is one-shot".into());
            }
            state.selector_installing = true;
            let deadline = Instant::now() + RESUME_VIEW_RENDEZVOUS_BUDGET;
            while state.candidate.is_none()
                && !state.candidate_conflict
                && Instant::now() < deadline
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                let (next, _) = self
                    .resume_view_changed
                    .wait_timeout(state, remaining)
                    .unwrap();
                state = next;
            }
            if state.candidate.as_ref() != Some(&candidate) || state.candidate_conflict {
                state.selector_installing = false;
                state.selector_failed = true;
                self.resume_view_changed.notify_all();
                return Err(
                    "Resume View selector did not match a unique observed ResumeWake".into(),
                );
            }
        }

        let lookup_result = (|| {
            let response = client
                .lookup(
                    LookupIntentRequest {
                        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                        key: original.correlation.key.clone(),
                    },
                    original.signed_payload.clone(),
                )
                .map_err(|error| format!("authenticated ResumeWake Lookup failed: {error}"))?;
            response
                .validate(&original.correlation)
                .map_err(|error| format!("authenticated ResumeWake Lookup mismatch: {error}"))?;
            let IntentOutcome::Committed { commit, .. } = response.outcome else {
                return Err("authenticated ResumeWake Lookup is not committed".into());
            };
            commit
                .validate()
                .map_err(|error| format!("ResumeWake CommitRef is invalid: {error}"))?;
            if commit.world != candidate.world {
                return Err("ResumeWake CommitRef world differs from candidate".into());
            }
            Ok(*commit)
        })();

        let commit = match lookup_result {
            Ok(commit) => commit,
            Err(error) => {
                let mut state = self.resume_view.lock().unwrap();
                state.selector_installing = false;
                state.selector_failed = true;
                self.resume_view_changed.notify_all();
                return Err(error);
            }
        };
        let selector = ResumeViewSelector {
            world: candidate.world.clone(),
            scope_id: scope_id.to_owned(),
            min_commit: commit.clone(),
            resume_correlation_digest: candidate.correlation_digest.clone(),
            resume_payload_digest: candidate.payload_digest.clone(),
        };
        let selector_digest = match oasis7::world_service::authority::request_digest(
            "resume-view-selector-v1",
            &selector,
        ) {
            Ok(digest) => digest,
            Err(error) => {
                let mut state = self.resume_view.lock().unwrap();
                state.selector_installing = false;
                state.selector_failed = true;
                self.resume_view_changed.notify_all();
                return Err(format!("Resume View selector digest failed: {error}"));
            }
        };
        let selector_path = root.join("world-resume-view-selector.json");
        if let Err(error) = write_new_json(&selector_path, &selector) {
            let mut state = self.resume_view.lock().unwrap();
            state.selector_installing = false;
            state.selector_failed = true;
            self.resume_view_changed.notify_all();
            return Err(format!(
                "immutable Resume View selector write failed: {error}"
            ));
        }
        {
            let mut state = self.resume_view.lock().unwrap();
            if state.candidate.as_ref() != Some(&candidate)
                || state.candidate_conflict
                || !state.selector_installing
                || state.selector.is_some()
            {
                state.selector_installing = false;
                state.selector_failed = true;
                self.resume_view_changed.notify_all();
                return Err("Resume View candidate changed during selector installation".into());
            }
            state.selector = Some(selector);
            state.selector_digest = Some(selector_digest.clone());
            state.selector_installing = false;
            self.resume_view_changed.notify_all();
        }
        Ok(ResumeViewSelectorEvidence {
            commit,
            selector_digest,
            correlation_digest: candidate.correlation_digest,
            payload_digest: candidate.payload_digest,
        })
    }

    pub(super) fn release_resume_view_gate_after_child_exit73(
        &self,
        status: &ExitStatus,
    ) -> Result<(), String> {
        if status.code() != Some(73) {
            return Err("Resume View gate release requires confirmed child exit 73".into());
        }
        let root = self
            .root
            .lock()
            .unwrap()
            .clone()
            .ok_or("Resume View gate root is not configured")?;
        {
            let mut state = self.resume_view.lock().unwrap();
            if !state.claimed || state.selector.is_none() || state.released {
                return Err("Resume View gate is not held for one-shot release".into());
            }
            write_new_json(
                &root.join("world-resume-view-release"),
                &serde_json::json!({
                    "child_exit_code": 73,
                    "child_exit_confirmed": true,
                    "abandoned_view_request": true,
                }),
            )
            .map_err(|error| format!("immutable Resume View release marker failed: {error}"))?;
            state.released = true;
            self.resume_view_changed.notify_all();
        }
        Ok(())
    }

    fn observe_resume_candidate(
        &self,
        _root: &Path,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) {
        if !self.resume_view_armed.load(Ordering::SeqCst) {
            return;
        }
        let Ok(candidate) = parse_resume_candidate(request) else {
            return;
        };
        let mut state = self.resume_view.lock().unwrap();
        if let Some(previous) = &state.candidate {
            if previous != &candidate {
                state.candidate_conflict = true;
            } else {
                // A repeated ResumeWake is itself a replay signal for this one-shot gate.
                state.candidate_conflict = true;
            }
        } else {
            state.candidate = Some(candidate);
        }
        self.resume_view_changed.notify_all();
    }

    fn pause_resume_view(
        &self,
        root: &Path,
        request: &wire::SignedReadRequest<ReadWorldViewRequest>,
    ) -> bool {
        if !self.resume_view_armed.load(Ordering::SeqCst) {
            return false;
        }
        let Some(min_commit) = request.request.min_commit.as_ref() else {
            return false;
        };
        if request.request.fixed_commit.is_some() {
            return false;
        }
        let scope_id = request.request.scope_id.as_str();
        let mut state = self.resume_view.lock().unwrap();
        let Some(candidate) = state.candidate.clone() else {
            return false;
        };
        if state.candidate_conflict
            || candidate.world != request.request.world
            || scope_id != format!("agent:{}", candidate.agent_id)
        {
            return false;
        }
        let deadline = Instant::now() + RESUME_VIEW_RENDEZVOUS_BUDGET;
        while state.selector.is_none()
            && !state.selector_failed
            && !state.candidate_conflict
            && Instant::now() < deadline
        {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (next, _) = self
                .resume_view_changed
                .wait_timeout(state, remaining)
                .unwrap();
            state = next;
        }
        let Some(selector) = state.selector.clone() else {
            return false;
        };
        if state.selector_failed
            || state.claimed
            || selector.world != request.request.world
            || selector.scope_id != request.request.scope_id
            || selector.min_commit != *min_commit
        {
            return false;
        }
        let selector_digest = state
            .selector_digest
            .clone()
            .expect("installed Resume View selector has digest");
        state.claimed = true;
        state.owner = Some(thread::current().id());
        drop(state);

        let request_digest =
            oasis7::world_service::authority::request_digest(VIEW_PATH, &request.request)
                .expect("verified Resume View request has digest");
        write_new_json(
            &root.join("world-resume-view-started"),
            &serde_json::json!({
                "signature_verified": true,
                "selector_matches_signed_view": true,
                "fixed_commit_none": true,
                "node_view_handler_dispatched": false,
                "world": selector.world,
                "scope_id": selector.scope_id,
                "min_commit": selector.min_commit,
                "resume_correlation_digest": selector.resume_correlation_digest,
                "resume_payload_digest": selector.resume_payload_digest,
                "selector_digest": selector_digest,
                "view_request_digest": request_digest,
            }),
        )
        .expect("immutable Resume View started marker must be unique");

        let deadline = Instant::now() + RESUME_VIEW_HOLD_BUDGET;
        while !resume_release_confirmed(root) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        if resume_release_confirmed(root) {
            // The process exited while its signed ReadView was held before Node dispatch.
            write_new_json(
                &root.join("world-resume-view-gate-returned"),
                &serde_json::json!({
                    "abandoned_after_confirmed_exit73": true,
                    "child_exit_code": 73,
                    "node_view_handler_dispatched": false,
                    "selector_digest": selector_digest,
                }),
            )
            .expect("immutable Resume View gate-returned marker must be unique");
            true
        } else {
            let _ = write_new_json(
                &root.join("world-resume-view-timeout"),
                &serde_json::json!({
                    "abandoned_after_confirmed_exit73": false,
                    "node_view_handler_dispatched": false,
                    "selector_digest": selector_digest,
                }),
            );
            let mut state = self.resume_view.lock().unwrap();
            if state.owner == Some(thread::current().id()) {
                state.owner = None;
            }
            false
        }
    }

    pub(super) fn pause(&self, path: &str, bytes: &[u8]) -> GateDisposition {
        self.observe_feedback_ack_typed_request(path, bytes);
        if self.pause_feedback_ack_before(path, bytes) {
            return GateDisposition::FeedbackAckAbandoned;
        }
        let Some(root) = self.root.lock().unwrap().clone() else {
            return GateDisposition::Continue;
        };
        if path == CHANGES_PATH {
            self.pause_coherence_changes(&root, bytes);
        }
        if path == SUBMIT_PATH {
            let body = crate::feedback_submit_api::extract_http_json_body(bytes).unwrap();
            if let Ok(request) =
                serde_json::from_slice::<SubmitIntentRequest<WorldServicePayloadV1>>(body)
            {
                self.pause_rejected_resume(&root, &request);
                self.observe_compensation_settle(&request);
                self.observe_resume_candidate(&root, &request);
                self.observe_admit_wait_candidate(&request);
                self.pause_rejected_admit_submit(&root, &request, bytes);
            }
        }
        let mut started_record = b"parsed actual operation".to_vec();
        let kind = if path == SUBMIT_PATH {
            let body = crate::feedback_submit_api::extract_http_json_body(bytes).unwrap();
            let Ok(request) =
                serde_json::from_slice::<SubmitIntentRequest<WorldServicePayloadV1>>(body)
            else {
                return GateDisposition::Continue;
            };
            match &request.signed_payload {
                WorldServicePayloadV1::Cognition(_) => "submit",
                WorldServicePayloadV1::Scheduler(signed)
                    if matches!(
                        &signed.request.operation,
                        SchedulerOperationV1::SettleLease { .. }
                    ) =>
                {
                    started_record = serde_json::to_vec(&request).unwrap();
                    "settle"
                }
                _ => return GateDisposition::Continue,
            }
        } else if path == LOOKUP_PATH {
            let body = crate::feedback_submit_api::extract_http_json_body(bytes).unwrap();
            let Ok(request) = serde_json::from_slice::<wire::AuthenticatedLookup>(body) else {
                return GateDisposition::Continue;
            };
            if !matches!(request.original, WorldServicePayloadV1::Cognition(_)) {
                return GateDisposition::Continue;
            }
            "lookup"
        } else if path == VIEW_PATH {
            let body = crate::feedback_submit_api::extract_http_json_body(bytes).unwrap();
            let Ok(request) =
                serde_json::from_slice::<wire::SignedReadRequest<ReadWorldViewRequest>>(body)
            else {
                return GateDisposition::Continue;
            };
            if oasis7::world_service::authority::verify_read_request(VIEW_PATH, &request).is_err() {
                return GateDisposition::Continue;
            }
            if request.request.validate().is_err() {
                return GateDisposition::Continue;
            }
            if self.pause_compensation_view(&root, &request) {
                return GateDisposition::CompensationSettleViewAbandoned;
            }
            if self.pause_admit_wait_view(&root, &request) {
                return GateDisposition::WaitAdmitViewAbandoned;
            }
            if self.pause_resume_view(&root, &request) {
                return GateDisposition::ResumeViewAbandoned;
            }
            if root.join("world-periodic-view-arm").exists() {
                if request.request.min_commit.is_none()
                    || root.join("world-periodic-view-release").exists()
                    || self.periodic_view_claimed.swap(true, Ordering::SeqCst)
                {
                    return GateDisposition::Continue;
                }
                let min_commit = request.request.min_commit.as_ref().unwrap();
                let request_digest =
                    oasis7::world_service::authority::request_digest(VIEW_PATH, &request.request)
                        .unwrap();
                started_record = serde_json::to_vec(&serde_json::json!({
                    "signature_verified": true,
                    "min_commit": min_commit,
                    "min_commit_position": min_commit.position,
                    "request_digest": request_digest,
                }))
                .unwrap();
                *self.periodic_view_owner.lock().unwrap() = Some(thread::current().id());
                "periodic-view"
            } else {
                "view"
            }
        } else {
            return GateDisposition::Continue;
        };
        if !root.join(format!("world-{kind}-arm")).exists()
            || root.join(format!("world-{kind}-release")).exists()
        {
            return GateDisposition::Continue;
        }
        super::application_hosted_wait::publish_marker(
            &root.join(format!("world-{kind}-started")),
            &started_record,
        )
        .unwrap();
        let gate_budget =
            if kind == "periodic-view" && root.join("world-periodic-slow-view").exists() {
                Duration::from_secs(4)
            } else if kind == "periodic-view" {
                Duration::from_millis(1_500)
            } else {
                Duration::from_secs(2)
            };
        let deadline = Instant::now() + gate_budget;
        while !root.join(format!("world-{kind}-release")).exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        println!(
            "world_service_operation_fault_gate kind={kind} original_bytes_preserved=true deadline_ms={}",
            gate_budget.as_millis()
        );
        if kind == "periodic-view" {
            fs::write(root.join("world-periodic-view-gate-returned"), b"released").unwrap();
        }
        GateDisposition::Continue
    }

    fn connection_worker_finished(&self) {
        let current = thread::current().id();
        self.coherence_handler_returned(current);
        self.rejected_resume_returned(current);
        let owns_periodic_view = {
            let mut owner = self.periodic_view_owner.lock().unwrap();
            if owner.as_ref() == Some(&current) {
                owner.take();
                true
            } else {
                false
            }
        };
        let resume_returned = {
            let mut state = self.resume_view.lock().unwrap();
            if state.owner == Some(current) && state.released && !state.handler_returned {
                state.handler_returned = true;
                true
            } else {
                false
            }
        };
        if let Some(root) = self.root.lock().unwrap().clone() {
            self.compensation_handler_returned(&root, current);
            self.admit_wait_connection_worker_finished(&root, current);
            self.rejected_admit_connection_worker_finished(&root, current);
            if owns_periodic_view && root.join("world-periodic-view-release").exists() {
                fs::write(
                    root.join("world-periodic-view-completed"),
                    b"handler returned",
                )
                .unwrap();
            }
            if resume_returned {
                write_new_json(
                    &root.join("world-resume-view-handler-returned"),
                    &serde_json::json!({
                        "connection_handler_returned": true,
                        "abandoned_after_confirmed_exit73": true,
                        "node_view_handler_dispatched": false,
                    }),
                )
                .expect("immutable Resume View handler-returned marker must be unique");
            }
        }
    }

    fn connection_worker_joined(&self, worker_id: thread::ThreadId) {
        self.coherence_worker_joined(worker_id);
        self.rejected_resume_joined(worker_id);
        let joined = {
            let mut state = self.resume_view.lock().unwrap();
            if state.owner == Some(worker_id)
                && state.released
                && state.handler_returned
                && !state.joined
            {
                state.joined = true;
                true
            } else {
                false
            }
        };
        if joined {
            if let Some(root) = self.root.lock().unwrap().clone() {
                self.compensation_worker_joined(&root, worker_id);
                self.admit_wait_connection_worker_joined(&root, worker_id);
                self.rejected_admit_connection_worker_joined(&root, worker_id);
                write_new_json(
                    &root.join("world-resume-view-worker-joined"),
                    &serde_json::json!({
                        "listener_owner_join_observed": true,
                        "worker_join_succeeded": true,
                        "abandoned_after_confirmed_exit73": true,
                        "node_view_handler_dispatched": false,
                    }),
                )
                .expect("immutable Resume View worker-joined marker must be unique");
            }
        } else if let Some(root) = self.root.lock().unwrap().clone() {
            self.compensation_worker_joined(&root, worker_id);
            self.admit_wait_connection_worker_joined(&root, worker_id);
            self.rejected_admit_connection_worker_joined(&root, worker_id);
        }
    }
}

fn resume_release_confirmed(root: &Path) -> bool {
    fs::read(root.join("world-resume-view-release"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| {
            value["child_exit_code"] == 73 && value["child_exit_confirmed"] == true
        })
}
