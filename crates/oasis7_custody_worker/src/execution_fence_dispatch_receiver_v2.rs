//! Dedicated actual receiver. Its first local SQL commits before the fixed
//! Account callback; every later request fails before callback or FENCES I/O.
use crate::execution_fence_config_v2::{clock, pin, rejected};
use oasis7_custody_core::{
    execution_fence_dispatch_v2::*, native_cleanup_v2::AccountMachinePurposeV2,
};
use wasm_bindgen::prelude::*;
use worker::*;
#[wasm_bindgen(module = "/platform/execution_fence_transport_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=claimExecutionFenceAccountLeaf)]
    async fn account_leaf(
        env: &JsValue,
        body: &str,
        until: f64,
    ) -> core::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=readExecutionFenceLeaf)]
    async fn read_leaf(
        env: &JsValue,
        body: &str,
        until: f64,
    ) -> core::result::Result<JsValue, JsValue>;
}
#[durable_object]
pub struct ExecutionFenceDispatcher {
    state: JsValue,
    object_id: String,
    env: Env,
}
impl DurableObject for ExecutionFenceDispatcher {
    fn new(state: State, env: Env) -> Self {
        Self {
            object_id: state.id().to_string(),
            state: state._inner().into(),
            env,
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        if request.method() != Method::Post
            || url.scheme() != "https"
            || url.host_str() != Some("fence-dispatch.internal")
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != EXECUTION_FENCE_DISPATCH_PATH_V2
        {
            return Response::error("not found", 404);
        }
        if request.headers().get("Content-Type")?.as_deref() != Some("application/json") {
            return Response::error("execution fence unavailable", 409);
        }
        let raw = crate::recovery_fence_control::bounded(request).await?;
        match self.run(&raw).await {
            Ok(value) => Response::from_json(&value),
            Err(_) => Response::error("execution fence unavailable", 409),
        }
    }
}
impl ExecutionFenceDispatcher {
    async fn run(&self, raw: &str) -> core::result::Result<ExecutionFenceObservationV2, JsValue> {
        let first = crate::execution_fence_dispatch_once_v2::claim(
            &self.state,
            &self.env,
            &self.object_id,
            raw,
        )?;
        let dispatch = first.proof();
        let digest = dispatch.digest().map_err(|_| rejected())?;
        let until = dispatch.binding.expires_at.0;
        // Only this private first-return token reaches the actual callback. The
        // service RPC and its Account Actor are separately present in master funding.
        let body = serde_json::to_string(dispatch).map_err(|_| rejected())?;
        let raw = account_leaf(self.env.as_ref(), &body, until as f64)
            .await?
            .as_string()
            .ok_or_else(rejected)?;
        if raw.len() > 32768 {
            return Err(rejected());
        }
        let read: SignedExecutionFenceReadV2 =
            serde_json::from_str(&raw).map_err(|_| rejected())?;
        let (pin, floor) = pin(&self.env, AccountMachinePurposeV2::ExecutionFenceReadV2)?;
        read.verify(&pin, &floor, clock()?)
            .map_err(|_| rejected())?;
        if read.claim.binding != dispatch.binding || read.claim.dispatch_digest != digest {
            return Err(rejected());
        }
        let read_digest = read.digest().map_err(|_| rejected())?;
        let claimed_at = read.claim.claimed_at.0;
        let request = ExecutionFenceReadRequestV2 {
            query: ExecutionFenceQueryV2 {
                api_version: 2,
                expected_service_epoch: read.claim.binding.expected_service_epoch,
            },
            authorization: read,
        };
        request.validate_binding().map_err(|_| rejected())?;
        let body = serde_json::to_string(&request).map_err(|_| rejected())?;
        let raw = read_leaf(self.env.as_ref(), &body, until as f64)
            .await?
            .as_string()
            .ok_or_else(rejected)?;
        if raw.len() > 32768 {
            return Err(rejected());
        }
        let result: ExecutionFenceObservationV2 =
            serde_json::from_str(&raw).map_err(|_| rejected())?;
        let now = clock()?;
        if result.api_version != 2
            || result.dispatch_digest != digest
            || result.read_digest != read_digest
            || result.fence.epoch() != dispatch.binding.expected_service_epoch.0
            || result.observed_at.0 < claimed_at
            || result.observed_at.0 > now
            || now >= until
        {
            return Err(rejected());
        }
        Ok(result)
    }
}
