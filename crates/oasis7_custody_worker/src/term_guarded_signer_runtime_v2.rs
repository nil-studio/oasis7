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
    let config = TermGuardedSignerConfigurationV2::new(pin, floor).map_err(|_| denied())?;
    let config = if env
        .var("CUSTODY_TERM_GUARDED_DUAL_SIGNER_V2_ENABLED")
        .ok()
        .is_some_and(|v| v.to_string() == "true")
    {
        let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
            env,
            AccountMachinePurposeV2::TermGuardedCloseSignerV2,
        )
        .map_err(|_| denied())?;
        config.with_close(pin, floor).map_err(|_| denied())?
    } else {
        config
    };
    Ok(Some(config))
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

pub(crate) async fn receive_close(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let config = installation(env)?.ok_or_else(denied)?;
    let permit = decode::<
        oasis7_custody_core::term_guarded_close_signer_v2::SignedTermGuardedCloseSignerPermitV2,
    >(raw)?;
    let signed = RoleKeyStore::new(state)
        .sign_term_guarded_close_v2(env, permit, &config)
        .await
        .map_err(|_| denied())?;
    Response::from_json(&signed)
}
pub(crate) fn receive_qualification(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let config = installation(env)?.ok_or_else(denied)?;
    let request = decode::<crate::role_key_store::TermOwnerSlotQualificationRequestV2>(raw)?;
    let reply = RoleKeyStore::new(state)
        .qualify_term_owner_slot_v2(request, &config)
        .map_err(|_| denied())?;
    Response::from_json(&reply)
}
fn decode<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(raw: &str) -> Result<T> {
    if raw.is_empty() || raw.len() > 32768 {
        return Err(denied());
    }
    let typed: T = serde_json::from_str(raw).map_err(|_| denied())?;
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|_| denied())?;
    if serde_json::to_value(&typed).map_err(|_| denied())? != value {
        return Err(denied());
    }
    Ok(typed)
}
