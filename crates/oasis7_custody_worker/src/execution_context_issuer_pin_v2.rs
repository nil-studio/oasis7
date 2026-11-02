//! Public fixed deployment pins only. No issuer seed/Native SID/KEK is loaded.
use oasis7_custody_core::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachinePurposeV2,
};
use wasm_bindgen::JsValue;
use worker::Env;
fn rejected() -> JsValue {
    JsValue::from_str("execution issuer pin rejected")
}
pub(crate) fn pin(
    env: &Env,
    purpose: AccountMachinePurposeV2,
) -> Result<(AccountMachineIssuerPinV2, AccountMachineIssuerFloorV2), JsValue> {
    let value = |name| env.var(name).map(|v| v.to_string()).map_err(|_| rejected());
    if value("CUSTODY_TEST_SESSION_V2_ENABLED")? != "true" {
        return Err(rejected());
    }
    let raw = value("CUSTODY_ACCOUNT_SERVICE_ISSUER_PINS")?;
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let pins: Vec<AccountMachineIssuerPinV2> =
        serde_json::from_str(&raw).map_err(|_| rejected())?;
    if pins.is_empty() || pins.len() > 16 {
        return Err(rejected());
    }
    let mut selected = pins.into_iter().filter(|p| p.allowed_purpose == purpose);
    let p = selected.next().ok_or_else(rejected)?;
    if selected.next().is_some()
        || p.environment != value("CUSTODY_ROLE_KEY_ENVIRONMENT")?
        || p.issuer_epoch.0 == 0
        || p.config_generation.0 == 0
        || p.public_key == value("CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY")?
        || p.key_id == value("CUSTODY_NATIVE_APPROVAL_KEY_ID")?
    {
        return Err(rejected());
    }
    let floor = AccountMachineIssuerFloorV2 {
        issuer_epoch: p.issuer_epoch,
        config_generation: p.config_generation,
    };
    Ok((p, floor))
}
