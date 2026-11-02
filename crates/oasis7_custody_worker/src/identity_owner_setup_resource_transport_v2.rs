//! Fixed new Resource sender; no ordinary-route fallback, retry or selected URL.
use super::{Observation, check, private::SessionSecrets};
use crate::{
    identity_native_runtime::js_denied,
    native_identity::provision_transport::NativeProvisionTransport,
    owner_setup_rpc::OwnerSetupStageRpc,
};
use oasis7_custody_core::provisioning::{
    OwnerSetupPaidResourceResultV2, OwnerSetupResourceFinalHttpV2,
};
use wasm_bindgen::JsValue;
use worker::Env;
pub(super) async fn send(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    observed: &Observation,
    secrets: &SessionSecrets,
    frame: &OwnerSetupResourceFinalHttpV2,
    source_record_digest: &str,
) -> Result<OwnerSetupPaidResourceResultV2, JsValue> {
    check(env, selector, observed)?;
    secrets.check_resource_generation()?;
    let now = crate::identity_native_runtime::clock()?;
    if now < frame.paid().payload.issued_at.0 || now >= frame.paid().payload.expires_at.0 {
        return Err(js_denied());
    }
    let v = &observed.view;
    let approved = v.approved_plan_v2.as_ref().ok_or_else(js_denied)?;
    if approved.selection.resource().is_none()
        || frame.participant() != &v.participant
        || frame.paid().payload.setup_id != selector.request.setup_id
        || frame.paid().payload.provision_id != selector.request.provision_id
        || frame.paid().payload.plan_digest != approved.digest().map_err(|_| js_denied())?
        || frame.paid().payload.source_record_digest != source_record_digest
    {
        return Err(js_denied());
    }
    let transport = NativeProvisionTransport::from_env(env).map_err(|_| js_denied())?;
    let reply = transport
        .request_owner_resource_final_v2::<OwnerSetupPaidResourceResultV2>(
            frame,
            &secrets.cookie,
            std::str::from_utf8(secrets.csrf.as_bytes()).map_err(|_| js_denied())?,
        )
        .await
        .map_err(|_| js_denied())?;
    secrets.check_resource_generation()?;
    check(env, selector, observed)?;
    if reply.http_status != 200 || reply.cookie.is_some() {
        return Err(js_denied());
    }
    let body = reply.body;
    body.verify_frame(frame, &v.plan.world_identity.service_public_key)
        .map_err(|_| js_denied())?;
    // Account independently replays actual issuer/dispatch/Book correlations and
    // both retained Node originals; Native online acceptance is not cold proof.
    Ok(body)
}
