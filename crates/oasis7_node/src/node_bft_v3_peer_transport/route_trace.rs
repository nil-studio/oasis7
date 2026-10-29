//! Opt-in test-hooks observations. Collection never waits or prints on hot paths.
use super::*;
use std::sync::{OnceLock, TryLockError, atomic::AtomicU64};
const RING: usize = 256;
const MESSAGES: usize = 2048;
static LOST: AtomicU64 = AtomicU64::new(0);
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
    routes: BTreeMap<(PeerId, PeerId), usize>,
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
            origin: Instant::now(),
            routes: BTreeMap::new(),
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
fn trace() -> &'static Mutex<Trace> {
    static TRACE: OnceLock<Mutex<Trace>> = OnceLock::new();
    TRACE.get_or_init(|| Mutex::new(Trace::default()))
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
    if !enabled() {
        return 256;
    }
    try_observe(trace(), &LOST, |trace| {
        if let Some(index) = trace.routes.get(&(from, to)) {
            return *index;
        }
        if trace.routes.len() >= 256 {
            trace.unknown += 1;
            return 256;
        }
        let index = trace.routes.len();
        trace.routes.insert((from, to), index);
        index
    })
    .unwrap_or(256)
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
    try_observe(trace(), &LOST, |trace| {
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
pub(super) fn report() {
    if !enabled() {
        return;
    }
    // Release the collector before formatting or stderr I/O. Rings are drained
    // once; totals remain cumulative and snapshots must never be added together.
    let Some((mut records, totals, sequence, recent_evicted, slow_evicted, unknown)) =
        try_observe(trace(), &LOST, |trace| {
            let mut records: Vec<_> = trace.recent.drain(..).collect();
            records.extend(trace.slow.drain(..));
            (
                records,
                trace.totals.clone(),
                trace.sequence,
                trace.recent_evicted,
                trace.slow_evicted,
                trace.unknown,
            )
        })
    else {
        return;
    };
    records.sort_by_key(|record| record.sequence);
    records.dedup_by_key(|record| record.sequence);
    for r in records {
        eprintln!(
            "BFT_ROUTE at_us={} seq={} route={} kind={} stage={} result={} parcel={} message={} elapsed_us={} age_us={} remaining_us={}",
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
    for ((route, kind, stage, result), total) in totals {
        eprintln!(
            "BFT_ROUTE_TOTAL route={route} kind={kind} stage={stage} result={result} count={} max_elapsed_us={} max_age_us={}",
            total.count, total.max_elapsed_us, total.max_age_us
        );
    }
    eprintln!(
        "BFT_ROUTE_SNAPSHOT sequence={sequence} ring_limit={RING} recent_evicted={recent_evicted} slow_evicted={slow_evicted} unknown={unknown} lost={}",
        LOST.load(Ordering::Relaxed)
    );
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
}
