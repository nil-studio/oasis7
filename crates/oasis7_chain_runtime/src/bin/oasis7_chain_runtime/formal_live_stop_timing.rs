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
}
impl StopTiming {
    pub(super) fn new(label: &'static str) -> Option<Self> {
        Self::from_flag(
            std::env::var("OASIS7_FORMAL_EXECUTION_TIMING")
                .ok()
                .as_deref(),
            label,
        )
    }
    fn from_flag(flag: Option<&str>, label: &'static str) -> Option<Self> {
        (flag == Some("1")).then(|| {
            Self(Arc::new(State {
                operation: NEXT_OPERATION.fetch_add(1, Ordering::Relaxed),
                label,
                started: Instant::now(),
                count: AtomicUsize::new(0),
                pending_reported: AtomicBool::new(false),
            }))
        })
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
        Some(format!(
            "formal_stop_timing operation={} label={} stage={} elapsed_ms={} record={}",
            self.0.operation,
            self.0.label,
            stage,
            self.0.started.elapsed().as_millis(),
            index + 1
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
