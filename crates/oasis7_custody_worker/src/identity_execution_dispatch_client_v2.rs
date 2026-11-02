//! Each call is the existing Owner service + Account Actor pair; never a local grant.
use crate::execution_dispatch_rpc_v2::{IdentityExecutionDispatchGateV2, MAX_FRAME, denied};
use oasis7_custody_core::{
    execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2,
    native_cleanup_v2::AccountMachinePurposeV2, native_execution_prepare_v2::*,
};
use wasm_bindgen::prelude::*;
use worker::Env;
#[wasm_bindgen(module = "/platform/execution_prepare_rpc_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=executionDispatchGateV2)]
    async fn gate(env: &Env, raw: &str) -> Result<JsValue, JsValue>;
}
pub(crate) async fn claim(
    env: &Env,
    q: &NativeExecutionPrepareRequestV2,
    previous: &SignedExecutionDispatchClaimV2,
    edge: ExecutionPreparePhysicalEdgeV2,
    body_digest: String,
) -> Result<SignedExecutionDispatchClaimV2, JsValue> {
    let query = IdentityExecutionDispatchGateV2 {
        api_version: 2,
        request: Box::new(q.clone()),
        previous: previous.clone(),
        edge,
        body_digest: body_digest.clone(),
    };
    let raw = serde_json::to_string(&query).map_err(|_| denied())?;
    if raw.len() > MAX_FRAME {
        return Err(denied());
    }
    // Exactly one named service RPC, whose actual fixed handler performs one
    // AccountActor fetch. Failure/unknown has no retry or history token rebuild.
    let raw = gate(env, &raw).await?.as_string().ok_or_else(denied)?;
    if raw.len() > 32768 {
        return Err(denied());
    }
    let signed: SignedExecutionDispatchClaimV2 =
        serde_json::from_str(&raw).map_err(|_| denied())?;
    let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
        env,
        AccountMachinePurposeV2::ExecutionDispatchClaimV2,
    )?;
    signed
        .verify_issuer(q, &pin, &floor, clock()?)
        .map_err(|_| denied())?;
    if signed.claim.leaf.edge != edge || signed.claim.body_digest != body_digest {
        return Err(denied());
    }
    Ok(signed)
}

fn clock() -> Result<u64, JsValue> {
    let value = js_sys::Date::now();
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > 9_007_199_254_740_991.0
    {
        return Err(denied());
    }
    Ok(value as u64)
}
