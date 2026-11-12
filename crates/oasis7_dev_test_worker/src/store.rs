//! Durable SQL adapter is provided only on Workers; no native filesystem fallback.
#[cfg(target_arch = "wasm32")]
mod sql {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen(module = "/platform/store.mjs")]
    extern "C" {
        #[wasm_bindgen(catch, js_name = loadRecord)]
        pub fn load(state: &JsValue, key: &str) -> Result<String, JsValue>;
        #[wasm_bindgen(catch, js_name = commitLedger)]
        pub fn commit_ledger(
            state: &JsValue,
            meta_previous: &str,
            meta_next: &str,
            key: &str,
            previous: &str,
            next: &str,
        ) -> Result<(), JsValue>;
        #[wasm_bindgen(catch, js_name = commitRecord)]
        pub fn commit(
            state: &JsValue,
            key: &str,
            previous: &str,
            next: &str,
        ) -> Result<(), JsValue>;
    }
}
#[cfg(target_arch = "wasm32")]
pub(crate) use sql::*;
