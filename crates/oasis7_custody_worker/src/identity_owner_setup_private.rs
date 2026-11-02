//! Setup's private session reader. It never creates a manifest or secret record.
use super::{Observation, check};
use crate::{
    envelope_crypto,
    identity_native_runtime::{CipherRead, actor, clock, js_denied, record_name},
    native_identity::provision_transport::{PrivateCookie, PrivateCookieKind},
    native_private_store::{
        NativePrivateConfig,
        manifest::{NativeHistoryLookup, NativeManifestStore},
    },
    owner_setup_rpc::OwnerSetupStageRpc,
};
use oasis7_custody_core::{
    approval::bytes_digest, envelope::KeyEnvelopeV1, provisioning::OwnerSetupReadPurposeV1,
};
use wasm_bindgen::JsValue;
use worker::Env;

pub(super) struct SessionSecrets {
    pub cookie: PrivateCookie,
    pub csrf: envelope_crypto::SecretBytes,
    resource_manifest: Option<(NativeManifestStore, oasis7_custody_core::DecimalU64)>,
}
impl SessionSecrets {
    pub(super) fn check_resource_generation(&self) -> Result<(), JsValue> {
        if let Some((manifest, expected)) = &self.resource_manifest {
            if manifest.current_private_session_generation()? != *expected {
                return Err(js_denied());
            }
        }
        Ok(())
    }
}

pub(super) async fn read(
    state: JsValue,
    env: &Env,
    selector: &OwnerSetupStageRpc,
    first: &Observation,
) -> Result<(Observation, SessionSecrets), JsValue> {
    check(env, selector, first)?;
    let session = &first.view.participant.session;
    let manifest = NativeManifestStore::new(state);
    let resource_profile = first
        .view
        .approved_plan_v2
        .as_ref()
        .is_some_and(|p| p.selection.resource().is_some());
    if resource_profile
        && manifest.current_private_session_generation()? != session.private_sid_generation
    {
        return Err(js_denied());
    }
    let snapshot = manifest
        .read_completed_history(&NativeHistoryLookup {
            environment: &session.scope.role_scope.context.identity.environment,
            account_id: &selector.request.custody_account_id,
            provision_id: &selector.request.provision_id,
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
        if resource_profile
            && manifest.current_private_session_generation()? != session.private_sid_generation
        {
            return Err(js_denied());
        }
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
        if resource_profile
            && manifest.current_private_session_generation()? != session.private_sid_generation
        {
            return Err(js_denied());
        }
        let envelope = envelope.ok_or_else(js_denied)?;
        if envelope.wrapping_key_version != config.wrapping_version {
            return Err(js_denied());
        }
        let secret = envelope_crypto::open(&envelope, &scope.context()?, &config.kek)
            .await
            .map_err(|_| js_denied())?;
        check(env, selector, first)?;
        if resource_profile
            && manifest.current_private_session_generation()? != session.private_sid_generation
        {
            return Err(js_denied());
        }
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
        if resource_profile
            && manifest.current_private_session_generation()? != session.private_sid_generation
        {
            return Err(js_denied());
        }
        if after.as_ref() != Some(&envelope) {
            return Err(js_denied());
        }
        secrets.push(secret);
    }
    let csrf = secrets.pop().ok_or_else(js_denied)?;
    let cookie = secrets.pop().ok_or_else(js_denied)?;
    // Secret use is forbidden until this second paid current observation and
    // independent Fence read complete after both decrypts.
    let second = super::observe(env, selector, OwnerSetupReadPurposeV1::Session).await?;
    if first.fingerprint()? != second.fingerprint()? {
        return Err(js_denied());
    }
    check(env, selector, &second)?;
    if resource_profile
        && manifest.current_private_session_generation()? != session.private_sid_generation
    {
        return Err(js_denied());
    }
    let cookie = PrivateCookie::from_private(
        PrivateCookieKind::ManagedSession,
        std::str::from_utf8(cookie.as_bytes())
            .map_err(|_| js_denied())?
            .into(),
    )
    .map_err(|_| js_denied())?;
    let _ = clock()?;
    let resource_manifest = resource_profile.then_some((manifest, session.private_sid_generation));
    Ok((
        second,
        SessionSecrets {
            cookie,
            csrf,
            resource_manifest,
        },
    ))
}
