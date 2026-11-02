//! Sixth independent Resource observation, never old FinalObservation DTO.
use super::*;
use oasis7_client_api::world_service::resource_basis as resource;
use oasis7_custody_core::provisioning::{
    CompleteOwnerSetupResourceFinalV2, OwnerSetupResourceFinalHttpV2,
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectReply {
    api_version: u8,
    body_json: String,
    proof_header: String,
}
pub(super) async fn finish(
    env: &Env,
    selector: &OwnerSetupStageRpc,
    prior: &Observation,
    secrets: &private::SessionSecrets,
    raw: &str,
) -> JResult<serde_json::Value> {
    let published: OwnerSetupResourcePublishedV2 =
        OwnerSetupResourcePublishedV2::from_exact_json(raw).map_err(|_| js_denied())?;
    let v = &prior.view;
    resource::verify_resource_goal_request(&published.candidate.original)
        .map_err(|_| js_denied())?;
    if published.permit.kind != OwnerSetupPermitKindV1::Preflight
        || published.permit.plan != v.plan
        || published.permit.approved_plan_v2 != v.approved_plan_v2
        || published.permit.participant != v.participant
        || published.permit.stage != selector.stage
        || published.permit.setup_id != v.setup_id
        || published.permit.source_revision != v.source_revision
        || published.permit.authority_revision != v.authority_revision
        || published.permit.expires_at.0 > v.expires_at.0
        || published.permit.observation.as_ref() != Some(&published.candidate.observation)
    {
        return Err(js_denied());
    }
    let source_record_digest = published.resource_source_record_digest;
    let candidate = published.candidate;
    let preflight = returned(env, selector, published.permit).await?;
    secrets.check_resource_generation()?;
    let session = transport::session(
        env,
        selector,
        &preflight,
        secrets,
        &preflight.view.preflight_session_observation_nonce,
    )
    .await?;
    let world: SignedServiceResponse<ws::participant_discovery::OwnerSetupObservationV1> =
        transport::original(
            env,
            selector,
            &preflight,
            secrets,
            NativePrivateRoute::ParticipantOwnerView,
            &candidate.observation,
        )
        .await?;
    secrets.check_resource_generation()?;
    check(env, selector, &preflight)?;
    let effect: EffectReply = rpc(
        env,
        selector,
        OwnerSetupRuntimeAction::Admit {
            evidence: Box::new(OwnerSetupAdmissionEvidenceV1 {
                world: OwnerSetupWorldEvidenceV1 {
                    request: candidate.observation.clone(),
                    response: world,
                },
                session,
            }),
        },
    )
    .await?;
    secrets.check_resource_generation()?;
    check(env, selector, &preflight)?;
    if effect.api_version != 2 {
        return Err(js_denied());
    }
    let frame = OwnerSetupResourceFinalHttpV2::decode_http(
        effect.body_json.as_bytes(),
        &effect.proof_header,
    )
    .map_err(|_| js_denied())?;
    let ws::entry_control::EntryOwnerOperationV1::ReadResourceGoal(original) =
        &frame.request().operation
    else {
        return Err(js_denied());
    };
    if frame.paid().payload.source_record_digest != source_record_digest
        || original != &candidate.original
        || frame.request().access != candidate.observation
    {
        return Err(js_denied());
    }
    let session = transport::session(
        env,
        selector,
        &preflight,
        secrets,
        &preflight.view.session_observation_nonce,
    )
    .await?;
    let response = resource_transport_v2::send(
        env,
        selector,
        &preflight,
        secrets,
        &frame,
        &source_record_digest,
    )
    .await?;
    resource::verify_resource_goal_response(
        &candidate.original,
        &response,
        &preflight.view.plan.world_identity.service_public_key,
    )
    .map_err(|_| js_denied())?;
    secrets.check_resource_generation()?;
    check(env, selector, &preflight)?;
    let progress = rpc(
        env,
        selector,
        OwnerSetupRuntimeAction::RecordResourceResult {
            completed: Box::new(CompleteOwnerSetupResourceFinalV2 { response, session }),
        },
    )
    .await?;
    secrets.check_resource_generation()?;
    Ok(progress)
}
