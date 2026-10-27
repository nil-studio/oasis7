//! Fixed client coordinator. SID/CSRF remain inside the NativeSession actor.
#[path = "identity_test_session_client_observation.rs"]
mod observation;
#[path = "identity_test_session_client_private.rs"]
mod private;
use crate::{
    identity_native_runtime::{actor, js_denied, manifest_name_parts, raw_text},
    owner_test_session_rpc::TestSessionSelectorV1,
    test_session_client_rpc::*,
};
use oasis7_custody_core::{
    approval::{ApprovalVerifyRequestV1, SignedApprovalVerificationV1},
    native_binding::SignedManagedNativeSessionLifetimeV1,
    provisioning::{SessionClientOperation, SessionControlKindV1},
};
use observation::Observation;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use worker::Env;
type Result<T> = std::result::Result<T, JsValue>;
#[wasm_bindgen(module = "/platform/test_session_client_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = ownerClientCall)]
    async fn call(env: &Env, operation: &str, raw: &str) -> Result<JsValue>;
}
fn tag(operation: SessionClientOperation) -> &'static str {
    match operation {
        SessionClientOperation::Challenge => "challenge",
        SessionClientOperation::CredentialClaim => "claim",
        SessionClientOperation::Control(kind) => match kind {
            SessionControlKindV1::Initialize => "initialize",
            SessionControlKindV1::ToolsList => "tools-list",
            SessionControlKindV1::Heartbeat => "heartbeat",
            SessionControlKindV1::Finish => "finish",
            SessionControlKindV1::Abort => "abort",
        },
    }
}
pub(super) fn operation(path: &str) -> Option<SessionClientOperation> {
    Some(match path {
        "/internal/client-challenge" => SessionClientOperation::Challenge,
        "/internal/client-claim" => SessionClientOperation::CredentialClaim,
        "/internal/client-initialize" => {
            SessionClientOperation::Control(SessionControlKindV1::Initialize)
        }
        "/internal/client-tools-list" => {
            SessionClientOperation::Control(SessionControlKindV1::ToolsList)
        }
        "/internal/client-heartbeat" => {
            SessionClientOperation::Control(SessionControlKindV1::Heartbeat)
        }
        // Finish/Abort are handled without SID access, even when native is unavailable.
        _ => return None,
    })
}
async fn rpc_raw(
    env: &Env,
    admission: &TestSessionClientAdmissionRpc,
    op: SessionClientOperation,
    action: TestSessionClientAction,
) -> Result<String> {
    let raw = zeroize::Zeroizing::new(
        serde_json::to_string(&TestSessionClientStageRpc {
            admission: admission.clone(),
            action,
        })
        .map_err(|_| js_denied())?,
    );
    if raw.len() > 128 * 1024 {
        return Err(js_denied());
    }
    Ok(raw_text(call(env, tag(op), &raw).await?, 128 * 1024)?.to_string())
}
async fn rpc<T: DeserializeOwned>(
    env: &Env,
    admission: &TestSessionClientAdmissionRpc,
    op: SessionClientOperation,
    action: TestSessionClientAction,
) -> Result<T> {
    let raw = zeroize::Zeroizing::new(rpc_raw(env, admission, op, action).await?);
    serde_json::from_str(&raw).map_err(|_| js_denied())
}
fn check(env: &Env, selector: &TestSessionSelectorV1, observation: &Observation) -> Result<()> {
    observation.check(env, selector)
}
pub(crate) async fn run(
    env: &Env,
    admission: TestSessionClientAdmissionRpc,
    op: SessionClientOperation,
) -> Result<String> {
    let _: serde_json::Value = rpc(env, &admission, op, TestSessionClientAction::Admit).await?;
    if matches!(
        op,
        SessionClientOperation::Control(SessionControlKindV1::Finish | SessionControlKindV1::Abort)
    ) {
        return rpc_raw(
            env,
            &admission,
            op,
            TestSessionClientAction::Complete { lifetime: None },
        )
        .await;
    }
    let request: ApprovalVerifyRequestV1 =
        rpc(env, &admission, op, TestSessionClientAction::ClaimSource).await?;
    let signed: SignedApprovalVerificationV1 =
        crate::native_identity::NativeIdentityConfig::from_env(env)
            .map_err(|_| js_denied())?
            .ok_or_else(js_denied)?
            .verify_approval(&request)
            .await
            .map_err(|_| js_denied())?;
    let _: () = rpc(
        env,
        &admission,
        op,
        TestSessionClientAction::RecordSource {
            signed: Box::new(signed),
        },
    )
    .await?;
    let selector = &admission.http.selector;
    let name = manifest_name_parts(env, &selector.custody_account_id, &selector.provision_id)
        .map_err(|_| js_denied())?;
    let reply: serde_json::Value = actor(
        env,
        &name,
        &format!("/internal/client-{}", tag(op)),
        &admission,
    )
    .await?;
    serde_json::to_string(&reply).map_err(|_| js_denied())
}
pub(super) async fn response(
    state: JsValue,
    env: &Env,
    path: &str,
    raw: &str,
) -> worker::Result<worker::Response> {
    let op = operation(path).ok_or_else(super::denied)?;
    let admission: TestSessionClientAdmissionRpc =
        serde_json::from_str(raw).map_err(|_| super::denied())?;
    let reply = execute(state, env, &admission, op)
        .await
        .map_err(|_| super::denied())?;
    let mut response = worker::Response::ok(reply)?;
    response
        .headers_mut()
        .set("content-type", "application/json")?;
    response.headers_mut().set("cache-control", "no-store")?;
    Ok(response)
}
async fn execute(
    state: JsValue,
    env: &Env,
    admission: &TestSessionClientAdmissionRpc,
    op: SessionClientOperation,
) -> Result<String> {
    let selector = &admission.http.selector;
    let first = Observation::new(
        env,
        selector,
        rpc(
            env,
            admission,
            op,
            TestSessionClientAction::Observe {
                index: TestSessionClientObservation::PreDecrypt,
            },
        )
        .await?,
        1,
    )
    .await?;
    let secrets = private::read(state, env, selector, &first).await?;
    let second = Observation::new(
        env,
        selector,
        rpc(
            env,
            admission,
            op,
            TestSessionClientAction::Observe {
                index: TestSessionClientObservation::PostDecrypt,
            },
        )
        .await?,
        2,
    )
    .await?;
    if first.fingerprint()? != second.fingerprint()? {
        return Err(js_denied());
    }
    let current = Observation::new(
        env,
        selector,
        rpc(env, admission, op, TestSessionClientAction::ClaimLifetime).await?,
        3,
    )
    .await?;
    if second.fingerprint()? != current.fingerprint()? {
        return Err(js_denied());
    }
    check(env, selector, &current)?;
    use crate::native_identity::provision_transport::{
        NativePrivateRoute, NativeProvisionTransport,
    };
    let transport = NativeProvisionTransport::from_env(env).map_err(|_| js_denied())?;
    let lifetime: SignedManagedNativeSessionLifetimeV1 = transport
        .request(
            NativePrivateRoute::SessionLifetime,
            &serde_json::to_string(&serde_json::json!({"nonce":current.descriptor.lifetime_nonce}))
                .map_err(|_| js_denied())?,
            Some(&secrets.cookie),
            Some(std::str::from_utf8(secrets.csrf.as_bytes()).map_err(|_| js_denied())?),
        )
        .await
        .map_err(|_| js_denied())?
        .body;
    transport
        .fact_pin()
        .session_lifetime(
            &lifetime,
            &current.descriptor.native_binding,
            &current.descriptor.lifetime_nonce,
            current.descriptor.session.native_binding_generation,
            current.service_epoch,
        )
        .await?;
    check(env, selector, &current)?;
    if lifetime.lifetime.status.session_ref != current.descriptor.session.session_ref {
        return Err(js_denied());
    }
    rpc_raw(
        env,
        admission,
        op,
        TestSessionClientAction::Complete {
            lifetime: Some(Box::new(lifetime)),
        },
    )
    .await
}
