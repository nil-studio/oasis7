//! Fixed manager and role ports. The default HTTP entrypoint exposes none of these.
use crate::{directory_rpc::ReadRpc, owner_test_session_rpc::*};
use oasis7_custody_core::{http::Role, provisioning::*};
use wasm_bindgen::JsValue;
use worker::{Env, Method, Request, RequestInit};
fn denied() -> JsValue {
    JsValue::from_str("test session unavailable")
}
fn diagnostic(stage: &str) {
    #[cfg(feature = "approval-harness")]
    worker::console_warn!("test_session.port.{}", stage);
    #[cfg(not(feature = "approval-harness"))]
    let _ = stage;
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
        diagnostic("account_status_rejected");
        return Err(denied());
    }
    crate::owner_directory::read_response(response)
        .await
        .map_err(|_| denied())
}
macro_rules! management {
    ($name:ident,$rpc:ty,$response:ty,$path:literal) => {
        pub(crate) async fn $name(env: &Env, raw: &str) -> Result<String, JsValue> {
            diagnostic(concat!(stringify!($name), ".entered"));
            if raw.len() > 32768 {
                return Err(denied());
            }
            let rpc: $rpc = serde_json::from_str(raw).map_err(|_| denied())?;
            rpc.request.validate().map_err(|_| denied())?;
            let lookup = serde_json::to_string(&ReadRpc {
                manager_session: rpc.manager_session.clone(),
                provision_id: rpc.provision_id.clone(),
            })
            .map_err(|_| denied())?;
            let view = crate::owner_directory::call_directory(
                env,
                "/internal/read",
                &lookup,
                &rpc.manager_session,
            )
            .await
            .map_err(|_| {
                diagnostic("directory_rejected");
                denied()
            })?;
            diagnostic("directory_returned");
            let view: ProvisionDraftViewV1 = serde_json::from_str(&view).map_err(|_| denied())?;
            if view.entry.provision_id != rpc.provision_id {
                return Err(denied());
            }
            let reply: $response = serde_json::from_str(
                &fetch_account(env, &view.entry.custody_account_id, $path, raw).await?,
            )
            .map_err(|_| denied())?;
            serde_json::to_string(&reply).map_err(|_| denied())
        }
    };
}
management!(
    invite,
    TestSessionInviteRpc,
    PairingInviteOriginalV1,
    "/internal/test-session-invite"
);
management!(
    basis,
    TestSessionBasisRpc,
    TestSessionBasisV1,
    "/internal/test-session-basis"
);
management!(
    prepare,
    TestSessionPrepareRpc,
    TestSessionPrepareResponseV1,
    "/internal/test-session-prepare"
);
management!(
    approve,
    TestSessionApproveRpc,
    TestSessionProgressV1,
    "/internal/test-session-approve"
);
management!(
    continue_session,
    TestSessionContinueRpc,
    TestSessionProgressV1,
    "/internal/test-session-continue"
);
pub(crate) async fn runtime(env: &Env, raw: &str, role: Role) -> Result<String, JsValue> {
    if raw.len() > 128 * 1024 {
        return Err(denied());
    }
    let rpc: TestSessionRuntimeRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    let r = &rpc.request;
    if r.api_version != 1
        || [&r.custody_account_id, &r.provision_id, &r.session_id]
            .iter()
            .any(|v| v.is_empty() || v.len() > 256)
    {
        return Err(denied());
    }
    let path = match role {
        Role::Identity => "/internal/test-session-identity",
        Role::Owner => "/internal/test-session-owner",
        _ => return Err(denied()),
    };
    fetch_account(env, &r.custody_account_id, path, raw).await
}
