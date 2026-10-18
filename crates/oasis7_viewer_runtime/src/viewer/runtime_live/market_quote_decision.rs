use super::control_plane::{
    ensure_agent_player_access_runtime, map_auth_verify_error_code, normalize_optional_public_key,
};
use super::session_policy::map_session_policy_error_code;
use super::*;
use crate::runtime::{MaterialLedgerId, MaterialStack};
use crate::viewer::auth::verify_market_quote_decision_auth_proof;
use crate::viewer::protocol::{
    GameplayActionError, MarketQuoteDecisionPreflight, MarketQuoteDecisionRequest,
    MarketQuoteMaterialContribution,
};

fn player_material_name(material: &str) -> String {
    let label = material
        .split('_')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut characters = label.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

impl ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn handle_market_quote_decision_request(
        &mut self,
        request: MarketQuoteDecisionRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let response = self
            .handle_market_quote_decision(request)
            .map(|quote| ViewerResponse::MarketQuoteDecisionPreflight { quote })
            .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error });
        send_response(writer, &response)
    }

    /// Produces a signed, read-only market preflight for the requesting player's bound Agent.
    /// It neither reserves materials nor submits a recipe or runtime event.
    pub(in crate::viewer::runtime_live) fn handle_market_quote_decision(
        &mut self,
        request: MarketQuoteDecisionRequest,
    ) -> Result<MarketQuoteDecisionPreflight, GameplayActionError> {
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "market cost preview requires auth proof".to_string(),
            action_id: Some("quote_market_decision".to_string()),
            target_agent_id: None,
        })?;
        let verified =
            verify_market_quote_decision_auth_proof(&request, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    action_id: Some("quote_market_decision".to_string()),
                    target_agent_id: None,
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_market_decision".to_string()),
                target_agent_id: None,
            })?;
        let agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: "market cost preview requires a bound player Agent session".to_string(),
                action_id: Some("quote_market_decision".to_string()),
                target_agent_id: None,
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            agent_id,
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some("quote_market_decision".to_string()),
            target_agent_id: err.agent_id,
        })?;

        if request.consume.is_empty()
            || request
                .consume
                .iter()
                .any(|material| material.material.trim().is_empty() || material.amount <= 0)
        {
            return Err(GameplayActionError {
                code: "market_quote_rejected".to_string(),
                message: "market cost preview requires at least one named material with a positive amount"
                    .to_string(),
                action_id: Some("quote_market_decision".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            });
        }

        let consume = request
            .consume
            .iter()
            .map(|material| MaterialStack::new(material.material.trim(), material.amount))
            .collect::<Vec<_>>();
        let preview = self
            .world
            .market_quote_decision_preview(&MaterialLedgerId::agent(agent_id), &consume);
        let contributions = preview
            .market_quotes
            .iter()
            .zip(preview.local_vs_world_delta.iter())
            .map(|(quote, delta)| MarketQuoteMaterialContribution {
                material: player_material_name(quote.kind.as_str()),
                requested_amount: quote.requested_amount,
                local_available_amount: quote.local_available_amount,
                world_available_amount: quote.world_available_amount,
                world_cover_amount: delta.world_cover_amount,
                shortfall_amount: delta.unsatisfied_shortfall_amount,
                transit_loss_bps: quote.transit_loss_bps,
                governance_tax_bps: quote.governance_tax_bps,
                effective_cost_index_ppm: quote.effective_cost_index_ppm,
            })
            .collect();
        let submission_allowed = preview.total_unsatisfied_shortfall == 0;
        Ok(MarketQuoteDecisionPreflight {
            consuming_agent_id: agent_id.to_string(),
            contributions,
            total_shortfall_amount: preview.total_unsatisfied_shortfall,
            submission_allowed,
            conditional_notice: "This is a conditional preview. Inventory, tax, transit, and price may change before submission."
                .to_string(),
            recommendation: match preview.recommendation.as_str() {
                "submit_with_local_supply" => "Submit with local materials".to_string(),
                "submit_with_world_supply" => "Submit using available world supply".to_string(),
                _ => "Reduce the request or obtain more materials".to_string(),
            },
            rationale: match preview.recommendation.as_str() {
                "submit_with_local_supply" => {
                    "Your local inventory covers the requested materials.".to_string()
                }
                "submit_with_world_supply" => {
                    "World supply can cover the local gap, subject to transit and tax at submission."
                        .to_string()
                }
                _ => "Available local and world materials do not cover this request.".to_string(),
            },
            next_action: match preview.next_reduction_action.as_str() {
                "submit_recipe" => "Submit the recipe when ready".to_string(),
                "use_local_materials" => "Use local materials to reduce market exposure".to_string(),
                _ => "Reduce requested amounts or source the missing materials".to_string(),
            },
        })
    }
}
