//! Participant binding checks mirror the native client without transport or keys.
use super::{Value, parse, ws};
use oasis7_agent_api::game::{
    GAME_PARTICIPANT_ADMISSION_DOMAIN, GAME_PARTICIPANT_REGISTER_DOMAIN, GameIdentityNamespaceV1,
    GameParticipantAdmissionV1, GameParticipantReceiptV1, GameParticipantRegisterV1,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmissionInput {
    owner: ws::SignedReadRequest<GameParticipantRegisterV1>,
    namespace: GameIdentityNamespaceV1,
    #[serde(rename = "issuerPublicKey")]
    issuer_public_key: String,
}
pub(super) fn admission(request: &str, payload: &str) -> Result<(), &'static str> {
    let input: AdmissionInput = parse(request)?;
    let admission: ws::SignedReadRequest<GameParticipantAdmissionV1> = parse(payload)?;
    let a = &admission.request;
    let owner = &input.owner;
    let r = &owner.request;
    r.validate().map_err(|_| "contract_mismatch")?;
    ws::verify_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN, owner)
        .map_err(|_| "contract_mismatch")?;
    ws::verify_read_request(GAME_PARTICIPANT_ADMISSION_DOMAIN, &admission)
        .map_err(|_| "contract_mismatch")?;
    if owner.subject_public_key != r.owner_public_key
        || admission.subject_public_key != input.issuer_public_key
        || a.api_rev != 1
        || a.identity_namespace != input.namespace
        || !a.identity_namespace.same_authority(&r.identity_namespace)
        || a.account_id != r.account_id
        || a.world_id != r.world_id
        || a.genesis_digest != r.genesis_digest
        || a.owner_public_key != r.owner_public_key
        || a.branch_id.trim().is_empty()
        || a.policy_generation.0 == 0
        || a.valid_until_tick.0 < a.valid_from_tick.0
        || a.business_digest
            != oasis7_client_api::world_service_correlation::participant_business_digest(r)
                .map_err(|_| "contract_mismatch")?
    {
        return Err("contract_mismatch");
    }
    Ok(())
}
pub(super) fn receipt(request: &str, payload: &str) -> Result<(), &'static str> {
    let r: GameParticipantRegisterV1 = parse(request)?;
    r.validate().map_err(|_| "contract_mismatch")?;
    let response: ws::IntentResponse<Value> = parse(payload)?;
    response
        .validate(&response.correlation)
        .map_err(|_| "contract_mismatch")?;
    let ws::IntentOutcome::Committed { commit, receipt } = response.outcome else {
        return Err("participant_unsettled");
    };
    let receipt: GameParticipantReceiptV1 =
        serde_json::from_value(receipt).map_err(|_| "contract_mismatch")?;
    let mut original = r.clone();
    original
        .client_operation_id
        .clone_from(&receipt.client_operation_id);
    if commit.world.world_id != r.world_id
        || commit.world.genesis_digest != r.genesis_digest
        || receipt.api_rev != 1
        || receipt.identity_namespace != r.identity_namespace
        || receipt.account_id != r.account_id
        || receipt.world_id != r.world_id
        || receipt.genesis_digest != r.genesis_digest
        || receipt.owner_public_key != r.owner_public_key
        || receipt.business_digest
            != oasis7_client_api::world_service_correlation::participant_business_digest(&original)
                .map_err(|_| "contract_mismatch")?
        || receipt.agent_id.is_empty()
        || receipt.owner_binding.is_empty()
        || receipt.owner_generation.0 == 0
    {
        return Err("contract_mismatch");
    }
    Ok(())
}
