use super::control_plane::{
    ensure_agent_player_access_runtime, map_auth_verify_error_code, normalize_optional_public_key,
};
use super::session_policy::map_session_policy_error_code;
use super::*;
use crate::simulator::WorldKernel;
use crate::viewer::auth::verify_power_sale_quote_auth_proof;
use crate::viewer::protocol::{
    GameplayActionError, PowerSaleQuotePreflight, PowerSaleQuoteRequest,
};
use oasis7_simulator::ResourceOwner;

impl ViewerRuntimeLiveServer {
    /// Computes the kernel's `SellPower` preflight for the seller bound to the signed session.
    /// This path only projects state into a fresh kernel and never records an action or event.
    pub(in crate::viewer::runtime_live) fn handle_power_sale_quote(
        &mut self,
        request: PowerSaleQuoteRequest,
    ) -> Result<PowerSaleQuotePreflight, GameplayActionError> {
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "quote_power_sale requires auth proof".to_string(),
            action_id: Some("quote_power_sale".to_string()),
            target_agent_id: None,
        })?;
        let verified = verify_power_sale_quote_auth_proof(&request, auth).map_err(|message| {
            GameplayActionError {
                code: map_auth_verify_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_power_sale".to_string()),
                target_agent_id: None,
            }
        })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_power_sale".to_string()),
                target_agent_id: None,
            })?;
        let seller_agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: "quote_power_sale requires a bound player Agent session".to_string(),
                action_id: Some("quote_power_sale".to_string()),
                target_agent_id: None,
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            seller_agent_id,
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some("quote_power_sale".to_string()),
            target_agent_id: err.agent_id,
        })?;
        let model = super::mapping::runtime_state_to_simulator_model(
            self.world.state(),
            &self.llm_sidecar,
            self.seed_model.as_ref(),
        );
        let kernel = WorldKernel::with_model(self.snapshot_config.clone(), model);
        let quote = kernel
            .quote_power_sale(
                &ResourceOwner::Agent {
                    agent_id: seller_agent_id.to_string(),
                },
                &ResourceOwner::Agent {
                    agent_id: request.buyer_agent_id.clone(),
                },
                request.amount,
                request.requested_price_per_pu,
            )
            .map_err(|reason| GameplayActionError {
                code: "power_sale_quote_rejected".to_string(),
                message: format!("quote_power_sale rejected: {reason:?}"),
                action_id: Some("quote_power_sale".to_string()),
                target_agent_id: Some(seller_agent_id.to_string()),
            })?;
        Ok(PowerSaleQuotePreflight {
            seller_agent_id: quote.agent_id,
            buyer_agent_id: request.buyer_agent_id,
            current_power_level: quote.current_power_level,
            power_state_before: quote.power_state_before,
            sale_amount: quote.sale_amount,
            price_per_pu: quote.price_per_pu,
            expected_revenue: quote.expected_revenue,
            power_state_after_sale: quote.power_state_after_sale,
            remaining_runway_ticks: quote.remaining_runway_ticks,
            next_action_affordability_after_sale: quote.next_action_affordability_after_sale,
            production_interrupt_risk: quote.production_interrupt_risk,
            recommended_sale_action: quote.recommended_sale_action,
            why_sale_is_safe_or_risky: quote.why_sale_is_safe_or_risky,
        })
    }

    pub(in crate::viewer::runtime_live) fn quote_power_sale(
        &mut self,
        request: PowerSaleQuoteRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        send_response(
            writer,
            &self
                .handle_power_sale_quote(request)
                .map(|quote| ViewerResponse::PowerSaleQuotePreflight { quote })
                .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
        )
    }
}
