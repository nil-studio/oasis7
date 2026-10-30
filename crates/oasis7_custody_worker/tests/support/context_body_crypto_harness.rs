//! Synthetic encrypted inputs only. No deployment profile includes this actor.
use oasis7_custody_core::{approval::bytes_digest, context_body::*};
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use worker::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    scope: ContextBodyScopeV1,
    record: ContextBodyRecordV1,
}
#[durable_object]
pub struct ContextBodyCryptoHarness {
    env: Env,
}
impl DurableObject for ContextBodyCryptoHarness {
    fn new(_state: State, env: Env) -> Self {
        Self { env }
    }
    async fn fetch(&self, mut request: Request) -> Result<Response> {
        let result = async {
            let raw = request.text().await.map_err(|_| ())?;
            let input: Input = serde_json::from_str(&raw).map_err(|_| ())?;
            let crypto =
                crate::context_body_crypto::ContextBodyCrypto::load(&self.env).map_err(|_| ())?;
            let opened = crypto
                .open(&input.scope, &input.record)
                .await
                .map_err(|_| ())?;
            Ok::<_, ()>(serde_json::json!({
                "bytes": opened.as_bytes().len(),
                "digest": bytes_digest(opened.as_bytes()),
            }))
        }
        .await;
        match result {
            Ok(value) => Response::from_json(&value),
            Err(()) => Response::error("synthetic context crypto rejected", 409),
        }
    }
}
