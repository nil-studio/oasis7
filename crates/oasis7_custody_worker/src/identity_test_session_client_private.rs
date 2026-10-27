//! Setup's private session reader. It never creates a manifest or secret record.
use super::{check, observation::Observation};
use crate::{
    envelope_crypto,
    identity_native_runtime::{CipherRead, actor, clock, js_denied, record_name},
    native_identity::provision_transport::{PrivateCookie, PrivateCookieKind},
    native_private_store::{
        NativePrivateConfig,
        manifest::{NativeHistoryLookup, NativeManifestStore},
    },
    owner_test_session_rpc::TestSessionSelectorV1,
};
use oasis7_custody_core::{approval::bytes_digest, envelope::KeyEnvelopeV1};
use wasm_bindgen::JsValue;
use worker::Env;

pub(super) struct SessionSecrets {
    pub cookie: PrivateCookie,
    pub csrf: envelope_crypto::SecretBytes,
}

pub(super) async fn read(
    state: JsValue,
    env: &Env,
    selector: &TestSessionSelectorV1,
    first: &Observation,
) -> Result<SessionSecrets, JsValue> {
    check(env, selector, first)?;
    let session = &first.descriptor.session;
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
    for (scope, digest) in [
        (&refs.cookie, &refs.cookie_digest),
        (&refs.csrf, &refs.csrf_digest),
    ] {
        check(env, selector, first)?;
        if scope.role_scope != session.scope.role_scope
            || scope.identity_namespace != session.scope.identity_namespace
            || scope.identity_instance_epoch != session.scope.identity_instance_epoch
        {
            return Err(js_denied());
        }
        let envelope: Option<KeyEnvelopeV1> = actor(
            env,
            &record_name(scope)?,
            "/internal/cipher-read",
            &CipherRead {
                scope: scope.clone(),
            },
        )
        .await?;
        check(env, selector, first)?;
        let envelope = envelope.ok_or_else(js_denied)?;
        if envelope.wrapping_key_version != config.wrapping_version {
            return Err(js_denied());
        }
        let secret = envelope_crypto::open(&envelope, &scope.context()?, &config.kek)
            .await
            .map_err(|_| js_denied())?;
        check(env, selector, first)?;
        if bytes_digest(secret.as_bytes()) != *digest || secret.as_bytes().len() > 12 * 1024 {
            return Err(js_denied());
        }
        let after: Option<KeyEnvelopeV1> = actor(
            env,
            &record_name(scope)?,
            "/internal/cipher-read",
            &CipherRead {
                scope: scope.clone(),
            },
        )
        .await?;
        check(env, selector, first)?;
        if after.as_ref() != Some(&envelope) {
            return Err(js_denied());
        }
        secrets.push(secret);
    }
    let csrf = secrets.pop().ok_or_else(js_denied)?;
    let cookie = secrets.pop().ok_or_else(js_denied)?;
    // The coordinator must obtain paid observation 2 before using either secret.
    check(env, selector, first)?;
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
