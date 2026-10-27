//! Fixed setup signer factory. Only Account-derived typed requests reach the Owner key.
use crate::{
    directory_config::{clock, denied},
    owner_setup_rpc::*,
    role_key_store::{
        ParticipantSigningKey, RoleKeyCurrent, RoleKeyStore, SetupSigningCurrent,
        setup_bundle_digest,
    },
};
use oasis7_custody_core::{
    DecimalU64,
    http::Role,
    provisioning::{OwnerSetupPermitKindV1, OwnerSetupReadPurposeV1},
    recovery::RecoveryFence,
};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, RequestInit, RequestRedirect, Response, Result};
use zeroize::Zeroizing;
type JResult<T> = std::result::Result<T, JsValue>;
fn rejected() -> JsValue {
    JsValue::from_str("owner setup signing unavailable")
}
#[wasm_bindgen(module = "/platform/owner_setup_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=ownerSetupKeyCall)]
    async fn account(env: &Env, raw: &str) -> JResult<JsValue>;
}
#[wasm_bindgen(module = "/platform/role_provision_rpc.mjs")]
extern "C" {
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
    selector: &OwnerSetupStageRpc,
    action: OwnerSetupRuntimeAction,
) -> JResult<T> {
    let raw = serde_json::to_string(&OwnerSetupRuntimeRpc {
        request: selector.request.clone(),
        stage: selector.stage,
        action,
    })
    .map_err(|_| rejected())?;
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let value = account(env, &raw).await?.as_string().ok_or_else(rejected)?;
    if value.len() > 32768 {
        return Err(rejected());
    }
    serde_json::from_str(&value).map_err(|_| rejected())
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
    view: OwnerSetupSignerPermit,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    fn current(&self, env: &Env, selector: &OwnerSetupStageRpc) -> JResult<SetupSigningCurrent> {
        let v = &self.view.permit;
        let scope = &v.participant.owner_scope;
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
            .var("CUSTODY_OWNER_SETUP_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
            || v.kind != OwnerSetupPermitKindV1::Sign
            || v.setup_id != selector.request.setup_id
            || v.stage != selector.stage
            || v.plan.provision_id != selector.request.provision_id
            || scope.provision_id != selector.request.provision_id
            || scope.context.identity.account_id != selector.request.custody_account_id
            || scope.context.identity.environment != environment
            || scope.context.identity.role != Role::Owner
            || scope.context.identity.key_id != v.participant.owner_key.key_id
            || scope.context.identity.key_version != v.participant.owner_key.key_version
            || v.service_epoch != epoch
            || v.target_security_epoch.0 == 0
            || v.observed_at.0 > now
            || now - v.observed_at.0 > 5000
            || now >= v.expires_at.0
            || self.fence_started > now
            || now - self.fence_started > 5000
        {
            return Err(rejected());
        }
        v.plan.validate().map_err(|_| rejected())?;
        scope.validate().map_err(|_| rejected())?;
        self.fence.check_epoch(epoch.0).map_err(|_| rejected())?;
        Ok(SetupSigningCurrent {
            role: RoleKeyCurrent {
                scope: scope.clone(),
                approved: true,
                frozen: false,
                cancelled: false,
                expires_at: v.expires_at,
                authority_revision: v.authority_revision.clone(),
            },
            setup_id: v.setup_id.clone(),
            bundle_digest: setup_bundle_digest(&self.view.signing).map_err(|_| rejected())?,
        })
    }
}
async fn observe(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    access: Option<Box<oasis7_custody_core::provisioning::OwnerSetupAccessEvidenceV1>>,
) -> JResult<Observation> {
    let view = rpc(
        env,
        selector,
        OwnerSetupRuntimeAction::GetPermit {
            purpose: OwnerSetupReadPurposeV1::Sign,
            access,
        },
    )
    .await?;
    let (fence, fence_started) = fence(env).await?;
    let value = Observation {
        view,
        fence,
        fence_started,
    };
    value.current(env, selector)?;
    Ok(value)
}
fn decode(raw: &str) -> Result<OwnerSetupStageRpc> {
    if raw.len() > 4096 {
        return Err(denied());
    }
    let value: OwnerSetupStageRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    value.request.validate().map_err(|_| denied())?;
    Ok(value)
}
pub(crate) async fn sign(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let request: OwnerSetupSignRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    let selector = request.selector;
    selector.request.validate().map_err(|_| denied())?;
    let observed = observe(env, &selector, Some(Box::new(request.access)))
        .await
        .map_err(|_| denied())?;
    let selector_raw = serde_json::to_string(&selector)?;
    let name = observed
        .view
        .permit
        .participant
        .owner_scope
        .task_digest()
        .map_err(|_| denied())?;
    let stub = env
        .durable_object("ROLE_KEYS")?
        .id_from_name(&name)?
        .get_stub()?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_body(Some(selector_raw.into()))
        .with_redirect(RequestRedirect::Manual);
    let mut req =
        Request::new_with_init("https://role-key.internal/internal/owner-setup-sign", &init)?;
    req.headers_mut()?.set("Content-Type", "application/json")?;
    let response = stub.fetch_with_request(req).await?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    let response: web_sys::Response = response.into();
    let raw = bounded_body(response.as_ref(), 32768)
        .await
        .map_err(|_| denied())?
        .as_string()
        .ok_or_else(denied)?;
    observed.current(env, &selector).map_err(|_| denied())?;
    let published: OwnerSetupPublished = serde_json::from_str(&raw).map_err(|_| denied())?;
    if published.permit.kind != OwnerSetupPermitKindV1::Preflight
        || published.permit.plan != observed.view.permit.plan
        || published.permit.stage != selector.stage
        || published.permit.setup_id != selector.request.setup_id
        || published.permit.original.as_ref() != Some(&published.candidate.original)
        || published.permit.observation.as_ref() != Some(&published.candidate.observation)
    {
        return Err(denied());
    }
    Ok(raw)
}
pub(crate) async fn sign_in_vault(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let selector = decode(raw)?;
    let observed = observe(env, &selector, None).await.map_err(|_| denied())?;
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
    let published = std::cell::RefCell::new(None);
    let observed_ref = &observed;
    let selector_ref = &selector;
    let published_ref = &published;
    let publish = move |candidate: oasis7_custody_core::provisioning::OwnerSetupCandidateV1| async move {
        let expected = candidate.clone();
        let committed: OwnerSetupPublished = rpc(
            env,
            selector_ref,
            OwnerSetupRuntimeAction::PublishCandidate {
                candidate: Box::new(candidate),
            },
        )
        .await?;
        let (fresh, started) = fence(env).await?;
        let now = clock().map_err(|_| rejected())?;
        if started > now
            || now - started > 5000
            || fresh
                .check_epoch(observed_ref.view.permit.service_epoch.0)
                .is_err()
            || committed.candidate != expected
            || committed.permit.kind != OwnerSetupPermitKindV1::Preflight
        {
            return Err(rejected());
        }
        observed_ref.current(env, selector_ref)?;
        published_ref.replace(Some(committed));
        Ok(expected)
    };
    RoleKeyStore::new(state)
        .sign_setup(
            ParticipantSigningKey {
                scope: &observed.view.permit.participant.owner_scope,
                wrapping_version,
                kek: &kek,
            },
            &observed.view.signing,
            &observed.view.permit.plan.world_identity,
            observed.view.permit.plan.deadline.0,
            || observed.current(env, &selector),
            publish,
        )
        .await
        .map_err(|_| denied())?;
    Response::from_json(&published.into_inner().ok_or_else(denied)?)
}
