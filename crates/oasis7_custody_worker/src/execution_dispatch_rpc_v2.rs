//! Closed fixed Prepare dispatch metadata. No private body, caller URL or Current.
#[cfg(feature = "identity")]
use oasis7_custody_core::{approval::bytes_digest, native_cleanup_v2::AccountMachinePurposeV2};
use oasis7_custody_core::{
    execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2, native_execution_prepare_v2::*,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use worker::Env;
pub(crate) const MAX_FRAME: usize = 128 * 1024;
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExecutionDispatchFormatV2 {
    PerLeafDispatchV2,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionDispatchFrameV2 {
    pub api_version: u8,
    pub format: ExecutionDispatchFormatV2,
    pub request: Box<NativeExecutionPrepareRequestV2>,
    pub authorization: SignedExecutionDispatchClaimV2,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IdentityExecutionDispatchGateV2 {
    pub api_version: u8,
    pub request: Box<NativeExecutionPrepareRequestV2>,
    pub previous: SignedExecutionDispatchClaimV2,
    pub edge: ExecutionPreparePhysicalEdgeV2,
    pub body_digest: String,
}
pub(crate) fn denied() -> JsValue {
    JsValue::from_str("execution dispatch unavailable")
}
pub(crate) fn require_enabled(env: &Env) -> Result<(), JsValue> {
    if env
        .var("CUSTODY_EXECUTION_PREPARE_DISPATCH_V2_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(denied());
    }
    Ok(())
}
impl ExecutionDispatchFrameV2 {
    pub(crate) fn wire_bytes(&self) -> Result<Vec<u8>, JsValue> {
        if self.api_version != 2 {
            return Err(denied());
        }
        self.request.wire_bytes().map_err(|_| denied())?;
        let raw = serde_json::to_vec(self).map_err(|_| denied())?;
        if raw.len() > MAX_FRAME {
            return Err(denied());
        }
        Ok(raw)
    }
    #[cfg(feature = "identity")]
    pub(crate) fn check(
        &self,
        env: &Env,
        expected: ExecutionPreparePhysicalEdgeV2,
        now: u64,
    ) -> Result<(), JsValue> {
        require_enabled(env)?;
        self.wire_bytes()?;
        let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
            env,
            AccountMachinePurposeV2::ExecutionDispatchClaimV2,
        )?;
        self.authorization
            .verify_issuer(&self.request, &pin, &floor, now)
            .map_err(|_| denied())?;
        if self.authorization.claim.leaf.edge != expected
            || self.authorization.claim.body_digest
                != bytes_digest(&self.request.wire_bytes().map_err(|_| denied())?)
        {
            return Err(denied());
        }
        Ok(())
    }
}
