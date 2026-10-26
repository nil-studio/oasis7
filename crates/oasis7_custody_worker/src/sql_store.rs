//! The independent Recovery Fence persists separately from account snapshots.
use oasis7_custody_core::{DecimalU64, recovery::RecoveryFence};
use wasm_bindgen::prelude::*;
use worker::{Error, Result};

#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = initializeTextRecord)]
    fn initialize_text_record(
        state: &JsValue,
        key: &str,
        initial: &str,
    ) -> std::result::Result<String, JsValue>;
}

pub(crate) fn read_frozen_fence(state: &JsValue, initial_epoch: &str) -> Result<RecoveryFence> {
    // Reject noncanonical/overflow epoch configuration before creating any state.
    let epoch: DecimalU64 = serde_json::from_value(serde_json::Value::String(initial_epoch.into()))
        .map_err(|_| Error::RustError("invalid fence epoch configuration".into()))?;
    let initial = serde_json::to_string(&RecoveryFence::new(epoch.0, true))?;
    let stored = initialize_text_record(state, "recovery-fence-v1", &initial)
        .map_err(|_| Error::RustError("fence storage unavailable".into()))?;
    serde_json::from_str(&stored).map_err(|_| Error::RustError("invalid persisted fence".into()))
}
