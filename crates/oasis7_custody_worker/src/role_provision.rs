//! Fixed-role key provisioning. Account permits and the independent fence are
//! read through private bindings; no caller supplies an approval boolean or key.
use crate::role_key_store::{RoleKeyCurrent, RoleKeyStore};
use ed25519_dalek::VerifyingKey;
use oasis7_custody_core::{
    DecimalU64,
    http::Role,
    provisioning::{ProvisionRoleKeyPermitViewV1, ProvisionRoleKeyRequestV1, PublicRoleKey},
    recovery::RecoveryFence,
};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use worker::*;
use zeroize::Zeroizing;

const MAX_BYTES: usize = 32768;
const MAX_OBSERVATION_AGE: u64 = 5000;
#[cfg(feature = "identity")]
const COMPILED_ROLE: Role = Role::Identity;
#[cfg(feature = "owner")]
const COMPILED_ROLE: Role = Role::Owner;
#[cfg(feature = "executor")]
const COMPILED_ROLE: Role = Role::Executor;

#[wasm_bindgen(module = "/platform/role_provision_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = verifyKeyPermit)]
    async fn verify_permit(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = readRecoveryFence)]
    async fn read_fence(env: &Env) -> std::result::Result<JsValue, JsValue>;
}
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readBoundedText)]
    async fn bounded_body(body: &JsValue, limit: u32) -> std::result::Result<JsValue, JsValue>;
}
fn denied() -> Error {
    Error::RustError("custody role key request rejected".into())
}
fn js_denied() -> JsValue {
    JsValue::from_str("custody role key request rejected")
}
fn clock() -> Result<u64> {
    let value = js_sys::Date::now();
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > 9_007_199_254_740_991.0
    {
        return Err(denied());
    }
    Ok(value as u64)
}
fn fresh(observed: u64, now: u64) -> Result<()> {
    if observed > now || now - observed > MAX_OBSERVATION_AGE {
        return Err(denied());
    }
    Ok(())
}
fn text(value: JsValue, limit: usize) -> Result<String> {
    value
        .as_string()
        .filter(|value| value.len() <= limit)
        .ok_or_else(denied)
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || b"-_.:".contains(&value))
}
fn decode(raw: &str) -> Result<ProvisionRoleKeyRequestV1> {
    if raw.len() > MAX_BYTES {
        return Err(denied());
    }
    let request: ProvisionRoleKeyRequestV1 = serde_json::from_str(raw).map_err(|_| denied())?;
    request.validate().map_err(|_| denied())?;
    Ok(request)
}
struct Config {
    environment: String,
    service_epoch: DecimalU64,
}
impl Config {
    fn load(env: &Env) -> Result<Self> {
        if env
            .var("CUSTODY_PROVISION_KEYS_ENABLED")
            .ok()
            .map(|value| value.to_string())
            .as_deref()
            != Some("true")
        {
            return Err(denied());
        }
        let environment = env
            .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
            .map(|value| value.to_string())
            .map_err(|_| denied())?;
        let service_epoch: DecimalU64 = serde_json::from_value(
            env.var("CUSTODY_SERVICE_EPOCH")
                .map(|value| value.to_string())
                .map_err(|_| denied())?
                .into(),
        )
        .map_err(|_| denied())?;
        if !identifier(&environment) || service_epoch.0 == 0 {
            return Err(denied());
        }
        Ok(Self {
            environment,
            service_epoch,
        })
    }
}
struct Observation {
    permit: ProvisionRoleKeyPermitViewV1,
    fence: RecoveryFence,
    fence_observed_at: u64,
}
impl Observation {
    fn current(
        &self,
        config: &Config,
        request: &ProvisionRoleKeyRequestV1,
    ) -> Result<RoleKeyCurrent> {
        let now = clock()?;
        let permit = &self.permit;
        permit
            .validate_for(request, COMPILED_ROLE)
            .map_err(|_| denied())?;
        permit.validate_at(now).map_err(|_| denied())?;
        fresh(self.fence_observed_at, now)?;
        self.fence
            .check_epoch(config.service_epoch.0)
            .map_err(|_| denied())?;
        if permit.scope.context.identity.environment != config.environment
            || permit.service_epoch != config.service_epoch
        {
            return Err(denied());
        }
        // These flags are derived only after the fixed Account permit oracle
        // succeeds; they are never deserialized from the incoming request.
        Ok(RoleKeyCurrent {
            scope: permit.scope.clone(),
            approved: true,
            frozen: false,
            cancelled: false,
            expires_at: permit.expires_at,
            authority_revision: permit.authority_revision.clone(),
        })
    }
}
async fn observe(
    env: &Env,
    config: &Config,
    request: &ProvisionRoleKeyRequestV1,
) -> Result<Observation> {
    let raw = serde_json::to_string(request)?;
    let raw = text(
        verify_permit(env, &raw).await.map_err(|_| denied())?,
        MAX_BYTES,
    )?;
    let permit: ProvisionRoleKeyPermitViewV1 = serde_json::from_str(&raw).map_err(|_| denied())?;
    // The fence has no server timestamp. Start time is a conservative bound;
    // never stamp a slow response as newly observed after it arrives.
    let fence_requested_at = clock()?;
    let raw = text(read_fence(env).await.map_err(|_| denied())?, 1024)?;
    let observation = Observation {
        permit,
        fence: serde_json::from_str(&raw).map_err(|_| denied())?,
        fence_observed_at: fence_requested_at,
    };
    observation.current(config, request)?;
    Ok(observation)
}
fn validate_public(value: &PublicRoleKey, expected: &RoleKeyCurrent) -> Result<()> {
    let bytes: [u8; 32] = hex::decode(&value.public_key)
        .map_err(|_| denied())?
        .try_into()
        .map_err(|_| denied())?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|_| denied())?;
    if value.key_id != expected.scope.context.identity.key_id
        || value.key_version != expected.scope.context.identity.key_version
        || hex::encode(bytes) != value.public_key
        || key.is_weak()
    {
        return Err(denied());
    }
    Ok(())
}

/// Fixed role entrypoint factory. A caller-selected account/task cannot create
/// a vault namespace before the Account authority validates its stored permit.
pub(crate) async fn ensure_role_key(env: &Env, raw: &str) -> Result<String> {
    let config = Config::load(env)?;
    let request = decode(raw)?;
    let observation = observe(env, &config, &request).await?;
    let expected = observation.current(&config, &request)?;
    let name = expected.scope.task_digest().map_err(|_| denied())?;
    let stub = env
        .durable_object("ROLE_KEYS")?
        .id_from_name(&name)?
        .get_stub()?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_body(Some(raw.into()))
        .with_redirect(RequestRedirect::Manual);
    let mut request = Request::new_with_init("https://role-key.internal/internal/ensure", &init)?;
    request
        .headers_mut()?
        .set("Content-Type", "application/json")?;
    let response = stub.fetch_with_request(request).await?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    let raw: web_sys::Response = response.into();
    let raw = text(
        bounded_body(raw.as_ref(), MAX_BYTES as u32)
            .await
            .map_err(|_| denied())?,
        MAX_BYTES,
    )?;
    let public: PublicRoleKey = serde_json::from_str(&raw).map_err(|_| denied())?;
    validate_public(&public, &expected)?;
    Ok(raw)
}

#[durable_object]
pub struct RoleKeyVault {
    state: JsValue,
    env: Env,
}
impl DurableObject for RoleKeyVault {
    fn new(state: State, env: Env) -> Self {
        Self {
            state: state._inner().into(),
            env,
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        #[cfg(feature = "owner")]
        if request.method() == Method::Post
            && matches!(
                url.path(),
                "/internal/participant-sign"
                    | "/internal/owner-setup-sign"
                    | "/internal/owner-setup-reconcile-sign"
                    | "/internal/test-session-sign"
            )
            && url.scheme() == "https"
            && url.host_str() == Some("role-key.internal")
            && url.query().is_none()
            && url.fragment().is_none()
            && request.headers().get("Content-Type")?.as_deref() == Some("application/json")
        {
            let raw = Zeroizing::new(text(
                bounded_body(request.inner().as_ref(), 4096)
                    .await
                    .map_err(|_| denied())?,
                4096,
            )?);
            let result = if url.path() == "/internal/test-session-sign" {
                crate::owner_test_session_signing::sign_in_vault(
                    self.state.clone(),
                    &self.env,
                    &raw,
                )
                .await
            } else if url.path() == "/internal/owner-setup-reconcile-sign" {
                crate::owner_setup_reconcile_signing::sign_in_vault(
                    self.state.clone(),
                    &self.env,
                    &raw,
                )
                .await
            } else if url.path() == "/internal/owner-setup-sign" {
                crate::owner_setup_signing::sign_in_vault(self.state.clone(), &self.env, &raw).await
            } else {
                crate::owner_participant_signing::sign_in_vault(self.state.clone(), &self.env, &raw)
                    .await
            };
            return match result {
                Ok(value) => Ok(value),
                Err(_) => Response::error("participant owner signing rejected", 409),
            };
        }
        #[cfg(feature = "identity")]
        if request.method() == Method::Post
            && url.path() == "/internal/native-sign"
            && url.scheme() == "https"
            && url.host_str() == Some("role-key.internal")
            && url.query().is_none()
            && url.fragment().is_none()
            && request.headers().get("Content-Type")?.as_deref() == Some("application/json")
        {
            let raw = bounded_body(request.inner().as_ref(), MAX_BYTES as u32)
                .await
                .map_err(|_| denied())?;
            let raw = Zeroizing::new(text(raw, MAX_BYTES)?);
            return match crate::identity_native_runtime::sign_role_native(
                self.state.clone(),
                &self.env,
                &raw,
            )
            .await
            {
                Ok(signature) => Response::ok(signature),
                Err(_) => Response::error("native role signing rejected", 409),
            };
        }
        if request.method() != Method::Post
            || url.path() != "/internal/ensure"
            || url.query().is_some()
        {
            return Response::error("not found", 404);
        }
        if request.headers().get("Content-Type")?.as_deref() != Some("application/json") {
            return Response::error("invalid content type", 415);
        }
        let raw = bounded_body(request.inner().as_ref(), MAX_BYTES as u32)
            .await
            .map_err(|_| denied())?;
        match self.ensure(&text(raw, MAX_BYTES)?).await {
            Ok(value) => Response::from_json(&value),
            Err(_) => Response::error("role key unavailable or rejected", 409),
        }
    }
}
impl RoleKeyVault {
    async fn ensure(&self, raw: &str) -> Result<PublicRoleKey> {
        let config = Config::load(&self.env)?;
        let request = decode(raw)?;
        let observations = RefCell::new(observe(&self.env, &config, &request).await?);
        let expected = observations.borrow().current(&config, &request)?;
        // The role's KEK is read only here, never transported through its factory,
        // Account permit, public response, audit or JS business code.
        let raw_kek = Zeroizing::new(
            self.env
                .secret("CUSTODY_ROLE_KEY_KEK")
                .map_err(|_| denied())?
                .to_string(),
        );
        if raw_kek.len() != 64
            || !raw_kek
                .bytes()
                .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
        {
            return Err(denied());
        }
        let mut kek = Zeroizing::new([0_u8; 32]);
        hex::decode_to_slice(raw_kek.as_bytes(), kek.as_mut_slice()).map_err(|_| denied())?;
        let version: DecimalU64 = serde_json::from_value(
            self.env
                .var("CUSTODY_ROLE_WRAPPING_KEY_VERSION")
                .map(|value| value.to_string())
                .map_err(|_| denied())?
                .into(),
        )
        .map_err(|_| denied())?;
        let current = || {
            let config = Config::load(&self.env).map_err(|_| js_denied())?;
            observations
                .borrow()
                .current(&config, &request)
                .map_err(|_| js_denied())
        };
        let refresh = || async {
            let updated = observe(&self.env, &config, &request)
                .await
                .map_err(|_| js_denied())?;
            *observations.borrow_mut() = updated;
            Ok(())
        };
        let public = RoleKeyStore::new(self.state.clone())
            .ensure_with_refresh(expected.scope.clone(), version, &kek, current, refresh)
            .await
            .map_err(|_| denied())?;
        validate_public(&public, &expected)?;
        Ok(public)
    }
}
