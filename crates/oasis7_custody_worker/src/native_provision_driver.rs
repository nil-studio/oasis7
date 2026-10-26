//! Identity-only native orchestration. Ports are fixed private capabilities implemented
//! by the runtime, not caller supplied grants, URLs or deserializable trusted readers.
use crate::{
    native_identity::provision_transport::*,
    native_private_store::{NativePrivateScope, manifest::*},
    native_provision_challenge::*,
};
use oasis7_custody_core::{DecimalU64, approval::bytes_digest, native_binding::*, provisioning::*};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wasm_bindgen::JsValue;
use zeroize::{Zeroize, Zeroizing};
type Result<T> = std::result::Result<T, JsValue>;
fn rejected() -> JsValue {
    JsValue::from_str("native provision phase rejected")
}
fn now() -> Result<u64> {
    let v = js_sys::Date::now();
    if !v.is_finite() || v < 0.0 || v.fract() != 0.0 || v > 9_007_199_254_740_991.0 {
        return Err(rejected());
    }
    Ok(v as u64)
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum NativeDriverCall {
    Provision {
        request: NativeStageRequestV1,
        attempt_id: String,
    },
    ManagedLogin {
        operation_id: String,
    },
}
#[derive(Clone)]
pub(crate) enum NativeDriverPermit {
    Provision(Box<NativeStagePermitViewV1>),
    ManagedLogin(Box<ManagedLoginPermitViewV1>),
}
pub(crate) struct OwnedNativeSessionAttestation {
    pub status_digest: String,
    pub session_ref: String,
    pub private_sid_generation: DecimalU64,
}
/// Every reserve_permit must first spend a real Account read ticket and run its
/// exact current-source/ACL/Fence checks. All private/signing ports refresh that
/// same exact authority around async cryptography and use scope-derived locators.
pub(crate) trait NativeProvisionPorts {
    async fn reserve_permit(&self, call: &NativeDriverCall) -> Result<NativeDriverPermit>;
    async fn load_private(
        &self,
        call: &NativeDriverCall,
        scope: &NativePrivateScope,
    ) -> Result<Option<Zeroizing<Vec<u8>>>>;
    async fn save_private(
        &self,
        call: &NativeDriverCall,
        scope: &NativePrivateScope,
        bytes: &[u8],
    ) -> Result<()>;
    async fn sign_challenge(
        &self,
        call: &NativeDriverCall,
        challenge: &NativeProvisionChallenge,
    ) -> Result<Zeroizing<String>>;
    async fn sign_possession(
        &self,
        call: &NativeDriverCall,
        statement: &SignedOwnerNativeBindingStatementV1,
    ) -> Result<Zeroizing<String>>;
    /// Account verifies/registers this root-signed untrusted fact before the fixed
    /// Owner constructs a statement from its own plan and current authority.
    async fn owner_statement(
        &self,
        call: &NativeDriverCall,
        challenge: &SignedNativeBindingChallengeV1,
    ) -> Result<SignedOwnerNativeBindingStatementV1>;
    async fn binding_statement(
        &self,
        call: &NativeDriverCall,
        binding_operation_id: &str,
    ) -> Result<Option<SignedOwnerNativeBindingStatementV1>>;
    async fn binding_receipt(
        &self,
        call: &NativeDriverCall,
    ) -> Result<SignedNativeBindingReceiptV1>;
    async fn record_result(
        &self,
        call: &NativeDriverCall,
        result: &NativeStageResultV1,
        session: Option<OwnedNativeSessionAttestation>,
    ) -> Result<()>;
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SecretContext {
    cookie: String,
    csrf: String,
    identity_service_id: String,
    environment_id: String,
}
impl Drop for SecretContext {
    fn drop(&mut self) {
        self.cookie.zeroize();
        self.csrf.zeroize();
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalComplete {
    challenge: NativeProvisionChallenge,
    complete_body: String,
}
impl Drop for OriginalComplete {
    fn drop(&mut self) {
        self.complete_body.zeroize();
    }
}
impl OriginalComplete {
    fn take_body(mut self) -> Zeroizing<String> {
        Zeroizing::new(std::mem::take(&mut self.complete_body))
    }
}
struct SecretJson(Value);
impl Drop for SecretJson {
    fn drop(&mut self) {
        fn wipe(value: &mut Value) {
            match value {
                Value::String(s) => s.zeroize(),
                Value::Array(values) => values.iter_mut().for_each(wipe),
                Value::Object(values) => values.values_mut().for_each(wipe),
                _ => {}
            }
        }
        wipe(&mut self.0);
    }
}
#[derive(Serialize)]
struct CompleteProof<'a> {
    operation_id: &'a str,
    ceremony_id: &'a str,
    signature: &'a str,
}
#[derive(Serialize)]
struct BindingComplete<'a> {
    api_version: u8,
    statement: &'a SignedOwnerNativeBindingStatementV1,
    login_key_signature: &'a str,
}
fn encode<T: Serialize>(value: &T) -> Result<Zeroizing<Vec<u8>>> {
    let bytes = Zeroizing::new(serde_json::to_vec(value).map_err(|_| rejected())?);
    if bytes.is_empty() || bytes.len() > 12 * 1024 {
        return Err(rejected());
    }
    Ok(bytes)
}
fn body<T: Serialize>(value: &T) -> Result<Zeroizing<String>> {
    Ok(Zeroizing::new(
        serde_json::to_string(value).map_err(|_| rejected())?,
    ))
}
pub(crate) struct NativeProvisionDriver<P> {
    ports: P,
    manifest: NativeManifestStore,
    transport: NativeProvisionTransport,
}
impl<P: NativeProvisionPorts> NativeProvisionDriver<P> {
    pub(crate) fn new(
        ports: P,
        manifest_state: JsValue,
        transport: NativeProvisionTransport,
    ) -> Self {
        Self {
            ports,
            manifest: NativeManifestStore::new(manifest_state),
            transport,
        }
    }
    pub(crate) async fn run(&self, call: NativeDriverCall) -> Result<NativeStageResultV1> {
        let permit = self.ports.reserve_permit(&call).await?;
        let mut phase = Phase {
            driver: self,
            call,
            permit,
        };
        phase.validate()?;
        let current = phase.current()?;
        let state = self.manifest.claim_phase(|| Ok(current.clone()))?;
        if let Some((result, generation)) =
            self.manifest.completed_result(|| Ok(current.clone()))?
        {
            let session = match (&result, generation) {
                (NativeStageResultV1::SessionStatus(status), Some(generation)) => {
                    Some(OwnedNativeSessionAttestation {
                        status_digest: status.status.digest().map_err(|_| rejected())?,
                        session_ref: status.status.session_ref.clone(),
                        private_sid_generation: generation,
                    })
                }
                (_, None) => None,
                _ => return Err(rejected()),
            };
            self.ports
                .record_result(&phase.call, &result, session)
                .await?;
            return Ok(result);
        }
        match current.phase {
            NativePhase::Register => phase.registration(state).await,
            NativePhase::Binding => phase.binding(state).await,
            NativePhase::InitialLogin | NativePhase::ManagedLogin => phase.login(state).await,
        }
    }
}
struct Phase<'a, P> {
    driver: &'a NativeProvisionDriver<P>,
    call: NativeDriverCall,
    permit: NativeDriverPermit,
}
impl<P: NativeProvisionPorts> Phase<'_, P> {
    fn validate(&self) -> Result<()> {
        let now = now()?;
        match (&self.call, &self.permit) {
            (
                NativeDriverCall::Provision {
                    request,
                    attempt_id,
                },
                NativeDriverPermit::Provision(v),
            ) => {
                request.validate().map_err(|_| rejected())?;
                self.driver
                    .transport
                    .validate_registration(&v.descriptor.registration)
                    .map_err(|_| rejected())?;
                if request != &v.request
                    || attempt_id != &request.operation_id
                    || attempt_id != &v.attempt_id
                    || v.request_digest != request.digest().map_err(|_| rejected())?
                    || v.identity_scope.provision_id != request.provision_id
                    || v.identity_scope.context.identity.account_id != request.custody_account_id
                    || v.identity_scope.context.identity.key_id != v.descriptor.identity_key.key_id
                    || v.identity_scope.context.identity.key_version
                        != v.descriptor.identity_key.key_version
                    || v.descriptor.registration.login_public_key
                        != v.descriptor.identity_key.public_key
                    || v.descriptor.http_origin != self.driver.transport.native_origin()
                    || v.observed_at.0 > now
                    || now.saturating_sub(v.observed_at.0) > 5_000
                    || v.expires_at.0 <= now
                {
                    return Err(rejected());
                }
            }
            (
                NativeDriverCall::ManagedLogin { operation_id },
                NativeDriverPermit::ManagedLogin(v),
            ) => {
                if operation_id != &v.task.operation_id
                    || v.task.claimed_at.is_none()
                    || v.task.result.is_some()
                    || v.http_origin != self.driver.transport.native_origin()
                    || v.observed_at.0 > now
                    || now.saturating_sub(v.observed_at.0) > 5_000
                    || v.expires_at.0 <= now
                    || v.task.login_key.public_key != v.binding.login_public_key
                    || v.task.native_account_id != v.binding.native_account_id
                    || v.task.native_identity_namespace != v.binding.identity_namespace
                {
                    return Err(rejected());
                }
            }
            _ => return Err(rejected()),
        }
        Ok(())
    }
    fn current(&self) -> Result<ManifestCurrent> {
        let (scope, phase, operation_id, permit_id, revision, expires, stable) = match &self.permit
        {
            NativeDriverPermit::Provision(v) => {
                let phase = match v.request.stage {
                    NativeStageKindV1::Registration => NativePhase::Register,
                    NativeStageKindV1::Binding => NativePhase::Binding,
                    NativeStageKindV1::Login => NativePhase::InitialLogin,
                };
                let operation = match phase {
                    NativePhase::Register => {
                        v.descriptor.registration.registration_operation_id.clone()
                    }
                    NativePhase::Binding => v.request.binding_operation_id.clone(),
                    _ => v.descriptor.login_request_id.clone(),
                };
                let scope = ManifestScope {
                    role_scope: v.identity_scope.clone(),
                    identity_namespace: v.descriptor.registration.identity_namespace.clone(),
                    identity_instance_epoch: v.descriptor.registration.identity_instance_epoch,
                    registration_operation_id: v
                        .descriptor
                        .registration
                        .registration_operation_id
                        .clone(),
                    initial_login_operation_id: v.descriptor.login_request_id.clone(),
                };
                let stable = oasis7_wasm_abi::encode_canonical_cbor(&(
                    "oasis7.custody.native-manifest-intent.v1",
                    &scope,
                    phase,
                    &operation,
                ))
                .map_err(|_| rejected())?;
                (
                    scope,
                    phase,
                    operation,
                    v.attempt_id.clone(),
                    v.authority_revision.clone(),
                    v.expires_at,
                    stable,
                )
            }
            NativeDriverPermit::ManagedLogin(v) => {
                // The original baseline comes from the actual persisted native receipt
                // plus Account's exact original descriptor, supplied by this view.
                let scope = ManifestScope {
                    role_scope: v.identity_scope.clone(),
                    identity_namespace: v.binding.identity_namespace.clone(),
                    identity_instance_epoch: v.binding.identity_instance_epoch,
                    registration_operation_id: v.binding.registration_operation_id.clone(),
                    initial_login_operation_id: v.initial_login_operation_id.clone(),
                };
                let stable = oasis7_wasm_abi::encode_canonical_cbor(&(
                    "oasis7.custody.native-manifest-intent.v1",
                    &scope,
                    NativePhase::ManagedLogin,
                    &v.task.login_operation_id,
                ))
                .map_err(|_| rejected())?;
                (
                    scope,
                    NativePhase::ManagedLogin,
                    v.task.login_operation_id.clone(),
                    v.task.operation_id.clone(),
                    v.authority_revision.clone(),
                    v.expires_at,
                    stable,
                )
            }
        };
        Ok(ManifestCurrent {
            scope,
            phase,
            operation_id,
            permit_id,
            request_digest: bytes_digest(&stable),
            authority_revision: revision,
            expires_at: expires,
        })
    }
    async fn load(
        &self,
        kind: PrivateRecordKind,
        registration: bool,
    ) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let current = self.current()?;
        let reference = if registration {
            self.driver
                .manifest
                .registration_record_reference(|| Ok(current.clone()), kind)?
        } else {
            self.driver
                .manifest
                .record_reference(|| Ok(current.clone()), kind)?
        };
        let Some((scope, digest, stored)) = reference else {
            return Ok(None);
        };
        let bytes = self.driver.ports.load_private(&self.call, &scope).await?;
        if let Some(bytes) = bytes {
            if bytes_digest(&bytes) != digest {
                return Err(rejected());
            }
            if !stored {
                self.driver
                    .manifest
                    .record_stored(|| Ok(current.clone()), &scope)?;
            }
            Ok(Some(bytes))
        } else if stored {
            Err(rejected())
        } else {
            Ok(None)
        }
    }
    async fn save(&self, kind: PrivateRecordKind, bytes: &[u8]) -> Result<()> {
        let current = self.current()?;
        let scope = self.driver.manifest.reserve_record(
            || Ok(current.clone()),
            kind,
            &bytes_digest(bytes),
        )?;
        self.driver
            .ports
            .save_private(&self.call, &scope, bytes)
            .await?;
        self.driver
            .manifest
            .record_stored(|| Ok(current.clone()), &scope)
    }
    async fn original(
        &self,
        kind: PrivateRecordKind,
        candidate: Zeroizing<String>,
    ) -> Result<Zeroizing<String>> {
        if let Some(original) = self.load(kind, false).await? {
            if original.as_slice() != candidate.as_bytes() {
                return Err(rejected());
            }
            return String::from_utf8(original.to_vec())
                .map(Zeroizing::new)
                .map_err(|_| rejected());
        }
        self.save(kind, candidate.as_bytes()).await?;
        Ok(candidate)
    }
    fn provision(&self) -> Result<&NativeStagePermitViewV1> {
        match &self.permit {
            NativeDriverPermit::Provision(v) => Ok(v),
            _ => Err(rejected()),
        }
    }
    async fn context(&self, registering: bool) -> Result<SecretContext> {
        if let Some(raw) = self.load(PrivateRecordKind::Preauth, true).await? {
            return serde_json::from_slice(&raw).map_err(|_| rejected());
        }
        if !registering {
            return Err(rejected());
        }
        let reply = self
            .driver
            .transport
            .request::<Value>(NativePrivateRoute::Capabilities, "", None, None)
            .await
            .map_err(|_| rejected())?;
        let cookie = reply.cookie.ok_or_else(rejected)?;
        let caps = SecretJson(reply.body);
        let native = &caps.0["native_key"];
        let service = native["identity_service_id"]
            .as_str()
            .ok_or_else(rejected)?;
        let environment = native["environment_id"].as_str().ok_or_else(rejected)?;
        let csrf = caps.0["csrf_token"].as_str().ok_or_else(rejected)?;
        let namespace = format!("{}:{}:{}", service.len(), service, environment);
        let expected = &self.provision()?.descriptor.registration;
        if native["available"] != true
            || native["algorithm"] != "Ed25519"
            || caps.0["machine_registration"]["available"] != true
            || namespace != expected.identity_namespace
            || csrf.len() != 64
            || !csrf
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(rejected());
        }
        let context = SecretContext {
            cookie: cookie.token().into(),
            csrf: csrf.into(),
            identity_service_id: service.into(),
            environment_id: environment.into(),
        };
        self.save(PrivateRecordKind::Preauth, &encode(&context)?)
            .await?;
        Ok(context)
    }
    async fn finish(
        &mut self,
        result: NativeStageResultV1,
        session: Option<OwnedNativeSessionAttestation>,
    ) -> Result<NativeStageResultV1> {
        let current = self.current()?;
        self.driver.manifest.persist_result(
            || Ok(current.clone()),
            &result,
            session.as_ref().map(|v| v.private_sid_generation),
        )?;
        // The manifest records private facts, never Account authority. Persist
        // these first so a committed Account result cannot strand its next phase.
        self.driver
            .ports
            .record_result(&self.call, &result, session)
            .await?;
        Ok(result)
    }
    async fn registration(&mut self, state: PhaseState) -> Result<NativeStageResultV1> {
        let current = self.current()?;
        if state != PhaseState::Prepared {
            self.driver.manifest.recover_phase(|| Ok(current.clone()))?;
            self.driver
                .manifest
                .claim_registration_recovery(|| Ok(current.clone()))?;
        }
        let context = self.context(state == PhaseState::Prepared).await?;
        let cookie =
            PrivateCookie::from_private(PrivateCookieKind::Preauth, context.cookie.clone())
                .map_err(|_| rejected())?;
        let registration = self.provision()?.descriptor.registration.clone();
        let options = self.original(PrivateRecordKind::OptionsRequest, body(&json!({"operation_id":registration.registration_operation_id,"public_key":registration.login_public_key,"display_name":registration.display_name}))?).await?;
        if state == PhaseState::Prepared {
            self.driver
                .manifest
                .set_phase_state(|| Ok(current.clone()), PhaseState::Dispatched)?;
        }
        let complete =
            if let Some(original) = self.load(PrivateRecordKind::CompleteRequest, false).await? {
                let original: OriginalComplete =
                    serde_json::from_slice(&original).map_err(|_| rejected())?;
                original.take_body()
            } else {
                let reply = self
                    .driver
                    .transport
                    .request::<Value>(
                        NativePrivateRoute::RegistrationOptions,
                        &options,
                        Some(&cookie),
                        Some(&context.csrf),
                    )
                    .await
                    .map_err(|_| rejected())?;
                self.proof(&context, &reply.body, NativeProvisionPurpose::Register)
                    .await?
            };
        self.driver
            .transport
            .request::<Value>(
                NativePrivateRoute::RegistrationComplete,
                &complete,
                Some(&cookie),
                Some(&context.csrf),
            )
            .await
            .map_err(|_| rejected())?;
        let challenge = self.binding_options(&cookie, &context.csrf).await?;
        self.finish(NativeStageResultV1::RegistrationChallenge(challenge), None)
            .await
    }
    async fn proof(
        &mut self,
        context: &SecretContext,
        reply: &Value,
        purpose: NativeProvisionPurpose,
    ) -> Result<Zeroizing<String>> {
        let challenge: NativeProvisionChallenge =
            serde_json::from_value(reply["options"].clone()).map_err(|_| rejected())?;
        let current = self.current()?;
        let expected = NativeChallengeScope {
            identity_service_id: context.identity_service_id.clone(),
            environment_id: context.environment_id.clone(),
            audience: self.driver.transport.native_origin().into(),
            operation_id: current.operation_id.clone(),
            public_key: match &self.permit {
                NativeDriverPermit::Provision(v) => v.descriptor.identity_key.public_key.clone(),
                NativeDriverPermit::ManagedLogin(v) => v.task.login_key.public_key.clone(),
            },
            purpose,
        };
        if reply["operation_id"] != expected.operation_id
            || reply["ceremony_id"] != expected.operation_id
            || reply["expires_at"] != challenge.expires_at
        {
            return Err(rejected());
        }
        challenge.signing_bytes(&expected, now()?)?;
        let signature = self
            .driver
            .ports
            .sign_challenge(&self.call, &challenge)
            .await?;
        let complete_body = body(&CompleteProof {
            operation_id: &expected.operation_id,
            ceremony_id: &expected.operation_id,
            signature: &signature,
        })?;
        let candidate = OriginalComplete {
            challenge,
            complete_body: complete_body.to_string(),
        };
        if let Some(original) = self.load(PrivateRecordKind::CompleteRequest, false).await? {
            let original: OriginalComplete =
                serde_json::from_slice(&original).map_err(|_| rejected())?;
            if original.challenge != candidate.challenge
                || original.complete_body != candidate.complete_body
            {
                return Err(rejected());
            }
            return Ok(original.take_body());
        }
        self.save(PrivateRecordKind::CompleteRequest, &encode(&candidate)?)
            .await?;
        Ok(complete_body)
    }
    async fn binding_options(
        &self,
        cookie: &PrivateCookie,
        csrf: &str,
    ) -> Result<SignedNativeBindingChallengeV1> {
        let permit = self.provision()?;
        let request = body(
            &json!({"registration_operation_id":permit.descriptor.registration.registration_operation_id,"binding_operation_id":permit.request.binding_operation_id}),
        )?;
        let reply = self
            .driver
            .transport
            .request(
                NativePrivateRoute::BindingOptions,
                &request,
                Some(cookie),
                Some(csrf),
            )
            .await
            .map_err(|_| rejected())?;
        self.driver
            .transport
            .fact_pin()
            .registration_challenge(
                &reply.body,
                &permit.descriptor.registration,
                &permit.request.binding_operation_id,
            )
            .await?;
        Ok(reply.body)
    }
    async fn binding(&mut self, state: PhaseState) -> Result<NativeStageResultV1> {
        let context = self.context(false).await?;
        let cookie =
            PrivateCookie::from_private(PrivateCookieKind::Preauth, context.cookie.clone())
                .map_err(|_| rejected())?;
        let request = body(
            &json!({"registration_operation_id":self.provision()?.descriptor.registration.registration_operation_id}),
        )?;
        let lookup = self
            .driver
            .transport
            .request::<Value>(
                NativePrivateRoute::BindingLookup,
                &request,
                Some(&cookie),
                Some(&context.csrf),
            )
            .await
            .map_err(|_| rejected())?;
        if lookup.http_status == 200 {
            let receipt: SignedNativeBindingReceiptV1 =
                serde_json::from_value(lookup.body).map_err(|_| rejected())?;
            let original = self
                .driver
                .ports
                .binding_statement(&self.call, &receipt.receipt.binding_operation_id)
                .await?
                .ok_or_else(rejected)?;
            self.driver
                .transport
                .fact_pin()
                .binding_receipt(&receipt, &original.statement)
                .await?;
            return self
                .finish(NativeStageResultV1::BindingReceipt(receipt), None)
                .await;
        }
        if state != PhaseState::Prepared {
            return Err(rejected());
        }
        let challenge = self.binding_options(&cookie, &context.csrf).await?;
        let statement = self
            .driver
            .ports
            .owner_statement(&self.call, &challenge)
            .await?;
        self.save(PrivateRecordKind::BindingStatement, &encode(&statement)?)
            .await?;
        let signature = self
            .driver
            .ports
            .sign_possession(&self.call, &statement)
            .await?;
        let complete = self
            .original(
                PrivateRecordKind::CompleteRequest,
                body(&BindingComplete {
                    api_version: 1,
                    statement: &statement,
                    login_key_signature: &signature,
                })?,
            )
            .await?;
        let current = self.current()?;
        self.driver
            .manifest
            .set_phase_state(|| Ok(current.clone()), PhaseState::Dispatched)?;
        let reply = self
            .driver
            .transport
            .request::<SignedNativeBindingReceiptV1>(
                NativePrivateRoute::BindingComplete,
                &complete,
                Some(&cookie),
                Some(&context.csrf),
            )
            .await
            .map_err(|_| rejected())?;
        self.driver
            .transport
            .fact_pin()
            .binding_receipt(&reply.body, &statement.statement)
            .await?;
        self.finish(NativeStageResultV1::BindingReceipt(reply.body), None)
            .await
    }
    async fn login(&mut self, state: PhaseState) -> Result<NativeStageResultV1> {
        let context = self.context(false).await?;
        let sid = self.load(PrivateRecordKind::SessionCookie, false).await?;
        let sid = if let Some(sid) = sid {
            sid
        } else {
            if state != PhaseState::Prepared {
                let current = self.current()?;
                self.driver.manifest.recover_phase(|| Ok(current.clone()))?;
                return self
                    .finish(NativeStageResultV1::ExplicitLoginRequired, None)
                    .await;
            }
            let cookie =
                PrivateCookie::from_private(PrivateCookieKind::Preauth, context.cookie.clone())
                    .map_err(|_| rejected())?;
            let current = self.current()?;
            let public_key = match &self.permit {
                NativeDriverPermit::Provision(v) => &v.descriptor.identity_key.public_key,
                NativeDriverPermit::ManagedLogin(v) => &v.task.login_key.public_key,
            };
            let options = self
                .original(
                    PrivateRecordKind::OptionsRequest,
                    body(&json!({"operation_id":current.operation_id,"public_key":public_key}))?,
                )
                .await?;
            let reply = self
                .driver
                .transport
                .request::<Value>(
                    NativePrivateRoute::LoginOptions,
                    &options,
                    Some(&cookie),
                    Some(&context.csrf),
                )
                .await
                .map_err(|_| rejected())?;
            let complete = self
                .proof(&context, &reply.body, NativeProvisionPurpose::Login)
                .await?;
            self.driver
                .manifest
                .set_phase_state(|| Ok(current.clone()), PhaseState::Dispatched)?;
            // After dispatch no path retries complete. A lost/missing Cookie is
            // explicit action even if native committed the original operation.
            let reply = self
                .driver
                .transport
                .request::<Value>(
                    NativePrivateRoute::LoginComplete,
                    &complete,
                    Some(&cookie),
                    Some(&context.csrf),
                )
                .await;
            let Some(sid) = reply.ok().and_then(|r| r.cookie) else {
                self.driver
                    .manifest
                    .set_phase_state(|| Ok(current.clone()), PhaseState::ExplicitLoginRequired)?;
                return self
                    .finish(NativeStageResultV1::ExplicitLoginRequired, None)
                    .await;
            };
            let raw = Zeroizing::new(sid.token().as_bytes().to_vec());
            self.save(PrivateRecordKind::SessionCookie, &raw).await?;
            raw
        };
        let sid = PrivateCookie::from_private(
            PrivateCookieKind::ManagedSession,
            String::from_utf8(sid.to_vec()).map_err(|_| rejected())?,
        )
        .map_err(|_| rejected())?;
        let csrf = if let Some(csrf) = self.load(PrivateRecordKind::SessionCsrf, false).await? {
            csrf
        } else {
            let caps = self
                .driver
                .transport
                .request::<Value>(NativePrivateRoute::Capabilities, "", Some(&sid), None)
                .await
                .map_err(|_| rejected())?;
            let caps = SecretJson(caps.body);
            let csrf = caps.0["csrf_token"].as_str().ok_or_else(rejected)?;
            if csrf.len() != 64
                || !csrf
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(rejected());
            }
            let bytes = Zeroizing::new(csrf.as_bytes().to_vec());
            self.save(PrivateRecordKind::SessionCsrf, &bytes).await?;
            bytes
        };
        let nonce = match &self.permit {
            NativeDriverPermit::Provision(v) => v.nonce.clone(),
            NativeDriverPermit::ManagedLogin(v) => v.task.status_nonce.clone(),
        };
        let status = self
            .driver
            .transport
            .request::<SignedManagedNativeSessionStatusV1>(
                NativePrivateRoute::SessionStatus,
                &body(&json!({"nonce":nonce}))?,
                Some(&sid),
                Some(std::str::from_utf8(&csrf).map_err(|_| rejected())?),
            )
            .await
            .map_err(|_| rejected())?
            .body;
        let receipt = self.driver.ports.binding_receipt(&self.call).await?;
        let service_epoch = match &self.permit {
            NativeDriverPermit::Provision(v) => v.service_epoch,
            NativeDriverPermit::ManagedLogin(v) => v.service_epoch,
        };
        self.driver
            .transport
            .fact_pin()
            .session_status(
                &status,
                &receipt.receipt,
                &nonce,
                DecimalU64(1),
                service_epoch,
            )
            .await?;
        let current = self.current()?;
        let generation = self
            .driver
            .manifest
            .sid_generation(|| Ok(current.clone()))?;
        let attestation = OwnedNativeSessionAttestation {
            status_digest: status.status.digest().map_err(|_| rejected())?,
            session_ref: status.status.session_ref.clone(),
            private_sid_generation: generation,
        };
        self.finish(
            NativeStageResultV1::SessionStatus(status),
            Some(attestation),
        )
        .await
    }
}
