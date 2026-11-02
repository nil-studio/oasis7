//! Identity-only exact completed manifest + SID/CSRF four-read kernel.
//! Each caller supplies its own authenticated Current and already-paid edge hook.
//! No service/Actor callback, private record relocation or key accessor is added.
use crate::{
    envelope_crypto,
    identity_native_runtime::{CipherRead, actor, clock, js_denied, record_name},
    native_identity::provision_transport::{PrivateCookie, PrivateCookieKind},
    native_private_store::{
        NativePrivateConfig, NativePrivateScope,
        manifest::{NativeHistoryLookup, NativeManifestStore},
    },
    owner_test_session_rpc::TestSessionSelectorV1,
};
use oasis7_custody_core::{
    approval::bytes_digest, envelope::KeyEnvelopeV1,
    native_execution_prepare_v2::ExecutionPreparePhysicalEdgeV2,
    provisioning::ParticipantSessionRefV1,
};
use wasm_bindgen::JsValue;
use worker::Env;
pub(super) struct SessionSecrets {
    pub cookie: PrivateCookie,
    pub csrf: envelope_crypto::SecretBytes,
}
pub(super) async fn read_with(
    state: JsValue,
    env: &Env,
    selector: &TestSessionSelectorV1,
    session: &ParticipantSessionRefV1,
    check: impl Fn() -> Result<(), JsValue>,
    mut before_cipher: impl FnMut(
        ExecutionPreparePhysicalEdgeV2,
        &NativePrivateScope,
    ) -> Result<(), JsValue>,
) -> Result<SessionSecrets, JsValue> {
    read_with_async(state, env, selector, session, check, |edge, scope| {
        std::future::ready(before_cipher(edge, &scope))
    })
    .await
}
pub(super) async fn read_with_async<Fut>(
    state: JsValue,
    env: &Env,
    selector: &TestSessionSelectorV1,
    session: &ParticipantSessionRefV1,
    check: impl Fn() -> Result<(), JsValue>,
    mut before_cipher: impl FnMut(ExecutionPreparePhysicalEdgeV2, NativePrivateScope) -> Fut,
) -> Result<SessionSecrets, JsValue>
where
    Fut: std::future::Future<Output = Result<(), JsValue>>,
{
    check()?;
    let snapshot = NativeManifestStore::new(state)
        .read_completed_history(&NativeHistoryLookup {
            environment: &session.scope.role_scope.context.identity.environment,
            account_id: &selector.custody_account_id,
            provision_id: &selector.provision_id,
            result_permit_id: &session.source_permit_id,
            phase: session.phase,
            native_operation_id: Some(&session.native_operation_id),
        })
        .map_err(|_| js_denied())?
        .ok_or_else(js_denied)?;
    let history = crate::identity_native_runtime::history::entry(snapshot.clone())
        .map_err(|_| js_denied())?
        .ok_or_else(js_denied)?;
    if history.scope != session.scope
        || history.result_permit_id != session.source_permit_id
        || history.session_attestation.as_ref().is_none_or(|v| {
            v.session_ref != session.session_ref
                || v.private_sid_generation != session.private_sid_generation
        })
    {
        return Err(js_denied());
    }
    let refs = snapshot.session_refs.ok_or_else(js_denied)?;
    if refs.private_sid_generation != session.private_sid_generation {
        return Err(js_denied());
    }
    let config = NativePrivateConfig::load(env)?;
    let mut secrets = Vec::with_capacity(2);
    for (scope, digest, before_edge, after_edge) in [
        (
            &refs.cookie,
            &refs.cookie_digest,
            ExecutionPreparePhysicalEdgeV2::SidCipherReadBefore,
            ExecutionPreparePhysicalEdgeV2::SidCipherReadAfter,
        ),
        (
            &refs.csrf,
            &refs.csrf_digest,
            ExecutionPreparePhysicalEdgeV2::CsrfCipherReadBefore,
            ExecutionPreparePhysicalEdgeV2::CsrfCipherReadAfter,
        ),
    ] {
        check()?;
        if scope.role_scope != session.scope.role_scope
            || scope.identity_namespace != session.scope.identity_namespace
            || scope.identity_instance_epoch != session.scope.identity_instance_epoch
        {
            return Err(js_denied());
        }
        before_cipher(before_edge, scope.clone()).await?;
        check()?;
        let envelope: Option<KeyEnvelopeV1> = actor(
            env,
            &record_name(scope)?,
            "/internal/cipher-read",
            &CipherRead {
                scope: scope.clone(),
            },
        )
        .await?;
        check()?;
        let envelope = envelope.ok_or_else(js_denied)?;
        if envelope.wrapping_key_version != config.wrapping_version {
            return Err(js_denied());
        }
        let secret = envelope_crypto::open(&envelope, &scope.context()?, &config.kek)
            .await
            .map_err(|_| js_denied())?;
        check()?;
        if bytes_digest(secret.as_bytes()) != *digest || secret.as_bytes().len() > 12 * 1024 {
            return Err(js_denied());
        }
        before_cipher(after_edge, scope.clone()).await?;
        check()?;
        let after: Option<KeyEnvelopeV1> = actor(
            env,
            &record_name(scope)?,
            "/internal/cipher-read",
            &CipherRead {
                scope: scope.clone(),
            },
        )
        .await?;
        check()?;
        if after.as_ref() != Some(&envelope) {
            return Err(js_denied());
        }
        secrets.push(secret);
    }
    let csrf = secrets.pop().ok_or_else(js_denied)?;
    let cookie = secrets.pop().ok_or_else(js_denied)?;
    // The coordinator must obtain paid observation 2 before using either secret.
    check()?;
    let cookie = PrivateCookie::from_private(
        PrivateCookieKind::ManagedSession,
        std::str::from_utf8(cookie.as_bytes())
            .map_err(|_| js_denied())?
            .into(),
    )
    .map_err(|_| js_denied())?;
    let _ = clock()?;
    Ok(SessionSecrets { cookie, csrf })
}
