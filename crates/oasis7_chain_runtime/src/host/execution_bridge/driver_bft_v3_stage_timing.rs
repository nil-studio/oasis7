//! Opt-in test-only stage timings. No payload, identity or filesystem data.
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("OASIS7_BFT_MEASURE").as_deref() == Ok("1"))
}
#[derive(Default)]
struct Totals {
    count: u64,
    total_us: u128,
    max_us: u128,
}
struct Timings {
    origin: Instant,
    stages: BTreeMap<(&'static str, &'static str), Totals>,
    phase: &'static str,
    details: usize,
    markers: usize,
}
fn timings() -> &'static Mutex<Timings> {
    static TIMINGS: OnceLock<Mutex<Timings>> = OnceLock::new();
    TIMINGS.get_or_init(|| {
        Mutex::new(Timings {
            origin: Instant::now(),
            stages: BTreeMap::new(),
            phase: "setup",
            details: 0,
            markers: 0,
        })
    })
}
pub(crate) struct Span {
    stage: &'static str,
    phase: &'static str,
    started: Option<Instant>,
}
impl Span {
    pub(crate) fn new(stage: &'static str) -> Self {
        let phase = if enabled() {
            timings()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .phase
        } else {
            "disabled"
        };
        Self {
            stage,
            phase,
            started: enabled().then(Instant::now),
        }
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        let Some(started) = self.started else {
            return;
        };
        let elapsed = started.elapsed().as_micros();
        let mut timings = timings()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let start_us = started.duration_since(timings.origin).as_micros();
        let totals = timings.stages.entry((self.stage, self.phase)).or_default();
        totals.count += 1;
        totals.total_us += elapsed;
        totals.max_us = totals.max_us.max(elapsed);
        let count = totals.count;
        // Bound detail volume while retaining every stage's complete totals.
        if timings.details < 512 && (count <= 4 || elapsed >= 50_000) {
            timings.details += 1;
            eprintln!(
                "BFT_DRIVER_STAGE stage={} phase={} thread={:?} start_us={start_us} elapsed_us={elapsed} call={count} unwind={}",
                self.stage,
                self.phase,
                std::thread::current().id(),
                std::thread::panicking()
            );
        }
    }
}
pub(crate) fn mark(stage: &'static str) {
    if !enabled() {
        return;
    }
    let mut timings = timings()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if stage == "continuation_clock_armed" {
        timings.phase = "continuation";
        timings.details = 0;
    }
    if stage == "membership_clock_armed" {
        timings.phase = "membership";
        timings.details = 0;
    }
    if timings.markers < 256 {
        timings.markers += 1;
        eprintln!(
            "BFT_DRIVER_MARK stage={stage} thread={:?} at_us={}",
            std::thread::current().id(),
            timings.origin.elapsed().as_micros()
        );
    }
}
pub(crate) fn proposal_signature(preimage: &[u8]) {
    if enabled()
        && preimage[..preimage.len().min(128)]
            .windows(b"oasis7.bft.proposal.v3.sign".len())
            .any(|part| part == b"oasis7.bft.proposal.v3.sign")
    {
        mark("proposal_signature");
    }
}
pub(crate) struct Report;
impl Drop for Report {
    fn drop(&mut self) {
        if !enabled() {
            return;
        }
        let timings = timings()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for ((stage, phase), totals) in &timings.stages {
            eprintln!(
                "BFT_DRIVER_TOTAL stage={stage} phase={phase} count={} total_us={} max_us={}",
                totals.count, totals.total_us, totals.max_us
            );
        }
        eprintln!(
            "BFT_DRIVER_TOTAL details={} markers={} detail_limit_per_phase=512 marker_limit=256",
            timings.details, timings.markers
        );
    }
}
