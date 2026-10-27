use super::*;
use crate::provisioning::{ParticipantFinalResultV1, ParticipantSignBundleV1};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use oasis7_agent_api::game::{
    GAME_PARTICIPANT_ADMISSION_DOMAIN, GAME_PARTICIPANT_REGISTER_DOMAIN, GameParticipantAdmissionV1,
};
use oasis7_client_api::world_service::{
    self as ws,
    participant_discovery::{
        PARTICIPANT_OWNER_VIEW_PATH, PARTICIPANT_POLICY_PATH, ParticipantPolicyOriginalV1,
    },
};
pub(crate) enum ParticipantSignatureFact<'a> {
    Policy(&'a ParticipantPolicyOriginalV1, &'a str),
    Bundle(&'a ParticipantSignBundleV1),
    Admission(
        &'a ws::SignedReadRequest<GameParticipantAdmissionV1>,
        &'a str,
    ),
    Final(&'a ParticipantFinalResultV1, &'a str),
}
fn hex_bytes(value: &str, size: usize) -> Result<Vec<u8>> {
    if value.len() != size * 2
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ApprovalAdmissionError::Invalid);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|v| {
            std::str::from_utf8(v)
                .ok()
                .and_then(|v| u8::from_str_radix(v, 16).ok())
                .ok_or(ApprovalAdmissionError::Invalid)
        })
        .collect()
}
fn job<T: Serialize>(
    key: &str,
    signature: &str,
    domain: &str,
    payload: &T,
) -> Result<SignatureCheck> {
    hex_bytes(key, 32)?;
    let sig = hex_bytes(signature, 64)?;
    Ok(SignatureCheck {
        key_id: format!("participant:{domain}"),
        public_key_hex: key.into(),
        signature_base64url: URL_SAFE_NO_PAD.encode(sig),
        canonical_bytes: ws::signing_bytes(domain, payload)
            .map_err(|_| ApprovalAdmissionError::Invalid)?,
    })
}
pub(crate) fn append_participant_signature_checks(
    mut checks: ExactSignatureChecks,
    fact: ParticipantSignatureFact<'_>,
) -> Result<ExactSignatureChecks> {
    let mut jobs = match fact {
        ParticipantSignatureFact::Policy(v, pin) => vec![
            job(
                &v.request.subject_public_key,
                &v.request.signature_hex,
                PARTICIPANT_POLICY_PATH,
                &v.request.request,
            )?,
            job(
                pin,
                &v.response.signature_hex,
                PARTICIPANT_POLICY_PATH,
                &(&v.response.request_digest, &v.response.payload),
            )?,
        ],
        ParticipantSignatureFact::Bundle(v) => vec![
            job(
                &v.owner.subject_public_key,
                &v.owner.signature_hex,
                GAME_PARTICIPANT_REGISTER_DOMAIN,
                &v.owner.request,
            )?,
            job(
                &v.read.subject_public_key,
                &v.read.signature_hex,
                ws::VIEW_JSON_PATH,
                &v.read.request,
            )?,
        ],
        ParticipantSignatureFact::Admission(v, pin) => vec![job(
            pin,
            &v.signature_hex,
            GAME_PARTICIPANT_ADMISSION_DOMAIN,
            &v.request,
        )?],
        ParticipantSignatureFact::Final(v, pin) => vec![
            job(
                pin,
                &v.response.signature_hex,
                v.response_source.response_domain(),
                &(&v.response.request_digest, &v.response.payload),
            )?,
            job(
                pin,
                &v.owner_response.signature_hex,
                PARTICIPANT_OWNER_VIEW_PATH,
                &(&v.owner_response.request_digest, &v.owner_response.payload),
            )?,
        ],
    };
    checks.checks.append(&mut jobs);
    checks.body_digest = canonical_digest(
        "oasis7.custody.participant-signature-jobs.v1",
        &(&checks.body_digest, &checks.checks),
    )?;
    Ok(checks)
}
