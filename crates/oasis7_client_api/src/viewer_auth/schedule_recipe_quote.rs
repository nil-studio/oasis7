use super::*;
use crate::viewer_protocol::ScheduleRecipeQuoteRequest;

#[derive(serde::Serialize)]
struct ScheduleRecipeQuoteSigningTarget<'a> {
    factory_id: &'a str,
    recipe_id: &'a str,
    batches: i64,
}

fn signing_request(request: &ScheduleRecipeQuoteRequest) -> GameplayActionRequest {
    GameplayActionRequest {
        action_id: "quote_schedule_recipe".to_string(),
        // The structured JSON representation is injective across all request fields.
        target_agent_id: serde_json::to_string(&ScheduleRecipeQuoteSigningTarget {
            factory_id: request.factory_id.as_str(),
            recipe_id: request.recipe_id.as_str(),
            batches: request.batches,
        })
        .expect("ScheduleRecipe quote signing target is serializable"),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_schedule_recipe_quote_auth_proof(
    request: &ScheduleRecipeQuoteRequest,
    nonce: u64,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<PlayerAuthProof, String> {
    sign_gameplay_action_auth_proof(
        &signing_request(request),
        nonce,
        signer_public_key_hex,
        signer_private_key_hex,
    )
}

pub fn verify_schedule_recipe_quote_auth_proof(
    request: &ScheduleRecipeQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
