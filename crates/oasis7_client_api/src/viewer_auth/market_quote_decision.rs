use super::*;
use crate::viewer_protocol::MarketQuoteDecisionRequest;

fn signing_request(request: &MarketQuoteDecisionRequest) -> GameplayActionRequest {
    let consume = serde_json::to_string(&request.consume)
        .expect("market quote request materials must serialize for auth binding");
    GameplayActionRequest {
        action_id: "quote_market_decision".to_string(),
        // Every requested material and amount is bound into the read-only quote signature.
        target_agent_id: format!("market_consume:{consume}"),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_market_quote_decision_auth_proof(
    request: &MarketQuoteDecisionRequest,
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

pub fn verify_market_quote_decision_auth_proof(
    request: &MarketQuoteDecisionRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
