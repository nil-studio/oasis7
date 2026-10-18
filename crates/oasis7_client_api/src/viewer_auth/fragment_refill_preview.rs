use super::*;
use crate::viewer_protocol::FragmentRefillPreviewRequest;

fn signing_request(request: &FragmentRefillPreviewRequest) -> GameplayActionRequest {
    GameplayActionRequest {
        action_id: "preview_fragment_replenishment".to_string(),
        // Bind the exact requested chunk into the normal signed gameplay envelope. The server
        // never turns this read-only preflight into an executable action.
        target_agent_id: format!(
            "chunk:{}:{}:{}",
            request.chunk.x, request.chunk.y, request.chunk.z
        ),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_fragment_refill_preview_auth_proof(
    request: &FragmentRefillPreviewRequest,
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

pub fn verify_fragment_refill_preview_auth_proof(
    request: &FragmentRefillPreviewRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
