use super::*;
use crate::viewer_protocol::PublishSocialFactQuoteRequest;

fn signing_request(request: &PublishSocialFactQuoteRequest) -> GameplayActionRequest {
    let parameters = serde_json::to_string(&(
        &request.schema_id,
        &request.subject_agent_id,
        &request.object_agent_id,
        &request.claim,
        request.confidence_ppm,
        &request.evidence_event_ids,
        request.ttl_ticks,
        &request.stake,
    ))
    .expect("publish social fact quote parameters must serialize for auth binding");
    GameplayActionRequest {
        action_id: "quote_publish_social_fact".to_string(),
        // Every candidate fact parameter is bound into this read-only preflight signature.
        target_agent_id: format!("publish_social_fact:{parameters}"),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_publish_social_fact_quote_auth_proof(
    request: &PublishSocialFactQuoteRequest,
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

pub fn verify_publish_social_fact_quote_auth_proof(
    request: &PublishSocialFactQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
