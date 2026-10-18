use super::*;
use crate::viewer_protocol::TransferMaterialQuoteRequest;

fn signing_request(request: &TransferMaterialQuoteRequest) -> GameplayActionRequest {
    let mut signing_target = serde_json::json!({
        "requester_agent_id": request.requester_agent_id,
        "from_ledger": request.from_ledger,
        "to_ledger": request.to_ledger,
        "kind": request.kind,
        "amount": request.amount,
        "distance_km": request.distance_km,
        "requested_priority": request.requested_priority,
    });
    // Keep signatures from legacy clients valid when all new route fields are at
    // their serde defaults, while binding every non-default route input.
    if let Some(route_id) = request.route_id.as_ref() {
        signing_target["route_id"] = serde_json::Value::String(route_id.clone());
    }
    if !request.route_ids.is_empty() {
        signing_target["route_ids"] = serde_json::to_value(&request.route_ids)
            .expect("transfer-material quote route ids are serializable");
    }
    if request.auto_reroute {
        signing_target["auto_reroute"] = serde_json::Value::Bool(true);
    }
    let target_agent_id = serde_json::to_string(&signing_target)
        .expect("transfer-material quote signing target is serializable");
    GameplayActionRequest {
        action_id: "quote_transfer_material".to_string(),
        // Every logistics input is represented in the structured signing target.
        target_agent_id,
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_transfer_material_quote_auth_proof(
    request: &TransferMaterialQuoteRequest,
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

pub fn verify_transfer_material_quote_auth_proof(
    request: &TransferMaterialQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_gameplay_action_auth_proof(&signing_request(request), proof)
}
