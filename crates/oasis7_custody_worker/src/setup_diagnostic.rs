//! Explicit harness-only setup failure attribution; never an authority input.
use oasis7_custody_core::provisioning::OwnerSetupStageV1;
use std::cell::Cell;
pub(crate) struct Trace {
    failed: Cell<bool>,
    lane: &'static str,
    stage: &'static str,
    started: Option<u64>,
    source: Option<u64>,
}
fn clock() -> Option<u64> {
    let n = js_sys::Date::now();
    (n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n <= 9_007_199_254_740_991.0)
        .then_some(n as u64)
}
impl Trace {
    pub(crate) fn new(lane: &'static str, stage: OwnerSetupStageV1, source: Option<u64>) -> Self {
        let stage = match stage {
            OwnerSetupStageV1::CreateTask => "create_task",
            OwnerSetupStageV1::PrepareGoal => "prepare_goal",
            OwnerSetupStageV1::PublishGoal => "publish_goal",
            OwnerSetupStageV1::Delegation => "delegation",
            OwnerSetupStageV1::Bind => "bind",
            OwnerSetupStageV1::FinalObservation => "final_observation",
        };
        Self {
            failed: Cell::new(false),
            lane,
            stage,
            started: clock(),
            source,
        }
    }
    pub(crate) fn fail(&self, code: &'static str) {
        if self.failed.replace(true) {
            return;
        }
        let now = clock();
        let elapsed = now.zip(self.started).and_then(|(n, s)| n.checked_sub(s));
        let age = now.zip(self.source).and_then(|(n, s)| n.checked_sub(s));
        worker::console_error!(
            "{{\"setup_failure\":{{\"lane\":\"{}\",\"stage\":\"{}\",\"code\":\"{}\",\"elapsed_ms\":{},\"source_age_ms\":{}}}}}",
            self.lane,
            self.stage,
            code,
            elapsed
                .map(|n| n.to_string())
                .unwrap_or_else(|| "null".into()),
            age.map(|n| n.to_string()).unwrap_or_else(|| "null".into())
        );
    }
}
