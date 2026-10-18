use super::control_plane::{
    ensure_agent_player_access_runtime, map_auth_verify_error_code, normalize_optional_public_key,
};
use super::session_policy::map_session_policy_error_code;
use super::support::send_response;
use super::*;
use crate::viewer::auth::verify_war_declaration_quote_auth_proof;
use crate::viewer::protocol::{
    GameplayActionError, WarDeclarationQuotePreflight, WarDeclarationQuoteRequest,
};

impl ViewerRuntimeLiveServer {
    /// Returns a signed, read-only core war projection; it never declares or reserves a war.
    pub(in crate::viewer::runtime_live) fn handle_war_declaration_quote(
        &mut self,
        request: WarDeclarationQuoteRequest,
    ) -> Result<WarDeclarationQuotePreflight, GameplayActionError> {
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "quote_declare_war requires auth proof".to_string(),
            action_id: Some("quote_declare_war".to_string()),
            target_agent_id: None,
        })?;
        let verified =
            verify_war_declaration_quote_auth_proof(&request, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    action_id: Some("quote_declare_war".to_string()),
                    target_agent_id: None,
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_declare_war".to_string()),
                target_agent_id: None,
            })?;
        let actor_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: "quote_declare_war requires a bound player Agent session".to_string(),
                action_id: Some("quote_declare_war".to_string()),
                target_agent_id: None,
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            actor_id,
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some("quote_declare_war".to_string()),
            target_agent_id: err.agent_id,
        })?;
        let quote = self.world.war_declaration_quote(actor_id, request.aggressor_alliance_id.as_str(), request.defender_alliance_id.as_str(), request.intensity).map_err(|_| GameplayActionError { code: "war_declaration_quote_unavailable".to_string(), message: "War quote is unavailable because the active settlement path does not expose an authoritative read-only projection.".to_string(), action_id: Some("quote_declare_war".to_string()), target_agent_id: Some(actor_id.to_string()) })?;
        Ok(WarDeclarationQuotePreflight {
            actor_alliance_id: quote.actor_alliance_id,
            target_alliance_id: quote.target_alliance_id,
            action_kind: quote.action_kind,
            intensity: quote.intensity,
            settlement_path: quote.settlement_path,
            conflict_status: quote.conflict_status,
            minimum_winning_intensity: quote.minimum_winning_intensity,
            war_duration_ticks: quote.war_duration_ticks,
            aggressor_score_estimate: quote.aggressor_score_estimate,
            defender_score_estimate: quote.defender_score_estimate,
            likely_winner_before_action: quote.likely_winner_before_action,
            projected_outcome: quote.projected_outcome,
            victory_margin_estimate: quote.victory_margin_estimate,
            conflict_window_blocked_until: quote.conflict_window_blocked_until,
            reentry_cooldown_or_active_conflict_blocker: quote
                .reentry_cooldown_or_active_conflict_blocker,
            expected_narrative_or_module_reward: quote.expected_narrative_or_module_reward,
            settlement_risk: quote.settlement_risk,
            settlement_risk_code: quote.settlement_risk_code,
            alternative_action: quote.alternative_action,
            recommended_war_action: quote.recommended_war_action,
            why_this_war_is_worth_or_risky: quote.why_this_war_is_worth_or_risky,
            mobilization_electricity_required: quote.mobilization_electricity_required,
            mobilization_electricity_current: quote.mobilization_electricity_current,
            mobilization_electricity_after: quote.mobilization_electricity_after,
            mobilization_data_required: quote.mobilization_data_required,
            mobilization_data_current: quote.mobilization_data_current,
            mobilization_data_after: quote.mobilization_data_after,
            mobilization_affordable: quote.mobilization_affordable,
            quoted_at_tick: quote.quoted_at_tick,
            state_fingerprint: quote.state_fingerprint,
        })
    }
    pub(in crate::viewer::runtime_live) fn quote_declare_war(
        &mut self,
        request: WarDeclarationQuoteRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        send_response(
            writer,
            &self
                .handle_war_declaration_quote(request)
                .map(|quote| ViewerResponse::WarDeclarationQuotePreflight { quote })
                .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
        )
    }
}
