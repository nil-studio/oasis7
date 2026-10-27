//! Paid read-only session observation. This capability cannot call options,
//! complete, role-key signing, private writes, or the original stage permit oracle.
use super::{CipherRead, actor, clock, denied, js_denied, raw_text, read_fence, record_name};
use crate::{
    envelope_crypto,
    native_identity::provision_transport::{
        NativePrivateRoute, NativeProvisionTransport, PrivateCookie, PrivateCookieKind,
    },
    native_private_store::{
        NativePrivateConfig, NativePrivateScope,
        manifest::{NativeHistoryLookup, NativeManifestStore},
    },
    native_runtime_rpc::{NativeRecoveryAction, NativeRecoveryRpc},
};
use oasis7_custody_core::{
    DecimalU64,
    approval::bytes_digest,
    envelope::KeyEnvelopeV1,
    native_binding::SignedManagedNativeSessionStatusV1,
    native_history::{NativeHistoryPhaseV1, NativeSessionStorageAttestationV1},
    provisioning::{
        NativeStageResultV1, SessionStatusRecoveryPermitViewV1, SessionStatusRecoveryRequestV1,
    },
    recovery::RecoveryFence,
};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use worker::{Env, Response, Result};
use zeroize::Zeroizing;
type JResult<T> = std::result::Result<T, JsValue>;
#[wasm_bindgen(module = "/platform/identity_native_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = ownerNativeRecover)]
    async fn owner_recover(env: &Env, raw: &str) -> JResult<JsValue>;
}
async fn owner<T: DeserializeOwned>(
    env: &Env,
    request: &SessionStatusRecoveryRequestV1,
    action: NativeRecoveryAction,
) -> JResult<T> {
    let raw = serde_json::to_string(&NativeRecoveryRpc {
        request: request.clone(),
        action,
    })
    .map_err(|_| js_denied())?;
    if raw.len() > 32 * 1024 {
        return Err(js_denied());
    }
    let reply = raw_text(owner_recover(env, &raw).await?, 32 * 1024)?;
    serde_json::from_str(&reply).map_err(|_| js_denied())
}
struct Observation {
    view: SessionStatusRecoveryPermitViewV1,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    fn check(&self, env: &Env, request: &SessionStatusRecoveryRequestV1) -> JResult<()> {
        let v = &self.view;
        let now = clock()?;
        v.history.validate().map_err(|_| js_denied())?;
        let environment = env
            .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
            .map_err(|_| js_denied())?
            .to_string();
        let epoch: DecimalU64 = serde_json::from_value(
            env.var("CUSTODY_SERVICE_EPOCH")
                .map_err(|_| js_denied())?
                .to_string()
                .into(),
        )
        .map_err(|_| js_denied())?;
        let Some(attestation) = &v.history.session_attestation else {
            return Err(js_denied());
        };
        let NativeStageResultV1::SessionStatus(old) = &v.history.result else {
            return Err(js_denied());
        };
        if &v.request != request
            || request.api_version != 1
            || request.custody_account_id != v.history.custody_account_id
            || request.provision_id != v.history.provision_id
            || request.history_digest != v.history.digest().map_err(|_| js_denied())?
            || v.source_permit_id != v.history.result_permit_id
            || v.history.environment != environment
            || !matches!(
                v.history.phase,
                NativeHistoryPhaseV1::InitialLogin | NativeHistoryPhaseV1::ManagedLogin
            )
            || v.private_sid_generation != attestation.private_sid_generation
            || v.private_sid_generation.0 == 0
            || v.native_binding_generation != old.status.generation
            || v.binding != old.status.receipt
            || v.service_epoch != epoch
            || v.target_security_epoch.0 == 0
            || v.observed_at.0 > now
            || now - v.observed_at.0 > 5000
            || now >= v.expires_at.0
            || self.fence_started > now
            || now - self.fence_started > 5000
        {
            return Err(js_denied());
        }
        self.fence.check_epoch(epoch.0).map_err(|_| js_denied())?;
        let transport = NativeProvisionTransport::from_env(env).map_err(|_| js_denied())?;
        transport
            .validate_binding(&v.binding)
            .map_err(|_| js_denied())?;
        if v.http_origin != transport.native_origin()
            || v.nonce.len() != 64
            || !v
                .nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(js_denied());
        }
        Ok(())
    }
    fn fingerprint(&self) -> JResult<String> {
        let v = &self.view;
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.native-status-recovery-observation.v1",
            &v.request,
            v.history.digest().map_err(|_| js_denied())?,
            &v.source_permit_id,
            &v.nonce,
            v.private_sid_generation,
            v.native_binding_generation,
            v.target_security_epoch,
            v.service_epoch,
            &v.source_revision,
            &v.authority_revision,
            &v.http_origin,
            &v.binding,
        ))
        .map_err(|_| js_denied())?;
        Ok(bytes_digest(&bytes))
    }
}
async fn observe(env: &Env, request: &SessionStatusRecoveryRequestV1) -> JResult<Observation> {
    // A real persisted Account recovery read ticket is spent by this fixed RPC.
    let view = owner(env, request, NativeRecoveryAction::GetPermit).await?;
    let fence_started = clock()?;
    let raw = raw_text(read_fence(env).await?, 1024)?;
    let observation = Observation {
        view,
        fence_started,
        fence: serde_json::from_str(&raw).map_err(|_| js_denied())?,
    };
    observation.check(env, request)?;
    Ok(observation)
}
struct ReadScope<'a> {
    env: &'a Env,
    request: &'a SessionStatusRecoveryRequestV1,
    current: Observation,
    original_fingerprint: String,
    original_deadline: DecimalU64,
}
impl ReadScope<'_> {
    fn check(&self, record: &NativePrivateScope) -> JResult<()> {
        self.current.check(self.env, self.request)?;
        if record.role_scope != self.current.view.history.scope.role_scope
            || record.identity_namespace != self.current.view.history.scope.identity_namespace
            || record.identity_instance_epoch
                != self.current.view.history.scope.identity_instance_epoch
            || self.current.fingerprint()? != self.original_fingerprint
            || clock()? >= self.original_deadline.0
        {
            return Err(js_denied());
        }
        record.context()?;
        Ok(())
    }
    async fn decrypt(
        &mut self,
        record: &NativePrivateScope,
        expected_digest: &str,
    ) -> JResult<envelope_crypto::SecretBytes> {
        self.check(record)?;
        let envelope: Option<KeyEnvelopeV1> = actor(
            self.env,
            &record_name(record)?,
            "/internal/cipher-read",
            &CipherRead {
                scope: record.clone(),
            },
        )
        .await?;
        let envelope = envelope.ok_or_else(js_denied)?;
        let config = NativePrivateConfig::load(self.env)?;
        if envelope.wrapping_key_version != config.wrapping_version {
            return Err(js_denied());
        }
        let secret = envelope_crypto::open(&envelope, &record.context()?, &config.kek)
            .await
            .map_err(|_| js_denied())?;
        if secret.as_bytes().len() > 12 * 1024 || bytes_digest(secret.as_bytes()) != expected_digest
        {
            return Err(js_denied());
        }
        self.current = observe(self.env, self.request).await?;
        self.check(record)?;
        // Cipher slots are immutable; reread still proves no damaged/replaced original.
        let saved: Option<KeyEnvelopeV1> = actor(
            self.env,
            &record_name(record)?,
            "/internal/cipher-read",
            &CipherRead {
                scope: record.clone(),
            },
        )
        .await?;
        if saved.as_ref() != Some(&envelope) {
            return Err(js_denied());
        }
        self.check(record)?;
        Ok(secret)
    }
}
pub(super) async fn response(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let request: SessionStatusRecoveryRequestV1 =
        serde_json::from_str(raw).map_err(|_| denied())?;
    let observation = observe(env, &request).await.map_err(|_| denied())?;
    let h = &observation.view.history;
    let snapshot = NativeManifestStore::new(state)
        .read_completed_history(&NativeHistoryLookup {
            environment: &h.environment,
            account_id: &h.custody_account_id,
            provision_id: &h.provision_id,
            result_permit_id: &h.result_permit_id,
            phase: h.phase,
            native_operation_id: Some(&h.native_operation_id),
        })
        .map_err(|_| denied())?
        .ok_or_else(denied)?;
    let local = super::history::entry(snapshot.clone())?.ok_or_else(denied)?;
    if &local != h {
        return Err(denied());
    }
    let refs = snapshot.session_refs.ok_or_else(denied)?;
    if refs.private_sid_generation != observation.view.private_sid_generation {
        return Err(denied());
    }
    let original_fingerprint = observation.fingerprint().map_err(|_| denied())?;
    let original_deadline = observation.view.expires_at;
    let mut scope = ReadScope {
        env,
        request: &request,
        current: observation,
        original_fingerprint,
        original_deadline,
    };
    let cookie = scope
        .decrypt(&refs.cookie, &refs.cookie_digest)
        .await
        .map_err(|_| denied())?;
    let csrf = scope
        .decrypt(&refs.csrf, &refs.csrf_digest)
        .await
        .map_err(|_| denied())?;
    let cookie = std::str::from_utf8(cookie.as_bytes()).map_err(|_| denied())?;
    let cookie = PrivateCookie::from_private(PrivateCookieKind::ManagedSession, cookie.into())?;
    let csrf = std::str::from_utf8(csrf.as_bytes()).map_err(|_| denied())?;
    if csrf.len() != 64
        || !csrf
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(denied());
    }
    let transport = NativeProvisionTransport::from_env(env)?;
    let body = Zeroizing::new(
        serde_json::to_string(&serde_json::json!({"nonce":scope.current.view.nonce}))
            .map_err(|_| denied())?,
    );
    let status = transport
        .request::<SignedManagedNativeSessionStatusV1>(
            NativePrivateRoute::SessionStatus,
            &body,
            Some(&cookie),
            Some(csrf),
        )
        .await?
        .body;
    transport
        .fact_pin()
        .session_status(
            &status,
            &scope.current.view.binding,
            &scope.current.view.nonce,
            scope.current.view.native_binding_generation,
            scope.current.view.service_epoch,
        )
        .await
        .map_err(|_| denied())?;
    scope.check(&refs.cookie).map_err(|_| denied())?;
    let session = NativeSessionStorageAttestationV1 {
        status_digest: status.status.digest().map_err(|_| denied())?,
        session_ref: status.status.session_ref.clone(),
        private_sid_generation: refs.private_sid_generation,
    };
    let committed: SignedManagedNativeSessionStatusV1 = owner(
        env,
        &request,
        NativeRecoveryAction::RecordResult {
            status: Box::new(status.clone()),
            session,
        },
    )
    .await
    .map_err(|_| denied())?;
    // The final response is an admitted fact, never a refreshed permission.
    if committed != status {
        return Err(denied());
    }
    scope.check(&refs.cookie).map_err(|_| denied())?;
    Response::from_json(&committed)
}
pub(crate) async fn recover_native_status_raw(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > 32 * 1024 {
        return Err(denied());
    }
    let request: SessionStatusRecoveryRequestV1 =
        serde_json::from_str(raw).map_err(|_| denied())?;
    // Router selectors grant nothing and cannot choose actor URLs or private slots.
    let name = super::manifest_name_parts(env, &request.custody_account_id, &request.provision_id)?;
    let status: SignedManagedNativeSessionStatusV1 =
        actor(env, &name, "/internal/recover-status", &request)
            .await
            .map_err(|_| denied())?;
    serde_json::to_string(&status).map_err(|_| denied())
}
