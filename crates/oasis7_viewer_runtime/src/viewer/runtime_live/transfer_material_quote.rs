use super::control_plane::{
    ensure_agent_player_access_runtime, map_auth_verify_error_code, normalize_optional_public_key,
};
use super::session_policy::map_session_policy_error_code;
use super::*;
use crate::runtime::{MaterialLedgerId, MaterialTransitPriority};
use crate::viewer::auth::verify_transfer_material_quote_auth_proof;
use crate::viewer::protocol::{
    GameplayActionError, TransferMaterialPriority, TransferMaterialQuotePreflight,
    TransferMaterialQuoteRequest,
};

impl ViewerRuntimeLiveServer {
    /// Computes the authoritative logistics quote without reserving material, capacity, or
    /// writing the runtime journal.
    pub(in crate::viewer::runtime_live) fn handle_transfer_material_quote(
        &mut self,
        request: TransferMaterialQuoteRequest,
    ) -> Result<TransferMaterialQuotePreflight, GameplayActionError> {
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "quote_transfer_material requires auth proof".to_string(),
            action_id: Some("quote_transfer_material".to_string()),
            target_agent_id: Some(request.requester_agent_id.clone()),
        })?;
        let verified =
            verify_transfer_material_quote_auth_proof(&request, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    action_id: Some("quote_transfer_material".to_string()),
                    target_agent_id: Some(request.requester_agent_id.clone()),
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_transfer_material".to_string()),
                target_agent_id: Some(request.requester_agent_id.clone()),
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            request.requester_agent_id.as_str(),
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some("quote_transfer_material".to_string()),
            target_agent_id: err.agent_id,
        })?;

        let from_ledger =
            MaterialLedgerId::try_from(request.from_ledger.clone()).map_err(|message| {
                GameplayActionError {
                    code: "transfer_material_quote_rejected".to_string(),
                    message: format!("quote_transfer_material rejected: {message}"),
                    action_id: Some("quote_transfer_material".to_string()),
                    target_agent_id: Some(request.requester_agent_id.clone()),
                }
            })?;
        let to_ledger =
            MaterialLedgerId::try_from(request.to_ledger.clone()).map_err(|message| {
                GameplayActionError {
                    code: "transfer_material_quote_rejected".to_string(),
                    message: format!("quote_transfer_material rejected: {message}"),
                    action_id: Some("quote_transfer_material".to_string()),
                    target_agent_id: Some(request.requester_agent_id.clone()),
                }
            })?;
        let requested_priority = request.requested_priority.map(|priority| match priority {
            TransferMaterialPriority::Urgent => MaterialTransitPriority::Urgent,
            TransferMaterialPriority::Standard => MaterialTransitPriority::Standard,
        });
        let quote = self
            .world
            .logistics_transfer_quote_with_route_id(
                request.requester_agent_id.as_str(),
                &from_ledger,
                &to_ledger,
                request.kind.as_str(),
                request.amount,
                request.distance_km,
                requested_priority,
                request.route_id.as_deref(),
                request.route_ids.as_slice(),
                request.auto_reroute,
            )
            .map_err(|reason| GameplayActionError {
                code: "transfer_material_quote_rejected".to_string(),
                message: format!("quote_transfer_material rejected: {reason:?}"),
                action_id: Some("quote_transfer_material".to_string()),
                target_agent_id: Some(request.requester_agent_id.clone()),
            })?;

        Ok(TransferMaterialQuotePreflight {
            requester_agent_id: quote.requester_agent_id,
            from_ledger: quote.from_ledger.to_string(),
            to_ledger: quote.to_ledger.to_string(),
            kind: quote.kind,
            requested_amount: quote.requested_amount,
            submission_feasible: quote.submission_feasible,
            max_transferable_amount: quote.max_transferable_amount,
            sent_amount: quote.sent_amount,
            distance_km: quote.distance_km,
            loss_bps: quote.loss_bps,
            expected_loss_amount: quote.expected_loss_amount,
            expected_received_amount: quote.expected_received_amount,
            source_amount_before: quote.source_amount_before,
            source_amount_after: quote.source_amount_after,
            destination_amount_before: quote.destination_amount_before,
            destination_expected_amount_after: quote.destination_expected_amount_after,
            ticks_until_arrival: quote.ticks_until_arrival,
            ready_at: quote.ready_at,
            effective_priority: match quote.effective_priority {
                MaterialTransitPriority::Urgent => TransferMaterialPriority::Urgent,
                MaterialTransitPriority::Standard => TransferMaterialPriority::Standard,
            },
            priority_reason: quote.priority_reason,
            inflight_before: quote.inflight_before,
            inflight_capacity: quote.inflight_capacity,
            path_id: quote.path_id,
            route_ids: quote.route_ids,
            tariff_electricity_total: quote.tariff_electricity_total,
            reroute_count: quote.reroute_count,
            recommendation: quote.recommendation,
            conditional: quote.conditional,
        })
    }

    pub(in crate::viewer::runtime_live) fn transfer_quote(
        &mut self,
        request: TransferMaterialQuoteRequest,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        send_response(
            writer,
            &self
                .handle_transfer_material_quote(request)
                .map(|quote| ViewerResponse::TransferMaterialQuotePreflight { quote })
                .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
        )
    }
}
