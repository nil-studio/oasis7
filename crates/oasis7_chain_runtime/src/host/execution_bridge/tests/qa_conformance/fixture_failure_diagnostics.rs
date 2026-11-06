//! Bounded, payload-free evidence retained before listener failure aggregation.
//! No panic hook or I/O runs in a worker's unwind path. The cleanup terminator
//! prints these fixed-schema records only after arming its 100 ms exit fallback.
use std::{
    cell::RefCell,
    io::{self, Write},
    panic::Location,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

const CAPACITY: usize = 8;
const SCENARIOS: &[&str] = &[
    "fixture",
    "other",
    "checkpoint-fairness",
    "checkpoint-scope-fence",
    "checkpoint-pause-resume",
    "repeated-turns",
    "slow-consumer",
    "periodic-fairness",
    "fresh-admission",
    "fresh-metadata",
    "fresh-paused",
    "fresh-bad-store",
    "missing-store",
    "hosted-wait",
    "hosted-wait-resume-crash",
    "hosted-wait-write-failure",
    "hosted-resume-write-failure",
    "hosted-final-budget",
    "hosted-final-budget-crash",
    "hosted-final-budget-write-failure",
    "resume-rejected",
    "resume-rejection-crash",
    "resume-rejection-write-failure",
    "wait-admit-crash",
    "wait-admit-rejected",
    "wait-capture-crash",
    "wait-capture-nonempty",
    "wait-rejection-crash",
    "wait-rejection-write-failure",
    "memory-crash",
    "memory-crash-tamper",
    "memory-ack-before",
    "memory-ack-after",
    "memory-ack-write-failure",
    "memory-ack-missing-runner",
    "pending-cadence",
    "hosted",
    "metadata-probe",
    "service-probe",
    "provider-closure",
    "wake",
    "release",
    "diagnostic-probe",
];

#[derive(Clone, Copy, Debug)]
pub(super) enum Worker {
    Listener,
    Connection,
    SerialConnection,
    CaptureReader,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Stage {
    ListenerStart,
    Accept,
    Spawn,
    SerialDispatch,
    ListenerJoin,
    JoinedCallback,
    ListenerFailed,
    ConfigureSocket,
    ReadRequest,
    ParseRequest,
    Gate,
    Outage,
    ObserveRequest,
    CaptureSetup,
    CaptureRead,
    Dispatch,
    DispatcherResult,
    CaptureJoin,
    ResponseRelay,
    AckResponse,
    ControlledCommit,
    ConnectionFinished,
}
#[derive(Clone, Copy, Debug)]
enum Route {
    Unknown,
    Describe,
    View,
    Changes,
    Lookup,
    Submit,
    Other,
}
#[derive(Clone, Copy, Debug)]
enum IoKind {
    None,
    ConnectionReset,
    BrokenPipe,
    TimedOut,
    WouldBlock,
    Other,
}
#[derive(Clone, Copy, Debug)]
enum IoSource {
    None,
    Typed,
    KnownMessage,
}
#[derive(Clone, Copy)]
struct Progress {
    stage: Stage,
    route: Route,
    errno: i32,
    io_kind: IoKind,
    io_source: IoSource,
    read_bytes: usize,
    dispatcher_elapsed_us: u64,
    site: &'static Location<'static>,
}
#[derive(Clone)]
pub(super) struct Context {
    diagnostics: Arc<Diagnostics>,
    worker: usize,
    parent: usize,
    role: Worker,
    progress: Progress,
}
struct Record {
    worker: usize,
    parent: usize,
    role: Worker,
    progress: Progress,
}

pub(super) struct Diagnostics {
    scenario: AtomicUsize,
    next_worker: AtomicUsize,
    failures: AtomicUsize,
    records: [OnceLock<Record>; CAPACITY],
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            scenario: AtomicUsize::new(0),
            next_worker: AtomicUsize::new(1),
            failures: AtomicUsize::new(0),
            records: std::array::from_fn(|_| OnceLock::new()),
        }
    }
}
impl Diagnostics {
    pub(super) fn set_scenario(&self, value: &str) {
        // Never retain an arbitrary test/env label, even if it looks printable.
        let index = SCENARIOS
            .iter()
            .position(|known| *known == value)
            .unwrap_or(1);
        self.scenario.store(index, Ordering::Relaxed);
    }
    fn record(&self, context: &Context) {
        let index = self.failures.fetch_add(1, Ordering::Relaxed);
        if let Some(slot) = self.records.get(index) {
            // Each slot has one writer, so set cannot wait for another initializer.
            let _ = slot.set(Record {
                worker: context.worker,
                parent: context.parent,
                role: context.role,
                progress: context.progress,
            });
        }
    }
    pub(super) fn write(&self, writer: &mut impl Write) -> io::Result<()> {
        let scenario = SCENARIOS[self.scenario.load(Ordering::Relaxed)];
        let count = self.failures.load(Ordering::Relaxed);
        writeln!(
            writer,
            "fixture_failure_summary scenario={scenario} failures={count} capacity={CAPACITY}"
        )?;
        for (index, record) in self.records.iter().enumerate() {
            let Some(record) = record.get() else {
                continue;
            };
            let progress = record.progress;
            // The site is the last instrumented operation, not a claimed panic
            // location. Never print a source path or arbitrary panic payload.
            let source = match progress.site.file().rsplit('/').next() {
                Some("qa_conformance.rs") => "fixture",
                Some("http_fixture.rs") => "http",
                Some("fixture_failure_diagnostics_tests.rs") => "probe",
                _ => "other",
            };
            writeln!(
                writer,
                "fixture_worker_failure index={index} scenario={scenario} worker={} parent={} role={:?} stage={:?} route={:?} errno={} io_kind={:?} io_source={:?} read_bytes={} dispatcher_elapsed_us={} site={source}:{}:{}",
                record.worker,
                record.parent,
                record.role,
                progress.stage,
                progress.route,
                progress.errno,
                progress.io_kind,
                progress.io_source,
                progress.read_bytes,
                progress.dispatcher_elapsed_us,
                progress.site.line(),
                progress.site.column()
            )?;
        }
        Ok(())
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Context>> = const { RefCell::new(None) };
}
pub(super) struct Scope(Option<Context>);
impl Scope {
    #[track_caller]
    pub(super) fn listener(diagnostics: Arc<Diagnostics>) -> Self {
        Self::enter(Some(Context {
            diagnostics,
            worker: 0,
            parent: 0,
            role: Worker::Listener,
            progress: Progress {
                stage: Stage::ListenerStart,
                route: Route::Unknown,
                errno: 0,
                io_kind: IoKind::None,
                io_source: IoSource::None,
                read_bytes: 0,
                dispatcher_elapsed_us: 0,
                site: Location::caller(),
            },
        }))
    }
    pub(super) fn enter(context: Option<Context>) -> Self {
        Self(ACTIVE.with(|active| active.replace(context)))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        let _ = ACTIVE.try_with(|active| {
            if let Ok(mut active) = active.try_borrow_mut() {
                if std::thread::panicking()
                    && let Some(context) = active.as_ref()
                {
                    context.diagnostics.record(context);
                }
                *active = self.0.take();
            }
        });
    }
}
#[track_caller]
pub(super) fn child(role: Worker) -> Option<Context> {
    let site = Location::caller();
    ACTIVE.with(|active| {
        active.borrow().as_ref().map(|parent| {
            let mut context = parent.clone();
            context.parent = parent.worker;
            context.worker = parent.diagnostics.next_worker.fetch_add(1, Ordering::Relaxed);
            context.role = role;
            context.progress.site = site;
            context
        })
    })
}
#[track_caller]
pub(super) fn at(stage: Stage) {
    let site = Location::caller();
    ACTIVE.with(|active| {
        if let Some(context) = active.borrow_mut().as_mut() {
            context.progress.stage = stage;
            context.progress.errno = 0;
            context.progress.io_kind = IoKind::None;
            context.progress.io_source = IoSource::None;
            context.progress.site = site;
        }
    });
}
pub(super) fn route(path: &str) {
    ACTIVE.with(|active| {
        if let Some(context) = active.borrow_mut().as_mut() {
            context.progress.route = match path {
                super::DESCRIBE_PATH => Route::Describe,
                super::VIEW_PATH => Route::View,
                super::CHANGES_PATH => Route::Changes,
                super::LOOKUP_PATH => Route::Lookup,
                super::SUBMIT_PATH => Route::Submit,
                _ => Route::Other,
            };
        }
    });
}
#[track_caller]
pub(super) fn io_error(error: &io::Error) {
    let site = Location::caller();
    ACTIVE.with(|active| {
        if let Some(context) = active.borrow_mut().as_mut() {
            context.progress.errno = error.raw_os_error().unwrap_or(0);
            context.progress.io_source = IoSource::Typed;
            context.progress.io_kind = match error.kind() {
                io::ErrorKind::ConnectionReset => IoKind::ConnectionReset,
                io::ErrorKind::BrokenPipe => IoKind::BrokenPipe,
                io::ErrorKind::TimedOut => IoKind::TimedOut,
                io::ErrorKind::WouldBlock => IoKind::WouldBlock,
                _ => IoKind::Other,
            };
            context.progress.site = site;
        }
    });
}
#[track_caller]
pub(super) fn observe_io<T>(result: io::Result<T>) -> io::Result<T> {
    if let Err(error) = &result {
        io_error(error);
    }
    result
}

/// The dispatcher erases io::Error into String. Only exact, locally generated
/// OS messages may recover a code; no substring matching or message retention.
#[track_caller]
pub(super) fn dispatcher_error(message: &str) {
    for errno in [libc::EPIPE, libc::ECONNRESET, libc::ETIMEDOUT, libc::EAGAIN] {
        let error = io::Error::from_raw_os_error(errno);
        if message == error.to_string() {
            io_error(&error);
            ACTIVE.with(|active| {
                if let Some(context) = active.borrow_mut().as_mut() {
                    context.progress.io_source = IoSource::KnownMessage;
                }
            });
            break;
        }
    }
}

pub(super) fn dispatcher_elapsed(elapsed: std::time::Duration) {
    ACTIVE.with(|active| {
        if let Some(context) = active.borrow_mut().as_mut() {
            context.progress.dispatcher_elapsed_us =
                elapsed.as_micros().min(u64::MAX as u128) as u64;
        }
    });
}

pub(super) fn request_bytes(count: usize) {
    ACTIVE.with(|active| {
        if let Some(context) = active.borrow_mut().as_mut() {
            context.progress.read_bytes = count;
        }
    });
}

#[path = "fixture_failure_diagnostics_tests.rs"]
mod tests;
