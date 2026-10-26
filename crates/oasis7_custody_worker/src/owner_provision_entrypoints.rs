//! Fixed Owner service factories. No request-selected route or actor role.
use super::rejected;
use wasm_bindgen::JsValue;
use worker::{Env, Method, Request, RequestInit};

pub(super) async fn managed_login_continue(
    env: &Env,
    raw: &str,
) -> std::result::Result<String, JsValue> {
    use crate::directory_rpc::{DriveRpc, ReadRpc};
    use oasis7_custody_core::provisioning::{ManagedLoginApprovalViewV1, ProvisionDraftViewV1};
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let rpc: DriveRpc = serde_json::from_str(raw).map_err(|_| rejected())?;
    rpc.request.validate().map_err(|_| rejected())?;
    let read = serde_json::to_string(&ReadRpc {
        manager_session: rpc.manager_session.clone(),
        provision_id: rpc.provision_id.clone(),
    })
    .map_err(|_| rejected())?;
    let view =
        crate::owner_directory::call_directory(env, "/internal/read", &read, &rpc.manager_session)
            .await
            .map_err(|_| rejected())?;
    let view: ProvisionDraftViewV1 = serde_json::from_str(&view).map_err(|_| rejected())?;
    if view.entry.provision_id != rpc.provision_id {
        return Err(rejected());
    }
    let stub = env
        .durable_object("ACCOUNTS")
        .map_err(|_| rejected())?
        .id_from_name(&view.entry.custody_account_id)
        .map_err(|_| rejected())?
        .get_stub()
        .map_err(|_| rejected())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let mut request = Request::new_with_init(
        "https://account.internal/internal/managed-login-continue",
        &init,
    )
    .map_err(|_| rejected())?;
    request
        .headers_mut()
        .map_err(|_| rejected())?
        .set("Content-Type", "application/json")
        .map_err(|_| rejected())?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| rejected())?;
    if response.status_code() != 200 {
        return Err(rejected());
    }
    let raw = crate::owner_directory::read_response(response)
        .await
        .map_err(|_| rejected())?;
    let result: ManagedLoginApprovalViewV1 = serde_json::from_str(&raw).map_err(|_| rejected())?;
    if result.api_version != 1
        || result.provision_id != rpc.provision_id
        || result.custody_account_id != view.entry.custody_account_id
        || result.operation_id != rpc.request.operation_id
    {
        return Err(rejected());
    }
    Ok(raw)
}

pub(super) async fn managed_login(env: &Env, raw: &str) -> std::result::Result<String, JsValue> {
    use crate::directory_rpc::{ManagedLoginApproveRpc, ReadRpc};
    use oasis7_custody_core::provisioning::{ManagedLoginApprovalViewV1, ProvisionDraftViewV1};
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let rpc: ManagedLoginApproveRpc = serde_json::from_str(raw).map_err(|_| rejected())?;
    rpc.request.validate().map_err(|_| rejected())?;
    let read = serde_json::to_string(&ReadRpc {
        manager_session: rpc.manager_session.clone(),
        provision_id: rpc.provision_id.clone(),
    })
    .map_err(|_| rejected())?;
    let view =
        crate::owner_directory::call_directory(env, "/internal/read", &read, &rpc.manager_session)
            .await
            .map_err(|_| rejected())?;
    let view: ProvisionDraftViewV1 = serde_json::from_str(&view).map_err(|_| rejected())?;
    if view.entry.provision_id != rpc.provision_id
        || view.entry.custody_account_id != rpc.request.input.request.context.custody_account_id
    {
        return Err(rejected());
    }
    let stub = env
        .durable_object("ACCOUNTS")
        .map_err(|_| rejected())?
        .id_from_name(&view.entry.custody_account_id)
        .map_err(|_| rejected())?
        .get_stub()
        .map_err(|_| rejected())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let mut request = Request::new_with_init(
        "https://account.internal/internal/managed-login-approve",
        &init,
    )
    .map_err(|_| rejected())?;
    request
        .headers_mut()
        .map_err(|_| rejected())?
        .set("Content-Type", "application/json")
        .map_err(|_| rejected())?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| rejected())?;
    if response.status_code() != 200 {
        return Err(rejected());
    }
    let raw = crate::owner_directory::read_response(response)
        .await
        .map_err(|_| rejected())?;
    let result: ManagedLoginApprovalViewV1 = serde_json::from_str(&raw).map_err(|_| rejected())?;
    if result.api_version != 1
        || result.provision_id != rpc.provision_id
        || result.custody_account_id != view.entry.custody_account_id
        || result.operation_id != rpc.request.input.operation_id
    {
        return Err(rejected());
    }
    Ok(raw)
}

pub(super) async fn native(env: &Env, raw: &str) -> std::result::Result<String, JsValue> {
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let rpc: crate::native_runtime_rpc::NativeRuntimeRpc =
        serde_json::from_str(raw).map_err(|_| rejected())?;
    if !rpc.call.validate() {
        return Err(rejected());
    }
    let stub = env
        .durable_object("ACCOUNTS")
        .map_err(|_| rejected())?
        .id_from_name(rpc.call.account_id())
        .map_err(|_| rejected())?
        .get_stub()
        .map_err(|_| rejected())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let mut request = Request::new_with_init("https://account.internal/internal/native", &init)
        .map_err(|_| rejected())?;
    request
        .headers_mut()
        .map_err(|_| rejected())?
        .set("Content-Type", "application/json")
        .map_err(|_| rejected())?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| rejected())?;
    if response.status_code() != 200 {
        return Err(rejected());
    }
    crate::owner_directory::read_response(response)
        .await
        .map_err(|_| rejected())
}

pub(super) async fn management(
    env: &Env,
    raw: &str,
    advance: bool,
) -> std::result::Result<String, JsValue> {
    use crate::directory_rpc::{DriveRpc, ReadRpc};
    use oasis7_custody_core::provisioning::{ProvisionDraftViewV1, ProvisionProgressViewV1};
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let (manager, provision_id) = if advance {
        let rpc: DriveRpc = serde_json::from_str(raw).map_err(|_| rejected())?;
        rpc.request.validate().map_err(|_| rejected())?;
        (rpc.manager_session, rpc.provision_id)
    } else {
        let rpc: ReadRpc = serde_json::from_str(raw).map_err(|_| rejected())?;
        (rpc.manager_session, rpc.provision_id)
    };
    let read = serde_json::to_string(&ReadRpc {
        manager_session: manager.clone(),
        provision_id: provision_id.clone(),
    })
    .map_err(|_| rejected())?;
    let view = crate::owner_directory::call_directory(env, "/internal/read", &read, &manager)
        .await
        .map_err(|_| rejected())?;
    let view: ProvisionDraftViewV1 = serde_json::from_str(&view).map_err(|_| rejected())?;
    if view.entry.provision_id != provision_id {
        return Err(rejected());
    }
    let stub = env
        .durable_object("ACCOUNTS")
        .map_err(|_| rejected())?
        .id_from_name(&view.entry.custody_account_id)
        .map_err(|_| rejected())?
        .get_stub()
        .map_err(|_| rejected())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let mut request = Request::new_with_init(
        if advance {
            "https://account.internal/internal/advance"
        } else {
            "https://account.internal/internal/provision-progress"
        },
        &init,
    )
    .map_err(|_| rejected())?;
    request
        .headers_mut()
        .map_err(|_| rejected())?
        .set("Content-Type", "application/json")
        .map_err(|_| rejected())?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| rejected())?;
    if response.status_code() != 200 {
        return Err(rejected());
    }
    let raw = crate::owner_directory::read_response(response)
        .await
        .map_err(|_| rejected())?;
    let progress: ProvisionProgressViewV1 = serde_json::from_str(&raw).map_err(|_| rejected())?;
    if progress.api_version != 1
        || progress.provision_id != provision_id
        || progress.custody_account_id != view.entry.custody_account_id
    {
        return Err(rejected());
    }
    Ok(raw)
}

pub(super) async fn permit(
    env: &Env,
    raw: &str,
    role: oasis7_custody_core::http::Role,
) -> std::result::Result<String, JsValue> {
    use oasis7_custody_core::provisioning::{
        ProvisionRoleKeyPermitViewV1, ProvisionRoleKeyRequestV1,
    };
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let request: ProvisionRoleKeyRequestV1 = serde_json::from_str(raw).map_err(|_| rejected())?;
    request.validate().map_err(|_| rejected())?;
    let stub = env
        .durable_object("ACCOUNTS")
        .map_err(|_| rejected())?
        .id_from_name(&request.custody_account_id)
        .map_err(|_| rejected())?
        .get_stub()
        .map_err(|_| rejected())?;
    let raw = serde_json::to_string(&crate::directory_rpc::RolePermitRpc {
        role,
        request: request.clone(),
    })
    .map_err(|_| rejected())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let mut rpc = Request::new_with_init("https://account.internal/internal/role-permit", &init)
        .map_err(|_| rejected())?;
    rpc.headers_mut()
        .map_err(|_| rejected())?
        .set("Content-Type", "application/json")
        .map_err(|_| rejected())?;
    let response = stub.fetch_with_request(rpc).await.map_err(|_| rejected())?;
    if response.status_code() != 200 {
        return Err(rejected());
    }
    let raw = crate::owner_directory::read_response(response)
        .await
        .map_err(|_| rejected())?;
    let view: ProvisionRoleKeyPermitViewV1 = serde_json::from_str(&raw).map_err(|_| rejected())?;
    view.validate_for(&request, role).map_err(|_| rejected())?;
    view.validate_at(crate::directory_config::clock().map_err(|_| rejected())?)
        .map_err(|_| rejected())?;
    Ok(raw)
}
