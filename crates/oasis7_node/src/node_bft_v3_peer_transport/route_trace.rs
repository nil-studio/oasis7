//! Opt-in test-hooks observations. Collection never waits or prints on hot paths.
use super::*;
use std::io::Write;
use std::sync::{OnceLock, TryLockError, atomic::AtomicU64};
const RING: usize = 128;
const MESSAGES: usize = 512;
static LOST: [AtomicU64; 257] = [const { AtomicU64::new(0) }; 257];
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
static UNTRACKED_ROUTES: AtomicU64 = AtomicU64::new(0);
pub(super) fn owner() -> u64 {
    enabled()
        .then(|| NEXT_OWNER.fetch_add(1, Ordering::Relaxed))
        .unwrap_or(0)
}
static NEXT_PARCEL: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy)]
pub(super) struct ParcelTrace {
    pub started: Instant,
    pub kind: &'static str,
    ordinal: u64,
}
pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("OASIS7_BFT_MEASURE").as_deref() == Ok("1"))
}
pub(super) fn kind(m: &BftPublicPeerMessageV3) -> &'static str {
    match m {
        BftPublicPeerMessageV3::Proposal(_) => "proposal",
        BftPublicPeerMessageV3::Vote(_) => "vote",
        BftPublicPeerMessageV3::Commit(_) => "commit",
        BftPublicPeerMessageV3::HistoryRequest(_) => "history_request",
        BftPublicPeerMessageV3::HistoryReply(_) => "history_reply",
    }
}
pub(super) fn parcel(m: &BftPublicPeerMessageV3) -> Option<ParcelTrace> {
    enabled().then(|| ParcelTrace {
        started: Instant::now(),
        kind: kind(m),
        ordinal: NEXT_PARCEL.fetch_add(1, Ordering::Relaxed),
    })
}
#[derive(Clone, Default)]
struct Total {
    count: u64,
    max_elapsed_us: u128,
    max_age_us: u128,
}
#[derive(Clone)]
struct Record {
    sequence: u64,
    at_us: u128,
    route: usize,
    kind: &'static str,
    stage: &'static str,
    result: &'static str,
    parcel: u64,
    message: u64,
    elapsed_us: u128,
    age_us: u128,
    remaining_us: u128,
}
struct Trace {
    origin: Instant,
    messages: BTreeMap<(usize, String), u64>,
    totals: BTreeMap<(usize, &'static str, &'static str, &'static str), Total>,
    recent: VecDeque<Record>,
    slow: VecDeque<Record>,
    sequence: u64,
    recent_evicted: u64,
    slow_evicted: u64,
    unknown: u64,
}
impl Default for Trace {
    fn default() -> Self {
        Self {
            origin: *origin(),
            messages: BTreeMap::new(),
            totals: BTreeMap::new(),
            recent: VecDeque::new(),
            slow: VecDeque::new(),
            sequence: 0,
            recent_evicted: 0,
            slow_evicted: 0,
            unknown: 0,
        }
    }
}
impl Trace {
    fn message(&mut self, route: usize, digest: Option<&str>) -> u64 {
        let Some(digest) = digest else {
            self.unknown += 1;
            return 0;
        };
        let key = (route, digest.to_owned());
        if let Some(id) = self.messages.get(&key) {
            return *id;
        }
        if self.messages.len() >= MESSAGES || route >= 256 {
            self.unknown += 1;
            return 0;
        }
        let id = self.messages.len() as u64 + 1;
        self.messages.insert(key, id);
        id
    }
    fn insert(&mut self, mut record: Record) {
        self.sequence += 1;
        record.sequence = self.sequence;
        let total = self
            .totals
            .entry((record.route, record.kind, record.stage, record.result))
            .or_default();
        total.count += 1;
        total.max_elapsed_us = total.max_elapsed_us.max(record.elapsed_us);
        total.max_age_us = total.max_age_us.max(record.age_us);
        let failure = !matches!(
            record.result,
            "queued" | "sending" | "ack" | "received" | "delivered"
        );
        if failure || record.elapsed_us >= 1_000_000 || record.age_us >= 10_000_000 {
            if self.slow.len() == RING {
                self.slow.pop_front();
                self.slow_evicted += 1;
            }
            self.slow.push_back(record.clone());
        }
        if self.recent.len() == RING {
            self.recent.pop_front();
            self.recent_evicted += 1;
        }
        self.recent.push_back(record);
    }
}
fn origin() -> &'static Instant {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now)
}
fn trace(route: usize) -> &'static Mutex<Trace> {
    static TRACES: OnceLock<[Mutex<Trace>; 257]> = OnceLock::new();
    &TRACES.get_or_init(|| std::array::from_fn(|_| Mutex::new(Trace::default())))[route.min(256)]
}
fn try_observe<T>(
    collector: &Mutex<Trace>,
    lost: &AtomicU64,
    observe: impl FnOnce(&mut Trace) -> T,
) -> Option<T> {
    match collector.try_lock() {
        Ok(mut trace) => Some(observe(&mut trace)),
        Err(TryLockError::Poisoned(error)) => Some(observe(&mut error.into_inner())),
        Err(TryLockError::WouldBlock) => {
            lost.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}
// The directed Noise route is internal. Only its bounded temporary ordinal is rendered.
pub(super) fn route(from: PeerId, to: PeerId) -> usize {
    static ROUTES: OnceLock<Mutex<BTreeMap<(PeerId, PeerId), usize>>> = OnceLock::new();
    if !enabled() {
        return 256;
    }
    let routes = ROUTES.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut routes = match routes.try_lock() {
        Ok(routes) => routes,
        Err(TryLockError::Poisoned(error)) => error.into_inner(),
        Err(TryLockError::WouldBlock) => {
            LOST[256].fetch_add(1, Ordering::Relaxed);
            return 256;
        }
    };
    if let Some(index) = routes.get(&(from, to)) {
        return *index;
    }
    let index = routes.len();
    if index >= 256 {
        UNTRACKED_ROUTES.fetch_add(1, Ordering::Relaxed);
        return 256;
    }
    routes.insert((from, to), index);
    index
}
#[expect(
    clippy::too_many_arguments,
    reason = "fixed observation fields carry no message authority"
)]
pub(super) fn event_message(
    route: usize,
    kind: &'static str,
    stage: &'static str,
    result: &'static str,
    elapsed: Duration,
    age: Duration,
    remaining: Duration,
    parcel: u64,
    digest: Option<&str>,
) {
    if !enabled() {
        return;
    }
    let now = Instant::now();
    try_observe(trace(route), &LOST[route.min(256)], |trace| {
        let message = trace.message(route, digest);
        let at_us = now.saturating_duration_since(trace.origin).as_micros();
        trace.insert(Record {
            sequence: 0,
            at_us,
            route,
            kind,
            stage,
            result,
            parcel,
            message,
            elapsed_us: elapsed.as_micros(),
            age_us: age.as_micros(),
            remaining_us: remaining.as_micros(),
        });
    });
}
pub(super) fn event(
    route: usize,
    kind: &'static str,
    stage: &'static str,
    result: &'static str,
    elapsed: Duration,
    age: Duration,
    remaining: Duration,
) {
    event_message(route, kind, stage, result, elapsed, age, remaining, 0, None);
}
pub(super) fn parcel_event(
    route: usize,
    p: &Parcel,
    stage: &'static str,
    result: &'static str,
    elapsed: Duration,
) {
    if let Some(t) = p.measurement {
        event_message(
            route,
            t.kind,
            stage,
            result,
            elapsed,
            t.started.elapsed(),
            p.deadline.saturating_duration_since(Instant::now()),
            t.ordinal,
            Some(&p.digest),
        );
    }
}
struct Snapshot {
    records: Vec<Record>,
    totals: BTreeMap<(usize, &'static str, &'static str, &'static str), Total>,
    sequence: u64,
    recent_evicted: u64,
    slow_evicted: u64,
    unknown: u64,
}
fn snapshot(collector: &Mutex<Trace>, lost: &AtomicU64) -> Option<Snapshot> {
    try_observe(collector, lost, |trace| {
        let mut records: Vec<_> = trace.recent.drain(..).collect();
        records.extend(trace.slow.drain(..));
        records.sort_by_key(|record| record.sequence);
        records.dedup_by_key(|record| record.sequence);
        Snapshot {
            records,
            totals: trace.totals.clone(),
            sequence: trace.sequence,
            recent_evicted: trace.recent_evicted,
            slow_evicted: trace.slow_evicted,
            unknown: trace.unknown,
        }
    })
}
fn collect_owned<'a>(
    owned: &[usize],
    lookup: impl Fn(usize) -> (&'a Mutex<Trace>, &'a AtomicU64),
) -> Vec<(usize, Option<Snapshot>)> {
    owned
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|route| {
            let (collector, lost) = lookup(route);
            (route, snapshot(collector, lost))
        })
        .collect()
}
pub(super) fn report(owner: u64, owned: &[usize]) {
    if !enabled() {
        return;
    }
    // Only this transport's directed routes are drained. Another fixture's
    // Drop cannot take its events; snapshots remain cumulative per route.
    let snapshots = collect_owned(owned, |route| (trace(route), &LOST[route.min(256)]));
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(
        stderr,
        "BFT_ROUTE_OWNER owner={owner} routes={owned:?} at_us={} untracked_routes={}",
        origin().elapsed().as_micros(),
        UNTRACKED_ROUTES.load(Ordering::Relaxed)
    );
    for (route, snapshot) in snapshots {
        let Some(snapshot) = snapshot else {
            let _ = writeln!(
                stderr,
                "BFT_ROUTE_SNAPSHOT owner={owner} route={route} available=0 lost={}",
                LOST[route.min(256)].load(Ordering::Relaxed)
            );
            continue;
        };
        for r in snapshot.records {
            let _ = writeln!(
                stderr,
                "BFT_ROUTE owner={owner} at_us={} seq={} route={} kind={} stage={} result={} parcel={} message={} elapsed_us={} age_us={} remaining_us={}",
                r.at_us,
                r.sequence,
                r.route,
                r.kind,
                r.stage,
                r.result,
                r.parcel,
                r.message,
                r.elapsed_us,
                r.age_us,
                r.remaining_us
            );
        }
        for ((route, kind, stage, result), total) in snapshot.totals {
            let _ = writeln!(
                stderr,
                "BFT_ROUTE_TOTAL owner={owner} route={route} kind={kind} stage={stage} result={result} count={} max_elapsed_us={} max_age_us={}",
                total.count, total.max_elapsed_us, total.max_age_us
            );
        }
        let _ = writeln!(
            stderr,
            "BFT_ROUTE_SNAPSHOT owner={owner} route={route} available=1 sequence={} ring_limit={RING} message_limit={MESSAGES} recent_evicted={} slow_evicted={} unknown={} lost={}",
            snapshot.sequence,
            snapshot.recent_evicted,
            snapshot.slow_evicted,
            snapshot.unknown,
            LOST[route.min(256)].load(Ordering::Relaxed)
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn record(age_us: u128, result: &'static str) -> Record {
        Record {
            sequence: 0,
            at_us: 1,
            route: 0,
            kind: "vote",
            stage: "send",
            result,
            parcel: 1,
            message: 1,
            elapsed_us: 0,
            age_us,
            remaining_us: 0,
        }
    }
    #[test]
    fn route_trace_recent_and_slow_rings_remain_bounded_and_count_evictions() {
        let mut trace = Trace::default();
        for _ in 0..RING + 3 {
            trace.insert(record(0, "rate_limited"));
        }
        for _ in 0..RING {
            trace.insert(record(0, "ack"));
        }
        assert_eq!(trace.recent.len(), RING);
        assert_eq!(trace.slow.len(), RING);
        assert_eq!(trace.recent_evicted, (RING + 3) as u64);
        assert_eq!(trace.slow_evicted, 3);
        assert!(trace.slow.iter().all(|r| r.result == "rate_limited"));
        assert_eq!(trace.sequence, (RING * 2 + 3) as u64);
    }
    #[test]
    fn route_trace_contention_counts_unknown_observation_without_waiting() {
        let collector = Mutex::new(Trace::default());
        let lost = AtomicU64::new(0);
        let held = collector.lock().unwrap();
        assert!(
            try_observe(&collector, &lost, |_| panic!(
                "must not acquire held collector"
            ))
            .is_none()
        );
        assert_eq!(lost.load(Ordering::Relaxed), 1);
        drop(held);
        assert!(try_observe(&collector, &lost, |trace| trace.insert(record(0, "ack"))).is_some());
    }
    #[test]
    fn route_trace_message_capacity_reports_unknown_and_duplicate_content_shares_only_message_id() {
        let mut trace = Trace::default();
        let first = trace.message(0, Some("internal-test-value"));
        assert_eq!(trace.message(0, Some("internal-test-value")), first);
        for n in 1..MESSAGES {
            assert_ne!(trace.message(0, Some(&n.to_string())), 0);
        }
        assert_eq!(trace.message(0, Some("overflow")), 0);
        assert_eq!(trace.unknown, 1);
        assert_eq!(trace.message(0, Some("internal-test-value")), first);
        assert_eq!(trace.message(0, None), 0);
        assert_eq!(trace.unknown, 2);
        assert_eq!(trace.messages.len(), MESSAGES);
    }
    #[test]
    fn route_trace_budgets_are_isolated_between_routes() {
        let mut first = Trace::default();
        let mut second = Trace::default();
        for n in 0..MESSAGES + 1 {
            first.message(0, Some(&n.to_string()));
        }
        for _ in 0..RING + 1 {
            first.insert(record(0, "rate_limited"));
        }
        assert_eq!(second.message(1, Some("other-route")), 1);
        let mut event = record(0, "ack");
        event.route = 1;
        second.insert(event);
        assert_eq!(first.unknown, 1);
        assert_eq!(first.slow_evicted, 1);
        assert_eq!(second.unknown, 0);
        assert_eq!(second.recent_evicted, 0);
        assert_eq!(second.recent.len(), 1);
    }
    #[test]
    fn route_trace_snapshot_cannot_drain_another_routes_events() {
        let first = Mutex::new(Trace::default());
        let second = Mutex::new(Trace::default());
        let lost = AtomicU64::new(0);
        first.lock().unwrap().insert(record(0, "ack"));
        second.lock().unwrap().insert(record(0, "rate_limited"));
        let snapshots = collect_owned(&[0, 0], |route| {
            if route == 0 {
                (&first, &lost)
            } else {
                (&second, &lost)
            }
        });
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].1.as_ref().unwrap().records.len(), 1);
        assert!(first.lock().unwrap().recent.is_empty());
        assert_eq!(second.lock().unwrap().recent.len(), 1);
        assert_eq!(second.lock().unwrap().slow.len(), 1);
        assert_eq!(snapshot(&second, &lost).unwrap().records.len(), 1);
        assert_eq!(
            first.lock().unwrap().totals.values().next().unwrap().count,
            1
        );
    }
}
