use super::*;
use crate::viewer_protocol::SocialContactQuoteRequest;

fn signing_request(request: &SocialContactQuoteRequest) -> GameplayActionRequest {
    let parameters = serde_json::to_string(&(
        &request.contact_purpose,
        &request.first_contact_class,
        &request.candidate_agent_id,
    ))
    .expect("social contact quote parameters must serialize for auth binding");
    GameplayActionRequest {
        action_id: "quote_social_contact".to_string(),
        target_agent_id: format!("social_contact:{parameters}"),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_social_contact_quote_auth_proof(
    request: &SocialContactQuoteRequest,
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

pub fn verify_social_contact_quote_auth_proof(
    request: &SocialContactQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
