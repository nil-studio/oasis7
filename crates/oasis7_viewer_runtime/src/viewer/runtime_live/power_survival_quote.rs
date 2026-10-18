use super::super::control_plane::{
    ensure_agent_player_access_runtime, map_auth_verify_error_code, normalize_optional_public_key,
};
use super::super::session_policy::map_session_policy_error_code;
use super::super::*;
use crate::simulator::WorldKernel;
use crate::viewer::auth::verify_power_survival_quote_auth_proof;
use crate::viewer::protocol::{
    GameplayActionError, PowerSurvivalQuotePreflight, PowerSurvivalQuoteRequest,
    PowerSurvivalRecoveryAction,
};
use oasis7_simulator::ResourceOwner;

impl ViewerRuntimeLiveServer {
    /// Computes the simulator-kernel `BuyPower` survival quote from a fresh, read-only runtime
    /// projection. No runtime action, event, auth nonce, or player binding is mutated by this path.
    pub(in crate::viewer::runtime_live) fn handle_power_survival_quote(
        &mut self,
        request: PowerSurvivalQuoteRequest,
    ) -> Result<PowerSurvivalQuotePreflight, GameplayActionError> {
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "quote_power_survival requires auth proof".to_string(),
            action_id: Some("quote_power_survival".to_string()),
            target_agent_id: None,
        })?;
        let verified =
            verify_power_survival_quote_auth_proof(&request, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    action_id: Some("quote_power_survival".to_string()),
                    target_agent_id: None,
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_power_survival".to_string()),
                target_agent_id: None,
            })?;
        let buyer_agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: "quote_power_survival requires a bound player Agent session".to_string(),
                action_id: Some("quote_power_survival".to_string()),
                target_agent_id: None,
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            buyer_agent_id,
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some("quote_power_survival".to_string()),
            target_agent_id: err.agent_id,
        })?;
        let model = super::super::mapping::runtime_state_to_simulator_model(
            self.world.state(),
            &self.llm_sidecar,
            self.seed_model.as_ref(),
        );
        let buyer = ResourceOwner::Agent {
            agent_id: buyer_agent_id.to_string(),
        };
        let kernel = WorldKernel::with_model(self.snapshot_config.clone(), model);
        let quote = match request.recovery_action {
            PowerSurvivalRecoveryAction::BuyPower => kernel.quote_power_survival(
                &buyer,
                &ResourceOwner::Agent {
                    agent_id: request.seller_agent_id.clone(),
                },
                request.amount,
                request.requested_price_per_pu,
            ),
            PowerSurvivalRecoveryAction::HarvestRadiation => {
                kernel.quote_harvest_radiation_survival(&buyer, request.amount)
            }
        }
        .map_err(|reason| GameplayActionError {
            code: "power_survival_quote_rejected".to_string(),
            message: match request.recovery_action {
                PowerSurvivalRecoveryAction::HarvestRadiation => {
                    "quote_power_survival harvest recovery is unavailable".to_string()
                }
                PowerSurvivalRecoveryAction::BuyPower => {
                    format!("quote_power_survival rejected: {reason:?}")
                }
            },
            action_id: Some("quote_power_survival".to_string()),
            target_agent_id: Some(buyer_agent_id.to_string()),
        })?;
        Ok(PowerSurvivalQuotePreflight {
            buyer_agent_id: quote.agent_id,
            seller_agent_id: request.seller_agent_id,
            current_power_level: quote.current_power_level,
            power_state_before: quote.power_state_before,
            recovery_action: quote.recovery_action,
            recovery_amount: quote.recovery_amount,
            power_gain_estimate: quote.power_gain_estimate,
            requested_price_per_pu: request.requested_price_per_pu,
            price_per_pu: quote.price_per_pu,
            price_or_time_cost: quote.price_or_time_cost,
            power_state_after_recovery: quote.power_state_after_recovery,
            survival_runway_ticks: quote.survival_runway_ticks,
            next_action_affordability_after_recovery: quote
                .next_action_affordability_after_recovery,
            shutdown_avoidance_reason: quote.shutdown_avoidance_reason,
            recommended_power_action: quote.recommended_power_action,
        })
    }

    pub(in crate::viewer::runtime_live) fn quote_power(
        &mut self,
        request: PowerSurvivalQuoteRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        send_response(
            writer,
            &self
                .handle_power_survival_quote(request)
                .map(|quote| ViewerResponse::PowerSurvivalQuotePreflight { quote })
                .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
        )
    }
}
