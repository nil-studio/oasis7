#[path = "owner_setup_restore_signature_scope.rs"]
mod restore_signature_scope;
use super::*;
use crate::provisioning::{OwnerSetupOriginalV1, OwnerSetupResultV1, OwnerSetupWorldEvidenceV1};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{
    self as ws,
    participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
    private_goal::{prepare_owner_signature_strict, verify_owner_signature_strict},
};
pub(crate) use restore_signature_scope::RestoreSignatureScope;
#[cfg(feature = "signature-metrics")]
pub use restore_signature_scope::RestoreSignatureSummary;
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
    scope: Option<&mut RestoreSignatureScope>,
) -> Result<SignatureCheck> {
    let prepared = prepare_owner_signature_strict(domain, payload, key, signature)
        .map_err(|_| ApprovalAdmissionError::Invalid)?;
    let check = SignatureCheck {
        key_id: format!("owner-setup:{domain}"),
        public_key_hex: key.into(),
        signature_base64url: URL_SAFE_NO_PAD.encode(prepared.signature_bytes()),
        canonical_bytes: prepared.canonical_bytes().to_vec(),
    };
    match scope {
        Some(scope) => scope.verify(prepared),
        None => prepared.verify_strict(),
    }
    .map_err(|_| ApprovalAdmissionError::Invalid)?;
    Ok(check)
}
pub(crate) fn verify_owner_setup_restore_signature<T: Serialize>(
    domain: &str,
    payload: &T,
    key: &str,
    signature: &str,
    scope: Option<&mut RestoreSignatureScope>,
) -> std::result::Result<(), String> {
    match scope {
        None => verify_owner_signature_strict(domain, payload, key, signature),
        Some(scope) => scope.verify(prepare_owner_signature_strict(
            domain, payload, key, signature,
        )?),
    }
}

pub(crate) fn append_owner_setup_checks(
    mut checks: ExactSignatureChecks,
    fact: OwnerSetupSignatureFact<'_>,
) -> Result<ExactSignatureChecks> {
    checks.checks.extend(owner_setup_jobs(fact, None)?);
    checks.body_digest = canonical_digest(
        "oasis7.custody.owner-setup-signature-jobs.v1",
        &(&checks.body_digest, &checks.checks),
    )?;
    Ok(checks)
}
pub(crate) fn validate_owner_setup_signature_fact(fact: OwnerSetupSignatureFact<'_>) -> Result<()> {
    owner_setup_jobs(fact, None)?;
    Ok(())
}
pub(crate) fn validate_owner_setup_signature_fact_scoped(
    fact: OwnerSetupSignatureFact<'_>,
    scope: &mut RestoreSignatureScope,
) -> Result<()> {
    owner_setup_jobs(fact, Some(scope))?;
    Ok(())
}
fn owner_setup_jobs(
    fact: OwnerSetupSignatureFact<'_>,
    mut scope: Option<&mut RestoreSignatureScope>,
) -> Result<Vec<SignatureCheck>> {
    macro_rules! job {
        ($key:expr,$sig:expr,$domain:expr,$payload:expr) => {
            job($key, $sig, $domain, $payload, scope.as_deref_mut())
        };
    }

    let jobs=match fact {
        OwnerSetupSignatureFact::World(v,key)=>vec![job!(key,&v.response.signature_hex,PARTICIPANT_OWNER_VIEW_PATH,&(&v.response.request_digest,&v.response.payload))?,job!(&v.request.read.subject_public_key,&v.request.read.signature_hex,ws::VIEW_JSON_PATH,&v.request.read.request)?],
        OwnerSetupSignatureFact::GoalRead(v,key)=>vec![job!(key,&v.signature_hex,ws::private_goal_read::GOAL_CURRENT_PATH,&v.request)?],
        OwnerSetupSignatureFact::Read(v,key)=>vec![job!(key,&v.read.signature_hex,ws::VIEW_JSON_PATH,&v.read.request)?],
        OwnerSetupSignatureFact::Original(v,key)=>match v {
            OwnerSetupOriginalV1::Canonical {submit,..}=>vec![match &submit.signed_payload {
                oasis7_client_api::world_service_wire::WorldServicePayloadV1::GameTaskCreate(s)=>job!(key,&s.signature_hex,GAME_TASK_CREATE_DOMAIN,&s.request)?,
                oasis7_client_api::world_service_wire::WorldServicePayloadV1::GameGoalPublish(s)=>job!(key,&s.signature_hex,GAME_GOAL_PUBLISH_DOMAIN,&s.request)?,
                oasis7_client_api::world_service_wire::WorldServicePayloadV1::GameExecutorControl(s)=>job!(key,&s.signature_hex,GAME_EXECUTOR_CONTROL_DOMAIN,&s.request)?,
                oasis7_client_api::world_service_wire::WorldServicePayloadV1::Delegation(s)=>job!(key,&s.signature_hex,"delegation",&s.request)?,
                _=>return Err(ApprovalAdmissionError::Invalid),
            }],
            OwnerSetupOriginalV1::PrepareGoal(s)=>vec![job!(key,&s.signature_hex,GAME_GOAL_PREPARE_DOMAIN,&s.request)?],
            OwnerSetupOriginalV1::FinalObservation {owner,goal}=>vec![job!(key,&owner.read.signature_hex,ws::VIEW_JSON_PATH,&owner.read.request)?,job!(key,&goal.signature_hex,ws::private_goal_read::GOAL_CURRENT_PATH,&goal.request)?],
        },
        OwnerSetupSignatureFact::Result(v,key)=>match v {
            OwnerSetupResultV1::Canonical {response,from_lookup}=>vec![job!(key,&response.signature_hex,if *from_lookup{ws::LOOKUP_PATH}else{ws::SUBMIT_PATH},&(&response.request_digest,&response.payload))?],
            OwnerSetupResultV1::PreparedGoal(v)=>vec![job!(key,&v.signature_hex,ws::private_goal_read::GOAL_PREPARE_PATH,&(&v.request_digest,&v.payload))?],
            OwnerSetupResultV1::FinalObservation {owner,goal}=>vec![job!(key,&owner.signature_hex,PARTICIPANT_OWNER_VIEW_PATH,&(&owner.request_digest,&owner.payload))?,job!(key,&goal.signature_hex,ws::private_goal_read::GOAL_CURRENT_PATH,&(&goal.request_digest,&goal.payload))?],
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
        None,
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

/// Fixed signed termination fact only; no live authority or side effect grant.
pub(crate) fn termination_archive_signature_checks(
    id: &str,
    kind: crate::provisioning::TerminationOperationV1,
    original: &OwnerSetupOriginalV1,
    result: &OwnerSetupResultV1,
    key: &str,
) -> Result<ExactSignatureChecks> {
    let checks = ExactSignatureChecks {
        operation_id: id.into(),
        body_digest: canonical_digest(
            "oasis7.custody.termination-archive-fact.v1",
            &(id, kind, original, result),
        )?,
        context_digest: canonical_digest(
            "oasis7.custody.termination-archive-pin.v1",
            &(id, kind, key),
        )?,
        verify_nonce: canonical_digest(
            "oasis7.custody.termination-archive-original.v1",
            &(id, kind, original),
        )?,
        verification_digest: Some(canonical_digest(
            "oasis7.custody.termination-archive-result.v1",
            result,
        )?),
        checks: Vec::new(),
    };
    append_owner_setup_checks(checks, OwnerSetupSignatureFact::Result(result, key))
}
