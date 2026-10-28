//! Fixed five-oracle TestSession Owner read factory and immutable Vault handler.
use crate::{
    directory_config::{clock, denied},
    owner_test_session_rpc::*,
    role_key_store::{
        ParticipantSigningKey, RoleKeyCurrent, RoleKeyStore, TestSessionSigningCurrent,
        test_session_signing_digest,
    },
};
use oasis7_custody_core::{
    DecimalU64, http::Role, provisioning::OwnerSetupReadPurposeV1, recovery::RecoveryFence,
};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, RequestInit, RequestRedirect, Response, Result};
use zeroize::Zeroizing;
type JResult<T> = std::result::Result<T, JsValue>;
fn rejected() -> JsValue {
    JsValue::from_str("test session signing unavailable")
}
#[wasm_bindgen(module = "/platform/test_session_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=ownerTestSessionKeyCall)]
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
    async fn bounded(body: &JsValue, limit: u32) -> JResult<JsValue>;
}
async fn rpc<T: DeserializeOwned>(
    env: &Env,
    request: &TestSessionSelectorV1,
    action: TestSessionRuntimeAction,
) -> JResult<T> {
    let raw = serde_json::to_string(&TestSessionRuntimeRpc {
        request: request.clone(),
        action,
    })
    .map_err(|_| rejected())?;
    if raw.len() > 32768 {
        return Err(rejected());
    }
    let result = account(env, &raw).await?.as_string().ok_or_else(rejected)?;
    if result.len() > 32768 {
        return Err(rejected());
    }
    serde_json::from_str(&result).map_err(|_| rejected())
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
    view: TestSessionSignerPermit,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    fn current(
        &self,
        env: &Env,
        request: &TestSessionSelectorV1,
        ticket: u8,
    ) -> JResult<TestSessionSigningCurrent> {
        let v = &self.view.permit;
        let d = &v.descriptor;
        let scope = &d.owner_scope;
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
            .var("CUSTODY_TEST_SESSION_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
            || request.api_version != 1
            || d.api_version != 1
            || d.session_id != request.session_id
            || v.role != Role::Owner
            || v.purpose != OwnerSetupReadPurposeV1::Sign
            || v.ticket != ticket
            || !matches!(ticket, 3 | 4)
            || scope.provision_id != request.provision_id
            || scope.context.identity.account_id != request.custody_account_id
            || scope.context.identity.environment != environment
            || scope.context.identity.role != Role::Owner
            || scope.context.identity.key_id != d.owner_key.key_id
            || scope.context.identity.key_version != d.owner_key.key_version
            || v.service_epoch != epoch
            || v.target_security_epoch.0 == 0
            || v.authority_revision.len() != 64
            || v.observed_at.get() > now
            || now - v.observed_at.get() > 5000
            || now >= v.expires_at.get()
            || self.fence_started > now
            || now - self.fence_started > 5000
            || self.view.signing.read != d.owner_read
        {
            return Err(rejected());
        }
        scope.validate().map_err(|_| rejected())?;
        self.fence.check_epoch(epoch.0).map_err(|_| rejected())?;
        Ok(TestSessionSigningCurrent {
            role: RoleKeyCurrent {
                scope: scope.clone(),
                approved: true,
                frozen: false,
                cancelled: false,
                expires_at: DecimalU64(v.expires_at.get()),
                authority_revision: v.authority_revision.clone(),
            },
            session_id: d.session_id.clone(),
            permit_digest: test_session_signing_digest(&self.view).map_err(|_| rejected())?,
        })
    }
}
async fn observe(
    env: &Env,
    request: &TestSessionSelectorV1,
    access: Option<Box<oasis7_custody_core::provisioning::OwnerSetupAccessEvidenceV1>>,
    ticket: u8,
) -> JResult<Observation> {
    let view = rpc(env, request, TestSessionRuntimeAction::GetPermit { access }).await?;
    let (fence, fence_started) = fence(env).await?;
    let result = Observation {
        view,
        fence,
        fence_started,
    };
    result.current(env, request, ticket)?;
    Ok(result)
}
fn selector(raw: &str) -> Result<TestSessionSelectorV1> {
    if raw.len() > 4096 {
        return Err(denied());
    }
    let request: TestSessionSelectorV1 = serde_json::from_str(raw).map_err(|_| denied())?;
    if request.api_version != 1
        || request.session_id.is_empty()
        || request.custody_account_id.is_empty()
        || request.provision_id.is_empty()
    {
        return Err(denied());
    }
    Ok(request)
}
fn published_exact(
    value: &TestSessionPublishedRead,
    observed: &Observation,
    request: &TestSessionSelectorV1,
) -> JResult<()> {
    let before = &observed.view;
    let p = &value.permit;
    if p.ticket!=5||p.role!=Role::Owner||p.purpose!=OwnerSetupReadPurposeV1::Sign||p.descriptor!=before.permit.descriptor
        ||p.descriptor.session_id!=request.session_id||p.service_epoch!=before.permit.service_epoch||p.target_security_epoch!=before.permit.target_security_epoch
        ||p.authority_revision!=before.permit.authority_revision||value.original.read.request!=before.signing.read
        ||value.original.read.subject_public_key!=before.permit.descriptor.owner_key.public_key
        ||value.original.owner!=before.signing.owner||value.original.admission!=before.signing.admission
        ||value.original.response_profile!=Some(oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1){return Err(rejected());}
    Ok(())
}
pub(crate) async fn sign(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let input: TestSessionSignRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    let request = input.request;
    let observed = observe(env, &request, Some(Box::new(input.access)), 3)
        .await
        .map_err(|_| denied())?;
    let name = observed
        .view
        .permit
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
        .with_body(Some(serde_json::to_string(&request)?.into()))
        .with_redirect(RequestRedirect::Manual);
    let mut req = Request::new_with_init(
        "https://role-key.internal/internal/test-session-sign",
        &init,
    )?;
    req.headers_mut()?.set("Content-Type", "application/json")?;
    let response = stub.fetch_with_request(req).await?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    let response: web_sys::Response = response.into();
    let raw = bounded(response.as_ref(), 32768)
        .await
        .map_err(|_| denied())?
        .as_string()
        .ok_or_else(denied)?;
    observed.current(env, &request, 3).map_err(|_| denied())?;
    let published: TestSessionPublishedRead = serde_json::from_str(&raw).map_err(|_| denied())?;
    published_exact(&published, &observed, &request).map_err(|_| denied())?;
    let permit = published.permit;
    Ok(serde_json::to_string(&TestSessionPublishedObservation {
        descriptor: permit.descriptor,
        authority_revision: permit.authority_revision,
        service_epoch: permit.service_epoch,
        target_security_epoch: permit.target_security_epoch,
        observed_at: permit.observed_at,
        expires_at: permit.expires_at,
        original: published.original,
    })?)
}
pub(crate) async fn sign_in_vault(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let request = selector(raw)?;
    let observed = observe(env, &request, None, 4)
        .await
        .map_err(|_| denied())?;
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
    let request_ref = &request;
    let published_ref = &published;
    let publish=move |candidate:oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewRequestV1|async move{
        let expected=candidate.clone();
        let committed:TestSessionPublishedRead=rpc(env,request_ref,TestSessionRuntimeAction::PublishOwnerRead{original:Box::new(candidate)}).await?;
        let (fresh,started)=fence(env).await?;let now=clock().map_err(|_|rejected())?;
        if started>now||now-started>5000||fresh.check_epoch(observed_ref.view.permit.service_epoch.0).is_err()||committed.original!=expected||committed.permit.observed_at.get()>now||now-committed.permit.observed_at.get()>5000||now>=committed.permit.expires_at.get(){return Err(rejected());}
        published_exact(&committed,observed_ref,request_ref)?;observed_ref.current(env,request_ref,4)?;
        published_ref.replace(Some(committed));Ok(expected)
    };
    RoleKeyStore::new(state)
        .sign_test_session_read(
            ParticipantSigningKey {
                scope: &observed.view.permit.descriptor.owner_scope,
                wrapping_version,
                kek: &kek,
            },
            &observed.view,
            || observed.current(env, &request, 4),
            publish,
        )
        .await
        .map_err(|_| denied())?;
    Response::from_json(&published.into_inner().ok_or_else(denied)?)
}
