//! Concrete fixed Identity capabilities. Account/Owner supply current authority;
//! the manifest factory owns private crypto, and record actors store opaque immutable
//! envelopes only. No inbound DTO is deserialized into a trusted current reader.
#[cfg(feature = "native-runtime-harness")]
#[path = "../tests/support/native_runtime_harness.rs"]
mod harness;
#[path = "identity_native_history.rs"]
pub(crate) mod history;
#[path = "identity_native_recovery.rs"]
pub(crate) mod recovery;
use crate::{
    envelope_crypto,
    native_identity::provision_transport::NativeProvisionTransport,
    native_private_store::{NativePrivateConfig, NativePrivateScope, driver::*},
    native_provision_challenge::*,
    native_runtime_rpc::*,
    role_key_store::{NativeSigningCurrent, NativeSigningKey, RoleKeyCurrent, RoleKeyStore},
};
use oasis7_custody_core::{
    DecimalU64, approval::bytes_digest, envelope::KeyEnvelopeV1, native_binding::*,
    provisioning::*, recovery::RecoveryFence, role_key_slot::RoleKeySlotScopeV1,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::cell::{Cell, RefCell};
use wasm_bindgen::{JsCast, prelude::*};
use worker::*;
use zeroize::Zeroizing;
const MAX: usize = 32 * 1024;
const RECORD: &str = "custody-native-cipher-v1";
type JResult<T> = std::result::Result<T, JsValue>;
fn denied() -> Error {
    Error::RustError("Identity native capability rejected".into())
}
fn js_denied() -> JsValue {
    JsValue::from_str("Identity native capability rejected")
}
fn clock() -> JResult<u64> {
    let n = js_sys::Date::now();
    if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n > 9_007_199_254_740_991.0 {
        return Err(js_denied());
    }
    Ok(n as u64)
}
fn raw_text(raw: JsValue, bound: usize) -> JResult<Zeroizing<String>> {
    raw.as_string()
        .filter(|v| v.len() <= bound)
        .map(Zeroizing::new)
        .ok_or_else(js_denied)
}
#[wasm_bindgen(module = "/platform/identity_native_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = ownerNativeCall)]
    async fn owner_call(env: &Env, raw: &str) -> JResult<JsValue>;
    #[wasm_bindgen(catch, js_name = readNativeCipher)]
    fn read_cipher(state: &JsValue) -> JResult<String>;
}
#[wasm_bindgen(module = "/platform/role_provision_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readRecoveryFence)]
    async fn read_fence(env: &Env) -> JResult<JsValue>;
}
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readBoundedText)]
    async fn bounded_body(body: &JsValue, limit: u32) -> JResult<JsValue>;
    #[wasm_bindgen(catch, js_name = accountTransition)]
    fn transition(
        state: &JsValue,
        key: &str,
        initial: &str,
        callback: &js_sys::Function,
    ) -> JResult<String>;
}
async fn owner<T: DeserializeOwned>(
    env: &Env,
    call: &NativeRuntimeCall,
    action: NativeRuntimeAction,
) -> JResult<T> {
    if !call.validate() {
        return Err(js_denied());
    }
    let raw = serde_json::to_string(&NativeRuntimeRpc {
        call: call.clone(),
        action,
    })
    .map_err(|_| js_denied())?;
    if raw.len() > MAX {
        return Err(js_denied());
    }
    let reply = raw_text(owner_call(env, &raw).await?, MAX)?;
    serde_json::from_str(&reply).map_err(|_| js_denied())
}
fn driver_call(call: &NativeRuntimeCall) -> NativeDriverCall {
    match call {
        NativeRuntimeCall::Provision { request } => NativeDriverCall::Provision {
            request: request.clone(),
            attempt_id: request.operation_id.clone(),
        },
        NativeRuntimeCall::ManagedLogin { operation_id, .. } => NativeDriverCall::ManagedLogin {
            operation_id: operation_id.clone(),
        },
    }
}
struct Observation {
    permit: NativeDriverPermit,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    fn parts(&self) -> (&RoleKeySlotScopeV1, &str, DecimalU64, DecimalU64) {
        match &self.permit {
            NativeDriverPermit::Provision(v) => (
                &v.identity_scope,
                &v.authority_revision,
                v.expires_at,
                v.service_epoch,
            ),
            NativeDriverPermit::ManagedLogin(v) => (
                &v.identity_scope,
                &v.authority_revision,
                v.expires_at,
                v.service_epoch,
            ),
        }
    }
    fn check(&self, env: &Env, call: &NativeRuntimeCall) -> JResult<()> {
        let (scope, revision, expires, epoch) = self.parts();
        let now = clock()?;
        let environment = env
            .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
            .map_err(|_| js_denied())?
            .to_string();
        let configured_epoch: DecimalU64 = serde_json::from_value(
            env.var("CUSTODY_SERVICE_EPOCH")
                .map_err(|_| js_denied())?
                .to_string()
                .into(),
        )
        .map_err(|_| js_denied())?;
        scope.validate().map_err(|_| js_denied())?;
        if !call.validate()
            || scope.context.identity.role != oasis7_custody_core::http::Role::Identity
            || scope.context.identity.account_id != call.account_id()
            || scope.provision_id != call.provision_id()
            || scope.context.identity.environment != environment
            || configured_epoch != epoch
            || expires.0 <= now
            || self.fence_started > now
            || now - self.fence_started > 5000
            || revision.len() != 64
            || !revision
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(js_denied());
        }
        self.fence.check_epoch(epoch.0).map_err(|_| js_denied())?;
        let transport = NativeProvisionTransport::from_env(env).map_err(|_| js_denied())?;
        match (&self.permit, call) {
            (NativeDriverPermit::Provision(v), NativeRuntimeCall::Provision { request }) => {
                transport
                    .validate_registration(&v.descriptor.registration)
                    .map_err(|_| js_denied())?;
                if &v.request != request
                    || v.attempt_id != request.operation_id
                    || v.request_digest != request.digest().map_err(|_| js_denied())?
                    || v.descriptor.http_origin != transport.native_origin()
                    || v.observed_at.0 > now
                    || now - v.observed_at.0 > 5000
                    || v.descriptor.identity_key.key_id != scope.context.identity.key_id
                    || v.descriptor.identity_key.key_version != scope.context.identity.key_version
                    || v.descriptor.identity_key.public_key
                        != v.descriptor.registration.login_public_key
                {
                    return Err(js_denied());
                }
            }
            (
                NativeDriverPermit::ManagedLogin(v),
                NativeRuntimeCall::ManagedLogin { operation_id, .. },
            ) => {
                transport
                    .validate_binding(&v.binding)
                    .map_err(|_| js_denied())?;
                if &v.task.operation_id != operation_id
                    || v.task.claimed_at.is_none()
                    || v.task.result.is_some()
                    || v.http_origin != transport.native_origin()
                    || v.observed_at.0 > now
                    || now - v.observed_at.0 > 5000
                    || v.task.login_key.key_id != scope.context.identity.key_id
                    || v.task.login_key.key_version != scope.context.identity.key_version
                    || v.task.login_key.public_key != v.binding.login_public_key
                {
                    return Err(js_denied());
                }
            }
            _ => return Err(js_denied()),
        }
        Ok(())
    }
    fn fingerprint(&self) -> JResult<String> {
        let bytes = match &self.permit {
            NativeDriverPermit::Provision(v) => oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.native-live-observation.v1",
                &v.request,
                &v.descriptor,
                &v.identity_scope,
                &v.authority_revision,
                &v.source_revision,
                &v.attempt_id,
                &v.request_bytes,
                &v.request_digest,
                v.target_security_epoch,
                v.service_epoch,
                &v.nonce,
            )),
            NativeDriverPermit::ManagedLogin(v) => oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.managed-live-observation.v1",
                &v.identity_scope,
                &v.authority_revision,
                &v.source_revision,
                v.service_epoch,
                &v.binding,
                &v.initial_login_operation_id,
                (
                    &v.task.operation_id,
                    &v.task.login_operation_id,
                    &v.task.native_account_id,
                    &v.task.login_key,
                    &v.task.native_identity_namespace,
                    v.task.target_security_epoch,
                    v.task.deadline,
                    &v.task.status_nonce,
                    &v.task.request_bytes,
                ),
            )),
        }
        .map_err(|_| js_denied())?;
        Ok(bytes_digest(&bytes))
    }
    fn role(&self) -> JResult<RoleKeyCurrent> {
        let (scope, _, expires, _) = self.parts();
        Ok(RoleKeyCurrent {
            scope: scope.clone(),
            approved: true,
            frozen: false,
            cancelled: false,
            expires_at: expires,
            authority_revision: self.fingerprint()?,
        })
    }
}
async fn observe(env: &Env, call: &NativeRuntimeCall) -> JResult<Observation> {
    // This is the sole source of the Rust cache. No received crypto/storage DTO
    // contains an Observation or a trusted reader.
    let permit = match call {
        NativeRuntimeCall::Provision { .. } => NativeDriverPermit::Provision(Box::new(
            owner(env, call, NativeRuntimeAction::GetPermit).await?,
        )),
        NativeRuntimeCall::ManagedLogin { .. } => NativeDriverPermit::ManagedLogin(Box::new(
            owner(env, call, NativeRuntimeAction::GetPermit).await?,
        )),
    };
    let fence_started = clock()?;
    let raw = raw_text(read_fence(env).await?, 1024)?;
    let observation = Observation {
        permit,
        fence: serde_json::from_str(&raw).map_err(|_| js_denied())?,
        fence_started,
    };
    observation.check(env, call)?;
    Ok(observation)
}
fn record_name(scope: &NativePrivateScope) -> JResult<String> {
    scope.context()?;
    let bytes =
        oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.native-record-address.v1", scope))
            .map_err(|_| js_denied())?;
    Ok(format!("record:{}", bytes_digest(&bytes)))
}
async fn actor<T: DeserializeOwned, S: Serialize>(
    env: &Env,
    name: &str,
    path: &str,
    value: &S,
) -> JResult<T> {
    let raw = Zeroizing::new(serde_json::to_string(value).map_err(|_| js_denied())?);
    if raw.len() > MAX {
        return Err(js_denied());
    }
    let stub = env
        .durable_object("NATIVE_SESSIONS")
        .map_err(|_| js_denied())?
        .id_from_name(name)
        .map_err(|_| js_denied())?
        .get_stub()
        .map_err(|_| js_denied())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_redirect(RequestRedirect::Manual)
        .with_body(Some(raw.as_str().into()));
    let mut request =
        Request::new_with_init(&format!("https://identity-native.internal{path}"), &init)
            .map_err(|_| js_denied())?;
    request
        .headers_mut()
        .map_err(|_| js_denied())?
        .set("Content-Type", "application/json")
        .map_err(|_| js_denied())?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| js_denied())?;
    if response.status_code() != 200 {
        return Err(js_denied());
    }
    let response: web_sys::Response = response.into();
    let raw = raw_text(bounded_body(response.as_ref(), MAX as u32).await?, MAX)?;
    serde_json::from_str(&raw).map_err(|_| js_denied())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CipherRead {
    scope: NativePrivateScope,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CipherCas {
    scope: NativePrivateScope,
    expected: Option<KeyEnvelopeV1>,
    candidate: KeyEnvelopeV1,
    expires_at: DecimalU64,
}
struct Ports {
    env: Env,
    call: NativeRuntimeCall,
    cache: RefCell<Option<Observation>>,
    initial_permit_available: Cell<bool>,
}
impl Ports {
    fn check_call(&self, call: &NativeDriverCall) -> JResult<()> {
        if &driver_call(&self.call) != call {
            return Err(js_denied());
        }
        Ok(())
    }
    fn cached(&self) -> JResult<(RoleKeySlotScopeV1, String, DecimalU64)> {
        let cache = self.cache.borrow();
        let value = cache.as_ref().ok_or_else(js_denied)?;
        value.check(&self.env, &self.call)?;
        let (scope, _, expires, _) = value.parts();
        Ok((scope.clone(), value.fingerprint()?, expires))
    }
    fn private_before(&self, scope: &NativePrivateScope) -> JResult<(String, DecimalU64)> {
        let (role, revision, expires) = self.cached()?;
        if scope.role_scope != role {
            return Err(js_denied());
        }
        scope.context()?;
        Ok((revision, expires))
    }
    fn final_clock(
        &self,
        before: &(String, DecimalU64),
        scope: &NativePrivateScope,
    ) -> JResult<()> {
        let after = self.private_before(scope)?;
        if before.0 != after.0 || clock()? >= before.1.0 {
            return Err(js_denied());
        }
        Ok(())
    }
    async fn refresh(
        &self,
        before: &(String, DecimalU64),
        scope: &NativePrivateScope,
    ) -> JResult<()> {
        let updated = observe(&self.env, &self.call).await?;
        *self.cache.borrow_mut() = Some(updated);
        self.final_clock(before, scope)
    }
    async fn cipher(&self, scope: &NativePrivateScope) -> JResult<Option<KeyEnvelopeV1>> {
        actor(
            &self.env,
            &record_name(scope)?,
            "/internal/cipher-read",
            &CipherRead {
                scope: scope.clone(),
            },
        )
        .await
    }
    async fn sign(&self, payload: SignRequest) -> JResult<Zeroizing<String>> {
        let (scope, revision, expires) = self.cached()?;
        let name = scope.task_digest().map_err(|_| js_denied())?;
        let raw = Zeroizing::new(serde_json::to_string(&payload).map_err(|_| js_denied())?);
        let stub = self
            .env
            .durable_object("ROLE_KEYS")
            .map_err(|_| js_denied())?
            .id_from_name(&name)
            .map_err(|_| js_denied())?
            .get_stub()
            .map_err(|_| js_denied())?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_redirect(RequestRedirect::Manual)
            .with_body(Some(raw.as_str().into()));
        let mut req =
            Request::new_with_init("https://role-key.internal/internal/native-sign", &init)
                .map_err(|_| js_denied())?;
        req.headers_mut()
            .map_err(|_| js_denied())?
            .set("Content-Type", "application/json")
            .map_err(|_| js_denied())?;
        let response = stub
            .fetch_with_request(req)
            .await
            .map_err(|_| js_denied())?;
        if response.status_code() != 200 {
            return Err(js_denied());
        }
        let response: web_sys::Response = response.into();
        let signature = raw_text(bounded_body(response.as_ref(), 256).await?, 256)?;
        if signature.len() != 86
            || !signature
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || self.cached()?.1 != revision
            || clock()? >= expires.0
        {
            return Err(js_denied());
        }
        Ok(signature)
    }
}
impl NativeProvisionPorts for Ports {
    async fn reserve_permit(&self, call: &NativeDriverCall) -> JResult<NativeDriverPermit> {
        self.check_call(call)?;
        if self.initial_permit_available.replace(false) {
            self.cached()?;
            return self
                .cache
                .borrow()
                .as_ref()
                .map(|o| o.permit.clone())
                .ok_or_else(js_denied);
        }
        let observation = observe(&self.env, &self.call).await?;
        let permit = observation.permit.clone();
        *self.cache.borrow_mut() = Some(observation);
        Ok(permit)
    }
    async fn load_private(
        &self,
        call: &NativeDriverCall,
        scope: &NativePrivateScope,
    ) -> JResult<Option<Zeroizing<Vec<u8>>>> {
        self.check_call(call)?;
        let before = self.private_before(scope)?;
        let Some(envelope) = self.cipher(scope).await? else {
            self.final_clock(&before, scope)?;
            return Ok(None);
        };
        let config = NativePrivateConfig::load(&self.env)?;
        if envelope.wrapping_key_version != config.wrapping_version {
            return Err(js_denied());
        }
        let secret = envelope_crypto::open(&envelope, &scope.context()?, &config.kek)
            .await
            .map_err(|_| js_denied())?;
        if secret.as_bytes().len() > 12 * 1024 {
            return Err(js_denied());
        }
        self.refresh(&before, scope).await?;
        if self.cipher(scope).await?.as_ref() != Some(&envelope) {
            return Err(js_denied());
        }
        self.final_clock(&before, scope)?;
        Ok(Some(Zeroizing::new(secret.as_bytes().to_vec())))
    }
    async fn save_private(
        &self,
        call: &NativeDriverCall,
        scope: &NativePrivateScope,
        bytes: &[u8],
    ) -> JResult<()> {
        self.check_call(call)?;
        if bytes.is_empty() || bytes.len() > 12 * 1024 {
            return Err(js_denied());
        }
        let before = self.private_before(scope)?;
        let config = NativePrivateConfig::load(&self.env)?;
        let original = self.cipher(scope).await?;
        let candidate = if let Some(envelope) = &original {
            if envelope.wrapping_key_version != config.wrapping_version {
                return Err(js_denied());
            }
            let secret = envelope_crypto::open(envelope, &scope.context()?, &config.kek)
                .await
                .map_err(|_| js_denied())?;
            if secret.as_bytes() != bytes {
                return Err(js_denied());
            }
            envelope.clone()
        } else {
            envelope_crypto::seal(
                &scope.context()?,
                config.wrapping_version,
                &config.kek,
                bytes,
            )
            .await
            .map_err(|_| js_denied())?
        };
        self.refresh(&before, scope).await?;
        let saved: KeyEnvelopeV1 = actor(
            &self.env,
            &record_name(scope)?,
            "/internal/cipher-cas",
            &CipherCas {
                scope: scope.clone(),
                expected: original,
                candidate: candidate.clone(),
                expires_at: before.1,
            },
        )
        .await?;
        if saved != candidate {
            return Err(js_denied());
        }
        self.final_clock(&before, scope)
    }
    async fn sign_challenge(
        &self,
        call: &NativeDriverCall,
        challenge: &NativeProvisionChallenge,
    ) -> JResult<Zeroizing<String>> {
        self.check_call(call)?;
        self.sign(SignRequest {
            call: self.call.clone(),
            action: SignAction::Challenge {
                challenge: challenge.clone(),
            },
        })
        .await
    }
    async fn sign_possession(
        &self,
        call: &NativeDriverCall,
        statement: &SignedOwnerNativeBindingStatementV1,
    ) -> JResult<Zeroizing<String>> {
        self.check_call(call)?;
        self.sign(SignRequest {
            call: self.call.clone(),
            action: SignAction::Possession {
                statement: Box::new(statement.clone()),
            },
        })
        .await
    }
    async fn owner_statement(
        &self,
        call: &NativeDriverCall,
        challenge: &SignedNativeBindingChallengeV1,
    ) -> JResult<SignedOwnerNativeBindingStatementV1> {
        self.check_call(call)?;
        owner(
            &self.env,
            &self.call,
            NativeRuntimeAction::OwnerStatement {
                challenge: challenge.clone(),
            },
        )
        .await
    }
    async fn binding_statement(
        &self,
        call: &NativeDriverCall,
        binding_operation_id: &str,
    ) -> JResult<Option<SignedOwnerNativeBindingStatementV1>> {
        self.check_call(call)?;
        owner(
            &self.env,
            &self.call,
            NativeRuntimeAction::BindingStatement {
                binding_operation_id: binding_operation_id.into(),
            },
        )
        .await
    }
    async fn binding_receipt(
        &self,
        call: &NativeDriverCall,
    ) -> JResult<SignedNativeBindingReceiptV1> {
        self.check_call(call)?;
        owner(&self.env, &self.call, NativeRuntimeAction::BindingReceipt).await
    }
    async fn record_result(
        &self,
        call: &NativeDriverCall,
        result: &NativeStageResultV1,
        session: Option<OwnedNativeSessionAttestation>,
    ) -> JResult<()> {
        self.check_call(call)?;
        let (_, revision, expires) = self.cached()?;
        let response: NativeStageResultV1 = owner(
            &self.env,
            &self.call,
            NativeRuntimeAction::RecordResult {
                result: result.clone(),
                session: session.map(|s| NativeSessionAttestation {
                    status_digest: s.status_digest,
                    session_ref: s.session_ref,
                    private_sid_generation: s.private_sid_generation,
                }),
            },
        )
        .await?;
        // Final admission is a fact commit, not a renewed grant. Keep the original
        // exact revision/deadline after this async Owner call.
        if &response != result || self.cached()?.1 != revision || clock()? >= expires.0 {
            return Err(js_denied());
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignRequest {
    call: NativeRuntimeCall,
    action: SignAction,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SignAction {
    Challenge {
        challenge: NativeProvisionChallenge,
    },
    Possession {
        statement: Box<SignedOwnerNativeBindingStatementV1>,
    },
}
fn namespace_parts(namespace: &str) -> JResult<(String, String)> {
    let (length, rest) = namespace.split_once(':').ok_or_else(js_denied)?;
    let length: usize = length.parse().map_err(|_| js_denied())?;
    let service = rest.get(..length).ok_or_else(js_denied)?;
    let environment = rest
        .get(length..)
        .and_then(|s| s.strip_prefix(':'))
        .ok_or_else(js_denied)?;
    if service.is_empty()
        || environment.is_empty()
        || format!("{}:{}:{}", service.len(), service, environment) != namespace
    {
        return Err(js_denied());
    }
    Ok((service.into(), environment.into()))
}
fn expected_challenge(observation: &Observation, origin: &str) -> JResult<NativeChallengeScope> {
    let (namespace, operation, key, purpose) = match &observation.permit {
        NativeDriverPermit::Provision(v) => {
            let purpose = match v.request.stage {
                NativeStageKindV1::Registration => NativeProvisionPurpose::Register,
                NativeStageKindV1::Login => NativeProvisionPurpose::Login,
                NativeStageKindV1::Binding => return Err(js_denied()),
            };
            let operation = if purpose == NativeProvisionPurpose::Register {
                &v.descriptor.registration.registration_operation_id
            } else {
                &v.descriptor.login_request_id
            };
            (
                &v.descriptor.registration.identity_namespace,
                operation,
                &v.descriptor.identity_key.public_key,
                purpose,
            )
        }
        NativeDriverPermit::ManagedLogin(v) => (
            &v.task.native_identity_namespace,
            &v.task.login_operation_id,
            &v.task.login_key.public_key,
            NativeProvisionPurpose::Login,
        ),
    };
    let (service, environment) = namespace_parts(namespace)?;
    Ok(NativeChallengeScope {
        identity_service_id: service,
        environment_id: environment,
        audience: origin.into(),
        operation_id: operation.clone(),
        public_key: key.clone(),
        purpose,
    })
}
pub(crate) async fn sign_role_native(state: JsValue, env: &Env, raw: &str) -> Result<String> {
    if raw.len() > MAX {
        return Err(denied());
    }
    let request: SignRequest = serde_json::from_str(raw).map_err(|_| denied())?;
    let observations = RefCell::new(observe(env, &request.call).await.map_err(|_| denied())?);
    let original_scope = observations.borrow().parts().0.clone();
    let transport = NativeProvisionTransport::from_env(env)?;
    let raw_kek = Zeroizing::new(
        env.secret("CUSTODY_ROLE_KEY_KEK")
            .map_err(|_| denied())?
            .to_string(),
    );
    let mut kek = Zeroizing::new([0_u8; 32]);
    if raw_kek.len() != 64 {
        return Err(denied());
    }
    hex::decode_to_slice(raw_kek.as_bytes(), kek.as_mut_slice()).map_err(|_| denied())?;
    if hex::encode(kek.as_slice()) != raw_kek.as_str() {
        return Err(denied());
    }
    let wrapping_version: DecimalU64 = serde_json::from_value(
        env.var("CUSTODY_ROLE_WRAPPING_KEY_VERSION")?
            .to_string()
            .into(),
    )?;
    let (bytes, expected) = match &request.action {
        SignAction::Challenge { challenge } => {
            let expected = expected_challenge(&observations.borrow(), transport.native_origin())
                .map_err(|_| denied())?;
            (
                challenge
                    .signing_bytes(&expected, clock().map_err(|_| denied())?)
                    .map_err(|_| denied())?,
                Some(expected),
            )
        }
        SignAction::Possession { statement } => {
            let permit = match &observations.borrow().permit {
                NativeDriverPermit::Provision(v) => v.clone(),
                _ => return Err(denied()),
            };
            if permit.request.stage != NativeStageKindV1::Binding
                || permit.request.binding_operation_id != statement.statement.binding_operation_id
            {
                return Err(denied());
            }
            let trusted: Option<SignedOwnerNativeBindingStatementV1> = owner(
                env,
                &request.call,
                NativeRuntimeAction::BindingStatement {
                    binding_operation_id: permit.request.binding_operation_id.clone(),
                },
            )
            .await
            .map_err(|_| denied())?;
            if trusted.as_ref() != Some(statement.as_ref()) {
                return Err(denied());
            }
            (
                statement
                    .statement
                    .possession_signing_bytes()
                    .map_err(|_| denied())?,
                None,
            )
        }
    };
    let digest = bytes_digest(&bytes);
    let current = || {
        let observation = observations.borrow();
        observation.check(env, &request.call)?;
        let permit_id = match &observation.permit {
            NativeDriverPermit::Provision(v) => v.attempt_id.clone(),
            NativeDriverPermit::ManagedLogin(v) => v.task.operation_id.clone(),
        };
        Ok(NativeSigningCurrent {
            role: observation.role()?,
            signing_digest: digest.clone(),
            native_stage_permit_id: permit_id,
        })
    };
    let refresh = || async {
        let updated = observe(env, &request.call).await?;
        *observations.borrow_mut() = updated;
        Ok(())
    };
    let key = NativeSigningKey {
        scope: &original_scope,
        wrapping_version,
        kek: &kek,
    };
    let store = RoleKeyStore::new(state);
    let signed = match (&request.action, expected) {
        (SignAction::Challenge { challenge }, Some(expected)) => {
            store
                .sign_native_challenge(key, challenge, &expected, current, refresh)
                .await
        }
        (SignAction::Possession { statement }, None) => {
            let expected = {
                let permit = observations.borrow();
                let NativeDriverPermit::Provision(v) = &permit.permit else {
                    return Err(denied());
                };
                NativeBindingScopeV1 {
                    environment: original_scope.context.identity.environment.clone(),
                    custody_account_id: request.call.account_id().into(),
                    provision_id: request.call.provision_id().into(),
                    approved_plan_digest: statement.statement.approved_plan_digest.clone(),
                    identity_namespace: v.descriptor.registration.identity_namespace.clone(),
                    identity_instance_epoch: v.descriptor.registration.identity_instance_epoch,
                    registration_operation_id: v
                        .descriptor
                        .registration
                        .registration_operation_id
                        .clone(),
                    registration_request_digest: v
                        .descriptor
                        .registration
                        .digest()
                        .map_err(|_| denied())?,
                    login_key: v.descriptor.identity_key.clone(),
                    plan_expires_at: v.expires_at,
                    source_expires_at: v.expires_at,
                }
            };
            store
                .sign_binding_possession(key, &statement.statement, &expected, current, refresh)
                .await
        }
        _ => return Err(denied()),
    };
    signed.map_err(|_| denied())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CipherSnapshot {
    schema_version: u8,
    scope: NativePrivateScope,
    envelope: Option<KeyEnvelopeV1>,
}
fn cipher_snapshot(raw: &str, scope: &NativePrivateScope) -> JResult<CipherSnapshot> {
    if raw.len() > MAX {
        return Err(js_denied());
    }
    let saved: CipherSnapshot = serde_json::from_str(raw).map_err(|_| js_denied())?;
    if saved.schema_version != 1 || &saved.scope != scope {
        return Err(js_denied());
    }
    if let Some(envelope) = &saved.envelope {
        envelope
            .validate_for(&scope.context()?)
            .map_err(|_| js_denied())?;
    }
    Ok(saved)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    records: Vec<InventoryRow>,
    table_names: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryRow {
    id: String,
    payload: String,
}
fn cipher_read_existing(
    state: &JsValue,
    scope: &NativePrivateScope,
) -> JResult<Option<KeyEnvelopeV1>> {
    scope.context()?;
    let raw = read_cipher(state)?;
    let inventory: Inventory = serde_json::from_str(&raw).map_err(|_| js_denied())?;
    if inventory.table_names.iter().any(|n| {
        !matches!(
            n.as_str(),
            "table:custody_records"
                | "table:custody_audit"
                | "table:sqlite_sequence"
                | "table:_cf_METADATA"
        )
    }) {
        return Err(js_denied());
    }
    if inventory.records.is_empty() {
        return Ok(None);
    }
    if inventory.records.len() != 1 || inventory.records[0].id != RECORD {
        return Err(js_denied());
    }
    Ok(cipher_snapshot(&inventory.records[0].payload, scope)?.envelope)
}
fn cipher_transition(
    state: &JsValue,
    scope: &NativePrivateScope,
    cas: Option<&CipherCas>,
) -> JResult<Option<KeyEnvelopeV1>> {
    let initial = serde_json::to_string(&CipherSnapshot {
        schema_version: 1,
        scope: scope.clone(),
        envelope: None,
    })
    .map_err(|_| js_denied())?;
    scope.context()?;
    let mut returned = None;
    let mut callback = |raw: String, inventory: String| -> JResult<js_sys::Array> {
        let inventory: Inventory = serde_json::from_str(&inventory).map_err(|_| js_denied())?;
        if inventory.records.len() != 1
            || inventory.records[0].id != RECORD
            || inventory.records[0].payload != raw
            || inventory.table_names.iter().any(|n| {
                !matches!(
                    n.as_str(),
                    "table:custody_records"
                        | "table:custody_audit"
                        | "table:sqlite_sequence"
                        | "table:_cf_METADATA"
                )
            })
        {
            return Err(js_denied());
        }
        let saved = cipher_snapshot(&raw, scope)?;
        let next = if let Some(cas) = cas {
            if clock()? >= cas.expires_at.0 || cas.expected != saved.envelope {
                return Err(js_denied());
            }
            cas.candidate
                .validate_for(&scope.context()?)
                .map_err(|_| js_denied())?;
            if saved.envelope.as_ref().is_some_and(|e| e != &cas.candidate) {
                return Err(js_denied());
            }
            returned = Some(Some(cas.candidate.clone()));
            serde_json::to_string(&CipherSnapshot {
                schema_version: 1,
                scope: scope.clone(),
                envelope: Some(cas.candidate.clone()),
            })
            .map_err(|_| js_denied())?
        } else {
            returned = Some(saved.envelope);
            raw.clone()
        };
        if next.len() > MAX {
            return Err(js_denied());
        }
        let changed = next != raw;
        let result = js_sys::Array::new();
        result.push(&next.into());
        if changed {
            result.push(&"1".into());
            result.push(&"native_cipher_original_saved".into());
        } else {
            result.push(&"".into());
            result.push(&"".into());
        }
        Ok(result)
    };
    let closure = Closure::borrow_mut(&mut callback);
    transition(state, RECORD, &initial, closure.as_ref().unchecked_ref())?;
    drop(closure);
    returned.ok_or_else(js_denied)
}
fn manifest_name(env: &Env, call: &NativeRuntimeCall) -> Result<String> {
    if !call.validate() {
        return Err(denied());
    }
    manifest_name_parts(env, call.account_id(), call.provision_id())
}
fn manifest_name_parts(env: &Env, account_id: &str, provision_id: &str) -> Result<String> {
    let valid = |v: &str| {
        !v.is_empty()
            && v.len() <= 256
            && v.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
    };
    if !valid(account_id) || !valid(provision_id) {
        return Err(denied());
    }
    let environment = env.var("CUSTODY_ROLE_KEY_ENVIRONMENT")?.to_string();
    let name = bytes_digest(
        &oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.native-manifest-address.v1",
            environment,
            account_id,
            provision_id,
        ))
        .map_err(|_| denied())?,
    );
    Ok(format!("manifest:{name}"))
}
/// Router address contains static environment/account/provision only. It does
/// not authorize or write; the actor's first action is the real Owner permit.
pub(crate) async fn run_native_raw(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > MAX {
        return Err(denied());
    }
    let call: NativeRuntimeCall = serde_json::from_str(raw).map_err(|_| denied())?;
    if !call.validate() {
        return Err(denied());
    }
    if env
        .var("CUSTODY_NATIVE_PROVISION_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(denied());
    }
    let result: NativeStageResultV1 =
        actor(env, &manifest_name(env, &call)?, "/internal/run", &call)
            .await
            .map_err(|_| denied())?;
    serde_json::to_string(&result).map_err(|_| denied())
}
#[durable_object]
pub struct NativeSession {
    state: JsValue,
    env: Env,
}
impl DurableObject for NativeSession {
    fn new(state: State, env: Env) -> Self {
        Self {
            state: state._inner().into(),
            env,
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        if request.method() != Method::Post
            || url.scheme() != "https"
            || url.host_str() != Some("identity-native.internal")
            || url.query().is_some()
            || url.fragment().is_some()
            || request.headers().get("Content-Type")?.as_deref() != Some("application/json")
        {
            return Response::error("not found", 404);
        }
        let raw = raw_text(
            bounded_body(request.inner().as_ref(), MAX as u32)
                .await
                .map_err(|_| denied())?,
            MAX,
        )
        .map_err(|_| denied())?;
        if url.path() == "/internal/history" {
            return history::response(self.state.clone(), &self.env, &raw);
        }
        if url.path() == "/internal/missing-session" {
            return history::missing_response(self.state.clone(), &self.env, &raw);
        }
        if self
            .env
            .var("CUSTODY_NATIVE_PROVISION_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Response::error("disabled", 409);
        }
        #[cfg(feature = "native-runtime-harness")]
        if url.path() == "/internal/harness" {
            return harness::handle(self.state.clone(), &raw).await;
        }
        if url.path() == "/internal/recover-status" {
            return recovery::response(self.state.clone(), &self.env, &raw).await;
        }
        let response = match url.path() {
            "/internal/run" => {
                let call: NativeRuntimeCall = serde_json::from_str(&raw).map_err(|_| denied())?;
                if !call.validate() {
                    return Err(denied());
                }
                // First authenticate through the fixed Owner before SQL, private
                // KEK access or native I/O. The first driver read consumes this
                // actual RPC response once; it is never supplied by an incoming DTO.
                let observation = observe(&self.env, &call).await.map_err(|_| denied())?;
                let ports = Ports {
                    env: self.env.clone(),
                    call: call.clone(),
                    cache: RefCell::new(Some(observation)),
                    initial_permit_available: Cell::new(true),
                };
                let driver = NativeProvisionDriver::new(
                    ports,
                    self.state.clone(),
                    NativeProvisionTransport::from_env(&self.env)?,
                );
                let result = driver.run(driver_call(&call)).await.map_err(|_| denied())?;
                serde_json::to_string(&result).map_err(|_| denied())?
            }
            "/internal/cipher-read" => {
                let req: CipherRead = serde_json::from_str(&raw).map_err(|_| denied())?;
                serde_json::to_string(
                    &cipher_read_existing(&self.state, &req.scope).map_err(|_| denied())?,
                )
                .map_err(|_| denied())?
            }
            "/internal/cipher-cas" => {
                let req: CipherCas = serde_json::from_str(&raw).map_err(|_| denied())?;
                serde_json::to_string(
                    &cipher_transition(&self.state, &req.scope, Some(&req))
                        .map_err(|_| denied())?
                        .ok_or_else(denied)?,
                )
                .map_err(|_| denied())?
            }
            _ => return Response::error("not found", 404),
        };
        let mut response = Response::ok(response)?;
        response
            .headers_mut()
            .set("Content-Type", "application/json")?;
        Ok(response)
    }
}
