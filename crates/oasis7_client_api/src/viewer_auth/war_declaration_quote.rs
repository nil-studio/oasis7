use super::*;
use crate::viewer_protocol::WarDeclarationQuoteRequest;

fn signing_request(request: &WarDeclarationQuoteRequest) -> GameplayActionRequest {
    GameplayActionRequest {
        action_id: "quote_declare_war".to_string(),
        target_agent_id: format!(
            "aggressor_alliance_id:{}|defender_alliance_id:{}|intensity:{}",
            request.aggressor_alliance_id, request.defender_alliance_id, request.intensity
        ),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_war_declaration_quote_auth_proof(
    request: &WarDeclarationQuoteRequest,
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

pub fn verify_war_declaration_quote_auth_proof(
    request: &WarDeclarationQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
