//! Fixed public receiver configuration. No private seed, SID or key material.
use oasis7_custody_core::native_cleanup_v2::*;
use wasm_bindgen::JsValue;
use worker::Env;
pub(crate) fn rejected() -> JsValue {
    JsValue::from_str("execution fence unavailable")
}
pub(crate) fn clock() -> Result<u64, JsValue> {
    let n = js_sys::Date::now();
    if !n.is_finite() || n <= 0.0 || n.fract() != 0.0 || n > 9_007_199_254_740_991.0 {
        return Err(rejected());
    }
    Ok(n as u64)
}
pub(crate) fn pin(
    env: &Env,
    purpose: AccountMachinePurposeV2,
) -> Result<(AccountMachineIssuerPinV2, AccountMachineIssuerFloorV2), JsValue> {
    if env
        .var("CUSTODY_EXECUTION_PENDING_START_V2_ENABLED")
        .ok()
        .is_none_or(|v| v.to_string() != "true")
    {
        return Err(rejected());
    }
    let raw = env
        .var("CUSTODY_ACCOUNT_SERVICE_ISSUER_PINS")
        .map_err(|_| rejected())?
        .to_string();
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let all: Vec<AccountMachineIssuerPinV2> = serde_json::from_str(&raw).map_err(|_| rejected())?;
    if all.is_empty() || all.len() > 16 {
        return Err(rejected());
    }
    let mut selected = all.into_iter().filter(|p| p.allowed_purpose == purpose);
    let pin = selected.next().ok_or_else(rejected)?;
    let raw_policy = env
        .var("CUSTODY_RECOVERY_AUTHORITY_POLICY")
        .map_err(|_| rejected())?
        .to_string();
    if raw_policy.len() > 8192 {
        return Err(rejected());
    }
    let policy: oasis7_custody_core::recovery::RecoveryAuthorityPolicyV1 =
        serde_json::from_str(&raw_policy).map_err(|_| rejected())?;
    policy.validate().map_err(|_| rejected())?;
    if selected.next().is_some() || pin.environment != policy.environment {
        return Err(rejected());
    }
    let floor = AccountMachineIssuerFloorV2 {
        issuer_epoch: pin.issuer_epoch,
        config_generation: pin.config_generation,
    };
    Ok((pin, floor))
}
