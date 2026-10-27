//! Fixed manager-to-Account setup transport. This never chooses a caller's actor.
use crate::{directory_rpc::ReadRpc, owner_setup_rpc::*};
use oasis7_custody_core::{approval::CustodySessionStatusV1, provisioning::*};
use wasm_bindgen::JsValue;
use worker::{Env, Method, Request, RequestInit};

fn denied() -> JsValue {
    JsValue::from_str("owner setup unavailable")
}

async fn draft(
    env: &Env,
    manager: &CustodySessionStatusV1,
    provision: &str,
) -> Result<ProvisionDraftViewV1, JsValue> {
    let raw = serde_json::to_string(&ReadRpc {
        manager_session: manager.clone(),
        provision_id: provision.into(),
    })
    .map_err(|_| denied())?;
    let view = crate::owner_directory::call_directory(env, "/internal/read", &raw, manager)
        .await
        .map_err(|_| denied())?;
    let view: ProvisionDraftViewV1 = serde_json::from_str(&view).map_err(|_| denied())?;
    if view.entry.provision_id != provision {
        return Err(denied());
    }
    Ok(view)
}

async fn fetch_account(
    env: &Env,
    account: &str,
    path: &'static str,
    raw: &str,
) -> Result<String, JsValue> {
    let stub = env
        .durable_object("ACCOUNTS")
        .map_err(|_| denied())?
        .id_from_name(account)
        .map_err(|_| denied())?
        .get_stub()
        .map_err(|_| denied())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let mut request = Request::new_with_init(&format!("https://account.internal{path}"), &init)
        .map_err(|_| denied())?;
    request
        .headers_mut()
        .map_err(|_| denied())?
        .set("Content-Type", "application/json")
        .map_err(|_| denied())?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| denied())?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    crate::owner_directory::read_response(response)
        .await
        .map_err(|_| denied())
}
async fn forward(
    env: &Env,
    account: &str,
    path: &'static str,
    raw: &str,
) -> Result<OwnerSetupProgressV1, JsValue> {
    serde_json::from_str(&fetch_account(env, account, path, raw).await?).map_err(|_| denied())
}

pub(super) async fn basis(env: &Env, raw: &str) -> Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let rpc: OwnerSetupBasisRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    rpc.request.validate().map_err(|_| denied())?;
    let view = draft(env, &rpc.manager_session, &rpc.provision_id).await?;
    let value: OwnerSetupBasisV1 = serde_json::from_str(
        &fetch_account(
            env,
            &view.entry.custody_account_id,
            "/internal/owner-setup-basis",
            raw,
        )
        .await?,
    )
    .map_err(|_| denied())?;
    if value.api_version != 1
        || value.custody_account_id != view.entry.custody_account_id
        || value.provision_id != rpc.provision_id
    {
        return Err(denied());
    }
    serde_json::to_string(&value).map_err(|_| denied())
}

pub(super) async fn prepare(env: &Env, raw: &str) -> Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let rpc: OwnerSetupPrepareRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    rpc.request.validate().map_err(|_| denied())?;
    let view = draft(env, &rpc.manager_session, &rpc.provision_id).await?;
    if rpc.request.plan.provision_id != rpc.provision_id {
        return Err(denied());
    }
    let value: OwnerSetupPrepareResponseV1 = serde_json::from_str(
        &fetch_account(
            env,
            &view.entry.custody_account_id,
            "/internal/owner-setup-prepare",
            raw,
        )
        .await?,
    )
    .map_err(|_| denied())?;
    if value.context.custody_account_id != view.entry.custody_account_id
        || value.approval_request_id != rpc.request.approval_request_id
        || value.intent != rpc.request.plan.intent().map_err(|_| denied())?
    {
        return Err(denied());
    }
    serde_json::to_string(&value).map_err(|_| denied())
}
pub(super) async fn runtime(
    env: &Env,
    raw: &str,
    role: oasis7_custody_core::http::Role,
) -> Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let rpc: OwnerSetupRuntimeRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    rpc.request.validate().map_err(|_| denied())?;
    let path = match role {
        oasis7_custody_core::http::Role::Identity => "/internal/owner-setup-identity",
        oasis7_custody_core::http::Role::Owner => "/internal/owner-setup-owner",
        _ => return Err(denied()),
    };
    fetch_account(env, &rpc.request.custody_account_id, path, raw).await
}

pub(super) async fn approve(env: &Env, raw: &str) -> Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let rpc: OwnerSetupApproveRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    rpc.request.validate().map_err(|_| denied())?;
    let view = draft(env, &rpc.manager_session, &rpc.provision_id).await?;
    if rpc.request.plan.provision_id != rpc.provision_id
        || rpc.request.input.request.context.custody_account_id != view.entry.custody_account_id
    {
        return Err(denied());
    }
    let result = forward(
        env,
        &view.entry.custody_account_id,
        "/internal/owner-setup-approve",
        raw,
    )
    .await?;
    if result.api_version != 1
        || result.custody_account_id != view.entry.custody_account_id
        || result.provision_id != rpc.provision_id
        || result.setup_id != rpc.request.plan.setup_id
    {
        return Err(denied());
    }
    serde_json::to_string(&result).map_err(|_| denied())
}

pub(super) async fn advance(env: &Env, raw: &str) -> Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let rpc: OwnerSetupContinueRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    rpc.request.validate().map_err(|_| denied())?;
    let view = draft(env, &rpc.manager_session, &rpc.provision_id).await?;
    if rpc.request.provision_id != rpc.provision_id
        || rpc.request.custody_account_id != view.entry.custody_account_id
    {
        return Err(denied());
    }
    let result = forward(
        env,
        &view.entry.custody_account_id,
        "/internal/owner-setup-continue",
        raw,
    )
    .await?;
    if result.api_version != 1
        || result.custody_account_id != view.entry.custody_account_id
        || result.provision_id != rpc.provision_id
        || result.setup_id != rpc.request.setup_id
    {
        return Err(denied());
    }
    serde_json::to_string(&result).map_err(|_| denied())
}
