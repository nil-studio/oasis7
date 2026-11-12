//! Private immutable SDK records. Missing/corrupt state never creates replacement operations.
use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::prelude::*;
use worker::Env;
#[wasm_bindgen(module = "/platform/native.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readNativeRecord)]
    async fn read_native(state: &JsValue, env: &Env, key: &str) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = putNativeRecord)]
    async fn put_native(state: &JsValue, env: &Env, key: &str, body: &str) -> Result<(), JsValue>;
}
pub(crate) struct Cache<'a> {
    pub state: &'a JsValue,
    pub env: &'a Env,
    pub scope: &'a str,
}
impl Cache<'_> {
    fn key(&self, kind: &str, operation: &str) -> String {
        format!("native:{}:{}:{}", self.scope, kind, operation)
    }
    pub async fn read<T: DeserializeOwned>(
        &self,
        kind: &str,
        operation: &str,
    ) -> Result<Option<T>, &'static str> {
        let value = read_native(self.state, self.env, &self.key(kind, operation))
            .await
            .map_err(|_| "native_original_unknown")?;
        let text = value.as_string().ok_or("native_original_unknown")?;
        if text.is_empty() {
            return Ok(None);
        }
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|_| "native_original_unknown")
    }
    pub async fn put<T: Serialize>(
        &self,
        kind: &str,
        operation: &str,
        value: &T,
    ) -> Result<(), &'static str> {
        put_native(
            self.state,
            self.env,
            &self.key(kind, operation),
            &serde_json::to_string(value).map_err(|_| "native_original_unknown")?,
        )
        .await
        .map_err(|_| "native_original_unknown")
    }
}
