//! Fixed new funded version of the existing private metadata service. The
//! ordinary attest method/DTO remains unchanged and cannot satisfy this reply.
use super::*;
use crate::{
    execution_context_stage_store_v2 as stage,
    owner_test_session_rpc::TestSessionAttestationRequest,
};
use oasis7_custody_core::{
    execution_private_current_v2::*, native_cleanup_v2::AccountMachinePurposeV2,
    provisioning::ExecutionPrivateCurrentMetadataV2,
};
use worker::{Env, Response, Result};
fn verify(env: &Env, p: &SignedAccountExecutionPrivateCurrentV2) -> JResult<()> {
    let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
        env,
        AccountMachinePurposeV2::ExecutionPrivateCurrentV2,
    )?;
    p.verify_issuer(&pin, &floor, clock()?)
        .map_err(|_| js_denied())?;
    let c = &p.certificate;
    if c.session.scope.role_scope.context.identity.environment
        != env
            .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
            .map_err(|_| js_denied())?
            .to_string()
    {
        return Err(js_denied());
    }
    Ok(())
}
pub(super) async fn response(state: JsValue, env: &Env, raw: &str) -> Result<Response> {
    if raw.len() > EXECUTION_PRIVATE_CURRENT_MAX_WIRE_V2 {
        return Err(denied());
    }
    let proof: SignedAccountExecutionPrivateCurrentV2 =
        serde_json::from_str(raw).map_err(|_| denied())?;
    verify(env, &proof).map_err(|_| denied())?;
    let role = &proof.certificate.session.scope.role_scope;
    let name = manifest_name_parts(env, &role.context.identity.account_id, &role.provision_id)?;
    stage::require_manifest_recipient(&state, env, &name).map_err(|_| denied())?;
    let claim = stage::claim_private_read(&state, &proof).map_err(|_| denied())?;
    // The claim transaction committed before the actual legacy metadata
    // kernel. Failure/expiry here does not release either physical once row.
    verify(env, &proof).map_err(|_| denied())?;
    let c = &proof.certificate;
    let actual = super::test_session_attestation::metadata(
        state.clone(),
        env,
        &TestSessionAttestationRequest {
            session: c.session.clone(),
            lifetime: c.lifetime.clone(),
        },
    )?;
    verify(env, &proof).map_err(|_| denied())?;
    let reply = ExecutionPrivateCurrentReplyV2 {
        api_version: 2,
        proof_digest: proof.digest().map_err(|_| denied())?,
        metadata: ExecutionPrivateCurrentMetadataV2 {
            session_ref: actual.session_ref,
            private_sid_generation: actual.private_sid_generation,
            lifetime_digest: actual.lifetime_digest,
        },
    };
    stage::finish_private_read(&state, claim, &reply).map_err(|_| denied())?;
    Response::from_json(&reply)
}
pub(crate) async fn read(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > EXECUTION_PRIVATE_CURRENT_MAX_WIRE_V2 {
        return Err(denied());
    }
    let proof: SignedAccountExecutionPrivateCurrentV2 =
        serde_json::from_str(raw).map_err(|_| denied())?;
    verify(env, &proof).map_err(|_| denied())?;
    let role = &proof.certificate.session.scope.role_scope;
    let name = manifest_name_parts(env, &role.context.identity.account_id, &role.provision_id)?;
    let reply: ExecutionPrivateCurrentReplyV2 =
        actor(env, &name, EXECUTION_PRIVATE_CURRENT_ACTOR_PATH_V2, &proof)
            .await
            .map_err(|_| denied())?;
    if reply.api_version != 2 || reply.proof_digest != proof.digest().map_err(|_| denied())? {
        return Err(denied());
    }
    serde_json::to_string(&reply).map_err(|_| denied())
}
