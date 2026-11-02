//! Fixed Owner Actor only; no provider, ordinary authority or Native callback.
use crate::role_key_store::{RoleKeyStore, TermGuardedSignerConfigurationV2};
use oasis7_custody_core::{
    native_cleanup_v2::AccountMachinePurposeV2,
    term_guarded_signer_v2::SignedTermGuardedStopSignerPermitV2,
};
use wasm_bindgen::JsValue;
use worker::{Env, Response, Result};
fn denied() -> worker::Error {
    worker::Error::RustError("Term Guarded signer unavailable".into())
}
pub(crate) fn installation(env: &Env) -> Result<Option<TermGuardedSignerConfigurationV2>> {
    if !env
        .var("CUSTODY_TERM_GUARDED_SIGNER_V2_ENABLED")
        .ok()
        .is_some_and(|v| v.to_string() == "true")
    {
        return Ok(None);
    }
    let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
        env,
        AccountMachinePurposeV2::TermGuardedStopSignerV2,
    )
    .map_err(|_| denied())?;
    let binding: oasis7_custody_core::native_binding::NativeBindingPinV1 = serde_json::from_str(
        &env.var("CUSTODY_NATIVE_BINDING_OWNER_PIN")
            .map_err(|_| denied())?
            .to_string(),
    )
    .map_err(|_| denied())?;
    binding.validate().map_err(|_| denied())?;
    if pin.public_key == binding.public_key
        || pin.key_id == binding.key_id
        || pin.service_id == binding.service_identity
    {
        return Err(denied());
    }
    TermGuardedSignerConfigurationV2::new(pin, floor)
        .map(Some)
        .map_err(|_| denied())
}
pub(crate) async fn receive(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let config = installation(env)?.ok_or_else(denied)?;
    if raw.is_empty() || raw.len() > 32768 {
        return Err(denied());
    }
    let permit: SignedTermGuardedStopSignerPermitV2 =
        serde_json::from_str(raw).map_err(|_| denied())?;
    let original: serde_json::Value = serde_json::from_str(raw).map_err(|_| denied())?;
    if serde_json::to_value(&permit).map_err(|_| denied())? != original {
        return Err(denied());
    }
    let result = RoleKeyStore::new(state)
        .sign_term_guarded_stop_v2(env, permit, &config)
        .await
        .map_err(|_| denied())?;
    Response::from_json(&result)
}
