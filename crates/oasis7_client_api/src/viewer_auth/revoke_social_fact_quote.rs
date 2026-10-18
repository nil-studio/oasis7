use super::*;
use crate::viewer_protocol::RevokeSocialFactQuoteRequest;

fn signing_request(request: &RevokeSocialFactQuoteRequest) -> GameplayActionRequest {
    let parameters = serde_json::to_string(&(request.fact_id, request.reason.trim()))
        .expect("revoke social fact quote parameters must serialize for auth binding");
    GameplayActionRequest {
        action_id: "quote_revoke_social_fact".to_string(),
        // Bind the fact and normalized reason into this read-only preflight signature.
        target_agent_id: format!("revoke_social_fact:{parameters}"),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_revoke_social_fact_quote_auth_proof(
    request: &RevokeSocialFactQuoteRequest,
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

pub fn verify_revoke_social_fact_quote_auth_proof(
    request: &RevokeSocialFactQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
