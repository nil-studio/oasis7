//! A single sealed first-wait window. No printing or blocking collection.
use super::*;
const PRELUDE: usize = 128;
const EVENTS: usize = 2048;
const CAPACITY: usize = PRELUDE + EVENTS;
// Only registered routes allocate a window; the untracked sentinel never does.
const ROUTES: usize = 256;
pub(super) const STAGE_COUNT: usize = 28;
const WORKER_STAGES: [&str; STAGE_COUNT] = [
    "worker.loop",
    "worker.poll",
    "worker.drain",
    "signer.lock_wait",
    "signer.proposal_rpc",
    "signer.vote_rpc",
    "commit.total",
    "commit.candidate",
    "commit.append_rpc",
    "commit.hook",
    "commit.confirm_rpc",
    "commit.install_lock_wait",
    "commit.install",
    "commit.start_timer",
    "commit.restore",
    "commit.status",
    "commit.hook_lock_wait",
    "worker.propose",
    "worker.broadcast",
    "worker.enqueue",
    "worker.fetch_poll",
    "worker.fetch_lifetime",
    "worker.materialize",
    "worker.accept_materialized",
    "worker.input",
    "worker.queued",
    "worker.selected",
    "worker.journal_advanced",
];
const MAX_RECORD_BYTES: usize = ROUTES * CAPACITY * std::mem::size_of::<Option<Record>>();
// Each route has one Arc allocation (snapshots clone only its pointer). Include
// Arc refcounts, Window metadata and up to 256 snapshot pointer slots per owner.
const MAX_WINDOW_BYTES: usize = 257 * std::mem::size_of::<[Option<Record>; STAGE_COUNT]>()
    + 259 * std::mem::size_of::<AtomicU64>()
    + MAX_RECORD_BYTES
    + ROUTES * (std::mem::size_of::<Window>() + 2 * std::mem::size_of::<usize>())
    + ROUTES * ROUTES * std::mem::size_of::<(usize, Option<Arc<Window>>)>();
pub(super) struct Window {
    records: Box<[Option<Record>]>,
    prelude: usize,
    count: usize,
    unknown: u64,
    overflow: u64,
    worker_totals: [Total; STAGE_COUNT],
    active: [Option<Record>; STAGE_COUNT],
}
impl Window {
    fn new(trace: &Trace) -> Self {
        let recent = &trace.recent;
        let mut records = vec![None; CAPACITY].into_boxed_slice();
        let prelude = recent.len().min(PRELUDE);
        for (index, record) in recent.iter().skip(recent.len() - prelude).enumerate() {
            records[index] = Some(record.clone());
        }
        Self {
            records,
            prelude,
            count: 0,
            unknown: recent
                .iter()
                .skip(recent.len() - prelude)
                .filter(|r| r.message == 0 && expects_message(r))
                .count() as u64,
            overflow: 0,
            worker_totals: std::array::from_fn(|_| Total::default()),
            active: trace
                .worker_active
                .as_deref()
                .cloned()
                .unwrap_or_else(|| std::array::from_fn(|_| None)),
        }
    }
    fn insert(&mut self, record: &Record) {
        self.unknown += u64::from(record.message == 0 && expects_message(record));
        if record.kind == "worker"
            && let Some(index) = WORKER_STAGES
                .iter()
                .position(|stage| *stage == record.stage)
        {
            self.active[index] = if record.result == "begin" {
                Some(record.clone())
            } else {
                None
            };
            if record.result != "begin" {
                let total = &mut self.worker_totals[index];
                total.count += 1;
                total.max_elapsed_us = total.max_elapsed_us.max(record.elapsed_us);
            }
        }
        // Fast loop/poll/drain/lock spans use bounded per-stage totals, leaving
        // the event budget for messages and synchronous commit boundaries.
        if !detailed(record) {
            return;
        }
        if self.count == EVENTS {
            self.overflow += 1;
            return;
        }
        self.records[PRELUDE + self.count] = Some(record.clone());
        self.count += 1;
    }
}
fn micros(scope: &NodeBftRouteTraceScopeV3, now: Instant) -> u64 {
    u64::try_from(
        now.saturating_duration_since(scope.inner.origin)
            .as_micros(),
    )
    .unwrap_or(u64::MAX)
    .max(1)
}
fn contains(scope: &NodeBftRouteTraceScopeV3, at: u64) -> bool {
    let begin = scope.inner.window_start.load(Ordering::Acquire);
    let end = scope.inner.window_end.load(Ordering::Acquire);
    begin != 0 && at >= begin && (end == 0 || at <= end)
}
pub(super) fn begin(scope: &NodeBftRouteTraceScopeV3) {
    // One shot: subsequent phases cannot reopen or replace the first window.
    if scope
        .inner
        .window_start
        .compare_exchange(
            0,
            micros(scope, Instant::now()),
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return;
    }
    // Walk the bounded slots, not the route registry, so setup never waits on it.
    for (route, collector) in scope.inner.traces.iter().take(ROUTES).enumerate() {
        try_observe(collector, &scope.inner.window_lost[route], |trace| {
            if (!trace.recent.is_empty() || trace.worker_active.is_some()) && trace.window.is_none()
            {
                trace.window = Some(Arc::new(Window::new(trace)));
            }
        });
    }
}
pub(super) fn end(scope: &NodeBftRouteTraceScopeV3) {
    let _ = scope.inner.window_end.compare_exchange(
        0,
        micros(scope, Instant::now()),
        Ordering::AcqRel,
        Ordering::Acquire,
    );
}
fn expects_message(record: &Record) -> bool {
    record.kind != "worker"
        || matches!(
            record.stage,
            "worker.enqueue"
                | "worker.fetch_lifetime"
                | "worker.materialize"
                | "worker.accept_materialized"
                | "worker.input"
                | "worker.queued"
                | "worker.selected"
                | "worker.journal_advanced"
        )
}
pub(super) fn detailed(record: &Record) -> bool {
    expects_message(record)
        || (record.stage.starts_with("commit.") && record.stage != "commit.status")
        || record.elapsed_us >= 100_000
}
pub(super) fn worker_active(trace: &mut Trace, record: &Record) {
    if record.kind != "worker" {
        return;
    }
    if let Some(index) = WORKER_STAGES
        .iter()
        .position(|stage| *stage == record.stage)
    {
        let active = trace
            .worker_active
            .get_or_insert_with(|| Box::new(std::array::from_fn(|_| None)));
        active[index] = if record.result == "begin" {
            Some(record.clone())
        } else {
            None
        };
    }
}
pub(super) fn observe(scope: &NodeBftRouteTraceScopeV3, trace: &mut Trace, record: &Record) {
    if record.route >= ROUTES
        || scope.inner.window_end.load(Ordering::Acquire) != 0
        || !contains(scope, record.at_us as u64)
    {
        return;
    }
    if trace.window.is_none() {
        trace.window = Some(Arc::new(Window::new(trace)));
    }
    let window = trace.window.as_mut().unwrap();
    if let Some(window) = Arc::get_mut(window) {
        window.insert(record);
    } else {
        scope.inner.window_lost[record.route].fetch_add(1, Ordering::Relaxed);
    }
}
pub(super) fn lost(scope: &NodeBftRouteTraceScopeV3, route: usize, at: Instant) {
    if contains(scope, micros(scope, at)) {
        scope.inner.window_lost[route.min(256)].fetch_add(1, Ordering::Relaxed);
    }
}
pub(super) fn snapshots(
    scope: &NodeBftRouteTraceScopeV3,
    owned: &[usize],
) -> Vec<(usize, Option<Arc<Window>>)> {
    owned
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|route| {
            let window = try_observe(
                trace(scope, route),
                &scope.inner.window_lost[route.min(256)],
                |trace| trace.window.clone(),
            )
            .flatten();
            (route, window)
        })
        .collect()
}
pub(super) fn render(
    scope: &NodeBftRouteTraceScopeV3,
    owner: u64,
    snapshots: Vec<(usize, Option<Arc<Window>>)>,
    output: &mut impl Write,
) {
    for (route, window) in snapshots {
        let lost = scope.inner.window_lost[route.min(256)].load(Ordering::Relaxed);
        let Some(window) = window else {
            let _ = writeln!(
                output,
                "BFT_WINDOW scope={} owner={owner} route={route} available=0 lost={lost}",
                scope.inner.ordinal
            );
            continue;
        };
        let _ = writeln!(
            output,
            "BFT_WINDOW scope={} owner={owner} route={route} available=1 start_us={} end_us={} prelude={} count={} unknown={} lost={lost} overflow={} route_record_bytes={} max_record_bytes={MAX_RECORD_BYTES} max_window_bytes={MAX_WINDOW_BYTES}",
            scope.inner.ordinal,
            scope.inner.window_start.load(Ordering::Acquire),
            scope.inner.window_end.load(Ordering::Acquire),
            window.prelude,
            window.count,
            window.unknown,
            window.overflow,
            CAPACITY * std::mem::size_of::<Option<Record>>()
        );
        for active in window.active.iter().flatten() {
            let _ = writeln!(
                output,
                "BFT_WINDOW_ACTIVE scope={} owner={owner} route={route} stage={} started_us={} worker={} message={} position_known={} height={} round={} phase={}",
                scope.inner.ordinal,
                active.stage,
                active.at_us,
                active.parcel,
                active.message,
                u8::from(active.position.is_some()),
                active.position.map(|p| p.height).unwrap_or(0),
                active.position.map(|p| p.round).unwrap_or(0),
                active.position.map(|p| p.phase).unwrap_or("unknown")
            );
        }
        for (stage, total) in WORKER_STAGES.iter().zip(&window.worker_totals) {
            let _ = writeln!(
                output,
                "BFT_WINDOW_WORKER scope={} owner={owner} route={route} stage={stage} count={} max_elapsed_us={}",
                scope.inner.ordinal, total.count, total.max_elapsed_us
            );
        }
        for record in window.records.iter().flatten() {
            let position = record.position;
            let _ = writeln!(
                output,
                "BFT_WINDOW_EVENT scope={} owner={owner} route={route} at_us={} seq={} kind={} stage={} result={} parcel={} message={} elapsed_us={} age_us={} remaining_us={} position_known={} height={} round={} phase={}",
                scope.inner.ordinal,
                record.at_us,
                record.sequence,
                record.kind,
                record.stage,
                record.result,
                record.parcel,
                record.message,
                record.elapsed_us,
                record.age_us,
                record.remaining_us,
                u8::from(position.is_some()),
                position.map(|p| p.height).unwrap_or(0),
                position.map(|p| p.round).unwrap_or(0),
                position.map(|p| p.phase).unwrap_or("unknown")
            );
        }
    }
}
// The boxed guard carries only this fixture's explicit scope and numeric owner.
pub(super) struct WorkerSpan {
    scope: NodeBftRouteTraceScopeV3,
    route: usize,
    owner: u64,
    stage: &'static str,
    started: Instant,
    digest: Option<String>,
    position: Option<Position>,
}
pub(in super::super) fn worker_span(
    scope: &NodeBftRouteTraceScopeV3,
    route: usize,
    owner: u64,
    stage: &'static str,
) -> Box<dyn Send> {
    let span = WorkerSpan {
        scope: scope.clone(),
        route,
        owner,
        stage,
        started: Instant::now(),
        digest: None,
        position: None,
    };
    span.record("begin", Duration::ZERO);
    Box::new(span)
}
// Digest stays private; event_message maps it to the existing route-local ordinal.
pub(in super::super) fn message_span(
    scope: &NodeBftRouteTraceScopeV3,
    route: usize,
    owner: u64,
    stage: &'static str,
    digest: String,
    position: Option<Position>,
) -> Box<dyn Send> {
    let span = WorkerSpan {
        scope: scope.clone(),
        route,
        owner,
        stage,
        started: Instant::now(),
        digest: Some(digest),
        position,
    };
    span.record("begin", Duration::ZERO);
    Box::new(span)
}
impl WorkerSpan {
    fn record(&self, result: &'static str, elapsed: Duration) {
        event_message(
            &self.scope,
            self.route,
            "worker",
            self.stage,
            result,
            elapsed,
            Duration::ZERO,
            Duration::ZERO,
            self.owner,
            self.digest.as_deref(),
            self.position,
        );
    }
}
impl Drop for WorkerSpan {
    fn drop(&mut self) {
        self.record(
            if std::thread::panicking() {
                "unwind"
            } else {
                "end"
            },
            self.started.elapsed(),
        );
    }
}
#[cfg(test)]
#[path = "route_window_tests.rs"]
mod tests;
