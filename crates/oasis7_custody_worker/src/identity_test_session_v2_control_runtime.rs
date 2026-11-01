//! Fixed client coordinator. SID/CSRF remain inside the NativeSession actor.
use super::credentials_v2::{observation::Observation, private};
use crate::{
    identity_native_runtime::{actor, js_denied, manifest_name_parts, raw_text},
    owner_test_session_rpc::TestSessionSelectorV1,
    test_session_client_rpc::TestSessionClientAdmissionRpc,
    test_session_v2_control_rpc::*,
};
use oasis7_custody_core::{
    approval::{ApprovalVerifyRequestV1, SignedApprovalVerificationV1},
    native_binding::SignedManagedNativeSessionLifetimeV1,
    provisioning::{SessionClientOperation, SessionControlKindV1, SessionControlRequestV1},
};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use worker::Env;
type Result<T> = std::result::Result<T, JsValue>;
#[wasm_bindgen(module = "/platform/test_session_client_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = ownerControlV2Call)]
    async fn call(env: &Env, operation: &str, raw: &str) -> Result<JsValue>;
}
fn tag(operation: SessionClientOperation) -> Result<&'static str> {
    match operation {
        SessionClientOperation::Challenge => Ok("challenge"),
        SessionClientOperation::Control(SessionControlKindV1::Initialize) => Ok("initialize"),
        SessionClientOperation::Control(SessionControlKindV1::ToolsList) => Ok("tools-list"),
        SessionClientOperation::Control(SessionControlKindV1::Heartbeat) => Ok("heartbeat"),
        SessionClientOperation::Control(SessionControlKindV1::Finish) => Ok("finish"),
        SessionClientOperation::Control(SessionControlKindV1::Abort) => Ok("abort"),
        _ => Err(js_denied()),
    }
}
pub(super) fn operation(path: &str) -> Option<SessionClientOperation> {
    match path {
        "/internal/client-v2-control-challenge" => Some(SessionClientOperation::Challenge),
        "/internal/client-v2-control-initialize" => Some(SessionClientOperation::Control(
            SessionControlKindV1::Initialize,
        )),
        "/internal/client-v2-control-tools-list" => Some(SessionClientOperation::Control(
            SessionControlKindV1::ToolsList,
        )),
        "/internal/client-v2-control-heartbeat" => Some(SessionClientOperation::Control(
            SessionControlKindV1::Heartbeat,
        )),
        // Pure Finish/Abort must be completed by the first Account Admit.
        _ => None,
    }
}
async fn rpc_raw(
    env: &Env,
    admission: &ControlBoundAdmissionV2,
    op: SessionClientOperation,
    action: TestSessionControlActionV2,
) -> Result<String> {
    let raw = zeroize::Zeroizing::new(
        serde_json::to_string(&TestSessionControlStageRpcV2 {
            api_version: 2,
            admission: admission.original.clone(),
            approved_input_digest: admission.approved_input_digest.clone(),
            action,
        })
        .map_err(|_| js_denied())?,
    );
    if raw.len() > 128 * 1024 {
        return Err(js_denied());
    }
    Ok(raw_text(call(env, tag(op)?, &raw).await?, 128 * 1024)?.to_string())
}
async fn rpc<T: DeserializeOwned>(
    env: &Env,
    admission: &ControlBoundAdmissionV2,
    op: SessionClientOperation,
    action: TestSessionControlActionV2,
) -> Result<T> {
    let raw = zeroize::Zeroizing::new(rpc_raw(env, admission, op, action).await?);
    serde_json::from_str(&raw).map_err(|_| js_denied())
}
fn check(env: &Env, selector: &TestSessionSelectorV1, observation: &Observation) -> Result<()> {
    observation.check(env, selector)
}
pub(crate) async fn continue_after_admit(
    env: &Env,
    mut original: TestSessionClientAdmissionRpc,
    op: SessionClientOperation,
    routing: ControlAdmissionRoutingV2,
) -> Result<String> {
    let _ = tag(op)?;
    let valid_digest = |v: &str| {
        v.len() == 64
            && v.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    };
    if routing.domain != CONTROL_ROUTING_DOMAIN_V2
        || routing.api_version != 2
        || routing.session_id != original.http.selector.session_id
        || routing.ingress_attempt_id != original.ingress_attempt_id
        || routing.operation
            != ControlRoutingOperationV2::from_operation(op).map_err(|_| js_denied())?
        || routing.raw_body_digest
            != oasis7_custody_core::approval::bytes_digest(original.http.original.body.as_bytes())
        || !valid_digest(&routing.approved_input_digest)
        || original.http.selector.api_version != 1
        || env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
    {
        return Err(js_denied());
    }
    // The original Account Admit already committed this actual ingress. Do not
    // send another Admit or regenerate the ingress ID/body/headers.
    if let Some(reply) = routing.immediate_reply {
        let q: SessionControlRequestV1 =
            serde_json::from_str(&original.http.original.body).map_err(|_| js_denied())?;
        match op {
            SessionClientOperation::Control(kind)
                if kind == q.kind && reply.kind == kind && reply.request_id == q.request_id => {}
            _ => return Err(js_denied()),
        }
        return serde_json::to_string(&reply).map_err(|_| js_denied());
    }
    if matches!(
        op,
        SessionClientOperation::Control(SessionControlKindV1::Finish | SessionControlKindV1::Abort)
    ) {
        return Err(js_denied());
    }
    original.http.selector.api_version = 2;
    let admission = ControlBoundAdmissionV2 {
        approved_input_digest: routing.approved_input_digest,
        original,
    };
    let request: ApprovalVerifyRequestV1 =
        rpc(env, &admission, op, TestSessionControlActionV2::ClaimSource).await?;
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
        TestSessionControlActionV2::RecordSource {
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
        &format!("/internal/client-v2-control-{}", tag(op)?),
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
    let admission: ControlBoundAdmissionV2 =
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
    admission: &ControlBoundAdmissionV2,
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
            TestSessionControlActionV2::Observe {
                index: TestSessionCredentialObservationV2::PreDecrypt,
            },
        )
        .await?,
        1,
    )
    .await?;
    if first.descriptor.approved_input_digest != admission.approved_input_digest {
        return Err(js_denied());
    }
    let secrets = private::read(state, env, selector, &first).await?;
    let second = Observation::new(
        env,
        selector,
        rpc(
            env,
            admission,
            op,
            TestSessionControlActionV2::Observe {
                index: TestSessionCredentialObservationV2::PostDecrypt,
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
        rpc(
            env,
            admission,
            op,
            TestSessionControlActionV2::ClaimLifetime,
        )
        .await?,
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
        TestSessionControlActionV2::Complete {
            lifetime: Box::new(lifetime),
        },
    )
    .await
}
