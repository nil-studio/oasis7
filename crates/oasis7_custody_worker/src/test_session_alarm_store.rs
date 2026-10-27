//! Local-only expiry facts. An alarm never signs, spends cleanup, or dispatches work.
use super::*;
use oasis7_custody_core::time_units::UnixMillis;

/// Internal scheduling metadata, never an authorization or a Closed receipt.
#[derive(serde::Serialize)]
pub(crate) struct AlarmExpiry {
    pub closed_count: u64,
    pub next_expiry: Option<UnixMillis>,
}
impl OwnerProvisionApprovalStore {
    /// Opening this Store already requires an existing valid component record.
    /// The final transaction independently checks actual inventory and time;
    /// an unchanged duplicate alarm writes neither the record nor its audit.
    pub(crate) fn expire_test_sessions_for_alarm(&self) -> Result<AlarmExpiry> {
        self.local_reservation(
            "test_session_expired",
            "test-session-alarm",
            |state, time| {
                let time = UnixMillis::new(time);
                let closed = state.expire_test_sessions(time).map_err(|_| rejected())?;
                let next_expiry = state
                    .next_test_session_expiry_at(time)
                    .map_err(|_| rejected())?;
                Ok(AlarmExpiry {
                    closed_count: u64::try_from(closed.len()).map_err(|_| rejected())?,
                    next_expiry,
                })
            },
        )
    }
}

#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/test_session_alarm.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = prearmAndCommitTestSessionAlarm)]
    async fn prearm_and_commit(
        state: &JsValue,
        deadline: &str,
        commit: &js_sys::Function,
    ) -> std::result::Result<JsValue, JsValue>;
}
impl OwnerProvisionApprovalStore {
    /// Only synchronous final SQL is inside the scheduling critical section.
    /// External verification and authority reads must have completed beforehand.
    pub(crate) async fn prearm_test_session_commit<R, F>(
        &self,
        deadline: UnixMillis,
        mut commit: F,
    ) -> Result<R>
    where
        F: FnMut() -> Result<R>,
    {
        let mut returned = None;
        {
            let mut callback = || -> Result<JsValue> {
                if returned.is_some() {
                    return Err(rejected());
                }
                returned = Some(commit()?);
                Ok(JsValue::NULL)
            };
            let closure = wasm_bindgen::closure::Closure::borrow_mut(&mut callback);
            let queued = prearm_and_commit(
                &self.state,
                &deadline.get().to_string(),
                closure.as_ref().unchecked_ref(),
            )
            .await;
            drop(closure);
            queued?;
        }
        returned.ok_or_else(rejected)
    }
}

pub(super) fn write_deadline(input: &ApprovalAdmissionInput) -> Result<UnixMillis> {
    match &input.request.intent {
        oasis7_custody_core::approval::CustodyIntentV1::TestSession { write_deadline, .. } => {
            Ok(UnixMillis::new(write_deadline.0))
        }
        _ => Err(rejected()),
    }
}
