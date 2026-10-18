//! Periodic service reads never occupy a connection's receive loop.
//! One worker and one queued job belong to one Viewer server. Each connection
//! owns at most one result mailbox; disconnect drops it without joining I/O.
use super::chain_link::PreparedChainLinkedRuntimeUpdate;
use super::periodic_connection_diagnostic::ConnectionTrace;
#[cfg(test)]
use super::periodic_connection_diagnostic::TraceSink;
use super::world_service_link::prepare_world_service_update_until;
use super::*;
use crate::world_service::WorldServicePayloadV1;
use crate::world_service::client::{WorldServiceClientConfig, WorldServiceQueryState};
use crate::world_service::verified_view::VerifiedWorldView;
use oasis7_client_api::world_service::RequestCorrelation;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

type Pending = Vec<(RequestCorrelation, WorldServicePayloadV1)>;

pub(super) struct Executor {
    sender: SyncSender<Job>,
    outstanding: Arc<AtomicUsize>,
    #[cfg(any(test, feature = "test_tier_required"))]
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

struct Permit(Arc<AtomicUsize>);
impl Permit {
    fn acquire(outstanding: &Arc<AtomicUsize>) -> Option<Self> {
        outstanding
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < 2).then_some(count + 1)
            })
            .ok()?;
        Some(Self(outstanding.clone()))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct Job {
    token: Token,
    pending: Pending,
    query_state: WorldServiceQueryState,
    destination: SyncSender<Completion>,
    permit: Permit,
    trace: Option<PeriodicIoTrace>,
}

struct Token {
    config: WorldServiceClientConfig,
    baseline: VerifiedWorldView,
    player: Option<String>,
    streams: HashSet<ViewerStream>,
    event_filters: Option<HashSet<ViewerEventKind>>,
    protocol: crate::viewer::protocol::NegotiatedViewerProtocol,
}

struct Completion {
    token: Token,
    originals: Pending,
    outcome: Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError>,
    _permit: Permit,
    trace: Option<PeriodicIoTrace>,
}

// Opt-in diagnostics emit a bounded lifecycle for each admitted read.
const TRACE_ELAPSED_MAX_MS: u128 = 120_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TraceStage {
    Admitted,
    Started,
    Terminal,
    ConsumedDelivery,
}

impl TraceStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Started => "started",
            Self::Terminal => "terminal",
            Self::ConsumedDelivery => "consumed_delivery",
        }
    }

    fn order(self) -> u8 {
        match self {
            Self::Admitted => 1,
            Self::Started => 2,
            Self::Terminal => 3,
            Self::ConsumedDelivery => 4,
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TraceObservation {
    stage: TraceStage,
    status: &'static str,
}

#[derive(Clone)]
struct TraceIdentity {
    sequence: u64,
    submitted_at: Instant,
    connection: ConnectionTrace,
    #[cfg(test)]
    observations: Option<Arc<Mutex<Vec<TraceObservation>>>>,
}

impl TraceIdentity {
    fn new(connection: ConnectionTrace) -> Self {
        let sequence = connection.job_sequence();
        Self {
            sequence,
            submitted_at: Instant::now(),
            connection,
            #[cfg(test)]
            observations: None,
        }
    }

    fn elapsed_ms(&self) -> u64 {
        Instant::now()
            .saturating_duration_since(self.submitted_at)
            .as_millis()
            .min(TRACE_ELAPSED_MAX_MS) as u64
    }

    fn emit(&self, stage: TraceStage, status: &'static str, details: String) {
        #[cfg(test)]
        if let Some(observations) = &self.observations {
            observations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(TraceObservation { stage, status });
        }
        self.connection.emit_job(
            self.sequence,
            stage.as_str(),
            stage.order(),
            status,
            self.elapsed_ms(),
            details,
        );
    }
}

#[derive(Clone, Copy)]
struct TraceCursor {
    commit_position: u64,
    event_sequence: u64,
}

impl TraceCursor {
    fn from_view(view: &VerifiedWorldView) -> Self {
        Self {
            commit_position: view.version().commit.position,
            event_sequence: view.continuation().sequence,
        }
    }
}

#[derive(Clone, Copy)]
enum TraceReadResult {
    Pending,
    Ok,
    Error,
}

impl TraceReadResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy)]
enum TraceApplyResult {
    NotRun,
    StaleToken,
    ReadError,
    MissingView,
    CursorRejected,
    BindingError,
    ApplyError,
    NoAdvance,
    Advanced,
    ServerLockError,
}

impl TraceApplyResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotRun => "not_run",
            Self::StaleToken => "stale_token",
            Self::ReadError => "read_error",
            Self::MissingView => "missing_view",
            Self::CursorRejected => "cursor_rejected",
            Self::BindingError => "binding_error",
            Self::ApplyError => "apply_error",
            Self::NoAdvance => "no_advance",
            Self::Advanced => "advanced",
            Self::ServerLockError => "server_lock_error",
        }
    }
}

#[derive(Clone, Copy)]
enum TraceBufferResult {
    NotAttempted,
    Ok,
    Error,
}

impl TraceBufferResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotAttempted => "not_attempted",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}

struct PeriodicIoTrace {
    identity: TraceIdentity,
    worker_started_at: Option<Instant>,
    worker_completed_at: Option<Instant>,
    baseline: TraceCursor,
    returned: Option<TraceCursor>,
    outstanding_at_submit: u8,
    pending_count: u8,
    read_result: TraceReadResult,
    apply_result: TraceApplyResult,
    buffer_result: TraceBufferResult,
    mailbox_wait_ms: u64,
    apply_ms: u64,
    snapshot_count: u16,
}

impl PeriodicIoTrace {
    fn new(
        baseline: TraceCursor,
        outstanding_at_submit: usize,
        pending_count: usize,
        connection: ConnectionTrace,
    ) -> Self {
        Self {
            identity: TraceIdentity::new(connection),
            worker_started_at: None,
            worker_completed_at: None,
            baseline,
            returned: None,
            outstanding_at_submit: outstanding_at_submit.min(2) as u8,
            pending_count: pending_count.min(32) as u8,
            read_result: TraceReadResult::Pending,
            apply_result: TraceApplyResult::NotRun,
            buffer_result: TraceBufferResult::NotAttempted,
            mailbox_wait_ms: 0,
            apply_ms: 0,
            snapshot_count: 0,
        }
    }

    #[cfg(test)]
    fn for_test(baseline: TraceCursor) -> (Self, Arc<Mutex<Vec<TraceObservation>>>) {
        let sink = TraceSink::for_test();
        let mut trace = Self::new(baseline, 1, 0, sink.connection());
        let observations = Arc::new(Mutex::new(Vec::new()));
        trace.identity.observations = Some(observations.clone());
        (trace, observations)
    }

    fn elapsed_ms(start: Option<Instant>, end: Option<Instant>) -> u64 {
        match (start, end) {
            (Some(start), Some(end)) => end
                .saturating_duration_since(start)
                .as_millis()
                .min(TRACE_ELAPSED_MAX_MS) as u64,
            _ => 0,
        }
    }

    fn mark_read_started(&mut self) {
        let started_at = Instant::now();
        self.worker_started_at = Some(started_at);
        self.identity.emit(
            TraceStage::Started,
            "running",
            format!(
                "queue_ms={}",
                Self::elapsed_ms(Some(self.identity.submitted_at), Some(started_at)),
            ),
        );
    }

    fn mark_read_finished(
        &mut self,
        outcome: &Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError>,
    ) {
        self.read_result = if outcome.is_ok() {
            TraceReadResult::Ok
        } else {
            TraceReadResult::Error
        };
        self.returned = outcome
            .as_ref()
            .ok()
            .and_then(|prepared| prepared.verified_view.as_ref())
            .map(TraceCursor::from_view);
        self.worker_completed_at = Some(Instant::now());
    }

    fn admission_event(&self) -> TraceAdmissionEvent {
        TraceAdmissionEvent {
            identity: self.identity.clone(),
            baseline: self.baseline,
            outstanding: self.outstanding_at_submit,
            pending: self.pending_count,
        }
    }

    fn terminal_event(&self) -> TraceTerminalEvent {
        TraceTerminalEvent {
            identity: self.identity.clone(),
            baseline: self.baseline,
            returned: self.returned,
            outstanding: self.outstanding_at_submit,
            pending: self.pending_count,
            read_result: self.read_result,
            read_ms: Self::elapsed_ms(self.worker_started_at, self.worker_completed_at),
        }
    }

    fn finish_delivery(self, outbox_succeeded: bool, outbox_elapsed: Duration) {
        let snapshot_delivery = if self.snapshot_count == 0 {
            "not_queued"
        } else if outbox_succeeded {
            "write_success"
        } else {
            "write_error"
        };
        let returned_position = self.returned.map_or(0, |cursor| cursor.commit_position);
        let returned_sequence = self.returned.map_or(0, |cursor| cursor.event_sequence);
        self.identity.emit(
            TraceStage::ConsumedDelivery,
            snapshot_delivery,
            format!(
                "apply={} response_buffer={} base_commit={} base_sequence={} returned_present={} returned_commit={} returned_sequence={} outstanding={} pending={} mailbox_ms={} apply_ms={} snapshots_queued={} outbox_ms={}",
                self.apply_result.as_str(),
                self.buffer_result.as_str(),
                self.baseline.commit_position,
                self.baseline.event_sequence,
                u8::from(self.returned.is_some()),
                returned_position,
                returned_sequence,
                self.outstanding_at_submit,
                self.pending_count,
                self.mailbox_wait_ms,
                self.apply_ms,
                self.snapshot_count,
                outbox_elapsed.as_millis().min(TRACE_ELAPSED_MAX_MS) as u64,
            ),
        );
    }
}

struct TraceAdmissionEvent {
    identity: TraceIdentity,
    baseline: TraceCursor,
    outstanding: u8,
    pending: u8,
}

impl TraceAdmissionEvent {
    fn emit(self) {
        self.identity.emit(
            TraceStage::Admitted,
            "accepted",
            format!(
                "base_commit={} base_sequence={} outstanding={} pending={}",
                self.baseline.commit_position,
                self.baseline.event_sequence,
                self.outstanding,
                self.pending,
            ),
        );
    }
}

struct TraceTerminalEvent {
    identity: TraceIdentity,
    baseline: TraceCursor,
    returned: Option<TraceCursor>,
    outstanding: u8,
    pending: u8,
    read_result: TraceReadResult,
    read_ms: u64,
}

impl TraceTerminalEvent {
    fn emit(self, mailbox_status: &'static str) {
        self.identity.emit(
            TraceStage::Terminal,
            mailbox_status,
            format!(
                "read={} base_commit={} base_sequence={} returned_present={} returned_commit={} returned_sequence={} outstanding={} pending={} read_ms={}",
                self.read_result.as_str(),
                self.baseline.commit_position,
                self.baseline.event_sequence,
                u8::from(self.returned.is_some()),
                self.returned.map_or(0, |cursor| cursor.commit_position),
                self.returned.map_or(0, |cursor| cursor.event_sequence),
                self.outstanding,
                self.pending,
                self.read_ms,
            ),
        );
    }
}

fn create_trace(
    baseline: TraceCursor,
    outstanding_at_submit: usize,
    pending_count: usize,
    connection: Option<&ConnectionTrace>,
) -> Option<PeriodicIoTrace> {
    connection.map(|connection| {
        PeriodicIoTrace::new(
            baseline,
            outstanding_at_submit,
            pending_count,
            connection.clone(),
        )
    })
}

fn send_traced_completion<T>(
    destination: &SyncSender<T>,
    completion: T,
    terminal: Option<TraceTerminalEvent>,
) {
    let mailbox_status = match destination.try_send(completion) {
        Ok(()) => "queued",
        Err(TrySendError::Full(completion)) => {
            drop(completion);
            "full"
        }
        Err(TrySendError::Disconnected(completion)) => {
            drop(completion);
            "disconnected"
        }
    };
    if let Some(terminal) = terminal {
        terminal.emit(mailbox_status);
    }
}

fn run_periodic_read(
    trace: &mut Option<PeriodicIoTrace>,
    read: impl FnOnce() -> Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError>,
) -> Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError> {
    if let Some(trace) = trace.as_mut() {
        trace.mark_read_started();
    }
    let outcome = read();
    if let Some(trace) = trace.as_mut() {
        trace.mark_read_finished(&outcome);
    }
    outcome
}

impl Executor {
    fn new() -> std::io::Result<Arc<Self>> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(1);
        let worker = std::thread::Builder::new()
            .name("viewer-periodic-service".into())
            .spawn(move || {
                // The worker owns no server/Sidecar Arc. Closing the server's
                // sender drains at most one queued job, then terminates it.
                while let Ok(mut job) = receiver.recv() {
                    let budget = job
                        .token
                        .config
                        .timeout
                        .clamp(Duration::from_secs(2), Duration::from_secs(30));
                    let config = job.token.config.clone();
                    let baseline = job.token.baseline.clone();
                    let pending = job.pending.clone();
                    let query_state = job.query_state;
                    let outcome = run_periodic_read(&mut job.trace, || {
                        prepare_world_service_update_until(
                            config,
                            Some(baseline),
                            pending,
                            query_state,
                            Some(Instant::now() + budget),
                        )
                    });
                    let terminal = job.trace.as_ref().map(PeriodicIoTrace::terminal_event);
                    let completion = Completion {
                        token: job.token,
                        originals: job.pending,
                        outcome,
                        _permit: job.permit,
                        trace: job.trace,
                    };
                    send_traced_completion(&job.destination, completion, terminal);
                }
            })?;
        #[cfg(not(any(test, feature = "test_tier_required")))]
        drop(worker);
        Ok(Arc::new(Self {
            sender,
            outstanding: Arc::new(AtomicUsize::new(0)),
            #[cfg(any(test, feature = "test_tier_required"))]
            worker: Mutex::new(Some(worker)),
        }))
    }

    #[cfg(test)]
    pub(super) fn outstanding_for_test(&self) -> usize {
        self.outstanding.load(Ordering::Acquire)
    }
}

#[derive(Default)]
pub(super) struct SessionPoll {
    result: Option<Receiver<Completion>>,
    executor: Option<Arc<Executor>>,
    retry_at: Option<Instant>,
    trace: Option<PeriodicIoTrace>,
    connection: Option<ConnectionTrace>,
}

impl SessionPoll {
    pub(super) fn with_connection(connection: Option<ConnectionTrace>) -> Self {
        let mut poll = Self::default();
        poll.connection = connection;
        poll
    }
}

impl Drop for SessionPoll {
    fn drop(&mut self) {
        if let Some(connection) = &self.connection {
            // Drop::drop runs before the receiver field is dropped. The outer
            // stream handler emits the captured state only after all original
            // connection-owned objects have then dropped in their old order.
            connection.capture_before_receiver_drop();
        }
    }
}

fn same_config(left: &WorldServiceClientConfig, right: &WorldServiceClientConfig) -> bool {
    left.endpoint == right.endpoint
        && left.trusted_service_public_key == right.trusted_service_public_key
        && left.expected_world == right.expected_world
        && left.scope_id == right.scope_id
        && left.read_private_key_hex == right.read_private_key_hex
        && left.timeout == right.timeout
        && left.max_response_bytes == right.max_response_bytes
}

impl Token {
    fn matches(&self, server: &ViewerRuntimeLiveServer, session: &RuntimeLiveSession) -> bool {
        server.authoritative_recovery_write_fence.is_none()
            && server
                .config
                .world_service
                .as_ref()
                .is_some_and(|current| same_config(current, &self.config))
            && server.verified_world_view.as_ref().is_some_and(|current| {
                current.version() == self.baseline.version()
                    && current.continuation() == self.baseline.continuation()
            })
            && session.current_player_id == self.player
            && session.subscribed == self.streams
            && session.event_filters == self.event_filters
            && session.negotiated_protocol == self.protocol
    }
}

impl SessionPoll {
    pub(super) fn deliver_output(
        &mut self,
        output: &mut super::response_outbox::ResponseOutbox,
        socket: &mut TcpStream,
    ) -> std::io::Result<()> {
        let started_at = self.trace.as_ref().map(|_| Instant::now());
        let delivery_result = output.deliver(socket);
        if let Some(trace) = self.trace.take() {
            let elapsed = started_at.map_or(Duration::ZERO, |started_at| {
                Instant::now().saturating_duration_since(started_at)
            });
            // Success only means this process completed the outbox socket write;
            // it does not attest that the peer received or applied the frame.
            let job_sequence = trace.identity.sequence;
            trace.finish_delivery(delivery_result.is_ok(), elapsed);
            if let Some(connection) = &self.connection {
                connection.clear_pending_job(job_sequence);
            }
        }
        delivery_result
    }

    fn defer(&mut self, interval: Duration) {
        self.retry_at = Some(
            Instant::now() + interval.clamp(Duration::from_millis(250), Duration::from_secs(2)),
        );
    }

    pub(super) fn poll(
        &mut self,
        shared: &Arc<Mutex<ViewerRuntimeLiveServer>>,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        if let Some(receiver) = &self.result {
            match receiver.try_recv() {
                Ok(mut completion) => {
                    self.result = None;
                    let mut trace = completion.trace.take();
                    if let Some(trace) = trace.as_mut() {
                        trace.mailbox_wait_ms = PeriodicIoTrace::elapsed_ms(
                            trace.worker_completed_at,
                            Some(Instant::now()),
                        );
                    }
                    let mut server = match lock_shared_server(shared) {
                        Ok(server) => server,
                        Err(error) => {
                            if let Some(trace) = trace.as_mut() {
                                trace.apply_result = TraceApplyResult::ServerLockError;
                            }
                            self.trace = trace;
                            return Err(error);
                        }
                    };
                    let interval = server.config.chain_poll_interval;
                    self.defer(interval);
                    if !completion.token.matches(&server, session) {
                        // Another verified read/configuration/session advanced
                        // while I/O ran. Re-read its cursor; never publish gaps.
                        if let Some(trace) = trace.as_mut() {
                            trace.apply_result = TraceApplyResult::StaleToken;
                        }
                        self.trace = trace;
                        return Ok(());
                    }
                    let mut prepared = match completion.outcome {
                        Ok(prepared) => prepared,
                        Err(_) => {
                            server.set_latest_player_gameplay_feedback(SelfFeedback::failure());
                            if let Some(trace) = trace.as_mut() {
                                trace.apply_result = TraceApplyResult::ReadError;
                            }
                            self.trace = trace;
                            return Err(ViewerRuntimeLiveServerError::Init(
                                "periodic service read deferred; original intents retained".into(),
                            ));
                        }
                    };
                    let Some(next) = prepared.verified_view.as_ref() else {
                        if let Some(trace) = trace.as_mut() {
                            trace.apply_result = TraceApplyResult::MissingView;
                        }
                        self.trace = trace;
                        return Err(ViewerRuntimeLiveServerError::Init(
                            "periodic service read lacks verified view".into(),
                        ));
                    };
                    let baseline = completion.token.baseline.continuation();
                    let cursor = next.continuation();
                    if cursor.stream_id != baseline.stream_id
                        || cursor.scope_id != baseline.scope_id
                        || cursor.era != baseline.era
                        || cursor.sequence < baseline.sequence
                    {
                        if let Some(trace) = trace.as_mut() {
                            trace.apply_result = TraceApplyResult::CursorRejected;
                        }
                        self.trace = trace;
                        return Ok(());
                    }
                    let satisfies_minimum = match next
                        .version()
                        .commit
                        .satisfies_minimum(&completion.token.baseline.version().commit)
                    {
                        Ok(satisfied) => satisfied,
                        Err(_) => {
                            if let Some(trace) = trace.as_mut() {
                                trace.apply_result = TraceApplyResult::BindingError;
                            }
                            self.trace = trace;
                            return Err(ViewerRuntimeLiveServerError::Init(
                                "periodic service binding changed; re-read required".into(),
                            ));
                        }
                    };
                    if !satisfies_minimum {
                        if let Some(trace) = trace.as_mut() {
                            trace.apply_result = TraceApplyResult::CursorRejected;
                        }
                        self.trace = trace;
                        return Ok(());
                    }
                    // Only originals still present may be reconciled. New
                    // admissions and changed payloads remain pending intact.
                    prepared.intent_results.retain(|result| {
                        completion.originals.iter().any(|original| {
                            original.0 == result.correlation
                                && server.pending_world_service_gameplay.iter().any(|current| {
                                    current.0 == original.0 && current.1 == original.1
                                })
                        })
                    });
                    if server
                        .latest_player_gameplay_feedback
                        .as_ref()
                        .is_some_and(|feedback| feedback.action == "world_service_read")
                    {
                        server.latest_player_gameplay_feedback = None;
                    }
                    let apply_started_at = trace.as_ref().map(|_| Instant::now());
                    let dispatch = match server.apply_chain_linked_runtime_update(prepared, session)
                    {
                        Ok(dispatch) => dispatch,
                        Err(error) => {
                            if let Some(trace) = trace.as_mut() {
                                trace.apply_result = TraceApplyResult::ApplyError;
                                trace.apply_ms = PeriodicIoTrace::elapsed_ms(
                                    apply_started_at,
                                    Some(Instant::now()),
                                );
                            }
                            self.trace = trace;
                            return Err(error);
                        }
                    };
                    if let Some(trace) = trace.as_mut() {
                        trace.apply_ms =
                            PeriodicIoTrace::elapsed_ms(apply_started_at, Some(Instant::now()));
                        trace.apply_result = if dispatch.advanced {
                            TraceApplyResult::Advanced
                        } else {
                            TraceApplyResult::NoAdvance
                        };
                    }
                    for response in dispatch.responses {
                        let is_snapshot = matches!(&response, ViewerResponse::Snapshot { .. });
                        if let Err(error) = send_response(writer, &response) {
                            if let Some(trace) = trace.as_mut() {
                                trace.buffer_result = TraceBufferResult::Error;
                            }
                            self.trace = trace;
                            return Err(error);
                        }
                        if let Some(trace) = trace.as_mut() {
                            trace.buffer_result = TraceBufferResult::Ok;
                        }
                        if is_snapshot && let Some(trace) = trace.as_mut() {
                            trace.snapshot_count = trace.snapshot_count.saturating_add(1);
                        }
                    }
                    self.trace = trace;
                }
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => {
                    self.result = None;
                    if let Some(job_sequence) = self.pending_job_sequence()
                        && let Some(connection) = &self.connection
                    {
                        connection.clear_pending_job(job_sequence);
                    }
                    let mut server = lock_shared_server(shared)?;
                    if let (Some(current), Some(owned)) =
                        (&server.periodic_service_executor, &self.executor)
                        && Arc::ptr_eq(current, owned)
                    {
                        server.periodic_service_executor = None;
                    }
                    self.executor = None;
                    self.defer(server.config.chain_poll_interval);
                    server.set_latest_player_gameplay_feedback(SelfFeedback::failure());
                    return Err(ViewerRuntimeLiveServerError::Init(
                        "periodic service worker unavailable; original intents retained".into(),
                    ));
                }
            }
        }
        if self
            .retry_at
            .is_some_and(|deadline| Instant::now() < deadline)
        {
            return Ok(());
        }
        let mut server = lock_shared_server(shared)?;
        if server.authoritative_recovery_write_fence.is_some() || !session.initial_snapshot_sent {
            return Ok(());
        }
        let Some(config) = server.config.world_service.clone() else {
            return Ok(());
        };
        let Some(baseline) = server.verified_world_view.clone() else {
            return Ok(());
        };
        if !session.should_poll_chain(server.config.chain_poll_interval) {
            return Ok(());
        }
        let executor = match &server.periodic_service_executor {
            Some(executor) => executor.clone(),
            None => {
                let executor = Executor::new()?;
                server.periodic_service_executor = Some(executor.clone());
                executor
            }
        };
        let Some(permit) = Permit::acquire(&executor.outstanding) else {
            self.defer(server.config.chain_poll_interval);
            return Ok(());
        };
        let (destination, result) = mpsc::sync_channel(1);
        let pending = server
            .pending_world_service_gameplay
            .iter()
            .take(32)
            .cloned()
            .collect::<Pending>();
        let trace = create_trace(
            TraceCursor::from_view(&baseline),
            executor.outstanding.load(Ordering::Acquire),
            pending.len(),
            self.connection.as_ref(),
        );
        let job_sequence = trace.as_ref().map(|trace| trace.identity.sequence);
        let admission = trace.as_ref().map(PeriodicIoTrace::admission_event);
        let job = Job {
            token: Token {
                config,
                baseline,
                player: session.current_player_id.clone(),
                streams: session.subscribed.clone(),
                event_filters: session.event_filters.clone(),
                protocol: session.negotiated_protocol.clone(),
            },
            pending,
            query_state: server.world_service_query_state.clone(),
            destination,
            permit,
            trace,
        };
        match executor.sender.try_send(job) {
            Ok(()) => {
                self.result = Some(result);
                self.executor = Some(executor);
                if let (Some(connection), Some(job_sequence)) = (&self.connection, job_sequence) {
                    connection.set_pending_job(job_sequence);
                }
                drop(server);
                if let Some(admission) = admission {
                    admission.emit();
                }
            }
            Err(TrySendError::Full(_)) => self.defer(server.config.chain_poll_interval),
            Err(TrySendError::Disconnected(_)) => {
                server.periodic_service_executor = None;
                self.defer(server.config.chain_poll_interval);
                return Err(ViewerRuntimeLiveServerError::Init(
                    "periodic service worker unavailable; original intents retained".into(),
                ));
            }
        }
        Ok(())
    }
}

impl SessionPoll {
    fn pending_job_sequence(&self) -> Option<u64> {
        self.connection
            .as_ref()
            .map(ConnectionTrace::pending_at_close)
            .filter(|pending| pending.present)
            .map(|pending| pending.job_sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_mailboxes_hold_admission_until_consumed_or_disconnected() {
        let outstanding = Arc::new(AtomicUsize::new(0));
        let first = Permit::acquire(&outstanding).unwrap();
        let second = Permit::acquire(&outstanding).unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        sender
            .try_send(first)
            .unwrap_or_else(|_| panic!("mailbox accepts its sole result"));
        assert!(
            Permit::acquire(&outstanding).is_none(),
            "completed but unread results consume capacity"
        );
        drop(receiver);
        let recovered =
            Permit::acquire(&outstanding).expect("disconnect releases mailbox capacity");
        assert!(Permit::acquire(&outstanding).is_none());
        drop(second);
        drop(recovered);
        assert_eq!(outstanding.load(Ordering::Acquire), 0);
    }

    #[test]
    fn configuration_fence_rejects_trust_scope_world_and_private_identity_drift() {
        let config = WorldServiceClientConfig {
            formal_read_trust: None,
            endpoint: "http://127.0.0.1:1".into(),
            trusted_service_public_key: "service".into(),
            expected_world: oasis7_client_api::world_service::WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
            },
            scope_id: "public".into(),
            read_private_key_hex: "private".into(),
            timeout: Duration::from_secs(1),
            max_response_bytes: 1024,
        };
        assert!(same_config(&config, &config.clone()));
        for index in 0..5 {
            let mut changed = config.clone();
            match index {
                0 => changed.endpoint.push('2'),
                1 => changed.trusted_service_public_key.push('2'),
                2 => changed.expected_world.genesis_digest.push('2'),
                3 => changed.scope_id.push('2'),
                _ => changed.read_private_key_hex.push('2'),
            }
            assert!(!same_config(&config, &changed));
        }
    }

    #[test]
    fn lifecycle_events_cover_a_blocked_read_and_its_queued_terminal() {
        let cursor = TraceCursor {
            commit_position: 4,
            event_sequence: 7,
        };
        let (trace, observations) = PeriodicIoTrace::for_test(cursor);
        let admission = trace.admission_event();
        let (job_tx, job_rx) = mpsc::sync_channel(1);
        job_tx
            .try_send(trace)
            .unwrap_or_else(|_| panic!("worker queue accepts the admitted test job"));
        admission.emit();

        let (read_started_tx, read_started_rx) = mpsc::sync_channel(1);
        let (release_read_tx, release_read_rx) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let mut trace = Some(job_rx.recv().unwrap());
            let outcome = run_periodic_read(&mut trace, || {
                read_started_tx.send(()).unwrap();
                release_read_rx
                    .recv_timeout(Duration::from_secs(2))
                    .expect("test releases the blocked read");
                Err(ViewerRuntimeLiveServerError::Init(
                    "fixed test failure".into(),
                ))
            });
            assert!(outcome.is_err());
            let terminal = trace.as_ref().map(PeriodicIoTrace::terminal_event);
            let (completion_tx, completion_rx) = mpsc::sync_channel(1);
            send_traced_completion(&completion_tx, (), terminal);
            completion_rx.recv().expect("test completion was queued");
        });

        read_started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("worker reaches the read stage");
        assert_eq!(
            *observations.lock().unwrap(),
            vec![
                TraceObservation {
                    stage: TraceStage::Admitted,
                    status: "accepted",
                },
                TraceObservation {
                    stage: TraceStage::Started,
                    status: "running",
                },
            ],
            "a pending read must already expose admission and worker start"
        );

        release_read_tx.send(()).unwrap();
        worker.join().unwrap();
        let observed = observations.lock().unwrap();
        assert_eq!(
            observed.len(),
            3,
            "one read emits at most four lifecycle events"
        );
        assert_eq!(observed[0].stage, TraceStage::Admitted);
        assert_eq!(observed[1].stage, TraceStage::Started);
        assert_eq!(observed[2].stage, TraceStage::Terminal);
        assert_eq!(observed[2].status, "queued");
    }

    #[test]
    fn delivery_event_uses_the_actual_response_outbox_result() {
        for expect_success in [true, false] {
            let config =
                ViewerRuntimeLiveServerConfig::new(oasis7_simulator::WorldScenario::Minimal);
            let mut output = super::super::response_outbox::ResponseOutbox::new(
                super::super::response_outbox::ResponseLimits::from_config(&config),
            );
            output.write_all(b"snapshot\n").unwrap();
            output.flush().unwrap();
            if !expect_success {
                let oversized = vec![b'x'; config.response_frame_max_bytes + 1];
                assert!(output.write_all(&oversized).is_err());
            }

            let (mut socket, mut peer) = local_tcp_pair();
            peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let (mut trace, observations) = PeriodicIoTrace::for_test(TraceCursor {
                commit_position: 1,
                event_sequence: 1,
            });
            trace.snapshot_count = 1;
            trace.buffer_result = if expect_success {
                TraceBufferResult::Ok
            } else {
                TraceBufferResult::Error
            };
            let mut poll = SessionPoll::default();
            poll.trace = Some(trace);

            let delivery = poll.deliver_output(&mut output, &mut socket);
            assert_eq!(delivery.is_ok(), expect_success);
            let mut received = [0_u8; 9];
            peer.read_exact(&mut received).unwrap();
            assert_eq!(&received, b"snapshot\n");
            assert_eq!(
                *observations.lock().unwrap(),
                vec![TraceObservation {
                    stage: TraceStage::ConsumedDelivery,
                    status: if expect_success {
                        "write_success"
                    } else {
                        "write_error"
                    },
                }]
            );
        }
    }

    #[test]
    fn disabled_trace_creates_no_lifecycle_and_delivery_still_runs() {
        assert!(
            create_trace(
                TraceCursor {
                    commit_position: 1,
                    event_sequence: 1,
                },
                1,
                0,
                None,
            )
            .is_none()
        );

        let mut poll = SessionPoll::default();
        let config = ViewerRuntimeLiveServerConfig::new(oasis7_simulator::WorldScenario::Minimal);
        let mut output = super::super::response_outbox::ResponseOutbox::new(
            super::super::response_outbox::ResponseLimits::from_config(&config),
        );
        output.write_all(b"ok\n").unwrap();
        output.flush().unwrap();
        let (mut socket, mut peer) = local_tcp_pair();
        poll.deliver_output(&mut output, &mut socket).unwrap();
        let mut received = [0_u8; 3];
        peer.read_exact(&mut received).unwrap();
        assert_eq!(&received, b"ok\n");
        assert!(poll.trace.is_none());
    }

    fn local_tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (peer, _) = listener.accept().unwrap();
        socket.set_nodelay(true).unwrap();
        (socket, peer)
    }
}

struct SelfFeedback;
impl SelfFeedback {
    fn failure() -> PlayerGameplayRecentFeedback {
        ViewerRuntimeLiveServer::make_player_gameplay_feedback(
            "world_service_read",
            "blocked",
            "periodic service read deferred; original intents retained",
            None,
            None,
            None,
            Some("wait for authenticated read/Lookup recovery".into()),
            0,
            0,
        )
    }
}

#[cfg(any(test, feature = "test_tier_required"))]
#[path = "periodic_service_io_test_drive.rs"]
mod test_drive;
