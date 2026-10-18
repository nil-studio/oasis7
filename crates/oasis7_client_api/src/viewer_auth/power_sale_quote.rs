use super::*;
use crate::viewer_protocol::PowerSaleQuoteRequest;

fn signing_request(request: &PowerSaleQuoteRequest) -> GameplayActionRequest {
    GameplayActionRequest {
        action_id: "quote_power_sale".to_string(),
        // Buyer, amount, and requested price are bound; the seller comes from the session.
        target_agent_id: format!(
            "buyer_agent_id:{}|amount:{}|requested_price_per_pu:{}",
            request.buyer_agent_id, request.amount, request.requested_price_per_pu
        ),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_power_sale_quote_auth_proof(
    request: &PowerSaleQuoteRequest,
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

pub fn verify_power_sale_quote_auth_proof(
    request: &PowerSaleQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
