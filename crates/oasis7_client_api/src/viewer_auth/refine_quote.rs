use super::*;
use crate::viewer_protocol::RefineQuoteRequest;

fn signing_request(request: &RefineQuoteRequest) -> GameplayActionRequest {
    GameplayActionRequest {
        action_id: "quote_refine_compound".to_string(),
        // The existing signed gameplay envelope is reused, with the requested amount bound
        // into its signed target field. The server never treats this as an executable action.
        target_agent_id: format!("compound_mass_g:{}", request.compound_mass_g),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_refine_quote_auth_proof(
    request: &RefineQuoteRequest,
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

pub fn verify_refine_quote_auth_proof(
    request: &RefineQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
