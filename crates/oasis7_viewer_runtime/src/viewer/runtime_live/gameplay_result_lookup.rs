//! Restore original-result reads after the browser and Viewer both restart.
use super::gameplay_feedback_scope::GameplayFeedbackOrigin;
use super::*;
use crate::viewer::GameplayActionRequest;
use crate::world_service::client::WorldServicePort;
use oasis7_client_api::world_service::{
    IntentOutcome, LookupIntentRequest, WORLD_SERVICE_CONTRACT_VERSION,
};

pub(super) struct PreparedGameplayLookup {
    original: GameplayActionRequest,
    origin: Option<GameplayFeedbackOrigin>,
    status: &'static str,
    receipt: Option<serde_json::Value>,
}

impl ViewerRuntimeLiveServer {
    fn gameplay_lookup_origin(
        &mut self,
        request: &GameplayActionRequest,
        session: &RuntimeLiveSession,
    ) -> Option<GameplayFeedbackOrigin> {
        if !session
            .negotiated_protocol
            .capabilities
            .iter()
            .any(|c| c == "gameplay_result_lookup_v1")
        {
            return None;
        }
        let auth = request.auth.as_ref()?;
        let verified = crate::viewer::verify_gameplay_action_auth_proof(request, auth).ok()?;
        self.session_policy
            .validate_known_session_key(&verified.player_id, &verified.public_key)
            .ok()?;
        let origin = self.new_gameplay_feedback_origin(&verified)?;
        self.gameplay_feedback_session_matches(session, Some(&origin))
            .then_some(origin)
    }

    pub(super) fn prepare_shared_gameplay_lookup(
        shared: &Arc<Mutex<Self>>,
        request: &ViewerRequest,
        session: &RuntimeLiveSession,
    ) -> Option<PreparedGameplayLookup> {
        let ViewerRequest::LookupGameplayAction { request } = request else {
            return None;
        };
        let mut prepared = PreparedGameplayLookup {
            original: request.clone(),
            origin: None,
            status: "unauthorized",
            receipt: None,
        };
        let (client, origin) = {
            let mut server = lock_shared_server(shared).ok()?;
            let Some(origin) = server.gameplay_lookup_origin(request, session) else {
                return Some(prepared);
            };
            let Ok(Some(client)) = server.world_service_client() else {
                prepared.status = "unavailable";
                return Some(prepared);
            };
            (client, origin)
        };
        prepared.origin = Some(origin);
        // Exact original bytes and correlation, using the existing verified
        // client. No admission/nonce consumption, submit or Viewer mutex here.
        let result = (|| {
            let payload = crate::world_service::WorldServicePayloadV1::GameplayJson(
                serde_json::to_vec(request).map_err(|e| e.to_string())?,
            );
            let correlation = crate::world_service::derive_correlation(
                client.config().expected_world.clone(),
                &payload,
            )?;
            client
                .lookup(
                    LookupIntentRequest {
                        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                        key: correlation.key,
                    },
                    payload,
                )
                .map_err(|e| e.to_string())
        })();
        prepared.status = match result {
            Ok(response) => match response.outcome {
                IntentOutcome::Committed { receipt, .. } => {
                    prepared.receipt = Some(receipt);
                    "committed"
                }
                IntentOutcome::Rejected { .. } => "rejected",
                IntentOutcome::Expired => "expired",
                IntentOutcome::Received { .. } => "received",
                IntentOutcome::Pending => "pending",
                IntentOutcome::Unknown => "unknown",
                IntentOutcome::HistoryUnavailable => "history_unavailable",
            },
            Err(_) => "unavailable",
        };
        Some(prepared)
    }

    pub(super) fn handle_gameplay_lookup(
        &mut self,
        request: GameplayActionRequest,
        session: &RuntimeLiveSession,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let (status, receipt) = self
            .prepared_gameplay_lookup
            .take()
            .filter(|prepared| {
                serde_json::to_value(&prepared.original).ok() == serde_json::to_value(&request).ok()
            })
            .map(|prepared| {
                if self.gameplay_feedback_session_matches(session, prepared.origin.as_ref()) {
                    (prepared.status, prepared.receipt)
                } else {
                    ("unauthorized", None)
                }
            })
            .unwrap_or(("unavailable", None));
        send_response(
            writer,
            &ViewerResponse::GameplayActionResult {
                action_id: request.action_id,
                target_agent_id: request.target_agent_id,
                player_id: request.player_id,
                request_auth_nonce: request.auth.map(|auth| auth.nonce),
                status: status.into(),
                receipt,
            },
        )
    }
}
