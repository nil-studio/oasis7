use super::*;
use crate::viewer_protocol::{PowerSurvivalQuoteRequest, PowerSurvivalRecoveryAction};

fn signing_request(request: &PowerSurvivalQuoteRequest) -> GameplayActionRequest {
    GameplayActionRequest {
        action_id: "quote_power_survival".to_string(),
        // The selected recovery action, seller, amount, and requested price are all bound into
        // the existing signed gameplay envelope. The server treats this strictly as a read-only
        // preflight, never as an action.
        target_agent_id: format!(
            "recovery_action:{}|seller_agent_id:{}|amount:{}|requested_price_per_pu:{}",
            request.recovery_action.as_str(),
            request.seller_agent_id,
            request.amount,
            request.requested_price_per_pu
        ),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

fn legacy_buy_power_signing_request(request: &PowerSurvivalQuoteRequest) -> GameplayActionRequest {
    GameplayActionRequest {
        action_id: "quote_power_survival".to_string(),
        target_agent_id: format!(
            "seller_agent_id:{}|amount:{}|requested_price_per_pu:{}",
            request.seller_agent_id, request.amount, request.requested_price_per_pu
        ),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    }
}

pub fn sign_power_survival_quote_auth_proof(
    request: &PowerSurvivalQuoteRequest,
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

pub fn verify_power_survival_quote_auth_proof(
    request: &PowerSurvivalQuoteRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    match verify_gameplay_action_auth_proof(&signing_request(request), proof) {
        Ok(verified) => Ok(verified),
        Err(current_error) if request.recovery_action == PowerSurvivalRecoveryAction::BuyPower => {
            verify_gameplay_action_auth_proof(&legacy_buy_power_signing_request(request), proof)
                .map_err(|_| current_error)
        }
        Err(error) => Err(error),
    }
}
