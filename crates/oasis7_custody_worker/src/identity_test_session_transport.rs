//! Fixed managed-session transport. Canonical facts can be archived before live revalidation.
use super::private::SessionSecrets;
use super::{JResult, Observation, check};
use crate::{
    identity_native_runtime::js_denied,
    native_identity::provision_transport::{NativePrivateRoute, NativeProvisionTransport},
    owner_test_session_rpc::TestSessionSelectorV1,
};
use oasis7_custody_core::{
    native_binding::SignedManagedNativeSessionStatusV1, provisioning::ParticipantSessionEvidenceV1,
};
use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::JsValue;
use worker::Env;

pub(super) async fn session(
    env: &Env,
    selector: &TestSessionSelectorV1,
    observation: &Observation,
    secrets: &SessionSecrets,
    nonce: &str,
) -> Result<ParticipantSessionEvidenceV1, JsValue> {
    check(env, selector, observation)?;
    let v = &observation.descriptor;
    if nonce != v.access_nonces.policy_session && nonce != v.access_nonces.authorize_session {
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
            &v.native_binding,
            nonce,
            v.session.native_binding_generation,
            observation.service_epoch,
        )
        .await?;
    check(env, selector, observation)?;
    if status.status.session_ref != v.session.session_ref {
        return Err(js_denied());
    }
    Ok(ParticipantSessionEvidenceV1 {
        status,
        private_sid_generation: v.session.private_sid_generation,
    })
}

/// Deliberately does not check Current after the native await. The caller must
/// archive a canonical response first, then revalidate before any live callback.
pub(super) async fn original<T: DeserializeOwned + Serialize, Q: Serialize>(
    env: &Env,
    selector: &TestSessionSelectorV1,
    observation: &Observation,
    secrets: &SessionSecrets,
    route: NativePrivateRoute,
    request: &Q,
) -> Result<T, JsValue> {
    check(env, selector, observation)?;
    if !matches!(
        route,
        NativePrivateRoute::ParticipantOwnerView
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
    if matches!(route, NativePrivateRoute::ParticipantOwnerView) {
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

pub(super) async fn lifetime(
    env: &Env,
    selector: &TestSessionSelectorV1,
    o: &Observation,
    secrets: &SessionSecrets,
) -> Result<oasis7_custody_core::native_binding::SignedManagedNativeSessionLifetimeV1, JsValue> {
    check(env, selector, o)?;
    let d = &o.descriptor;
    let transport = NativeProvisionTransport::from_env(env).map_err(|_| js_denied())?;
    let signed = transport
        .request(
            NativePrivateRoute::SessionLifetime,
            &serde_json::to_string(&serde_json::json!({"nonce":d.lifetime_nonce}))
                .map_err(|_| js_denied())?,
            Some(&secrets.cookie),
            Some(std::str::from_utf8(secrets.csrf.as_bytes()).map_err(|_| js_denied())?),
        )
        .await
        .map_err(|_| js_denied())?
        .body;
    transport
        .fact_pin()
        .session_lifetime(
            &signed,
            &d.native_binding,
            &d.lifetime_nonce,
            d.session.native_binding_generation,
            o.service_epoch,
        )
        .await?;
    check(env, selector, o)?;
    if signed.lifetime.status.session_ref != d.session.session_ref {
        return Err(js_denied());
    }
    Ok(signed)
}

pub(super) async fn world(
    env: &Env,
    selector: &TestSessionSelectorV1,
    o: &Observation,
    secrets: &SessionSecrets,
    request: &ws::participant_discovery::ParticipantOwnerViewRequestV1,
) -> Result<SignedServiceResponse<ws::participant_discovery::OwnerSetupObservationV1>, JsValue> {
    let value = original(
        env,
        selector,
        o,
        secrets,
        NativePrivateRoute::ParticipantOwnerView,
        request,
    )
    .await?;
    check(env, selector, o)?;
    Ok(value)
}

use oasis7_client_api::world_service::{self as ws, SignedServiceResponse};
use oasis7_custody_core::provisioning::OwnerSetupAccessEvidenceV1;
pub(super) async fn access(
    env: &Env,
    selector: &TestSessionSelectorV1,
    view: &Observation,
    secrets: &SessionSecrets,
) -> JResult<OwnerSetupAccessEvidenceV1> {
    let nonces = &view.descriptor.access_nonces;
    // The two distinct Root observations are independent. Complete both with the
    // unchanged checks before policy, and keep policy before authorize's mutation.
    let (policy_session, authorize_session) = access_statuses(
        session(env, selector, view, secrets, &nonces.policy_session),
        session(env, selector, view, secrets, &nonces.authorize_session),
    )
    .await;
    let policy_session = policy_session?;
    let authorize_session = authorize_session?;
    check(env, selector, view)?;
    let policy = original(
        env,
        selector,
        view,
        secrets,
        NativePrivateRoute::ParticipantPolicyOriginal,
        &serde_json::json!({"observation_nonce":nonces.policy}),
    )
    .await?;
    check(env, selector, view)?;
    #[derive(serde::Deserialize, serde::Serialize)]
    #[serde(deny_unknown_fields)]
    struct AdmissionReply {
        api_version: u8,
        ok: bool,
        phase: String,
        state: String,
        new_registration_enabled: bool,
        admission: ws::SignedReadRequest<oasis7_agent_api::game::GameParticipantAdmissionV1>,
        next_action: String,
    }
    let reply: AdmissionReply = original(
        env,
        selector,
        view,
        secrets,
        NativePrivateRoute::ParticipantAuthorize,
        &view.descriptor.participant_owner,
    )
    .await?;
    check(env, selector, view)?;
    // The native authorizer has a separate response shape, never a World receipt.
    if reply.api_version != 2
        || !reply.ok
        || reply.phase != "participant_authorized"
        || reply.state != "pending"
        || reply.next_action != "query_original_participant_request"
    {
        return Err(js_denied());
    }
    // Closed registration still permits existing-binding access; the signed
    // policy/admission and independent current OwnerControl decide this lane.
    let _ = reply.new_registration_enabled;
    let admission = reply.admission;
    Ok(OwnerSetupAccessEvidenceV1 {
        policy,
        policy_session,
        admission,
        authorize_session,
    })
}

// No spawned task or dependency: each completed read is retained, never repolled.
async fn access_statuses<A, B>(a: A, b: B) -> (A::Output, B::Output)
where
    A: std::future::Future,
    B: std::future::Future,
{
    use std::{future::poll_fn, task::Poll};
    let (mut a, mut b) = (std::pin::pin!(a), std::pin::pin!(b));
    let (mut first, mut second) = (None, None);
    poll_fn(|cx| {
        if first.is_none() {
            if let Poll::Ready(value) = a.as_mut().poll(cx) {
                first = Some(value);
            }
        }
        if second.is_none() {
            if let Poll::Ready(value) = b.as_mut().poll(cx) {
                second = Some(value);
            }
        }
        if first.is_some() && second.is_some() {
            Poll::Ready((
                first.take().expect("completed policy status"),
                second.take().expect("completed authorize status"),
            ))
        } else {
            Poll::Pending
        }
    })
    .await
}
