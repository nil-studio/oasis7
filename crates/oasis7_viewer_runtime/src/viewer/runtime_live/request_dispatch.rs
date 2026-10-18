//! Dispatches one Viewer protocol request within its connection session.
use super::*;

impl ViewerRuntimeLiveServer {
    pub(super) fn handle_request(
        &mut self,
        request: ViewerRequest,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        self.handle_request_with_chain_prime(request, session, writer, None)
    }

    pub(super) fn handle_request_with_chain_prime(
        &mut self,
        request: ViewerRequest,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
        mut chain_prime: Option<Result<(), ViewerRuntimeLiveServerError>>,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        self.resolve_authoritative_recovery_write_fence()?;
        if self.config.world_service.is_some()
            && matches!(
                &request,
                ViewerRequest::AuthoritativeRecovery { .. }
                    | ViewerRequest::PlaybackControl { .. }
                    | ViewerRequest::LiveControl { .. }
                    | ViewerRequest::Control { .. }
            )
            && let Some(Err(error)) = chain_prime.take()
        {
            return Err(error);
        }
        if matches!(&request, ViewerRequest::QuoteRevokeSocialFact { .. })
            && !viewer_protocol_supports_revoke_social_fact_quote(&session.negotiated_protocol)
        {
            send_response(
                writer,
                &ViewerResponse::GameplayActionError {
                    error: GameplayActionError {
                        code: "protocol_upgrade_required".to_string(),
                        message: format!(
                            "quote_revoke_social_fact requires Viewer capability {}",
                            REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY
                        ),
                        action_id: Some("quote_revoke_social_fact".to_string()),
                        target_agent_id: None,
                    },
                },
            )?;
            return Ok(());
        }
        if self.authoritative_recovery_write_fence.is_some()
            && !matches!(
                &request,
                ViewerRequest::Hello { .. }
                    | ViewerRequest::HelloV2 { .. }
                    | ViewerRequest::Subscribe { .. }
                    | ViewerRequest::RequestSnapshot
                    | ViewerRequest::RequestScopeSnapshot { .. }
                    | ViewerRequest::RequestWorldFeed { .. }
                    | ViewerRequest::RequestWorldView { .. }
            )
        {
            return Err(ViewerRuntimeLiveServerError::Init(
                "authoritative recovery commit status is unknown; runtime is read-only until durable generation readback resolves"
                    .to_string(),
            ));
        }
        self.invalidate_owner_read_session(session);
        match request {
            ViewerRequest::RequestCanonicalAgentOwnerReadContext { agent_id } => {
                self.handle_owner_read_context(agent_id, session, writer)?;
            }
            ViewerRequest::CanonicalAgentOwnerRead { request } => {
                self.handle_owner_read(request, session, writer)?;
            }
            ViewerRequest::Hello { version: _, .. } => {
                session.negotiated_protocol =
                    crate::viewer::protocol::NegotiatedViewerProtocol::v1_without_capabilities();
                let capabilities = Vec::new();
                send_response(
                    writer,
                    &ViewerResponse::HelloAck {
                        server: "oasis7".to_string(),
                        version: VIEWER_PROTOCOL_VERSION,
                        min_version: 1,
                        max_version: VIEWER_PROTOCOL_VERSION,
                        capabilities,
                        world_id: self.config.world_id.clone(),
                        control_profile: ViewerControlProfile::Live,
                        authority_epoch: None,
                    },
                )?;
            }
            ViewerRequest::HelloV2 {
                version,
                capabilities: offered,
                ..
            } => {
                let mut selected = Vec::new();
                let hosted_local_mock_capability_ready = if version >= 2
                    && self.hosted_local_mock_test_lane_active
                    && self.chain_link_enabled()
                    && self.world.state().agents.is_empty()
                    && !session.chain_runtime_authoritatively_primed
                {
                    let prime_result = chain_prime.take().unwrap_or_else(|| {
                        self.prime_chain_linked_runtime_for_snapshot().map(|_| ())
                    });
                    match prime_result {
                        Ok(()) => {
                            session.chain_runtime_authoritatively_primed = true;
                        }
                        Err(error) => emit_stderr_or_event(
                            Level::WARN,
                            format!(
                                "viewer runtime live: Hosted local-mock HelloV2 prime skipped: {error:?}"
                            )
                            .as_str(),
                            "viewer runtime live Hosted local-mock HelloV2 prime skipped",
                        ),
                    }
                    !self.world.state().agents.is_empty()
                        && !self.world.capability_invocation_contexts().is_empty()
                } else {
                    !self.hosted_local_mock_test_lane_active
                        || (!self.world.state().agents.is_empty()
                            && !self.world.capability_invocation_contexts().is_empty())
                };
                if version >= 2 {
                    if offered.iter().any(|capability| capability == "world_view") {
                        selected.push("world_view".to_string());
                        for capability in [
                            "world_view_delta_v1",
                            "world_view_aggregate_v1",
                            "world_view_event_anchor_v1",
                        ] {
                            if offered.iter().any(|offered| offered == capability) {
                                selected.push(capability.to_string());
                            }
                        }
                        if offered
                            .iter()
                            .any(|capability| capability == "world_view_pins_v1")
                        {
                            selected.push("world_view_pins_v1".to_string());
                        }
                    }
                    if self.authoritative_recovery_dir().is_some()
                        && offered.iter().any(|capability| {
                            capability
                                == crate::viewer::protocol::GOVERNED_ROLLBACK_REPLAY_CAPABILITY
                        })
                    {
                        selected.push(
                            crate::viewer::protocol::GOVERNED_ROLLBACK_REPLAY_CAPABILITY
                                .to_string(),
                        );
                    }
                    if self.config.world_service.is_some()
                        && offered
                            .iter()
                            .any(|capability| capability == "canonical_agent_chat_v1")
                    {
                        selected.push("canonical_agent_chat_v1".into());
                    }
                    if offered
                        .iter()
                        .any(|capability| capability == REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY)
                    {
                        selected.push(REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY.to_string());
                    }
                    if hosted_local_mock_capability_ready
                        && self.llm_sidecar.supports_prompt_control_result()
                        && offered
                            .iter()
                            .any(|capability| capability == PROMPT_CONTROL_RESULT_CAPABILITY)
                    {
                        selected.push(PROMPT_CONTROL_RESULT_CAPABILITY.to_string());
                    }
                }
                session.negotiated_protocol = crate::viewer::protocol::NegotiatedViewerProtocol {
                    version,
                    capabilities: selected.clone(),
                };
                send_response(
                    writer,
                    &ViewerResponse::HelloAck {
                        server: "oasis7".to_string(),
                        version: VIEWER_PROTOCOL_VERSION,
                        min_version: 1,
                        max_version: VIEWER_PROTOCOL_VERSION,
                        capabilities: selected,
                        world_id: self.config.world_id.clone(),
                        control_profile: ViewerControlProfile::Live,
                        authority_epoch: if viewer_protocol_supports_prompt_control_result(
                            &session.negotiated_protocol,
                        ) {
                            Some(self.prompt_control_authority.authority_epoch.clone())
                        } else {
                            None
                        },
                    },
                )?;
            }
            ViewerRequest::Subscribe {
                streams,
                event_kinds,
            } => {
                session.subscribed = streams.into_iter().collect();
                session.event_filters = if event_kinds.is_empty() {
                    None
                } else {
                    Some(event_kinds.into_iter().collect())
                };
            }
            ViewerRequest::RequestSnapshot => {
                // HelloV2 may already have authoritatively primed this
                // chain-linked server. Reusing that validated projection
                // avoids a second status read racing the first snapshot.
                if self.chain_link_enabled()
                    && !session.initial_snapshot_sent
                    && !session.chain_runtime_authoritatively_primed
                {
                    let prime_result = chain_prime.take().unwrap_or_else(|| {
                        self.prime_chain_linked_runtime_for_snapshot().map(|_| ())
                    });
                    match prime_result {
                        Ok(()) => {
                            session.chain_runtime_authoritatively_primed = true;
                        }
                        Err(err) => {
                            if self.config.world_service.is_some()
                                || self.config.chain_link_policy == ChainLinkPolicy::Enforcing
                            {
                                return Err(err);
                            }
                            emit_stderr_or_event(
                                Level::WARN,
                                format!(
                                    "viewer runtime live: initial chain sync skipped before snapshot: {err:?}"
                                )
                                .as_str(),
                                "viewer runtime live initial chain sync skipped",
                            );
                        }
                    }
                }
                if session.wants_initial_snapshot() {
                    let snapshot = self.compat_snapshot_for_session(session);
                    send_response(writer, &ViewerResponse::Snapshot { snapshot })?;
                }
                if session.wants_initial_recovery_metadata() {
                    let cursor = self.current_recovery_cursor()?;
                    send_response(
                        writer,
                        &ViewerResponse::AuthoritativeRecoveryAck {
                            ack: AuthoritativeRecoveryAck {
                                status: AuthoritativeRecoveryStatus::CatchUpReady,
                                reorg_epoch: self.reorg_epoch,
                                snapshot_height: cursor.snapshot_height,
                                snapshot_hash: cursor.snapshot_hash,
                                log_cursor: cursor.log_cursor,
                                stable_batch_id: cursor.stable_batch_id,
                                player_id: None,
                                agent_id: None,
                                session_pubkey: None,
                                replaced_by_pubkey: None,
                                session_epoch: None,
                                binding_epoch: None,
                                message: Some("snapshot_sync_metadata".to_string()),
                                revoke_reason: None,
                                revoked_by: None,
                                rollback_receipt: None,
                                acknowledged_at_tick: self.world.state().time,
                            },
                        },
                    )?;
                }
                if session.explicitly_subscribed_to(ViewerStream::Metrics) {
                    session.metrics = runtime_metrics(&self.world);
                    send_response(
                        writer,
                        &ViewerResponse::Metrics {
                            time: Some(self.world.state().time),
                            metrics: session.metrics.clone(),
                        },
                    )?;
                }
                if session.explicitly_subscribed_to(ViewerStream::Events) {
                    self.emit_authoritative_batch_snapshot(writer)?;
                    self.emit_authoritative_challenge_snapshot(writer)?;
                }
                session.initial_snapshot_sent = true;
                self.enable_auto_play_for_session_if_available(session);
            }
            ViewerRequest::RequestScopeSnapshot { request_generation } => {
                if request_generation.is_empty() || request_generation.len() > 64 {
                    send_response(
                        writer,
                        &ViewerResponse::Error {
                            message: "invalid_scope_snapshot_generation".into(),
                        },
                    )?;
                } else {
                    send_response(
                        writer,
                        &ViewerResponse::ScopeSnapshot {
                            request_generation,
                            snapshot: self.compat_snapshot_for_session(session),
                        },
                    )?;
                }
            }
            ViewerRequest::RequestWorldView { query } => {
                let supports = |capability: &str| {
                    session
                        .negotiated_protocol
                        .capabilities
                        .iter()
                        .any(|selected| selected == capability)
                };
                let delta_enabled = supports("world_view_delta_v1");
                let unnegotiated = (query.get("resume_cursor").is_some() && !delta_enabled)
                    || (query.get("detail_level").is_some()
                        && !supports("world_view_aggregate_v1"))
                    || (query.get("include_events").is_some()
                        && !supports("world_view_event_anchor_v1"));
                if unnegotiated {
                    let view = serde_json::json!({"world_id":self.config.world_id,"request_generation":query.get("request_generation"),"completeness":"unknown","entities":[],"unavailable_reasons":["capability_unavailable"]});
                    send_response(writer, &ViewerResponse::WorldView { view })?;
                    return Ok(());
                }
                if (self.config.world_service.is_some() && self.verified_world_view.is_none())
                    || (self.chain_link_enabled()
                        && !session.chain_runtime_authoritatively_primed
                        && !session.initial_snapshot_sent)
                {
                    session.world_view_projection = None;
                    let view = serde_json::json!({"world_id":self.config.world_id,"request_generation":query.get("request_generation"),"completeness":"unknown","entities":[],"unavailable_reasons":["source_unavailable"]});
                    send_response(writer, &ViewerResponse::WorldView { view })?;
                    return Ok(());
                }
                let mut cache = session.world_view_projection.take();
                let world_id = self.config.world_id.clone();
                let audience = session
                    .current_player_id
                    .clone()
                    .unwrap_or_else(|| "public".into());
                let epoch = self.reorg_epoch;
                let authority = self.world_view_source_authority(&audience);
                let view = crate::viewer::world_view::build_session_evolving(
                    &mut cache,
                    || self.compat_snapshot_for_session(session),
                    (&world_id, epoch, &audience),
                    query,
                    authority,
                    delta_enabled,
                );
                session.world_view_projection = cache;
                send_response(writer, &ViewerResponse::WorldView { view })?;
            }
            ViewerRequest::RequestWorldFeed { cursor, limit } => {
                let feed = world_feed::build_world_feed(
                    &self.config.world_id,
                    self.reorg_epoch,
                    self.world.journal(),
                    &self.world.state().crises,
                    cursor.as_deref(),
                    limit,
                    self.config.major_world_event_visibility,
                );
                send_response(writer, &ViewerResponse::WorldFeed { feed })?;
            }
            ViewerRequest::PlaybackControl { mode, request_id } => {
                self.apply_control_mode(ViewerControl::from(mode), request_id, session, writer)?;
            }
            ViewerRequest::LiveControl { mode, request_id } => {
                self.apply_control_mode(ViewerControl::from(mode), request_id, session, writer)?;
            }
            ViewerRequest::Control { mode, request_id } => {
                self.apply_control_mode(mode, request_id, session, writer)?;
            }
            ViewerRequest::PromptControl { command } => {
                let feedback_request = matches!(
                    command.as_ref(),
                    crate::viewer::PromptControlCommand::Apply { .. }
                        | crate::viewer::PromptControlCommand::Rollback { .. }
                );
                if feedback_request {
                    let origin =
                        self.gameplay_request_feedback_origin(&ViewerRequest::PromptControl {
                            command: command.clone(),
                        });
                    self.active_gameplay_feedback_origin =
                        self.begin_gameplay_feedback_request(session, origin);
                    self.active_gameplay_feedback_authorized = false;
                }
                let result =
                    self.handle_prompt_control_for_protocol(*command, &session.negotiated_protocol);
                if feedback_request {
                    self.finish_gameplay_feedback_request(session);
                }
                match result {
                    Ok(ack) => {
                        send_response(writer, &ViewerResponse::PromptControlAck { ack })?;
                    }
                    Err(error) => {
                        send_response(writer, &ViewerResponse::PromptControlError { error })?;
                    }
                }
            }
            ViewerRequest::AgentChat { request } => {
                let origin = self.gameplay_request_feedback_origin(&ViewerRequest::AgentChat {
                    request: request.clone(),
                });
                self.active_gameplay_feedback_origin =
                    self.begin_gameplay_feedback_request(session, origin);
                self.active_gameplay_feedback_authorized = false;
                let result = self.handle_agent_chat(request);
                self.finish_gameplay_feedback_request(session);
                match result {
                    Ok(ack) => {
                        send_response(writer, &ViewerResponse::AgentChatAck { ack })?;
                        let provider_errors = self.enqueue_pending_provider_agent_chat_replies();
                        for error in provider_errors {
                            send_response(writer, &ViewerResponse::AgentChatError { error })?;
                        }
                        self.flush_pending_virtual_events(session, writer)?;
                    }
                    Err(error) => {
                        send_response(writer, &ViewerResponse::AgentChatError { error })?;
                    }
                }
            }
            ViewerRequest::GameplayAction { request } => {
                let origin =
                    self.gameplay_request_feedback_origin(&ViewerRequest::GameplayAction {
                        request: request.clone(),
                    });
                let origin = self.begin_gameplay_feedback_request(session, origin);
                self.active_gameplay_feedback_authorized = self.config.world_service.is_some()
                    && self.gameplay_feedback_session_matches(session, origin.as_ref());
                self.active_gameplay_feedback_origin = origin.clone();
                let result = self.handle_gameplay_action(request);
                let origin = self.finish_gameplay_feedback_request(session);
                match result {
                    Ok(ack) => {
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
            }
            ViewerRequest::CollectData { command } => {
                self.handle_collect_data_protocol_request(command, session, writer)?;
            }
            ViewerRequest::QuoteRefineCompound { request } => send_response(
                writer,
                &self
                    .handle_refine_quote(request)
                    .map(|quote| ViewerResponse::RefineQuotePreflight { quote })
                    .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
            )?,
            ViewerRequest::QuoteScheduleRecipe { request } => send_response(
                writer,
                &self
                    .handle_schedule_recipe_quote(request)
                    .map(|quote| ViewerResponse::ScheduleRecipeQuotePreflight { quote })
                    .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
            )?,
            ViewerRequest::QuoteProductValidation { request } => send_response(
                writer,
                &self
                    .handle_product_validation_quote(request)
                    .map(|quote| ViewerResponse::ProductValidationQuotePreflight { quote })
                    .unwrap_or_else(|error| ViewerResponse::GameplayActionError { error }),
            )?,
            ViewerRequest::QuotePowerSurvival { request } => self.quote_power(request, writer)?,
            ViewerRequest::QuotePowerSale { request } => self.quote_power_sale(request, writer)?,
            request @ (ViewerRequest::QuoteDeclareSocialEdge { .. }
            | ViewerRequest::QuotePublishSocialFact { .. }
            | ViewerRequest::QuoteAdjudicateSocialFact { .. }
            | ViewerRequest::QuoteRevokeSocialFact { .. }
            | ViewerRequest::QuoteSocialContact { .. }
            | ViewerRequest::QuoteGovernanceVote { .. }
            | ViewerRequest::QuoteDeclareWar { .. }) => {
                self.handle_social_quote_request(request, writer)?
            }
            ViewerRequest::PreviewFragmentReplenishment { request } => {
                self.preview_refill(request, writer)?
            }
            ViewerRequest::QuoteMarketDecision { request } => {
                self.handle_market_quote_decision_request(request, writer)?
            }
            ViewerRequest::QuoteTransferMaterial { request } => {
                self.transfer_quote(request, writer)?
            }
            ViewerRequest::AuthoritativeChallenge { command } => {
                match self.handle_authoritative_challenge(command) {
                    Ok((ack, maybe_batch_update)) => {
                        send_response(writer, &ViewerResponse::AuthoritativeChallengeAck { ack })?;
                        if let Some(batch) = maybe_batch_update {
                            send_response(writer, &ViewerResponse::AuthoritativeBatch { batch })?;
                        }
                    }
                    Err(error) => {
                        send_response(
                            writer,
                            &ViewerResponse::AuthoritativeChallengeError { error },
                        )?;
                    }
                }
            }
            ViewerRequest::AuthoritativeRecovery { command } => {
                match self.handle_authoritative_recovery_for_protocol(
                    command,
                    &session.negotiated_protocol,
                ) {
                    Ok((ack, emit_snapshot_after_ack)) => {
                        let ack_player_id = ack.player_id.clone();
                        let ack_status = ack.status;
                        send_response(writer, &ViewerResponse::AuthoritativeRecoveryAck { ack })?;
                        if let Some(player_id) = ack_player_id.as_deref() {
                            // Legacy reconnect accepts an asserted player ID,
                            // not a fresh signed proof for private feedback.
                            self.clear_session_gameplay_feedback(session);
                            match ack_status {
                                AuthoritativeRecoveryStatus::SessionRevoked => {
                                    if session.current_player_id.as_deref() == Some(player_id) {
                                        session.current_player_id = None;
                                        session.owner_read_context = None;
                                        session.owner_read_view = None;
                                        session.owner_read_commit = None;
                                    }
                                }
                                _ => session.current_player_id = Some(player_id.to_string()),
                            }
                        }
                        if emit_snapshot_after_ack {
                            let snapshot = self.compat_snapshot_for_session(session);
                            send_response(writer, &ViewerResponse::Snapshot { snapshot })?;
                            self.emit_authoritative_batch_snapshot(writer)?;
                            self.emit_authoritative_challenge_snapshot(writer)?;
                        }
                    }
                    Err(error) => {
                        send_response(
                            writer,
                            &ViewerResponse::AuthoritativeRecoveryError { error },
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}
