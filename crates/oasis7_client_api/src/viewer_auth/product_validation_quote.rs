use super::*;
use crate::viewer_protocol::ProductValidationQuoteRequest;

fn signing_request(request: &ProductValidationQuoteRequest) -> GameplayActionRequest {
    GameplayActionRequest {
        action_id: "quote_validate_product".to_string(),
        // Product and amount are both bound into the existing signed gameplay envelope.
        // The server treats this strictly as a read-only preflight, never as an action.
        target_agent_id: format!(
            "product_id:{}|amount:{}",
            request.product_id, request.amount
        ),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_product_validation_quote_auth_proof(
    request: &ProductValidationQuoteRequest,
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

pub fn verify_product_validation_quote_auth_proof(
    request: &ProductValidationQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
