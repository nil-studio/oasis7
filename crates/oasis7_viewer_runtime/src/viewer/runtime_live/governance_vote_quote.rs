use super::control_plane::{
    ensure_agent_player_access_runtime, map_auth_verify_error_code, normalize_optional_public_key,
};
use super::session_policy::map_session_policy_error_code;
use super::*;
use crate::viewer::auth::verify_governance_vote_quote_auth_proof;
use crate::viewer::protocol::{
    GameplayActionError, GovernanceVoteQuotePreflight, GovernanceVoteQuoteRequest,
};

impl ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn handle_governance_vote_quote(
        &mut self,
        request: GovernanceVoteQuoteRequest,
    ) -> Result<GovernanceVoteQuotePreflight, GameplayActionError> {
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "quote_governance_vote requires auth proof".to_string(),
            action_id: Some("quote_governance_vote".to_string()),
            target_agent_id: None,
        })?;
        let verified =
            verify_governance_vote_quote_auth_proof(&request, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    action_id: Some("quote_governance_vote".to_string()),
                    target_agent_id: None,
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_governance_vote".to_string()),
                target_agent_id: None,
            })?;
        let actor_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: "quote_governance_vote requires a bound player Agent session".to_string(),
                action_id: Some("quote_governance_vote".to_string()),
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
            action_id: Some("quote_governance_vote".to_string()),
            target_agent_id: err.agent_id,
        })?;
        let quote = self.world.governance_vote_quote(request.proposal_key.as_str(), actor_id, request.option.as_str(), request.weight).map_err(|_| GameplayActionError { code: "governance_vote_quote_unavailable".to_string(), message: "Governance vote quote is unavailable because the proposal is not open or the requested vote is invalid.".to_string(), action_id: Some("quote_governance_vote".to_string()), target_agent_id: Some(actor_id.to_string()) })?;
        Ok(GovernanceVoteQuotePreflight {
            proposal_id: quote.proposal_id,
            proposal_topic: quote.proposal_topic,
            actor_id: quote.actor_id,
            action_kind: quote.action_kind,
            closes_at_tick: quote.closes_at_tick,
            ticks_remaining: quote.ticks_remaining,
            current_quorum_weight: quote.current_quorum_weight,
            required_quorum_weight: quote.required_quorum_weight,
            current_pass_bps: quote.current_pass_bps,
            required_pass_bps: quote.required_pass_bps,
            actor_vote_weight: quote.actor_vote_weight,
            vote_swing_potential: quote.vote_swing_potential,
            likely_outcome_before_action: quote.likely_outcome_before_action,
            likely_outcome_after_action: quote.likely_outcome_after_action,
            affected_rule_or_priority: quote.affected_rule_or_priority,
            world_change_if_passed: quote.world_change_if_passed,
            cost_or_cooldown_if_failed: quote.cost_or_cooldown_if_failed,
            recommended_governance_action: quote.recommended_governance_action,
            why_this_vote_matters: quote.why_this_vote_matters,
        })
    }

    pub(in crate::viewer::runtime_live) fn quote_governance_vote(
        &mut self,
        request: GovernanceVoteQuoteRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        send_response(
            writer,
            &self
                .handle_governance_vote_quote(request)
                .map(|quote| ViewerResponse::GovernanceVoteQuotePreflight { quote })
                .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
        )
    }
}
