use super::*;
use crate::viewer_protocol::GovernanceVoteQuoteRequest;

fn signing_request(request: &GovernanceVoteQuoteRequest) -> GameplayActionRequest {
    GameplayActionRequest {
        action_id: "quote_governance_vote".to_string(),
        target_agent_id: format!(
            "proposal_key:{}|option:{}|weight:{}",
            request.proposal_key, request.option, request.weight
        ),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_governance_vote_quote_auth_proof(
    request: &GovernanceVoteQuoteRequest,
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

pub fn verify_governance_vote_quote_auth_proof(
    request: &GovernanceVoteQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
