use super::*;
use crate::world_service::{WorldServicePayloadV1, client::WorldServicePort};
use oasis7_client_api::world_service::{
    IntentOutcome, SubmitIntentRequest, SubmitObservation, WORLD_SERVICE_CONTRACT_VERSION,
};

impl ViewerRuntimeLiveServer {
    pub(super) fn handle_canonical_agent_chat(
        &mut self,
        request: AgentChatRequest,
    ) -> Result<AgentChatAck, AgentChatError> {
        let error = |code: &str, message: String| AgentChatError {
            code: code.into(),
            message,
            agent_id: Some(request.agent_id.clone()),
        };
        let proof = request.auth.as_ref().ok_or_else(|| {
            error(
                "auth_proof_required",
                "canonical owner signature required".into(),
            )
        })?;
        let verified = crate::viewer::verify_agent_chat_auth_proof(&request, proof)
            .map_err(|reason| error("auth_invalid", reason))?;
        self.session_policy
            .validate_known_session_key(&verified.player_id, &verified.public_key)
            .map_err(|reason| error("session_key_invalid", reason))?;
        if request.canonical_authority.is_none() {
            return Err(error(
                "canonical_authority_required",
                "signed branch and Agent identity generation required".into(),
            ));
        }
        let client = self
            .world_service_client()
            .map_err(|reason| error("world_service_unavailable", format!("{reason:?}")))?
            .ok_or_else(|| {
                error(
                    "world_service_unavailable",
                    "canonical chat requires WorldService".into(),
                )
            })?;
        let payload = WorldServicePayloadV1::AgentChat(request.clone());
        let correlation = crate::world_service::derive_correlation(
            client.config().expected_world.clone(),
            &payload,
        )
        .map_err(|reason| error("auth_invalid", reason))?;
        let origin = self.reuse_pending_gameplay_feedback_origin(
            &correlation,
            self.active_gameplay_feedback_origin.clone(),
        );
        self.active_gameplay_feedback_origin = origin.clone();
        let observation = if let Some(prepared) = self.prepared_world_service_submission.take() {
            if prepared.correlation != correlation {
                return Err(error(
                    "world_service_invalid_request",
                    "prepared chat correlation mismatch".into(),
                ));
            }
            if let Some(admission) = prepared.admission_error {
                return Err(error(&admission.code, admission.message));
            }
            prepared
                .outcome
                .map_err(|reason| error("world_service_unavailable", reason))?
        } else {
            client
                .submit(SubmitIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    correlation: correlation.clone(),
                    deadline_unix_ms: None,
                    signed_payload: payload.clone(),
                })
                .map_err(|reason| error("world_service_unavailable", reason.to_string()))?
        };
        let disposition = match observation {
            SubmitObservation::OutcomeUnknown(_) => {
                self.pending_gameplay_feedback_origin(&correlation, origin);
                self.remember_pending_world_service_gameplay(correlation, payload);
                return Err(error(
                    "world_service_outcome_unknown",
                    "query the original signed request; do not change its nonce".into(),
                ));
            }
            SubmitObservation::Response(response) => match response.outcome {
                IntentOutcome::Committed { receipt, .. } => Some(
                    serde_json::from_value::<crate::runtime::AgentIntentReplayDisposition>(receipt)
                        .map_err(|reason| {
                            error("world_service_invalid_receipt", reason.to_string())
                        })?,
                ),
                IntentOutcome::Received { .. } | IntentOutcome::Pending => {
                    self.pending_gameplay_feedback_origin(&correlation, origin);
                    self.remember_pending_world_service_gameplay(correlation, payload);
                    None
                }
                _ => {
                    return Err(error(
                        "canonical_intent_unresolved",
                        "original canonical chat result is not committed".into(),
                    ));
                }
            },
        };
        // Transport receipt is not Runtime acceptance. Provider goals are
        // restored only by the authenticated committed Agent view on sync.
        Ok(AgentChatAck {
            auth_nonce: Some(verified.nonce),
            agent_id: request.agent_id,
            accepted_at_tick: disposition.as_ref().map_or(0, |value| value.logical_time),
            message_len: request.message.chars().count(),
            player_id: Some(verified.player_id),
            intent_tick: request.intent_tick,
            intent_seq: request.intent_seq,
            idempotent_replay: disposition.is_some(),
            intent_id: disposition.as_ref().map(|value| value.intent_id.clone()),
            accepted_event_seq: disposition.as_ref().map(|value| value.event_seq),
            status: Some(
                disposition
                    .as_ref()
                    .map_or("pending", |value| value.status.as_str())
                    .into(),
            ),
            receipt_ref: disposition
                .as_ref()
                .and_then(|value| value.receipt_ref.clone()),
            replaced_by: disposition.and_then(|value| value.replaced_by),
        })
    }
}
