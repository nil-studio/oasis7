//! Harness-only per-invocation failure attribution. Never a Current or an authority cache.
use std::cell::Cell;
pub(crate) struct Trace {
    failed: Cell<bool>,
    started: u64,
    source_observed: Option<u64>,
}
impl Trace {
    pub(crate) fn new(started: u64, source_observed: Option<u64>) -> Self {
        Self {
            failed: Cell::new(false),
            started,
            source_observed,
        }
    }
    pub(crate) fn fail(&self, stage: &'static str) {
        if self.failed.replace(true) {
            return;
        }
        let raw = js_sys::Date::now();
        let now =
            (raw.is_finite() && raw >= 0.0 && raw.fract() == 0.0 && raw <= 9_007_199_254_740_991.0)
                .then_some(raw as u64);
        let elapsed = now.and_then(|n| n.checked_sub(self.started));
        let age = now
            .zip(self.source_observed)
            .and_then(|(n, o)| n.checked_sub(o));
        worker::console_error!(
            "{{\"activation_failure\":{{\"stage\":\"{}\",\"elapsed_ms\":{},\"source_age_ms\":{}}}}}",
            stage,
            elapsed
                .map(|n| n.to_string())
                .unwrap_or_else(|| "null".into()),
            age.map(|n| n.to_string()).unwrap_or_else(|| "null".into())
        );
    }
}
