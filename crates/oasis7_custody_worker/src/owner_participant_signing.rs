//! Owner key capability. Callers select only an Account-registered operation, never bytes/domains.
use crate::{
    directory_config::{clock, denied},
    participant_runtime_rpc::{ParticipantRuntimeAction, ParticipantRuntimeRpc},
    role_key_store::{
        ParticipantSigningCurrent, ParticipantSigningKey, RoleKeyCurrent, RoleKeyStore,
        participant_bundle_digest,
    },
};
use oasis7_custody_core::{
    DecimalU64,
    http::Role,
    provisioning::{
        ParticipantPermitViewV1, ParticipantProvisionRequestV1, ParticipantSignBundleV1,
        ParticipantStepV1,
    },
    recovery::RecoveryFence,
};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, RequestInit, RequestRedirect, Response, Result};
use zeroize::Zeroizing;
type JResult<T> = std::result::Result<T, JsValue>;
fn rejected() -> JsValue {
    JsValue::from_str("participant owner signing rejected")
}
#[wasm_bindgen(module = "/platform/role_provision_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=verifyParticipantKeyPermit)]
    async fn account(env: &Env, raw: &str) -> JResult<JsValue>;
    #[wasm_bindgen(catch,js_name=readRecoveryFence)]
    async fn read_fence(env: &Env) -> JResult<JsValue>;
}
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readBoundedText)]
    async fn bounded_body(body: &JsValue, limit: u32) -> JResult<JsValue>;
}
async fn rpc<T: DeserializeOwned>(
    env: &Env,
    request: &ParticipantProvisionRequestV1,
    action: ParticipantRuntimeAction,
) -> JResult<T> {
    let raw = serde_json::to_string(&ParticipantRuntimeRpc {
        request: request.clone(),
        action,
    })
    .map_err(|_| rejected())?;
    if raw.len() > 32 * 1024 {
        return Err(rejected());
    }
    let raw = account(env, &raw).await?.as_string().ok_or_else(rejected)?;
    if raw.len() > 32 * 1024 {
        return Err(rejected());
    }
    serde_json::from_str(&raw).map_err(|_| rejected())
}
async fn fence(env: &Env) -> JResult<(RecoveryFence, u64)> {
    let started = clock().map_err(|_| rejected())?;
    let raw = read_fence(env).await?.as_string().ok_or_else(rejected)?;
    if raw.len() > 4096 {
        return Err(rejected());
    }
    Ok((serde_json::from_str(&raw).map_err(|_| rejected())?, started))
}
struct Observation {
    view: ParticipantPermitViewV1,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    fn current(
        &self,
        env: &Env,
        request: &ParticipantProvisionRequestV1,
    ) -> JResult<ParticipantSigningCurrent> {
        let v = &self.view;
        let scope = &v.descriptor.owner_scope;
        let now = clock().map_err(|_| rejected())?;
        let epoch: DecimalU64 = serde_json::from_value(
            env.var("CUSTODY_SERVICE_EPOCH")
                .map_err(|_| rejected())?
                .to_string()
                .into(),
        )
        .map_err(|_| rejected())?;
        let environment = env
            .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
            .map_err(|_| rejected())?
            .to_string();
        if env
            .var("CUSTODY_PARTICIPANT_PROVISION_ENABLED")
            .map_err(|_| rejected())?
            .to_string()
            != "true"
            || &v.request != request
            || v.step != ParticipantStepV1::SignBundle
            || v.permit_id != request.operation_id
            || v.service_epoch != epoch
            || v.target_security_epoch.0 != 1
            || v.observed_at.0 > now
            || now - v.observed_at.0 > 5000
            || now >= v.expires_at.0
            || self.fence_started > now
            || now - self.fence_started > 5000
            || scope.context.identity.role != Role::Owner
            || scope.context.identity.environment != environment
            || scope.context.identity.account_id != request.custody_account_id
            || scope.provision_id != request.provision_id
            || scope.context.identity.key_id != v.descriptor.owner_key.key_id
            || scope.context.identity.key_version != v.descriptor.owner_key.key_version
        {
            return Err(rejected());
        }
        self.fence.check_epoch(epoch.0).map_err(|_| rejected())?;
        scope.validate().map_err(|_| rejected())?;
        Ok(ParticipantSigningCurrent {
            role: RoleKeyCurrent {
                scope: scope.clone(),
                approved: true,
                frozen: false,
                cancelled: false,
                expires_at: v.expires_at,
                authority_revision: v.authority_revision.clone(),
            },
            participant_permit_id: v.permit_id.clone(),
            bundle_digest: participant_bundle_digest(&v.descriptor)?,
        })
    }
}
async fn observe(env: &Env, request: &ParticipantProvisionRequestV1) -> JResult<Observation> {
    let view = rpc(
        env,
        request,
        ParticipantRuntimeAction::GetPermit {
            step: ParticipantStepV1::SignBundle,
        },
    )
    .await?;
    let (fence, fence_started) = fence(env).await?;
    let result = Observation {
        view,
        fence,
        fence_started,
    };
    result.current(env, request)?;
    Ok(result)
}
fn decode(raw: &str) -> Result<ParticipantProvisionRequestV1> {
    if raw.len() > 4096 {
        return Err(denied());
    }
    let request: ParticipantProvisionRequestV1 = serde_json::from_str(raw).map_err(|_| denied())?;
    request.validate().map_err(|_| denied())?;
    Ok(request)
}
pub(crate) async fn sign(env: &Env, raw: &str) -> Result<String> {
    let request = decode(raw)?;
    // First signer oracle: a selected task cannot create/read an arbitrary key actor.
    let observed = observe(env, &request).await.map_err(|_| denied())?;
    let name = observed
        .view
        .descriptor
        .owner_scope
        .task_digest()
        .map_err(|_| denied())?;
    let stub = env
        .durable_object("ROLE_KEYS")?
        .id_from_name(&name)?
        .get_stub()?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_body(Some(raw.into()))
        .with_redirect(RequestRedirect::Manual);
    let mut req =
        Request::new_with_init("https://role-key.internal/internal/participant-sign", &init)?;
    req.headers_mut()?.set("Content-Type", "application/json")?;
    let response = stub.fetch_with_request(req).await?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    let response: web_sys::Response = response.into();
    let raw = bounded_body(response.as_ref(), 32 * 1024)
        .await
        .map_err(|_| denied())?
        .as_string()
        .ok_or_else(denied)?;
    if raw.len() > 32 * 1024 {
        return Err(denied());
    }
    observed.current(env, &request).map_err(|_| denied())?;
    let bundle: ParticipantSignBundleV1 = serde_json::from_str(&raw).map_err(|_| denied())?;
    if bundle.owner.request != observed.view.descriptor.register
        || bundle.read.request != observed.view.descriptor.owner_read
        || bundle.owner.subject_public_key != observed.view.descriptor.owner_key.public_key
        || bundle.read.subject_public_key != observed.view.descriptor.owner_key.public_key
    {
        return Err(denied());
    }
    Ok(raw)
}
pub(crate) async fn sign_in_vault(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let request = decode(raw)?;
    // Second signer oracle, directly from the fixed Owner-only named capability.
    let observed = observe(env, &request).await.map_err(|_| denied())?;
    let descriptor = observed.view.descriptor.clone();
    let text = Zeroizing::new(env.secret("CUSTODY_ROLE_KEY_KEK")?.to_string());
    let mut kek = Zeroizing::new([0u8; 32]);
    if text.len() != 64 {
        return Err(denied());
    }
    hex::decode_to_slice(text.as_bytes(), kek.as_mut_slice()).map_err(|_| denied())?;
    if hex::encode(kek.as_slice()) != text.as_str() {
        return Err(denied());
    }
    let wrapping_version: DecimalU64 = serde_json::from_value(
        env.var("CUSTODY_ROLE_WRAPPING_KEY_VERSION")?
            .to_string()
            .into(),
    )?;
    let current = || observed.current(env, &request);
    let publish_observed = &observed;
    let publish_request = &request;
    let publish = move |candidate: ParticipantSignBundleV1| async move {
        let expected = candidate.clone();
        let committed: ParticipantSignBundleV1 = rpc(
            env,
            publish_request,
            ParticipantRuntimeAction::PublishBundle { candidate },
        )
        .await?;
        let (fence, started) = fence(env).await?;
        let now = clock().map_err(|_| rejected())?;
        if started > now
            || now - started > 5000
            || fence
                .check_epoch(publish_observed.view.service_epoch.0)
                .is_err()
            || committed != expected
        {
            return Err(rejected());
        }
        publish_observed.current(env, publish_request)?;
        Ok(committed)
    };
    let bundle = RoleKeyStore::new(state)
        .sign_participant_bundle(
            ParticipantSigningKey {
                scope: &descriptor.owner_scope,
                wrapping_version,
                kek: &kek,
            },
            &descriptor,
            current,
            publish,
        )
        .await
        .map_err(|_| denied())?;
    Response::from_json(&bundle)
}
