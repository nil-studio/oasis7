//! Fixed private service entrypoints. No caller-selected route or actor role.
use wasm_bindgen::prelude::*;
use worker::Env;
fn rejected() -> JsValue {
    JsValue::from_str("custody service request rejected")
}
#[cfg(feature = "identity")]
#[wasm_bindgen]
pub async fn native_verify_approval(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let request: oasis7_custody_core::approval::ApprovalVerifyRequestV1 =
        serde_json::from_str(&raw).map_err(|_| rejected())?;
    let config = crate::native_identity::NativeIdentityConfig::from_env(&env)
        .map_err(|_| rejected())?
        .ok_or_else(rejected)?;
    let response = config
        .verify_approval(&request)
        .await
        .map_err(|_| rejected())?;
    serde_json::to_string(&response).map_err(|_| rejected())
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_reserve_draft(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let rpc: crate::directory_rpc::ReserveRpc =
        serde_json::from_str(&raw).map_err(|_| rejected())?;
    crate::owner_directory::call_directory(&env, "/internal/reserve", &raw, &rpc.manager_session)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_read_draft(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let rpc: crate::directory_rpc::ReadRpc = serde_json::from_str(&raw).map_err(|_| rejected())?;
    crate::owner_directory::call_directory(&env, "/internal/read", &raw, &rpc.manager_session)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_freeze_manager(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let rpc: crate::directory_rpc::FreezeRpc =
        serde_json::from_str(&raw).map_err(|_| rejected())?;
    crate::owner_directory::call_directory(&env, "/internal/freeze", &raw, &rpc.manager_session)
        .await
        .map_err(|_| rejected())
}

#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_close_source_event(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    crate::owner_directory::source_revocation::call(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "identity")]
#[wasm_bindgen]
pub async fn native_pump_revocations(env: Env) -> std::result::Result<(), JsValue> {
    crate::native_identity::source_revocation::pump(&env)
        .await
        .map_err(|_| rejected())
}
