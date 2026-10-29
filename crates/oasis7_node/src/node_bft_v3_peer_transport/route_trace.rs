//! Opt-in, bounded test-hooks public-route observations. No message identities.
use super::*;
use std::sync::OnceLock;
#[derive(Clone, Copy)]
pub(super) struct ParcelTrace {
    pub started: Instant,
    pub kind: &'static str,
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
    })
}
// One bounded ordinal per directed authenticated route across this process.
// Peer identities remain internal and are never rendered in observations.
pub(super) fn route(from: PeerId, to: PeerId) -> usize {
    static ROUTES: OnceLock<Mutex<BTreeMap<(PeerId, PeerId), usize>>> = OnceLock::new();
    if !enabled() {
        return 256;
    }
    let mut routes = ROUTES
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(index) = routes.get(&(from, to)) {
        return *index;
    }
    let index = routes.len();
    if index >= 256 {
        return 256;
    }
    routes.insert((from, to), index);
    index
}
#[derive(Default)]
struct Total {
    count: u64,
    max_elapsed_us: u128,
    max_age_us: u128,
}
#[derive(Default)]
struct Trace {
    details: usize,
    totals: BTreeMap<(usize, &'static str, &'static str, &'static str), Total>,
}
fn trace() -> &'static Mutex<Trace> {
    static TRACE: OnceLock<Mutex<Trace>> = OnceLock::new();
    TRACE.get_or_init(|| Mutex::new(Trace::default()))
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
    if !enabled() {
        return;
    }
    let mut trace = trace()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let total = trace
        .totals
        .entry((route, kind, stage, result))
        .or_default();
    total.count += 1;
    total.max_elapsed_us = total.max_elapsed_us.max(elapsed.as_micros());
    total.max_age_us = total.max_age_us.max(age.as_micros());
    let count = total.count;
    if trace.details < 512
        && (count <= 2 || elapsed >= Duration::from_secs(1) || age >= Duration::from_secs(10))
    {
        trace.details += 1;
        eprintln!(
            "BFT_ROUTE stage={stage} route={route} kind={kind} result={result} elapsed_us={} age_us={} remaining_us={} count={count}",
            elapsed.as_micros(),
            age.as_micros(),
            remaining.as_micros()
        );
    }
}
pub(super) fn parcel_event(
    route: usize,
    p: &Parcel,
    stage: &'static str,
    result: &'static str,
    elapsed: Duration,
) {
    if let Some(t) = p.measurement {
        event(
            route,
            t.kind,
            stage,
            result,
            elapsed,
            t.started.elapsed(),
            p.deadline.saturating_duration_since(Instant::now()),
        );
    }
}
pub(super) fn report() {
    if !enabled() {
        return;
    }
    let trace = trace()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for ((route, kind, stage, result), total) in &trace.totals {
        eprintln!(
            "BFT_ROUTE_TOTAL route={route} kind={kind} stage={stage} result={result} count={} max_elapsed_us={} max_age_us={}",
            total.count, total.max_elapsed_us, total.max_age_us
        );
    }
    eprintln!("BFT_ROUTE_TOTAL details={} limit=512", trace.details);
}
