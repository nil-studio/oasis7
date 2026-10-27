//! Local expiry only; no Native calls or World cleanup acknowledgements.
use crate::provision_approval_store::OwnerProvisionApprovalStore;
use wasm_bindgen::{JsCast, prelude::*};
#[wasm_bindgen(module = "/platform/test_session_alarm.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = testSessionAlarmPresent)]
    fn present(state: &JsValue) -> Result<bool, JsValue>;
    #[wasm_bindgen(catch, js_name = reconcileTestSessionAlarm)]
    async fn reconcile(state: &JsValue, calculate: &js_sys::Function) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = retryTestSessionAlarm)]
    async fn retry(state: &JsValue) -> Result<JsValue, JsValue>;
}
pub(crate) async fn repair(state: &JsValue) -> Result<(), JsValue> {
    let outcome = async {
        if !present(state)? {
            return Ok(());
        }
        let mut calculate = || -> Result<String, JsValue> {
            let saved = OwnerProvisionApprovalStore::open(state.clone())?
                .expire_test_sessions_for_alarm()?;
            Ok(saved
                .next_expiry
                .map(|v| v.get().to_string())
                .unwrap_or_default())
        };
        let closure = wasm_bindgen::closure::Closure::borrow_mut(&mut calculate);
        let result = reconcile(state, closure.as_ref().unchecked_ref()).await;
        drop(closure);
        result.map(|_| ())
    }
    .await;
    if outcome.is_err() {
        retry(state).await?;
    }
    outcome
}
