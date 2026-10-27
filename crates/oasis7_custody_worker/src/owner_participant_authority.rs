//! A participant permit uses actual target epoch 1 and the original signed source unchanged.
use super::*;
use crate::directory_config::DirectoryConfig;
use ed25519_dalek::VerifyingKey;
use oasis7_custody_core::approval::{ApprovedWorldV1, SignedApprovalVerificationV1};
use oasis7_custody_core::approval_admission::ApprovalAdmissionInput;

pub(super) fn enabled(env: &Env) -> Result<ApprovedWorldV1> {
    if env
        .var("CUSTODY_PARTICIPANT_PROVISION_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(denied());
    }
    let directory = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
    let raw: [u8; 32] = hex::decode(&directory.world.service_public_key)
        .map_err(|_| denied())?
        .try_into()
        .map_err(|_| denied())?;
    let key = VerifyingKey::from_bytes(&raw).map_err(|_| denied())?;
    if key.is_weak() || hex::encode(raw) != directory.world.service_public_key {
        return Err(denied());
    }
    Ok(directory.world)
}
pub(super) async fn observe(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    input: ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
    inspection_only: bool,
) -> Result<authority::Observations> {
    let world = enabled(env)?;
    if saved.state.draft().plan.world_identity != world.world_id {
        return Err(denied());
    }
    authority::observe(env, saved.state.draft(), input, signed, inspection_only).await
}
pub(super) fn current(
    state: &JsValue,
    env: &Env,
    observed: &authority::Observations,
    require_business: bool,
) -> std::result::Result<NativeStageCurrent, JsValue> {
    let world = enabled(env).map_err(|_| JsValue::from_str("participant authority unavailable"))?;
    let current = authority::current(state, env, observed, require_business)?;
    if current.provision.approval.authority.security_version.0 != 1
        || current.provision.world_identity != world
    {
        return Err(JsValue::from_str("participant authority unavailable"));
    }
    Ok(current)
}
