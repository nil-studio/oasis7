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
}
fn timings() -> &'static Mutex<Timings> {
    static TIMINGS: OnceLock<Mutex<Timings>> = OnceLock::new();
    TIMINGS.get_or_init(|| {
        Mutex::new(Timings {
            origin: Instant::now(),
            stages: BTreeMap::new(),
            phase: "setup",
            details: 0,
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
        if timings.details < 512 && (count <= 4 || elapsed >= 1_000_000) {
            timings.details += 1;
            eprintln!(
                "BFT_NODE_STAGE stage={} phase={} thread={:?} start_us={start_us} elapsed_us={elapsed} call={count} unwind={}",
                self.stage,
                self.phase,
                std::thread::current().id(),
                std::thread::panicking()
            );
        }
    }
}
pub(crate) fn finished(stage: &'static str, started: Option<Instant>) {
    drop(Span {
        stage,
        phase: "setup",
        started,
    });
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
                "BFT_NODE_TOTAL stage={stage} phase={phase} count={} total_us={} max_us={}",
                totals.count, totals.total_us, totals.max_us
            );
        }
        eprintln!(
            "BFT_NODE_TOTAL details={} detail_limit=512",
            timings.details
        );
    }
}
