//! Only the existing Identity→Account Admit RPC may obtain this private effect frame.
use super::*;
#[derive(serde::Serialize)]
#[serde(deny_unknown_fields)]
struct ResourceEffectReplyV2 {
    api_version: u8,
    body_json: String,
    proof_header: String,
}
pub(super) async fn admit(
    state: &JsValue,
    env: &Env,
    store: &OwnerProvisionApprovalStore,
    id: &str,
    evidence: OwnerSetupAdmissionEvidenceV1,
) -> Result<String> {
    let saved = store.snapshot().map_err(|_| denied())?;
    let plan = saved
        .state
        .owner_setup_v2_plan()
        .filter(|p| p.setup_id == id && p.selection.resource().is_some())
        .ok_or_else(denied)?;
    let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)?;
    let pin = issuer.owner_resource_final_pin(&plan.world_identity)?;
    let generation: oasis7_custody_core::DecimalU64 = serde_json::from_value(
        env.var("CUSTODY_OWNER_RESOURCE_FINAL_FEATURE_GENERATION")
            .map_err(|_| denied())?
            .to_string()
            .into(),
    )
    .map_err(|_| denied())?;
    if generation.0 == 0 {
        return Err(denied());
    }
    let issue = ResourceFinalIssueContextV2 {
        issuer: &pin,
        resource_feature_generation: generation,
    };
    let observed = observe(
        state,
        env,
        id,
        Some(OwnerSetupStageV1::FinalObservation),
        false,
    )
    .await?;
    let prepared = store
        .admit_and_claim_owner_resource_send_v2(id, evidence, &issue, || {
            setup_authority::current(state, env, &observed, true)
        })
        .await
        .map_err(|_| denied())?;
    // SQL already owns the paid attempt. Any signing failure remains consumed;
    // no new attempt, success cache, HTTP retry or Native callback is issued.
    let frame = issuer.sign_owner_resource_final(prepared, crate::approval_store::now()?)?;
    let body_json =
        String::from_utf8(frame.body_bytes().map_err(|_| denied())?).map_err(|_| denied())?;
    let reply = ResourceEffectReplyV2 {
        api_version: 2,
        body_json,
        proof_header: frame.proof_header().map_err(|_| denied())?,
    };
    let bytes = serde_json::to_string(&reply)?;
    if bytes.len() > 128 * 1024 {
        return Err(denied());
    }
    Ok(bytes)
}
