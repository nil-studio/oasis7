//! One prepaid first-binding workflow. Never replays a claimed workflow or writes a secret.
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
    participant_runtime_rpc::{ParticipantRuntimeAction, ParticipantRuntimeRpc},
};
use oasis7_agent_api::game::{
    GAME_PARTICIPANT_REGISTER_DOMAIN, GameParticipantAdmissionV1, GameParticipantReceiptV1,
};
use oasis7_client_api::world_service::{
    self as ws, SignedReadRequest, SignedServiceResponse,
    participant_discovery::{ParticipantOwnerControlResponseV1, ParticipantPolicyOriginalV1},
    private_goal::verify_owner_signature_strict,
};
use oasis7_custody_core::{
    DecimalU64,
    approval::bytes_digest,
    envelope::KeyEnvelopeV1,
    native_binding::SignedManagedNativeSessionStatusV1,
    provisioning::{
        ParticipantFinalResultV1, ParticipantPermitViewV1, ParticipantProvisionRequestV1,
        ParticipantResultSourceV1, ParticipantSessionEvidenceV1, ParticipantSignBundleV1,
        ParticipantStepV1,
    },
    recovery::RecoveryFence,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use wasm_bindgen::prelude::*;
use worker::{Env, Response, Result};
use zeroize::Zeroizing;
type JResult<T> = std::result::Result<T, JsValue>;
#[wasm_bindgen(module = "/platform/identity_native_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = ownerParticipantCall)]
    async fn call_owner(env: &Env, raw: &str) -> JResult<JsValue>;
    #[wasm_bindgen(catch, js_name = ownerParticipantSign)]
    async fn call_signer(env: &Env, raw: &str) -> JResult<JsValue>;
    #[wasm_bindgen(catch, js_name = waitParticipantCommit)]
    async fn wait_commit() -> JResult<JsValue>;
}
async fn owner<T: DeserializeOwned>(
    env: &Env,
    request: &ParticipantProvisionRequestV1,
    action: ParticipantRuntimeAction,
) -> JResult<T> {
    let raw = serde_json::to_string(&ParticipantRuntimeRpc {
        request: request.clone(),
        action,
    })
    .map_err(|_| js_denied())?;
    if raw.len() > 32 * 1024 {
        return Err(js_denied());
    }
    serde_json::from_str(&raw_text(call_owner(env, &raw).await?, 32 * 1024)?)
        .map_err(|_| js_denied())
}
struct Observation {
    view: ParticipantPermitViewV1,
    fence: RecoveryFence,
    fence_started: u64,
}
impl Observation {
    fn check(&self, env: &Env, request: &ParticipantProvisionRequestV1) -> JResult<()> {
        let now = clock()?;
        let v = &self.view;
        let epoch: DecimalU64 = serde_json::from_value(
            env.var("CUSTODY_SERVICE_EPOCH")
                .map_err(|_| js_denied())?
                .to_string()
                .into(),
        )
        .map_err(|_| js_denied())?;
        let environment = env
            .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
            .map_err(|_| js_denied())?
            .to_string();
        if env
            .var("CUSTODY_PARTICIPANT_PROVISION_ENABLED")
            .map_err(|_| js_denied())?
            .to_string()
            != "true"
            || &v.request != request
            || v.permit_id != request.operation_id
            || v.target_security_epoch.0 != 1
            || v.service_epoch != epoch
            || v.descriptor.native_binding.environment != environment
            || v.descriptor.session.scope.role_scope != v.descriptor.identity_scope
            || v.descriptor.session.private_sid_generation.0 == 0
            || v.observed_at.0 > now
            || now - v.observed_at.0 > 5000
            || now >= v.expires_at.0
            || self.fence_started > now
            || now - self.fence_started > 5000
            || v.session_observation_nonce.len() != 64
            || !v
                .session_observation_nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(js_denied());
        }
        self.fence.check_epoch(epoch.0).map_err(|_| js_denied())?;
        NativeProvisionTransport::from_env(env)
            .map_err(|_| js_denied())?
            .validate_binding(&v.descriptor.native_binding)
            .map_err(|_| js_denied())?;
        Ok(())
    }
    fn fingerprint(&self) -> JResult<String> {
        let v = &self.view;
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.participant-identity-scope.v1",
            &v.request,
            &v.descriptor,
            &v.permit_id,
            &v.source_revision,
            &v.authority_revision,
            v.target_security_epoch,
            v.service_epoch,
            v.expires_at,
        ))
        .map_err(|_| js_denied())?;
        Ok(bytes_digest(&bytes))
    }
}
async fn observe(
    env: &Env,
    request: &ParticipantProvisionRequestV1,
    step: ParticipantStepV1,
) -> JResult<Observation> {
    let view: ParticipantPermitViewV1 =
        owner(env, request, ParticipantRuntimeAction::GetPermit { step }).await?;
    if view.step != step {
        return Err(js_denied());
    }
    let fence_started = clock()?;
    let fence =
        serde_json::from_str(&raw_text(read_fence(env).await?, 1024)?).map_err(|_| js_denied())?;
    let result = Observation {
        view,
        fence,
        fence_started,
    };
    result.check(env, request)?;
    Ok(result)
}
async fn decrypt_pair(
    env: &Env,
    request: &ParticipantProvisionRequestV1,
    first: &Observation,
    cookie: &NativePrivateScope,
    cookie_digest: &str,
    csrf: &NativePrivateScope,
    csrf_digest: &str,
) -> JResult<(envelope_crypto::SecretBytes, envelope_crypto::SecretBytes)> {
    let config = NativePrivateConfig::load(env)?;
    let mut secrets = Vec::new();
    for (record, digest) in [(cookie, cookie_digest), (csrf, csrf_digest)] {
        first.check(env, request)?;
        if record.role_scope != first.view.descriptor.session.scope.role_scope
            || record.identity_namespace != first.view.descriptor.session.scope.identity_namespace
            || record.identity_instance_epoch
                != first.view.descriptor.session.scope.identity_instance_epoch
        {
            return Err(js_denied());
        }
        let envelope: Option<KeyEnvelopeV1> = actor(
            env,
            &record_name(record)?,
            "/internal/cipher-read",
            &CipherRead {
                scope: record.clone(),
            },
        )
        .await?;
        first.check(env, request)?;
        let envelope = envelope.ok_or_else(js_denied)?;
        if envelope.wrapping_key_version != config.wrapping_version {
            return Err(js_denied());
        }
        let secret = envelope_crypto::open(&envelope, &record.context()?, &config.kek)
            .await
            .map_err(|_| js_denied())?;
        first.check(env, request)?;
        if bytes_digest(secret.as_bytes()) != digest || secret.as_bytes().len() > 12 * 1024 {
            return Err(js_denied());
        }
        let after: Option<KeyEnvelopeV1> = actor(
            env,
            &record_name(record)?,
            "/internal/cipher-read",
            &CipherRead {
                scope: record.clone(),
            },
        )
        .await?;
        first.check(env, request)?;
        if after.as_ref() != Some(&envelope) {
            return Err(js_denied());
        }
        secrets.push(secret);
    }
    let csrf = secrets.pop().ok_or_else(js_denied)?;
    let cookie = secrets.pop().ok_or_else(js_denied)?;
    Ok((cookie, csrf))
}
fn same(
    env: &Env,
    request: &ParticipantProvisionRequestV1,
    view: &Observation,
    fingerprint: &str,
) -> JResult<()> {
    view.check(env, request)?;
    if view.fingerprint()? != fingerprint {
        return Err(js_denied());
    }
    Ok(())
}
async fn session(
    env: &Env,
    request: &ParticipantProvisionRequestV1,
    view: &Observation,
    cookie: &PrivateCookie,
    csrf: &str,
) -> JResult<ParticipantSessionEvidenceV1> {
    session_for_nonce(
        env,
        request,
        view,
        &view.view.session_observation_nonce,
        cookie,
        csrf,
    )
    .await
}
async fn session_for_nonce(
    env: &Env,
    request: &ParticipantProvisionRequestV1,
    view: &Observation,
    nonce: &str,
    cookie: &PrivateCookie,
    csrf: &str,
) -> JResult<ParticipantSessionEvidenceV1> {
    view.check(env, request)?;
    if nonce != view.view.session_observation_nonce
        && view.view.lookup_session_observation_nonce.as_deref() != Some(nonce)
    {
        return Err(js_denied());
    }
    let transport = NativeProvisionTransport::from_env(env).map_err(|_| js_denied())?;
    let status = transport
        .request::<SignedManagedNativeSessionStatusV1>(
            NativePrivateRoute::SessionStatus,
            &serde_json::to_string(&serde_json::json!({"nonce":nonce})).map_err(|_| js_denied())?,
            Some(cookie),
            Some(csrf),
        )
        .await
        .map_err(|_| js_denied())?
        .body;
    transport
        .fact_pin()
        .session_status(
            &status,
            &view.view.descriptor.native_binding,
            nonce,
            view.view.descriptor.session.native_binding_generation,
            view.view.service_epoch,
        )
        .await?;
    view.check(env, request)?;
    if status.status.session_ref != view.view.descriptor.session.session_ref {
        return Err(js_denied());
    }
    Ok(ParticipantSessionEvidenceV1 {
        status,
        private_sid_generation: view.view.descriptor.session.private_sid_generation,
    })
}
async fn forward<T: DeserializeOwned, Q: Serialize>(
    env: &Env,
    request: &ParticipantProvisionRequestV1,
    view: &Observation,
    route: NativePrivateRoute,
    body: &Q,
    cookie: &PrivateCookie,
    csrf: &str,
) -> JResult<T> {
    view.check(env, request)?;
    let raw = Zeroizing::new(serde_json::to_string(body).map_err(|_| js_denied())?);
    let body = NativeProvisionTransport::from_env(env)
        .map_err(|_| js_denied())?
        .request::<serde_json::Value>(route, &raw, Some(cookie), Some(csrf))
        .await
        .map_err(|_| js_denied())?
        .body;
    view.check(env, request)?;
    if matches!(
        route,
        NativePrivateRoute::ParticipantSubmit
            | NativePrivateRoute::ParticipantLookup
            | NativePrivateRoute::ParticipantOwnerView
    ) {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct NativeWorldReply<T> {
            api_version: u8,
            ok: bool,
            world_response: T,
        }
        let reply: NativeWorldReply<T> = serde_json::from_value(body).map_err(|_| js_denied())?;
        if reply.api_version != 2 || !reply.ok {
            return Err(js_denied());
        }
        Ok(reply.world_response)
    } else {
        serde_json::from_value(body).map_err(|_| js_denied())
    }
}
pub(super) async fn response(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    let request: ParticipantProvisionRequestV1 = serde_json::from_str(raw).map_err(|_| denied())?;
    request.validate().map_err(|_| denied())?;
    let first = observe(env, &request, ParticipantStepV1::Policy)
        .await
        .map_err(|_| denied())?;
    let s = &first.view.descriptor.session;
    let snapshot = NativeManifestStore::new(state)
        .read_completed_history(&NativeHistoryLookup {
            environment: &s.scope.role_scope.context.identity.environment,
            account_id: &request.custody_account_id,
            provision_id: &request.provision_id,
            result_permit_id: &s.source_permit_id,
            phase: s.phase,
            native_operation_id: Some(&s.native_operation_id),
        })
        .map_err(|_| denied())?
        .ok_or_else(denied)?;
    let history = super::history::entry(snapshot.clone())?.ok_or_else(denied)?;
    if history.scope != s.scope
        || history.result_permit_id != s.source_permit_id
        || history.session_attestation.as_ref().is_none_or(|a| {
            a.private_sid_generation != s.private_sid_generation || a.session_ref != s.session_ref
        })
    {
        return Err(denied());
    }
    let refs = snapshot.session_refs.ok_or_else(denied)?;
    if refs.private_sid_generation != s.private_sid_generation {
        return Err(denied());
    }
    let fingerprint = first.fingerprint().map_err(|_| denied())?;
    let (cookie, csrf) = decrypt_pair(
        env,
        &request,
        &first,
        &refs.cookie,
        &refs.cookie_digest,
        &refs.csrf,
        &refs.csrf_digest,
    )
    .await
    .map_err(|_| denied())?;
    let cookie = PrivateCookie::from_private(
        PrivateCookieKind::ManagedSession,
        std::str::from_utf8(cookie.as_bytes())
            .map_err(|_| denied())?
            .into(),
    )?;
    let csrf = std::str::from_utf8(csrf.as_bytes()).map_err(|_| denied())?;
    let policy_view = observe(env, &request, ParticipantStepV1::Policy)
        .await
        .map_err(|_| denied())?;
    same(env, &request, &policy_view, &fingerprint).map_err(|_| denied())?;
    let status = session(env, &request, &policy_view, &cookie, csrf)
        .await
        .map_err(|_| denied())?;
    let policy: ParticipantPolicyOriginalV1 = forward(
        env,
        &request,
        &policy_view,
        NativePrivateRoute::ParticipantPolicyOriginal,
        &serde_json::json!({"observation_nonce":policy_view.view.descriptor.policy_nonce}),
        &cookie,
        csrf,
    )
    .await
    .map_err(|_| denied())?;
    let _: serde_json::Value = owner(
        env,
        &request,
        ParticipantRuntimeAction::RecordPolicy {
            original: policy,
            session: status,
        },
    )
    .await
    .map_err(|_| denied())?;
    let bundle: ParticipantSignBundleV1 = serde_json::from_str(
        &raw_text(
            call_signer(env, &serde_json::to_string(&request)?)
                .await
                .map_err(|_| denied())?,
            32 * 1024,
        )
        .map_err(|_| denied())?,
    )?;
    if bundle.owner.request != first.view.descriptor.register
        || bundle.read.request != first.view.descriptor.owner_read
        || bundle.owner.subject_public_key != first.view.descriptor.owner_key.public_key
        || bundle.read.subject_public_key != first.view.descriptor.owner_key.public_key
    {
        return Err(denied());
    }
    verify_owner_signature_strict(
        GAME_PARTICIPANT_REGISTER_DOMAIN,
        &bundle.owner.request,
        &bundle.owner.subject_public_key,
        &bundle.owner.signature_hex,
    )
    .map_err(|_| denied())?;
    verify_owner_signature_strict(
        ws::VIEW_JSON_PATH,
        &bundle.read.request,
        &bundle.read.subject_public_key,
        &bundle.read.signature_hex,
    )
    .map_err(|_| denied())?;
    let authorize = observe(env, &request, ParticipantStepV1::Authorize)
        .await
        .map_err(|_| denied())?;
    same(env, &request, &authorize, &fingerprint).map_err(|_| denied())?;
    let status = session(env, &request, &authorize, &cookie, csrf)
        .await
        .map_err(|_| denied())?;
    let admission: serde_json::Value = forward(
        env,
        &request,
        &authorize,
        NativePrivateRoute::ParticipantAuthorize,
        &bundle.owner,
        &cookie,
        csrf,
    )
    .await
    .map_err(|_| denied())?;
    let admission: SignedReadRequest<GameParticipantAdmissionV1> =
        serde_json::from_value(admission.get("admission").cloned().ok_or_else(denied)?)?;
    let _: serde_json::Value = owner(
        env,
        &request,
        ParticipantRuntimeAction::RecordAdmission {
            admission,
            session: status,
        },
    )
    .await
    .map_err(|_| denied())?;
    let submit = observe(env, &request, ParticipantStepV1::Submit)
        .await
        .map_err(|_| denied())?;
    same(env, &request, &submit, &fingerprint).map_err(|_| denied())?;
    let submit_status = session(env, &request, &submit, &cookie, csrf)
        .await
        .map_err(|_| denied())?;
    let submitted: JResult<SignedServiceResponse<ws::IntentResponse<serde_json::Value>>> = forward(
        env,
        &request,
        &submit,
        NativePrivateRoute::ParticipantSubmit,
        submit.view.submit.as_ref().ok_or_else(denied)?,
        &cookie,
        csrf,
    )
    .await;
    let owner_view = observe(env, &request, ParticipantStepV1::OwnerView)
        .await
        .map_err(|_| denied())?;
    same(env, &request, &owner_view, &fingerprint).map_err(|_| denied())?;
    let (result, response_source, lookup_session) = match submitted {
        Ok(result) if matches!(result.payload.outcome, ws::IntentOutcome::Committed { .. }) => {
            (result, ParticipantResultSourceV1::Submit, None)
        }
        _ => {
            wait_commit().await.map_err(|_| denied())?;
            same(env, &request, &owner_view, &fingerprint).map_err(|_| denied())?;
            let lookup_status = session_for_nonce(
                env,
                &request,
                &owner_view,
                owner_view
                    .view
                    .lookup_session_observation_nonce
                    .as_deref()
                    .ok_or_else(denied)?,
                &cookie,
                csrf,
            )
            .await
            .map_err(|_| denied())?;
            let looked: SignedServiceResponse<ws::IntentResponse<serde_json::Value>> = forward(
                env,
                &request,
                &owner_view,
                NativePrivateRoute::ParticipantLookup,
                owner_view.view.lookup.as_ref().ok_or_else(denied)?,
                &cookie,
                csrf,
            )
            .await
            .map_err(|_| denied())?;
            if !matches!(looked.payload.outcome, ws::IntentOutcome::Committed { .. }) {
                return Err(denied());
            }
            (
                looked,
                ParticipantResultSourceV1::Lookup,
                Some(lookup_status),
            )
        }
    };
    let status = session(env, &request, &owner_view, &cookie, csrf)
        .await
        .map_err(|_| denied())?;
    let owner_response: SignedServiceResponse<ParticipantOwnerControlResponseV1> = forward(
        env,
        &request,
        &owner_view,
        NativePrivateRoute::ParticipantOwnerView,
        owner_view.view.owner_view.as_ref().ok_or_else(denied)?,
        &cookie,
        csrf,
    )
    .await
    .map_err(|_| denied())?;
    let receipt: GameParticipantReceiptV1 = owner(
        env,
        &request,
        ParticipantRuntimeAction::RecordResult {
            result: Box::new(ParticipantFinalResultV1 {
                response: result,
                owner_response,
                submit_session: submit_status,
                response_source,
                lookup_session,
            }),
            session: status,
        },
    )
    .await
    .map_err(|_| denied())?;
    Response::from_json(&receipt)
}
pub(crate) async fn run(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > 4096 {
        return Err(denied());
    }
    let request: ParticipantProvisionRequestV1 = serde_json::from_str(raw).map_err(|_| denied())?;
    request.validate().map_err(|_| denied())?;
    let name = super::manifest_name_parts(env, &request.custody_account_id, &request.provision_id)?;
    let result: GameParticipantReceiptV1 = actor(env, &name, "/internal/participant", &request)
        .await
        .map_err(|_| denied())?;
    Ok(serde_json::to_string(&result)?)
}
