//! Fixed private service entrypoints. No caller-selected route or actor role.
use wasm_bindgen::prelude::*;
use worker::Env;
#[cfg(feature = "identity")]
#[wasm_bindgen]
pub async fn identity_run_native(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    crate::identity_native_runtime::run_native_raw(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "identity")]
#[wasm_bindgen]
pub async fn identity_read_native_history(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    crate::identity_native_runtime::history::read_native_history_raw(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "identity")]
#[wasm_bindgen]
pub async fn identity_recover_native_status(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    crate::identity_native_runtime::recovery::recover_native_status_raw(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "identity")]
#[wasm_bindgen]
pub async fn identity_read_missing_native_session(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    crate::identity_native_runtime::history::read_missing_native_session_raw(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "owner")]
#[path = "owner_provision_entrypoints.rs"]
mod provisioning;
#[cfg(feature = "owner")]
#[path = "owner_setup_entrypoints.rs"]
mod setup;
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_approve_owner_setup(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    setup::approve(&env, &raw).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_continue_owner_setup(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    setup::advance(&env, &raw).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_setup_identity_call(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    setup::runtime(&env, &raw, oasis7_custody_core::http::Role::Identity).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_setup_key_call(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    setup::runtime(&env, &raw, oasis7_custody_core::http::Role::Owner).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_setup_basis(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    setup::basis(&env, &raw).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_setup_prepare(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    setup::prepare(&env, &raw).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_sign_owner_setup(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    crate::owner_setup_signing::sign(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "owner")]
#[path = "owner_setup_reconcile_entrypoints.rs"]
mod setup_reconcile;
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_approve_owner_setup_reconcile(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    setup_reconcile::approve(&env, &raw).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_continue_owner_setup_reconcile(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    setup_reconcile::advance(&env, &raw).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_setup_reconcile_identity_call(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    setup_reconcile::runtime(&env, &raw, oasis7_custody_core::http::Role::Identity).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_setup_reconcile_key_call(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    setup_reconcile::runtime(&env, &raw, oasis7_custody_core::http::Role::Owner).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_setup_reconcile_basis(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    setup_reconcile::basis(&env, &raw).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_setup_reconcile_prepare(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    setup_reconcile::prepare(&env, &raw).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_sign_owner_setup_reconcile(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    crate::owner_setup_reconcile_signing::sign(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "identity")]
#[wasm_bindgen]
pub async fn identity_run_owner_setup(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    crate::identity_native_runtime::setup::run(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "identity")]
#[wasm_bindgen]
pub async fn identity_run_owner_setup_reconcile(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    crate::identity_native_runtime::setup_reconcile::run(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "identity")]
#[wasm_bindgen]
pub async fn identity_run_participant(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    crate::identity_native_runtime::participant::run(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_participant_identity_call(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    provisioning::participant(&env, &raw, oasis7_custody_core::http::Role::Identity).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_participant_key_call(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    provisioning::participant(&env, &raw, oasis7_custody_core::http::Role::Owner).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_sign_participant(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    crate::owner_participant_signing::sign(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_approve_managed_login(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    provisioning::managed_login(&env, &raw).await
}

#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_continue_managed_login(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    provisioning::managed_login_continue(&env, &raw).await
}

#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_native_call(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    provisioning::native(&env, &raw).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_native_recovery_call(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    provisioning::native_recovery(&env, &raw).await
}

#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_advance_provision(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    provisioning::management(&env, &raw, true).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_read_provision_progress(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    provisioning::management(&env, &raw, false).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_identity_key_permit(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    provisioning::permit(&env, &raw, oasis7_custody_core::http::Role::Identity).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_owner_key_permit(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    provisioning::permit(&env, &raw, oasis7_custody_core::http::Role::Owner).await
}
#[cfg(feature = "owner")]
#[wasm_bindgen]
pub async fn owner_executor_key_permit(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    provisioning::permit(&env, &raw, oasis7_custody_core::http::Role::Executor).await
}
fn rejected() -> JsValue {
    JsValue::from_str("custody service request rejected")
}
#[cfg(any(feature = "identity", feature = "owner", feature = "executor"))]
#[wasm_bindgen]
pub async fn ensure_role_key(env: Env, raw: String) -> std::result::Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(rejected());
    }
    crate::role_provision::ensure_role_key(&env, &raw)
        .await
        .map_err(|_| rejected())
}
#[cfg(feature = "recovery-fence")]
#[wasm_bindgen]
pub async fn read_recovery_fence(env: Env) -> std::result::Result<String, JsValue> {
    use worker::{Method, Request};
    let stub = env
        .durable_object("FENCES")
        .map_err(|_| rejected())?
        .id_from_name("service-fence-v1")
        .map_err(|_| rejected())?
        .get_stub()
        .map_err(|_| rejected())?;
    let response = stub
        .fetch_with_request(
            Request::new(
                "https://fence.internal/internal/recovery-fence",
                Method::Get,
            )
            .map_err(|_| rejected())?,
        )
        .await
        .map_err(|_| rejected())?;
    if response.status_code() != 200 {
        return Err(rejected());
    }
    #[wasm_bindgen(module = "/platform/sql.mjs")]
    extern "C" {
        #[wasm_bindgen(catch, js_name = readBoundedText)]
        async fn read_body(body: &JsValue, limit: u32) -> std::result::Result<JsValue, JsValue>;
    }
    let raw: web_sys::Response = response.into();
    let text = read_body(raw.as_ref(), 1024)
        .await?
        .as_string()
        .ok_or_else(rejected)?;
    let _: oasis7_custody_core::recovery::RecoveryFence =
        serde_json::from_str(&text).map_err(|_| rejected())?;
    Ok(text)
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
pub async fn owner_approve_provision(
    env: Env,
    raw: String,
) -> std::result::Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(rejected());
    }
    crate::owner_directory::approve_provision(&env, &raw)
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
