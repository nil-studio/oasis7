//! Opt-in test-hooks observations. Collection never waits or prints on hot paths.
use super::*;
use std::io::Write;
use std::sync::{TryLockError, atomic::AtomicU64};
const RING: usize = 128;
const MESSAGES: usize = 512;
#[path = "route_window.rs"]
pub(super) mod window;
static NEXT_SCOPE: AtomicU64 = AtomicU64::new(1);
/// Explicit diagnostic scope, available only with the existing test-hooks feature.
/// Clone this same handle for the transports of one fixture; it carries no authority.
#[derive(Clone)]
pub struct NodeBftRouteTraceScopeV3 {
    inner: Arc<ScopeData>,
}
struct ScopeData {
    ordinal: u64,
    origin: Instant,
    routes: Mutex<BTreeMap<(PeerId, PeerId), usize>>,
    traces: [Mutex<Trace>; 257],
    lost: [AtomicU64; 257],
    next_owner: AtomicU64,
    next_parcel: AtomicU64,
    untracked: AtomicU64,
    phases: Mutex<VecDeque<PhaseRecord>>,
    phase_lost: AtomicU64,
    phase_evicted: AtomicU64,
    window_start: AtomicU64,
    window_end: AtomicU64,
    window_lost: [AtomicU64; 257],
}
impl Default for NodeBftRouteTraceScopeV3 {
    fn default() -> Self {
        Self::new()
    }
}
impl NodeBftRouteTraceScopeV3 {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ScopeData {
                ordinal: NEXT_SCOPE.fetch_add(1, Ordering::Relaxed),
                origin: Instant::now(),
                routes: Mutex::new(BTreeMap::new()),
                traces: std::array::from_fn(|_| Mutex::new(Trace::default())),
                lost: std::array::from_fn(|_| AtomicU64::new(0)),
                next_owner: AtomicU64::new(1),
                next_parcel: AtomicU64::new(1),
                untracked: AtomicU64::new(0),
                phases: Mutex::new(VecDeque::new()),
                phase_lost: AtomicU64::new(0),
                phase_evicted: AtomicU64::new(0),
                window_start: AtomicU64::new(0),
                window_end: AtomicU64::new(0),
                window_lost: std::array::from_fn(|_| AtomicU64::new(0)),
            }),
        }
    }
}
#[derive(Clone)]
struct PhaseRecord {
    at_us: u128,
    label: &'static str,
    boundary: &'static str,
    elapsed_us: u128,
    unwind: bool,
}
pub struct NodeBftRoutePhaseGuardV3 {
    scope: NodeBftRouteTraceScopeV3,
    label: &'static str,
    started: Instant,
}
impl NodeBftRouteTraceScopeV3 {
    pub fn phase_for_test(&self, label: &'static str) -> NodeBftRoutePhaseGuardV3 {
        if label == "first_wait_h6" {
            window::begin(self);
        }
        self.phase(label, "begin", Duration::ZERO, false);
        NodeBftRoutePhaseGuardV3 {
            scope: self.clone(),
            label,
            started: Instant::now(),
        }
    }
    fn phase(&self, label: &'static str, boundary: &'static str, elapsed: Duration, unwind: bool) {
        let mut phases = match self.inner.phases.try_lock() {
            Ok(phases) => phases,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
            Err(TryLockError::WouldBlock) => {
                self.inner.phase_lost.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        if phases.len() == 32 {
            phases.pop_front();
            self.inner.phase_evicted.fetch_add(1, Ordering::Relaxed);
        }
        phases.push_back(PhaseRecord {
            at_us: self.inner.origin.elapsed().as_micros(),
            label,
            boundary,
            elapsed_us: elapsed.as_micros(),
            unwind,
        });
    }
}
impl Drop for NodeBftRoutePhaseGuardV3 {
    fn drop(&mut self) {
        if self.label == "first_wait_h6" {
            window::end(&self.scope);
        }
        self.scope.phase(
            self.label,
            "end",
            self.started.elapsed(),
            std::thread::panicking(),
        );
    }
}
#[derive(Clone, Copy)]
pub(super) struct Position {
    height: u64,
    round: u64,
    phase: &'static str,
}
pub(super) fn position(message: &BftPublicPeerMessageV3) -> Option<Position> {
    use oasis7_proto::distributed_bft::v3::BftVotePhaseV3;
    match message {
        BftPublicPeerMessageV3::Proposal(p) => Some(Position {
            height: p.header.height,
            round: p.header.round,
            phase: "proposal",
        }),
        BftPublicPeerMessageV3::Vote(v) => Some(Position {
            height: v.height,
            round: v.round,
            phase: match v.phase {
                BftVotePhaseV3::Prevote => "prevote",
                BftVotePhaseV3::Precommit => "precommit",
            },
        }),
        BftPublicPeerMessageV3::Commit(c) => Some(Position {
            height: c.execution_commitment.height,
            round: c.certificate.proposal.round,
            phase: "commit",
        }),
        _ => None,
    }
}
pub(super) fn owner(scope: &NodeBftRouteTraceScopeV3) -> u64 {
    scope.inner.next_owner.fetch_add(1, Ordering::Relaxed)
}
#[derive(Clone)]
pub(super) struct ParcelTrace {
    pub started: Instant,
    pub kind: &'static str,
    position: Option<Position>,
    ordinal: u64,
    scope: NodeBftRouteTraceScopeV3,
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
pub(super) fn parcel(
    scope: Option<&NodeBftRouteTraceScopeV3>,
    m: &BftPublicPeerMessageV3,
) -> Option<ParcelTrace> {
    scope.map(|scope| ParcelTrace {
        started: Instant::now(),
        kind: kind(m),
        position: position(m),
        ordinal: scope.inner.next_parcel.fetch_add(1, Ordering::Relaxed),
        scope: scope.clone(),
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
    position: Option<Position>,
}
#[derive(Default)]
struct Trace {
    messages: BTreeMap<(usize, String), u64>,
    totals: BTreeMap<(usize, &'static str, &'static str, &'static str), Total>,
    recent: VecDeque<Record>,
    slow: VecDeque<Record>,
    sequence: u64,
    recent_evicted: u64,
    slow_evicted: u64,
    unknown: u64,
    window: Option<Arc<window::Window>>,
    worker_active: Option<Box<[Option<Record>; window::STAGE_COUNT]>>,
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
        // Fast worker cadence must not evict the message prelude or slow ring.
        if !window::detailed(&record) {
            return;
        }
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
fn trace(scope: &NodeBftRouteTraceScopeV3, route: usize) -> &Mutex<Trace> {
    &scope.inner.traces[route.min(256)]
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
pub(super) fn route(scope: &NodeBftRouteTraceScopeV3, from: PeerId, to: PeerId) -> usize {
    let mut routes = match scope.inner.routes.try_lock() {
        Ok(routes) => routes,
        Err(TryLockError::Poisoned(error)) => error.into_inner(),
        Err(TryLockError::WouldBlock) => {
            scope.inner.lost[256].fetch_add(1, Ordering::Relaxed);
            return 256;
        }
    };
    if let Some(index) = routes.get(&(from, to)) {
        return *index;
    }
    let index = routes.len();
    if index >= 256 {
        scope.inner.untracked.fetch_add(1, Ordering::Relaxed);
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
    scope: &NodeBftRouteTraceScopeV3,
    route: usize,
    kind: &'static str,
    stage: &'static str,
    result: &'static str,
    elapsed: Duration,
    age: Duration,
    remaining: Duration,
    parcel: u64,
    digest: Option<&str>,
    position: Option<Position>,
) {
    let now = Instant::now();
    try_observe(
        trace(scope, route),
        &scope.inner.lost[route.min(256)],
        |trace| {
            let message = if kind == "worker" && digest.is_none() {
                0
            } else {
                trace.message(route, digest)
            };
            let at_us = now
                .saturating_duration_since(scope.inner.origin)
                .as_micros();
            let record = Record {
                sequence: trace.sequence + 1,
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
                position,
            };
            window::worker_active(trace, &record);
            window::observe(scope, trace, &record);
            trace.insert(record);
        },
    )
    .or_else(|| {
        window::lost(scope, route, now);
        None
    });
}
#[expect(
    clippy::too_many_arguments,
    reason = "fixed diagnostic fields carry no authority"
)]
pub(super) fn event(
    scope: &NodeBftRouteTraceScopeV3,
    route: usize,
    kind: &'static str,
    stage: &'static str,
    result: &'static str,
    elapsed: Duration,
    age: Duration,
    remaining: Duration,
) {
    event_message(
        scope, route, kind, stage, result, elapsed, age, remaining, 0, None, None,
    );
}
pub(super) fn parcel_event(
    route: usize,
    p: &Parcel,
    stage: &'static str,
    result: &'static str,
    elapsed: Duration,
) {
    if let Some(t) = &p.measurement {
        event_message(
            &t.scope,
            route,
            t.kind,
            stage,
            result,
            elapsed,
            t.started.elapsed(),
            p.deadline.saturating_duration_since(Instant::now()),
            t.ordinal,
            Some(&p.digest),
            t.position,
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
pub(super) fn report(scope: &NodeBftRouteTraceScopeV3, owner: u64, owned: &[usize]) {
    // Only this transport's directed routes are drained. Another fixture's
    // Drop cannot take its events; snapshots remain cumulative per route.
    let snapshots = collect_owned(owned, |route| {
        (trace(scope, route), &scope.inner.lost[route.min(256)])
    });
    let phases = match scope.inner.phases.try_lock() {
        Ok(phases) => Some(phases.clone()),
        Err(TryLockError::Poisoned(error)) => Some(error.into_inner().clone()),
        Err(TryLockError::WouldBlock) => {
            scope.inner.phase_lost.fetch_add(1, Ordering::Relaxed);
            None
        }
    };
    let windows = window::snapshots(scope, owned);
    let mut stderr = std::io::stderr().lock();
    window::render(scope, owner, windows, &mut stderr);
    if let Some(phases) = phases {
        for phase in phases {
            let _ = writeln!(
                stderr,
                "BFT_ROUTE_PHASE scope={} at_us={} label={} boundary={} elapsed_us={} unwind={}",
                scope.inner.ordinal,
                phase.at_us,
                phase.label,
                phase.boundary,
                phase.elapsed_us,
                phase.unwind
            );
        }
    }
    let _ = writeln!(
        stderr,
        "BFT_ROUTE_PHASE_STATUS scope={} lost={} evicted={} capacity=32",
        scope.inner.ordinal,
        scope.inner.phase_lost.load(Ordering::Relaxed),
        scope.inner.phase_evicted.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        stderr,
        "BFT_ROUTE_OWNER scope={} owner={owner} routes={owned:?} at_us={} untracked_routes={}",
        scope.inner.ordinal,
        scope.inner.origin.elapsed().as_micros(),
        scope.inner.untracked.load(Ordering::Relaxed)
    );
    for (route, snapshot) in snapshots {
        let Some(snapshot) = snapshot else {
            let _ = writeln!(
                stderr,
                "BFT_ROUTE_SNAPSHOT owner={owner} route={route} available=0 lost={}",
                scope.inner.lost[route.min(256)].load(Ordering::Relaxed)
            );
            continue;
        };
        for r in snapshot.records {
            let _ = writeln!(
                stderr,
                "BFT_ROUTE owner={owner} at_us={} seq={} route={} kind={} stage={} result={} parcel={} message={} elapsed_us={} age_us={} remaining_us={} position_known={} height={} round={} phase={}",
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
                r.remaining_us,
                u8::from(r.position.is_some()),
                r.position.map(|p| p.height).unwrap_or(0),
                r.position.map(|p| p.round).unwrap_or(0),
                r.position.map(|p| p.phase).unwrap_or("unknown")
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
            scope.inner.lost[route.min(256)].load(Ordering::Relaxed)
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
            position: None,
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
    #[test]
    fn explicit_scope_crosses_threads_and_isolates_reused_noise_routes() {
        let first = NodeBftRouteTraceScopeV3::new();
        let second = NodeBftRouteTraceScopeV3::new();
        let from = PeerId::random();
        let to = PeerId::random();
        let route_a = route(&first, from, to);
        let route_b = route(&second, from, to);
        assert_eq!(route_a, 0);
        assert_eq!(route_b, 0);
        let callback = first.clone();
        std::thread::spawn(move || {
            event_message(
                &callback,
                route_a,
                "vote",
                "send",
                "ack",
                Duration::ZERO,
                Duration::ZERO,
                Duration::ZERO,
                7,
                Some("internal-only"),
                None,
            )
        })
        .join()
        .unwrap();
        assert_eq!(trace(&first, route_a).lock().unwrap().sequence, 1);
        assert_eq!(trace(&second, route_b).lock().unwrap().sequence, 0);
        assert_ne!(first.inner.ordinal, second.inner.ordinal);
        use oasis7_consensus::tendermint_v3::BftPeerHistoryRequestV3;
        let message = BftPublicPeerMessageV3::HistoryRequest(BftPeerHistoryRequestV3 {
            nonce: "00".repeat(32),
            after_height: 1,
            after_header_id: "00".repeat(32),
            max_entries: 1,
        });
        assert!(parcel(None, &message).is_none());
        assert!(parcel(Some(&first), &message).is_some());
        assert_eq!(second.inner.next_parcel.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn scope_phase_guard_records_end_on_success_and_unwind_with_bounded_loss() {
        let scope = NodeBftRouteTraceScopeV3::new();
        {
            let _phase = scope.phase_for_test("success");
        }
        let failed = std::panic::catch_unwind(|| {
            let _phase = scope.phase_for_test("failure");
            panic!("controlled phase unwind");
        });
        assert!(failed.is_err());
        {
            let phases = scope.inner.phases.lock().unwrap();
            assert_eq!(phases.len(), 4);
            assert_eq!(phases[0].boundary, "begin");
            assert_eq!(phases[1].boundary, "end");
            assert!(!phases[1].unwind);
            assert_eq!(phases[3].boundary, "end");
            assert!(phases[3].unwind);
            scope.phase("held", "begin", Duration::ZERO, false);
        }
        assert_eq!(scope.inner.phase_lost.load(Ordering::Relaxed), 1);
        for _ in 0..40 {
            scope.phase("bounded", "begin", Duration::ZERO, false);
        }
        assert_eq!(scope.inner.phases.lock().unwrap().len(), 32);
        assert_eq!(scope.inner.phase_evicted.load(Ordering::Relaxed), 12);
    }
    #[test]
    fn message_position_preserves_height_round_and_vote_phase() {
        use oasis7_proto::distributed_bft::{
            v2::BftScopeV2,
            v3::{BftVotePhaseV3, BftVoteV3, BftVoteValueV3},
        };
        let vote = BftVoteV3::sign(
            BftScopeV2 {
                world_id: "trace-test".into(),
                chain_id: "chain".into(),
                genesis_hash: "01".repeat(32),
            },
            6,
            1,
            BftVotePhaseV3::Precommit,
            "02".repeat(32),
            "03".repeat(32),
            BftVoteValueV3::Nil,
            "validator".into(),
            &ed25519_dalek::SigningKey::from_bytes(&[17; 32]),
        )
        .unwrap();
        let message = BftPublicPeerMessageV3::Vote(Box::new(vote));
        let actual = position(&message).unwrap();
        assert_eq!(
            (actual.height, actual.round, actual.phase),
            (6, 1, "precommit")
        );
        let scope = NodeBftRouteTraceScopeV3::new();
        let outbound = parcel(Some(&scope), &message).unwrap();
        let actual = outbound.position.unwrap();
        assert_eq!(
            (actual.height, actual.round, actual.phase),
            (6, 1, "precommit")
        );
    }
}
