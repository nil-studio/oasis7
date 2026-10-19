//! Test-binary-only bounded scalar observation of one owned stop operation.
use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use std::time::Instant;
static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);
#[derive(Clone)]
pub(super) struct StopTiming(Arc<State>);
struct State {
    operation: u64,
    label: &'static str,
    started: Instant,
    count: AtomicUsize,
    pending_reported: AtomicBool,
    remaining_ms: AtomicU64,
}
impl StopTiming {
    pub(super) fn new(label: &'static str) -> Option<Self> {
        Self::new_at(label, Instant::now())
    }
    pub(super) fn new_at(label: &'static str, started: Instant) -> Option<Self> {
        Self::from_flag_at(
            std::env::var("OASIS7_FORMAL_EXECUTION_TIMING")
                .ok()
                .as_deref(),
            label,
            started,
        )
    }
    fn from_flag(flag: Option<&str>, label: &'static str) -> Option<Self> {
        Self::from_flag_at(flag, label, Instant::now())
    }
    fn from_flag_at(flag: Option<&str>, label: &'static str, started: Instant) -> Option<Self> {
        (flag == Some("1")).then(|| {
            Self(Arc::new(State {
                operation: NEXT_OPERATION.fetch_add(1, Ordering::Relaxed),
                label,
                started,
                count: AtomicUsize::new(0),
                pending_reported: AtomicBool::new(false),
                remaining_ms: AtomicU64::new(u64::MAX),
            }))
        })
    }
    pub(super) fn set_remaining(&self, deadline: Instant) {
        self.0.remaining_ms.store(
            deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
    }
    fn line(&self, stage: &'static str) -> Option<String> {
        if stage == "wait_pending" && self.0.pending_reported.swap(true, Ordering::Relaxed) {
            return None;
        }
        let index = self
            .0
            .count
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                (value < 10).then_some(value + 1)
            })
            .ok()?;
        let remaining = self.0.remaining_ms.load(Ordering::Relaxed);
        let extra = if stage == "spawn" && remaining != u64::MAX {
            format!(" remaining_ms={remaining}")
        } else {
            String::new()
        };
        Some(format!(
            "formal_stop_timing operation={} label={} stage={} elapsed_ms={} record={}{} mono_ns={}",
            self.0.operation,
            self.0.label,
            stage,
            self.0.started.elapsed().as_millis(),
            index + 1,
            extra,
            super::super::execution_bridge_real_tests::real_execution_bridge::formal_execution_timing::clock_ns()
        ))
    }
    pub(super) fn record(&self, stage: &'static str) {
        if let Some(line) = self.line(stage) {
            // Diagnostic persistence must never change stop error semantics.
            let _ = writeln!(std::io::stderr().lock(), "{line}");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_one_enables_observation() {
        for flag in [None, Some("0"), Some("true"), Some("01"), Some("")] {
            assert!(StopTiming::from_flag(flag, "unit").is_none());
        }
        assert!(StopTiming::from_flag(Some("1"), "unit").is_some());
    }
    #[test]
    fn async_clones_share_operation_label_and_total_cap() {
        let trace = StopTiming::from_flag(Some("1"), "restarted_stop").unwrap();
        let worker = trace.clone();
        let a = trace.line("operation_begin").unwrap();
        let b = std::thread::spawn(move || worker.line("mutex_wait_begin").unwrap())
            .join()
            .unwrap();
        for text in [&a, &b] {
            assert!(text.contains(&format!("operation={} ", trace.0.operation)));
            assert!(text.contains("label=restarted_stop "));
        }
        for _ in 0..8 {
            assert!(trace.line("constant_stage").is_some());
        }
        assert!(trace.line("constant_stage").is_none());
        assert_eq!(trace.0.count.load(Ordering::Relaxed), 10);
    }
    #[test]
    fn entry_clock_remaining_and_pending_completion_fit_same_cap() {
        let started = Instant::now() - std::time::Duration::from_millis(25);
        let trace = StopTiming::from_flag_at(Some("1"), "entry", started).unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        for stage in [
            "stop_entry",
            "request_stop_end_admission_wait_begin",
            "admission_wait_end",
        ] {
            let line = trace.line(stage).unwrap();
            let elapsed: u128 = line
                .split("elapsed_ms=")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            assert!(elapsed >= 25);
        }
        trace.set_remaining(deadline);
        let spawn = trace.clone().line("spawn").unwrap();
        assert!(spawn.contains("remaining_ms="));
        assert!(trace.0.remaining_ms.load(Ordering::Relaxed) <= 10000);
        for stage in [
            "mutex_wait_end",
            "main_worker_join_begin",
            "wait_pending",
            "main_worker_join_end",
            "worker_completed",
            "wait_completed",
        ] {
            assert!(trace.clone().line(stage).is_some());
        }
        assert_eq!(trace.0.count.load(Ordering::Relaxed), 10);
        assert!(trace.line("extra").is_none());
        trace.set_remaining(Instant::now() - std::time::Duration::from_millis(1));
        assert_eq!(trace.0.remaining_ms.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn execution_and_stop_share_the_same_process_clock() {
        let before = super::super::super::execution_bridge_real_tests::real_execution_bridge::formal_execution_timing::clock_ns();
        let trace = StopTiming::from_flag(Some("1"), "clock").unwrap();
        let line = std::thread::spawn(move || trace.line("spawn").unwrap())
            .join()
            .unwrap();
        let observed: u128 = line.split("mono_ns=").nth(1).unwrap().parse().unwrap();
        let after = super::super::super::execution_bridge_real_tests::real_execution_bridge::formal_execution_timing::clock_ns();
        assert!(before <= observed && observed <= after);
    }
    #[test]
    fn repeated_pending_waits_do_not_reset_or_spend_async_end_budget() {
        let trace = StopTiming::from_flag(Some("1"), "unit").unwrap();
        assert!(trace.line("wait_pending").is_some());
        for _ in 0..100 {
            assert!(trace.clone().line("wait_pending").is_none());
        }
        assert!(trace.line("main_worker_join_end").is_some());
        assert_eq!(trace.0.count.load(Ordering::Relaxed), 2);
    }
}
