use super::*;
use crate::viewer_protocol::DeclareSocialEdgeQuoteRequest;

fn signing_request(request: &DeclareSocialEdgeQuoteRequest) -> GameplayActionRequest {
    let parameters = serde_json::to_string(&(
        &request.schema_id,
        &request.relation_kind,
        &request.from_agent_id,
        &request.to_agent_id,
        request.weight_bps,
        &request.backing_fact_ids,
        request.ttl_ticks,
    ))
    .expect("declare social edge quote parameters must serialize for auth binding");
    GameplayActionRequest {
        action_id: "quote_declare_social_edge".to_string(),
        // Every candidate edge parameter is bound into this read-only preflight signature.
        target_agent_id: format!("declare_social_edge:{parameters}"),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_declare_social_edge_quote_auth_proof(
    request: &DeclareSocialEdgeQuoteRequest,
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

pub fn verify_declare_social_edge_quote_auth_proof(
    request: &DeclareSocialEdgeQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
