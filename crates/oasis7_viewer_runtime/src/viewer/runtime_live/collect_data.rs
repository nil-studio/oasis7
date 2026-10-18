use super::super::auth::{VerifiedPlayerAuth, verify_collect_data_auth_proof};
use super::super::protocol::{CollectDataCommand, GameplayActionAck, GameplayActionError};
use super::control_plane::{
    ensure_agent_player_access_runtime, map_auth_verify_error_code, normalize_optional_public_key,
};
use super::player_gameplay::{CollectDataResult, data_collection_preflight};
use super::*;
use crate::runtime::Action as RuntimeAction;
use oasis7_agent_api::world_action::ResourceKind;

impl ViewerRuntimeLiveServer {
    pub(super) fn handle_collect_data_protocol_request(
        &mut self,
        command: CollectDataCommand,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let origin = self.gameplay_request_feedback_origin(&ViewerRequest::CollectData {
            command: command.clone(),
        });
        let origin = self.begin_gameplay_feedback_request(session, origin);
        self.active_gameplay_feedback_authorized = self.config.world_service.is_some()
            && matches!(command, CollectDataCommand::Submit { .. })
            && self.gameplay_feedback_session_matches(session, origin.as_ref());
        self.active_gameplay_feedback_origin = origin.clone();
        let result = self.handle_collect_data(command);
        let origin = self.finish_gameplay_feedback_request(session);
        match result {
            Ok(CollectDataResult::Preflight(quote)) => {
                send_response(writer, &ViewerResponse::CollectDataPreflight { quote })?;
            }
            Ok(CollectDataResult::Submit(ack)) => {
                let ack_player_id = ack.player_id.clone();
                send_response(writer, &ViewerResponse::GameplayActionAck { ack })?;
                if !ack_player_id.trim().is_empty() {
                    self.set_gameplay_feedback_session_player(session, ack_player_id);
                }
                if session.explicitly_subscribed_to(ViewerStream::Snapshot) {
                    let snapshot = self.compat_snapshot_for_session(session);
                    send_response(writer, &ViewerResponse::Snapshot { snapshot })?;
                }
            }
            Err(error) => {
                self.record_session_gameplay_rejection(session, &error, origin);
                send_response(writer, &ViewerResponse::GameplayActionError { error })?;
                if session.explicitly_subscribed_to(ViewerStream::Snapshot) {
                    let snapshot = self.compat_snapshot_for_session(session);
                    send_response(writer, &ViewerResponse::Snapshot { snapshot })?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn handle_collect_data(
        &mut self,
        command: CollectDataCommand,
    ) -> Result<CollectDataResult, GameplayActionError> {
        if self.config.world_service.is_some()
            && let CollectDataCommand::Submit { request } = &command
        {
            let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
                code: "auth_proof_required".into(),
                message: "collect_data requires auth proof".into(),
                action_id: Some("collect_data".into()),
                target_agent_id: None,
            })?;
            let verified = verify_collect_data_auth_proof(&command, auth).map_err(|message| {
                GameplayActionError {
                    code: map_auth_verify_error_code(&message).into(),
                    message,
                    action_id: Some("collect_data".into()),
                    target_agent_id: None,
                }
            })?;
            self.session_policy
                .validate_known_session_key(&verified.player_id, &verified.public_key)
                .map_err(|message| GameplayActionError {
                    code: map_session_policy_error_code(&message).into(),
                    message,
                    action_id: Some("collect_data".into()),
                    target_agent_id: None,
                })?;
            let (runtime_action_id, consensus_action_payload_hash) =
                self.submit_world_service_gameplay(&command)?;
            if runtime_action_id != 0 {
                self.runtime_action_players
                    .insert(runtime_action_id, verified.player_id.clone());
            }
            let target_agent_id = self
                .collect_data_quote_state()
                .ok()
                .and_then(|state| {
                    crate::world_service::gameplay::resolve_collect_data_collector(
                        state,
                        &verified.player_id,
                        &verified.public_key,
                    )
                    .ok()
                })
                .unwrap_or_default()
                .to_owned();
            return Ok(CollectDataResult::Submit(GameplayActionAck {
                consensus_action_payload_hash,
                action_id: "collect_data".into(),
                target_agent_id,
                player_id: verified.player_id,
                runtime_action_id,
                accepted_at_tick: self.world.state().time,
                message: Some("received by world service; await committed outcome".into()),
            }));
        }
        let (verified, collector_agent_id) = self.authorize_collect_data(&command)?;
        let request = match &command {
            CollectDataCommand::Preflight { request } | CollectDataCommand::Submit { request } => {
                request
            }
        };
        let available_electricity = self
            .collect_data_quote_state()?
            .agents
            .get(collector_agent_id.as_str())
            .map(|agent| agent.state.resources.get(ResourceKind::Electricity))
            .ok_or_else(|| GameplayActionError {
                code: "collector_agent_missing".to_string(),
                message: format!("bound collector Agent {collector_agent_id} is not in the world"),
                action_id: Some("collect_data".to_string()),
                target_agent_id: Some(collector_agent_id.clone()),
            })?;
        let quote = data_collection_preflight(
            collector_agent_id.clone(),
            available_electricity,
            request.electricity_cost,
            request.data_amount,
        );
        match &command {
            CollectDataCommand::Preflight { .. } => Ok(CollectDataResult::Preflight(quote)),
            CollectDataCommand::Submit { .. } => {
                if !quote.can_execute {
                    return Err(GameplayActionError {
                        code: "collect_data_preflight_blocked".to_string(),
                        message: quote
                            .blocked_reason
                            .clone()
                            .unwrap_or_else(|| "data collection is blocked".to_string()),
                        action_id: Some("collect_data".to_string()),
                        target_agent_id: Some(collector_agent_id),
                    });
                }
                if self.config.world_service.is_some() {
                    let (runtime_action_id, consensus_action_payload_hash) =
                        self.submit_world_service_gameplay(&command)?;
                    if runtime_action_id != 0 {
                        self.runtime_action_players
                            .insert(runtime_action_id, verified.player_id.clone());
                    }
                    return Ok(CollectDataResult::Submit(GameplayActionAck {
                        consensus_action_payload_hash,
                        action_id: "collect_data".into(),
                        target_agent_id: collector_agent_id,
                        player_id: verified.player_id,
                        runtime_action_id,
                        accepted_at_tick: self.world.state().time,
                        message: Some("received by world service; await committed outcome".into()),
                    }));
                }
                if let Some(chain_status_bind) = self
                    .config
                    .chain_status_bind
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    let chain_submit_bind = self
                        .config
                        .chain_submit_bind
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or(chain_status_bind)
                        .to_string();
                    let submitted = chain_link::submit_chain_linked_collect_data(
                        chain_submit_bind.as_str(),
                        &command,
                    )?;
                    let runtime_action_id = submitted.action_id.expect(
                        "chain collect_data submit must include action_id after ok=true validation",
                    );
                    self.runtime_action_players
                        .insert(runtime_action_id, verified.player_id.clone());
                    self.set_collect_data_submission_feedback(
                        &verified,
                        &collector_agent_id,
                        runtime_action_id,
                        "submitted",
                    );
                    return Ok(CollectDataResult::Submit(GameplayActionAck {
                        consensus_action_payload_hash: submitted.consensus_action_payload_hash,
                        action_id: "collect_data".to_string(),
                        target_agent_id: collector_agent_id,
                        player_id: verified.player_id,
                        runtime_action_id,
                        accepted_at_tick: self.world.state().time,
                        message: Some(
                            "submitted data collection to chain runtime; wait for committed world sync"
                                .to_string(),
                        ),
                    }));
                }
                let runtime_action_id = self.world.submit_action(RuntimeAction::CollectData {
                    collector_agent_id: collector_agent_id.clone(),
                    electricity_cost: request.electricity_cost,
                    data_amount: request.data_amount,
                });
                self.runtime_action_players
                    .insert(runtime_action_id, verified.player_id.clone());
                self.set_collect_data_submission_feedback(
                    &verified,
                    &collector_agent_id,
                    runtime_action_id,
                    "queued",
                );
                Ok(CollectDataResult::Submit(GameplayActionAck {
                    consensus_action_payload_hash: None,
                    action_id: "collect_data".to_string(),
                    target_agent_id: collector_agent_id,
                    player_id: verified.player_id,
                    runtime_action_id,
                    accepted_at_tick: self.world.state().time,
                    message: Some(
                        "advance 1-2 steps to apply the queued data collection".to_string(),
                    ),
                }))
            }
        }
    }

    fn set_collect_data_submission_feedback(
        &mut self,
        verified: &VerifiedPlayerAuth,
        collector_agent_id: &str,
        runtime_action_id: u64,
        stage: &str,
    ) {
        self.set_verified_player_gameplay_feedback_for_action(
            verified,
            Some(runtime_action_id),
            Self::make_player_gameplay_feedback(
                "gameplay_action:collect_data",
                stage,
                format!("{stage} data collection as runtime action {runtime_action_id}"),
                Some("collect data using the authorized collector".into()),
                Some(collector_agent_id.into()),
                None,
                Some("observe the matching committed data collection result".into()),
                0,
                0,
            ),
        );
    }

    fn authorize_collect_data(
        &mut self,
        command: &CollectDataCommand,
    ) -> Result<(VerifiedPlayerAuth, String), GameplayActionError> {
        self.ensure_gameplay_ready_for_action("collect_data", Some("collect_data"), None)
            .map_err(|(code, message)| GameplayActionError {
                code,
                message,
                action_id: Some("collect_data".to_string()),
                target_agent_id: None,
            })?;
        let request = match command {
            CollectDataCommand::Preflight { request } | CollectDataCommand::Submit { request } => {
                request
            }
        };
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "collect_data requires auth proof".to_string(),
            action_id: Some("collect_data".to_string()),
            target_agent_id: None,
        })?;
        let verified = verify_collect_data_auth_proof(command, auth).map_err(|message| {
            GameplayActionError {
                code: map_auth_verify_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("collect_data".to_string()),
                target_agent_id: None,
            }
        })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("collect_data".to_string()),
                target_agent_id: None,
            })?;
        self.llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
            .map_err(|message| GameplayActionError {
                code: "auth_nonce_replay".to_string(),
                message,
                action_id: Some("collect_data".to_string()),
                target_agent_id: None,
            })?;
        self.authorize_active_gameplay_feedback();
        if self.chain_link_enabled() {
            let collector = crate::world_service::gameplay::resolve_collect_data_collector(
                self.collect_data_quote_state()?,
                &verified.player_id,
                &verified.public_key,
            )
            .map_err(|error| GameplayActionError {
                code: error.code().into(),
                message: error.message().into(),
                action_id: Some("collect_data".into()),
                target_agent_id: None,
            })?
            .to_owned();
            let bound = self
                .llm_sidecar
                .bound_agent_for_player(&verified.player_id)
                .ok_or_else(|| GameplayActionError {
                    code: "player_agent_binding_required".into(),
                    message: "refresh committed world state to resolve this player's Agent binding"
                        .into(),
                    action_id: Some("collect_data".into()),
                    target_agent_id: Some(collector.clone()),
                })?;
            if bound != collector {
                return Err(GameplayActionError {
                    code: "collect_data_binding_mismatch".into(),
                    message: "canonical collector differs from the current player binding; refresh committed world state".into(),
                    action_id: Some("collect_data".into()),
                    target_agent_id: Some(collector),
                });
            }
            return Ok((verified, collector));
        }
        let collector_agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: "collect_data requires a bound player Agent session".to_string(),
                action_id: Some("collect_data".to_string()),
                target_agent_id: None,
            })?
            .to_string();
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            collector_agent_id.as_str(),
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some("collect_data".to_string()),
            target_agent_id: err.agent_id,
        })?;
        Ok((verified, collector_agent_id))
    }

    /// Admission and quote evaluation run under the same Viewer lock without
    /// I/O between them. Service mode reads only its immutable verified view;
    /// the disposable local World is not an authority fallback.
    fn collect_data_quote_state(&self) -> Result<&crate::runtime::WorldState, GameplayActionError> {
        if let Some(config) = &self.config.world_service {
            return self
                .verified_world_view
                .as_ref()
                .filter(|view| view.version().commit.world == config.expected_world)
                .map(|view| &view.projection().state)
                .ok_or_else(|| GameplayActionError {
                    code: "runtime_sync_unavailable".into(),
                    message: "collect_data requires an authenticated committed world view".into(),
                    action_id: Some("collect_data".into()),
                    target_agent_id: None,
                });
        }
        Ok(self.world.state())
    }
}
