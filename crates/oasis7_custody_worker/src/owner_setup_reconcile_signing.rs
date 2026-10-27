//! Fixed reconciliation signer. Only registered read domains reach the Owner key.
use crate::{
    directory_config::{clock, denied},
    owner_setup_reconcile_rpc::*,
    role_key_store::{
        ParticipantSigningKey, RoleKeyCurrent, RoleKeyStore, SetupReconcileSigningCurrent,
        reconcile_permit_digest,
    },
};
use oasis7_custody_core::{
    DecimalU64,
    http::Role,
    provisioning::{OwnerSetupReadPurposeV1, OwnerSetupReconcilePermitKindV1},
    recovery::RecoveryFence,
};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, RequestInit, RequestRedirect, Response, Result};
use zeroize::Zeroizing;
type JResult<T> = std::result::Result<T, JsValue>;
fn rejected() -> JsValue {
    JsValue::from_str("owner setup reconciliation signing unavailable")
}
#[wasm_bindgen(module = "/platform/owner_setup_reconcile_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=ownerSetupReconcileKeyCall)]
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
    selector: &ReconcileSelector,
    action: ReconcileAction,
) -> JResult<T> {
    let raw = serde_json::to_string(&ReconcileRuntimeRpc {
        selector: selector.clone(),
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
    view: oasis7_custody_core::provisioning::OwnerSetupReconcilePermitV1,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    fn current(
        &self,
        env: &Env,
        selector: &ReconcileSelector,
    ) -> JResult<SetupReconcileSigningCurrent> {
        let v = &self.view;
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
            || v.kind != OwnerSetupReconcilePermitKindV1::Sign
            || v.reconcile_id != selector.request.reconcile_id
            || v.attempt_id != selector.request.attempt_id
            || v.provision_id != selector.provision_id
            || scope.provision_id != selector.provision_id
            || scope.context.identity.account_id != selector.custody_account_id
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
        scope.validate().map_err(|_| rejected())?;
        self.fence.check_epoch(epoch.0).map_err(|_| rejected())?;
        Ok(SetupReconcileSigningCurrent {
            role: RoleKeyCurrent {
                scope: scope.clone(),
                approved: true,
                frozen: false,
                cancelled: false,
                expires_at: v.expires_at,
                authority_revision: v.authority_revision.clone(),
            },
            reconcile_id: v.reconcile_id.clone(),
            attempt_id: v.attempt_id.clone(),
            permit_digest: reconcile_permit_digest(&self.view).map_err(|_| rejected())?,
        })
    }
}
async fn observe(
    env: &Env,
    selector: &ReconcileSelector,
    access: Option<Box<oasis7_custody_core::provisioning::OwnerSetupAccessEvidenceV1>>,
) -> JResult<Observation> {
    let view = rpc(
        env,
        selector,
        ReconcileAction::GetPermit {
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
fn decode(raw: &str) -> Result<ReconcileSelector> {
    if raw.len() > 4096 {
        return Err(denied());
    }
    let value: ReconcileSelector = serde_json::from_str(raw).map_err(|_| denied())?;
    value.request.validate().map_err(|_| denied())?;
    Ok(value)
}
pub(crate) async fn sign(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let request: ReconcileSignRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    let selector = request.selector;
    selector.request.validate().map_err(|_| denied())?;
    let observed = observe(env, &selector, Some(Box::new(request.access)))
        .await
        .map_err(|_| denied())?;
    let selector_raw = serde_json::to_string(&selector)?;
    let name = observed
        .view
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
    let mut req = Request::new_with_init(
        "https://role-key.internal/internal/owner-setup-reconcile-sign",
        &init,
    )?;
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
    let published: ReconcilePublished = serde_json::from_str(&raw).map_err(|_| denied())?;
    if published.permit.kind != OwnerSetupReconcilePermitKindV1::Preflight
        || published.permit.reconcile_id != selector.request.reconcile_id
        || published.permit.attempt_id != selector.request.attempt_id
        || published.permit.setup_id != observed.view.setup_id
        || published.permit.participant != observed.view.participant
        || published.candidate.observation.read.request != observed.view.observation
        || published.candidate.observation.response_profile != Some(oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1)
        || published.candidate.goal.as_ref().map(|g| &g.request) != observed.view.final_goal.as_ref()
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
    let publish =
        move |candidate: oasis7_custody_core::provisioning::OwnerSetupReconcileReadCandidateV1| async move {
            let expected = candidate.clone();
            let committed: ReconcilePublished = rpc(
                env,
                selector_ref,
                ReconcileAction::PublishReads {
                    candidate: Box::new(candidate),
                },
            )
            .await?;
            let (fresh, started) = fence(env).await?;
            let now = clock().map_err(|_| rejected())?;
            if started > now
                || now - started > 5000
                || fresh
                    .check_epoch(observed_ref.view.service_epoch.0)
                    .is_err()
                || committed.candidate != expected
                || committed.permit.kind != OwnerSetupReconcilePermitKindV1::Preflight
            {
                return Err(rejected());
            }
            observed_ref.current(env, selector_ref)?;
            published_ref.replace(Some(committed));
            Ok(expected)
        };
    RoleKeyStore::new(state)
        .sign_setup_reconcile(
            ParticipantSigningKey {
                scope: &observed.view.participant.owner_scope,
                wrapping_version,
                kek: &kek,
            },
            &observed.view,
            || observed.current(env, &selector),
            publish,
        )
        .await
        .map_err(|_| denied())?;
    Response::from_json(&published.into_inner().ok_or_else(denied)?)
}
