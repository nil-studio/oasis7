use super::*;
use crate::provisioning::{OwnerSetupOriginalV1, OwnerSetupResultV1, OwnerSetupWorldEvidenceV1};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{
    self as ws, participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
    private_goal::verify_owner_signature_strict,
};
pub(crate) enum OwnerSetupSignatureFact<'a> {
    World(&'a OwnerSetupWorldEvidenceV1, &'a str),
    Original(&'a OwnerSetupOriginalV1, &'a str),
    GoalRead(
        &'a ws::SignedReadRequest<ws::private_goal_read::CurrentGoalRequest>,
        &'a str,
    ),
    Read(
        &'a ws::participant_discovery::ParticipantOwnerViewRequestV1,
        &'a str,
    ),
    Result(&'a OwnerSetupResultV1, &'a str),
}
fn job<T: Serialize>(
    key: &str,
    signature: &str,
    domain: &str,
    payload: &T,
) -> Result<SignatureCheck> {
    verify_owner_signature_strict(domain, payload, key, signature)
        .map_err(|_| ApprovalAdmissionError::Invalid)?;
    let bytes: Result<Vec<u8>> = signature
        .as_bytes()
        .chunks_exact(2)
        .map(|v| {
            std::str::from_utf8(v)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or(ApprovalAdmissionError::Invalid)
        })
        .collect();
    Ok(SignatureCheck {
        key_id: format!("owner-setup:{domain}"),
        public_key_hex: key.into(),
        signature_base64url: URL_SAFE_NO_PAD.encode(bytes?),
        canonical_bytes: ws::signing_bytes(domain, payload)
            .map_err(|_| ApprovalAdmissionError::Invalid)?,
    })
}
pub(crate) fn append_owner_setup_checks(
    mut checks: ExactSignatureChecks,
    fact: OwnerSetupSignatureFact<'_>,
) -> Result<ExactSignatureChecks> {
    checks.checks.extend(owner_setup_jobs(fact)?);
    checks.body_digest = canonical_digest(
        "oasis7.custody.owner-setup-signature-jobs.v1",
        &(&checks.body_digest, &checks.checks),
    )?;
    Ok(checks)
}
pub(crate) fn validate_owner_setup_signature_fact(fact: OwnerSetupSignatureFact<'_>) -> Result<()> {
    owner_setup_jobs(fact)?;
    Ok(())
}
fn owner_setup_jobs(fact: OwnerSetupSignatureFact<'_>) -> Result<Vec<SignatureCheck>> {
    let jobs=match fact {
        OwnerSetupSignatureFact::World(v,key)=>vec![job(key,&v.response.signature_hex,PARTICIPANT_OWNER_VIEW_PATH,&(&v.response.request_digest,&v.response.payload))?,job(&v.request.read.subject_public_key,&v.request.read.signature_hex,ws::VIEW_JSON_PATH,&v.request.read.request)?],
        OwnerSetupSignatureFact::GoalRead(v,key)=>vec![job(key,&v.signature_hex,ws::private_goal_read::GOAL_CURRENT_PATH,&v.request)?],
        OwnerSetupSignatureFact::Read(v,key)=>vec![job(key,&v.read.signature_hex,ws::VIEW_JSON_PATH,&v.read.request)?],
        OwnerSetupSignatureFact::Original(v,key)=>match v {
            OwnerSetupOriginalV1::Canonical {submit,..}=>vec![match &submit.signed_payload {
                oasis7_client_api::world_service_wire::WorldServicePayloadV1::GameTaskCreate(s)=>job(key,&s.signature_hex,GAME_TASK_CREATE_DOMAIN,&s.request)?,
                oasis7_client_api::world_service_wire::WorldServicePayloadV1::GameGoalPublish(s)=>job(key,&s.signature_hex,GAME_GOAL_PUBLISH_DOMAIN,&s.request)?,
                oasis7_client_api::world_service_wire::WorldServicePayloadV1::GameExecutorControl(s)=>job(key,&s.signature_hex,GAME_EXECUTOR_CONTROL_DOMAIN,&s.request)?,
                oasis7_client_api::world_service_wire::WorldServicePayloadV1::Delegation(s)=>job(key,&s.signature_hex,"delegation",&s.request)?,
                _=>return Err(ApprovalAdmissionError::Invalid),
            }],
            OwnerSetupOriginalV1::PrepareGoal(s)=>vec![job(key,&s.signature_hex,GAME_GOAL_PREPARE_DOMAIN,&s.request)?],
            OwnerSetupOriginalV1::FinalObservation {owner,goal}=>vec![job(key,&owner.read.signature_hex,ws::VIEW_JSON_PATH,&owner.read.request)?,job(key,&goal.signature_hex,ws::private_goal_read::GOAL_CURRENT_PATH,&goal.request)?],
        },
        OwnerSetupSignatureFact::Result(v,key)=>match v {
            OwnerSetupResultV1::Canonical {response,from_lookup}=>vec![job(key,&response.signature_hex,if *from_lookup{ws::LOOKUP_PATH}else{ws::SUBMIT_PATH},&(&response.request_digest,&response.payload))?],
            OwnerSetupResultV1::PreparedGoal(v)=>vec![job(key,&v.signature_hex,ws::private_goal_read::GOAL_PREPARE_PATH,&(&v.request_digest,&v.payload))?],
            OwnerSetupResultV1::FinalObservation {owner,goal}=>vec![job(key,&owner.signature_hex,PARTICIPANT_OWNER_VIEW_PATH,&(&owner.request_digest,&owner.payload))?,job(key,&goal.signature_hex,ws::private_goal_read::GOAL_CURRENT_PATH,&(&goal.request_digest,&goal.payload))?],
        }
    };
    Ok(jobs)
}

pub(crate) fn owner_setup_archive_signature_checks(
    id: &str,
    stage: crate::provisioning::OwnerSetupStageV1,
    fact: &crate::provisioning::OwnerSetupCanonicalFactV1,
    key: &str,
    original: &OwnerSetupOriginalV1,
    plan_digest: &str,
) -> Result<ExactSignatureChecks> {
    let check = job(
        key,
        &fact.response.signature_hex,
        fact.source.response_domain(),
        &(&fact.response.request_digest, &fact.response.payload),
    )?;
    let scope = (id, stage, fact.source, original, plan_digest);
    Ok(ExactSignatureChecks {
        operation_id: id.into(),
        body_digest: canonical_digest(
            "oasis7.custody.owner-setup-archive-fact.v1",
            &(&scope, fact),
        )?,
        context_digest: canonical_digest(
            "oasis7.custody.owner-setup-archive-pin.v1",
            &(key, &scope),
        )?,
        verify_nonce: canonical_digest(
            "oasis7.custody.owner-setup-archive-source.v1",
            &(id, stage, fact.source),
        )?,
        verification_digest: Some(canonical_digest(
            "oasis7.custody.owner-setup-archive-observation.v1",
            fact,
        )?),
        checks: vec![check],
    })
}
