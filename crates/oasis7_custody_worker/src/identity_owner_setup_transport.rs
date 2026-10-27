//! Fixed managed-session transport. Canonical facts can be archived before live revalidation.
use super::private::SessionSecrets;
use super::{Observation, check};
use crate::{
    identity_native_runtime::js_denied,
    native_identity::provision_transport::{NativePrivateRoute, NativeProvisionTransport},
    owner_setup_rpc::OwnerSetupStageRpc,
};
use oasis7_custody_core::{
    native_binding::SignedManagedNativeSessionStatusV1, provisioning::ParticipantSessionEvidenceV1,
};
use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::JsValue;
use worker::Env;

pub(super) async fn session(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    observation: &Observation,
    secrets: &SessionSecrets,
    nonce: &str,
) -> Result<ParticipantSessionEvidenceV1, JsValue> {
    check(env, selector, observation)?;
    let v = &observation.view;
    if nonce != v.preflight_session_observation_nonce
        && nonce != v.session_observation_nonce
        && v.lookup_session_observation_nonce.as_deref() != Some(nonce)
        && nonce != v.access_nonces.policy_session
        && nonce != v.access_nonces.authorize_session
    {
        return Err(js_denied());
    }
    let transport = NativeProvisionTransport::from_env(env).map_err(|_| js_denied())?;
    let status = transport
        .request::<SignedManagedNativeSessionStatusV1>(
            NativePrivateRoute::SessionStatus,
            &serde_json::to_string(&serde_json::json!({"nonce":nonce})).map_err(|_| js_denied())?,
            Some(&secrets.cookie),
            Some(std::str::from_utf8(secrets.csrf.as_bytes()).map_err(|_| js_denied())?),
        )
        .await
        .map_err(|_| js_denied())?
        .body;
    transport
        .fact_pin()
        .session_status(
            &status,
            &v.participant.native_binding,
            nonce,
            v.participant.session.native_binding_generation,
            v.service_epoch,
        )
        .await?;
    check(env, selector, observation)?;
    if status.status.session_ref != v.participant.session.session_ref {
        return Err(js_denied());
    }
    Ok(ParticipantSessionEvidenceV1 {
        status,
        private_sid_generation: v.participant.session.private_sid_generation,
    })
}

/// Deliberately does not check Current after the native await. The caller must
/// archive a canonical response first, then revalidate before any live callback.
pub(super) async fn original<T: DeserializeOwned + Serialize, Q: Serialize>(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    observation: &Observation,
    secrets: &SessionSecrets,
    route: NativePrivateRoute,
    request: &Q,
) -> Result<T, JsValue> {
    check(env, selector, observation)?;
    if !matches!(
        route,
        NativePrivateRoute::OwnerControl
            | NativePrivateRoute::ParticipantOwnerView
            | NativePrivateRoute::ParticipantPolicyOriginal
            | NativePrivateRoute::ParticipantAuthorize
    ) {
        return Err(js_denied());
    }
    let raw = zeroize::Zeroizing::new(serde_json::to_string(request).map_err(|_| js_denied())?);
    let value = NativeProvisionTransport::from_env(env)
        .map_err(|_| js_denied())?
        .request::<serde_json::Value>(
            route,
            &raw,
            Some(&secrets.cookie),
            Some(std::str::from_utf8(secrets.csrf.as_bytes()).map_err(|_| js_denied())?),
        )
        .await
        .map_err(|_| js_denied())?
        .body;
    if matches!(
        route,
        NativePrivateRoute::OwnerControl | NativePrivateRoute::ParticipantOwnerView
    ) {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Reply {
            api_version: u8,
            ok: bool,
            world_response: serde_json::Value,
        }
        let reply: Reply = serde_json::from_value(value).map_err(|_| js_denied())?;
        if reply.api_version != 2 || !reply.ok {
            return Err(js_denied());
        }
        exact(reply.world_response)
    } else {
        exact(value)
    }
}

fn exact<T: DeserializeOwned + Serialize>(value: serde_json::Value) -> Result<T, JsValue> {
    let typed: T = serde_json::from_value(value.clone()).map_err(|_| js_denied())?;
    if serde_json::to_value(&typed).map_err(|_| js_denied())? != value {
        return Err(js_denied());
    }
    Ok(typed)
}
