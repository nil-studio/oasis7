use super::*;
use crate::runtime::{AgentIntentV2, WorldEventBody as RuntimeWorldEventBody};
use crate::simulator::WorldKernel;
use oasis7_simulator::ChunkRuntimeConfig;

fn canonical_player_intent_status(status: &str) -> Option<&'static str> {
    match status.trim() {
        "proposed" => Some("proposed"),
        "submitted" => Some("submitted"),
        "accepted" | "accepted_new" | "reprioritized" | "unchanged" => Some("accepted"),
        "blocked" | "resume_required" => Some("blocked"),
        "completed" => Some("completed"),
        "rejected" => Some("rejected"),
        "expired" => Some("expired"),
        "cancelled" => Some("cancelled"),
        "superseded" => Some("superseded"),
        _ => None,
    }
}

/// Project a runtime receipt only when the runtime has committed the exact
/// receipt event referenced by a completed AgentIntentV2. The player-facing
/// contract never exposes the runtime's opaque string as proof.
fn committed_receipt_tuple(
    intent: &AgentIntentV2,
    journal: &crate::runtime::Journal,
) -> Option<serde_json::Value> {
    if intent.status != "completed" {
        return None;
    }
    let effect_intent_id = intent
        .effect_intent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let raw_receipt_ref = intent.receipt_ref.as_deref()?.trim();
    let receipt_event_id = raw_receipt_ref
        .strip_prefix("world-event:")
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|event_id| *event_id > 0)?;
    let world_id = intent
        .world_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let reorg_epoch = intent.reorg_epoch?;
    let receipt_event = journal
        .events
        .iter()
        .find(|event| event.id == receipt_event_id && event.id < intent.event_seq)?;
    let RuntimeWorldEventBody::ReceiptAppended(receipt) = &receipt_event.body else {
        return None;
    };
    if receipt.identity_key() != effect_intent_id || receipt.status.trim().is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "intent_id": intent.intent_id,
        "world_id": world_id,
        "reorg_epoch": reorg_epoch,
        "logical_time": intent.logical_time,
        "event_seq": intent.event_seq.to_string(),
        "receipt_id": raw_receipt_ref,
    }))
}

impl ViewerRuntimeLiveServer {
    #[expect(
        clippy::too_many_arguments,
        reason = "Player-facing blocked feedback keeps its stable protocol fields explicit"
    )]
    pub(super) fn tolerate_background_play_gameplay_block(
        &mut self,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
        action: &str,
        play_step_interval: Duration,
        effect: &str,
        reason: String,
        delta_logical_time: u64,
        delta_event_seq: u64,
    ) -> Result<bool, ViewerRuntimeLiveServerError> {
        let confirmed_runtime_progress = self.world.state().time > self.initial_world_time;
        let is_background_play = action == "play" && session.playing;
        if !is_background_play || !confirmed_runtime_progress {
            return Ok(false);
        }
        session.transient_play_failures = session.transient_play_failures.saturating_add(1);
        if session.transient_play_failures >= BACKGROUND_PLAY_TRANSIENT_FAILURE_BUDGET {
            return Ok(false);
        }
        self.defer_next_auto_play_step_after_completion(
            play_step_interval.max(BACKGROUND_PLAY_TRANSIENT_FAILURE_RETRY_DELAY),
        );
        let hint = Self::llm_gameplay_hint_for_reason(&reason);
        self.set_latest_player_gameplay_feedback(Self::make_player_gameplay_feedback(
            action,
            "blocked",
            effect,
            Some("continue advancing the live world".to_string()),
            None,
            Some(reason),
            Some(hint),
            delta_logical_time,
            delta_event_seq,
        ));
        if session.explicitly_subscribed_to(ViewerStream::Snapshot) {
            let snapshot = self.compat_snapshot_for_session(session);
            send_response(writer, &ViewerResponse::Snapshot { snapshot })?;
        }
        Ok(true)
    }

    pub(super) fn flush_pending_virtual_events(
        &mut self,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        if self.pending_virtual_events.is_empty() {
            return Ok(());
        }
        let mapped_events: Vec<_> = self.pending_virtual_events.drain(..).collect();
        let pending_batch = self.register_authoritative_batch(mapped_events.as_slice())?;
        let batch_finality_updates =
            self.advance_authoritative_batch_finality(self.world.state().time)?;

        if session.explicitly_subscribed_to(ViewerStream::Events) {
            for event in &mapped_events {
                if session.event_allowed(event) {
                    send_response(
                        writer,
                        &ViewerResponse::Event {
                            event: event.clone(),
                        },
                    )?;
                }
            }
            send_response(
                writer,
                &ViewerResponse::AuthoritativeBatch {
                    batch: pending_batch,
                },
            )?;
            for batch in batch_finality_updates {
                send_response(writer, &ViewerResponse::AuthoritativeBatch { batch })?;
            }
        }

        if session.explicitly_subscribed_to(ViewerStream::Snapshot) {
            let snapshot = self.compat_snapshot_for_session(session);
            send_response(writer, &ViewerResponse::Snapshot { snapshot })?;
        }

        session.metrics = runtime_metrics(&self.world);
        if session.explicitly_subscribed_to(ViewerStream::Metrics) {
            send_response(
                writer,
                &ViewerResponse::Metrics {
                    time: Some(self.world.state().time),
                    metrics: session.metrics.clone(),
                },
            )?;
        }

        Ok(())
    }

    pub(super) fn compat_snapshot(&mut self, current_player_id: Option<&str>) -> WorldSnapshot {
        self.prune_scoped_gameplay_feedback();
        let feedback = self.scoped_feedback_for_player(current_player_id);
        self.compat_snapshot_with_feedback(current_player_id, feedback.as_ref())
    }

    pub(super) fn compat_snapshot_with_feedback(
        &mut self,
        current_player_id: Option<&str>,
        request_feedback: Option<&PlayerGameplayRecentFeedback>,
    ) -> WorldSnapshot {
        let mut runtime_snapshot = self.world.snapshot();
        super::snapshot_privacy::omit_private_canonical_chat(&mut runtime_snapshot);
        let runtime_state = &runtime_snapshot.state;
        let runtime_journal_len = runtime_snapshot.journal_len;
        let next_event_id = runtime_snapshot.last_event_id.saturating_add(1).max(1);
        let next_action_id = runtime_snapshot.next_action_id.max(1);
        // The shared Agent service executor refreshes provider metadata outside
        // the Viewer mutex. Snapshots only project its cached check state.
        if self.config.world_service.is_none() {
            self.llm_sidecar.refresh_provider_check_snapshot();
        }
        let gameplay_gate = if self.llm_sidecar.is_llm_mode() {
            None
        } else {
            Some("gameplay requires runtime live server running with --llm".to_string())
        };
        let snapshot_player_id = current_player_id
            .map(str::trim)
            .filter(|player_id| !player_id.is_empty());
        let canonical_chat = self
            .verified_world_view
            .as_ref()
            .and_then(|view| view.projection().canonical_agent_chat.as_ref())
            .filter(|chat| {
                snapshot_player_id == Some(chat.player_id.as_str())
                    && self
                        .session_policy
                        .validate_known_session_key(&chat.player_id, &chat.public_key)
                        .is_ok()
            });
        let snapshot_bound_agent_id = if self.chain_link_enabled() {
            canonical_chat.map(|chat| chat.agent_id.as_str())
        } else {
            snapshot_player_id
                .and_then(|player_id| self.llm_sidecar.bound_agent_for_player(player_id))
        };
        let primary_agent_claim = snapshot_bound_agent_id.and_then(|agent_id| {
            build_player_agent_claim_snapshot(
                runtime_state,
                agent_id,
                self.world.governance_execution_policy().epoch_length_ticks,
            )
        });
        let first_agent_claim_target_bound = runtime_state
            .agents
            .contains_key(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
            && self
                .llm_sidecar
                .agent_player_bindings
                .contains_key(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID);
        let first_agent_claim_target_available = snapshot_player_id.is_some()
            && snapshot_bound_agent_id.is_none()
            && !first_agent_claim_target_bound;
        let primary_intent = snapshot_bound_agent_id.and_then(|agent_id| {
            let Some(runtime_agent) = runtime_state.agents.get(agent_id) else {
                return Some(crate::simulator::persist::PlayerGameplayPrimaryIntent {
                    status: "unavailable".to_string(),
                    message: None,
                    resume_required: true,
                    schema_version: Some(2),
                    intent_id: None,
                    agent_id: Some(agent_id.to_string()),
                    kind: None,
                    source: None,
                    target_id: None,
                    world_id: None,
                    reorg_epoch: None,
                    logical_time: None,
                    updated_at: None,
                    source_class: Some("runtime_projection".to_string()),
                    freshness: Some("stale".to_string()),
                    control_state: Some("control_lost".to_string()),
                    event_seq: None,
                    reason_code: None,
                    reason_summary: None,
                    receipt_ref: None,
                    next_step: None,
                    agency_read_model: None,
                });
            };
            if let Some(intent) = runtime_agent.intent.as_ref() {
                Some({
                    let status =
                        canonical_player_intent_status(&intent.status).unwrap_or("unavailable");
                    let has_authority_position = intent.world_id.as_deref()
                        == Some(self.config.world_id.as_str())
                        && intent.reorg_epoch == Some(self.reorg_epoch);
                    crate::simulator::persist::PlayerGameplayPrimaryIntent {
                        status: status.to_string(),
                        message: Some(intent.summary.clone()),
                        resume_required: intent.status == "blocked",
                        schema_version: Some(intent.schema_version),
                        intent_id: Some(intent.intent_id.clone()),
                        agent_id: Some(intent.agent_id.clone()),
                        kind: Some(intent.kind.clone()),
                        source: Some(intent.source.clone()),
                        target_id: intent.target_id.clone(),
                        world_id: intent.world_id.clone(),
                        reorg_epoch: intent.reorg_epoch,
                        logical_time: Some(intent.logical_time),
                        updated_at: Some(intent.updated_at),
                        source_class: Some("runtime_projection".to_string()),
                        freshness: Some(
                            if has_authority_position {
                                "current"
                            } else {
                                "unknown"
                            }
                            .to_string(),
                        ),
                        control_state: Some(
                            if has_authority_position {
                                "controllable"
                            } else {
                                "unavailable"
                            }
                            .to_string(),
                        ),
                        event_seq: Some(intent.event_seq.to_string()),
                        reason_code: intent.reason_code.clone(),
                        reason_summary: intent.reason_summary.clone(),
                        receipt_ref: committed_receipt_tuple(intent, self.world.journal()),
                        next_step: (intent.status == "blocked")
                            .then(|| "Recheck runtime state before resuming.".to_string()),
                        agency_read_model: None,
                    }
                })
            } else if self.chain_link_enabled() {
                None
            } else {
                // Prompt-control predates AgentIntentV2. Keep that handoff
                // visible for compatibility, but mark it explicitly as a
                // sidecar/unknown projection rather than runtime authority.
                self.llm_sidecar
                    .primary_intents
                    .get(agent_id)
                    .map(
                        |legacy| crate::simulator::persist::PlayerGameplayPrimaryIntent {
                            status: canonical_player_intent_status(&legacy.status)
                                .unwrap_or("unavailable")
                                .to_string(),
                            message: legacy.message.clone(),
                            resume_required: legacy.resume_required,
                            schema_version: None,
                            intent_id: None,
                            agent_id: Some(agent_id.to_string()),
                            kind: None,
                            source: None,
                            target_id: None,
                            world_id: None,
                            reorg_epoch: None,
                            logical_time: None,
                            updated_at: None,
                            source_class: Some("prompt_control_compatibility".to_string()),
                            freshness: Some("unknown".to_string()),
                            control_state: Some("unknown".to_string()),
                            event_seq: None,
                            reason_code: None,
                            reason_summary: None,
                            receipt_ref: None,
                            next_step: None,
                            agency_read_model: None,
                        },
                    )
            }
        });
        let mut model = runtime_state_to_simulator_model(
            runtime_state,
            &self.llm_sidecar,
            self.seed_model.as_ref(),
        );
        if self.config.world_service.is_some() {
            for profile in model.agent_prompt_profiles.values_mut() {
                profile.short_term_goal_override = None;
            }
        }

        let projection_kernel = WorldKernel::with_model(self.snapshot_config.clone(), model);
        let model = projection_kernel.model();
        let micro_depot_facilities = projection_kernel.micro_depot_player_facility_snapshots();
        // A shared runtime/provider fault remains visible even when this
        // connection also has a rejected private request. Successful world
        // progress is the fallback when there is no scoped request feedback.
        let world_feedback = self.latest_player_gameplay_feedback.as_ref();
        let world_blocked = world_feedback.is_some_and(|feedback| feedback.stage == "blocked");
        let recent_feedback = if world_blocked {
            world_feedback
        } else {
            request_feedback.or(world_feedback)
        };
        let causality = if !world_blocked && request_feedback.is_some() {
            None
        } else {
            self.latest_player_gameplay_causality.as_ref()
        };
        let mut player_gameplay = build_player_gameplay_snapshot(
            runtime_state,
            snapshot_bound_agent_id,
            self.confirmed_player_gameplay_progress_time.is_some(),
            recent_feedback,
            primary_intent,
            causality,
            gameplay_gate.is_none(),
            gameplay_gate.as_deref(),
            self.llm_sidecar.is_llm_mode() && self.supports_agent_chat(),
            first_agent_claim_target_available,
            primary_agent_claim,
        );
        player_gameplay.canonical_agent_chat = canonical_chat.cloned();
        if let (Some(player_id), Some(agent_id)) = (snapshot_player_id, snapshot_bound_agent_id)
            && self
                .llm_sidecar
                .agent_player_bindings
                .get(agent_id)
                .map(String::as_str)
                == Some(player_id)
            && self
                .world
                .state()
                .starter_oc_claims
                .get(agent_id)
                .is_some_and(|claim| claim.player_id == player_id)
            && let Some(primary_intent) = player_gameplay.primary_intent.as_mut()
            && primary_intent.source_class.as_deref() == Some("runtime_projection")
            && primary_intent.freshness.as_deref() == Some("current")
        {
            primary_intent.agency_read_model = Some(
                super::player_agency_projection::project_player_agency_read_model(
                    &self.world,
                    &self.llm_sidecar,
                    agent_id,
                    primary_intent.intent_id.as_deref(),
                ),
            );
        }
        player_gameplay.micro_depot_facilities = micro_depot_facilities;
        if snapshot_player_id.is_some() && snapshot_bound_agent_id.is_none() {
            player_gameplay.available_actions.retain(|action| {
                action.target_agent_id.is_none()
                    || action.action_id == crate::viewer::ACTION_CLAIM_FIRST_AGENT
            });
            if first_agent_claim_target_available
                && !player_gameplay
                    .available_actions
                    .iter()
                    .any(|action| action.action_id == crate::viewer::ACTION_CLAIM_FIRST_AGENT)
            {
                player_gameplay
                    .available_actions
                    .push(crate::simulator::PlayerGameplayAction {
                        action_id: crate::viewer::ACTION_CLAIM_FIRST_AGENT.to_string(),
                        label: "Claim first Agent".to_string(),
                        protocol_action: "gameplay_action.submit".to_string(),
                        target_agent_id: Some(
                            crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string(),
                        ),
                        disabled_reason: None,
                    });
            }
        }
        apply_runtime_snapshot_empty_entities_blocker(
            &mut player_gameplay,
            model.agents.is_empty(),
            model.locations.is_empty(),
        );
        let chain_resource_manifest = runtime_snapshot.chain_resource_manifest.clone();
        let latest_chain_resource_delta = runtime_snapshot
            .latest_chain_resource_delta
            .clone()
            .unwrap_or_default();
        WorldSnapshot {
            version: SNAPSHOT_VERSION,
            chunk_generation_schema_version: CHUNK_GENERATION_SCHEMA_VERSION,
            time: runtime_state.time,
            config: self.snapshot_config.clone(),
            model: projection_kernel.into_model(),
            runtime_snapshot: Some(runtime_snapshot),
            player_gameplay: Some(player_gameplay),
            chain_resource_manifest,
            latest_chain_resource_delta,
            chunk_runtime: ChunkRuntimeConfig::default(),
            intel_ttl_ticks: 0,
            next_event_id,
            next_action_id,
            pending_actions: Vec::new(),
            journal_len: runtime_journal_len,
        }
    }

    pub(super) fn ensure_gameplay_ready_for_control(
        &mut self,
        mode: &ViewerControl,
    ) -> Result<(), String> {
        match mode {
            ViewerControl::Pause => Ok(()),
            ViewerControl::Play | ViewerControl::Step { .. } => {
                self.configure_service_provider();
                self.llm_sidecar
                    .ensure_gameplay_ready(&self.world, &self.snapshot_config)
            }
            ViewerControl::Seek { .. } => Ok(()),
        }
    }

    pub(super) fn ensure_gameplay_ready_for_action(
        &mut self,
        _action: &str,
        action_id: Option<&str>,
        target_agent_id: Option<&str>,
    ) -> Result<(), (String, String)> {
        self.configure_service_provider();
        self.llm_sidecar
            .ensure_gameplay_ready(&self.world, &self.snapshot_config)
            .map_err(|message| {
                let code = if self.llm_sidecar.is_llm_mode() {
                    "llm_init_failed"
                } else {
                    "llm_mode_required"
                };
                let detail = match (action_id, target_agent_id) {
                    (Some(action_id), Some(target_agent_id)) => format!(
                        "{message} (action_id={action_id}, target_agent_id={target_agent_id})"
                    ),
                    _ => message,
                };
                (code.to_string(), detail)
            })
    }

    pub(super) fn control_completion_delta(
        &self,
        baseline_logical_time: u64,
        baseline_event_seq: u64,
    ) -> (u64, u64) {
        (
            self.world
                .state()
                .time
                .saturating_sub(baseline_logical_time),
            latest_runtime_event_seq(&self.world).saturating_sub(baseline_event_seq),
        )
    }

    pub(super) fn gameplay_control_error(&self, reason: String) -> (String, String) {
        let code = if self.llm_sidecar.is_llm_mode() {
            "llm_init_failed"
        } else {
            "llm_mode_required"
        };
        (code.to_string(), reason)
    }

    pub(super) fn llm_gameplay_hint_for_reason(reason: &str) -> String {
        let normalized = reason.to_ascii_lowercase();
        if normalized.contains("provider_gateway_unreachable")
            || normalized.contains("provider_http_502")
            || normalized.contains("provider_http_503")
            || normalized.contains("provider_http_504")
            || normalized.contains("provider bridge returned http 502")
            || normalized.contains("provider bridge returned http 503")
            || normalized.contains("provider bridge returned http 504")
            || normalized.contains("read operation timed out")
            || normalized.contains("operation timed out")
            || normalized.contains("timed out")
        {
            LLM_PROVIDER_GATEWAY_TIMEOUT_HINT.to_string()
        } else {
            LLM_GAMEPLAY_REQUIRED_HINT.to_string()
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Player-facing gameplay completion ack preserves the stable control seam"
    )]
    pub(super) fn block_gameplay_control(
        &mut self,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
        action: &str,
        effect: &str,
        reason: String,
        request_id: Option<u64>,
        delta_logical_time: u64,
        delta_event_seq: u64,
        emit_snapshot: bool,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let (error_code, error_message) = self.gameplay_control_error(reason.clone());
        self.pause_auto_play(session);
        session.next_play_step_at = None;
        let hint = Self::llm_gameplay_hint_for_reason(&reason);
        self.set_latest_player_gameplay_feedback(Self::make_player_gameplay_feedback(
            action,
            "blocked",
            effect,
            Some("advance the live world".to_string()),
            None,
            Some(reason),
            Some(hint),
            delta_logical_time,
            delta_event_seq,
        ));
        if let Some(request_id) = request_id {
            let ack = ControlCompletionAck {
                request_id,
                status: ControlCompletionStatus::Blocked,
                delta_logical_time,
                delta_event_seq,
                error_code: Some(error_code),
                error_message: Some(error_message),
            };
            send_response(writer, &ViewerResponse::ControlCompletionAck { ack })?;
        }
        if emit_snapshot && session.explicitly_subscribed_to(ViewerStream::Snapshot) {
            let snapshot = self.compat_snapshot_for_session(session);
            send_response(writer, &ViewerResponse::Snapshot { snapshot })?;
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Player-facing runtime completion ack preserves the stable control seam"
    )]
    pub(super) fn block_runtime_control(
        &mut self,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
        action: &str,
        effect: &str,
        error: ViewerRuntimeLiveServerError,
        request_id: Option<u64>,
        delta_logical_time: u64,
        delta_event_seq: u64,
        emit_snapshot: bool,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let (error_code, error_message, hint) = runtime_control_error_details(&error);
        emit_stderr_or_event(
            Level::WARN,
            format!("viewer runtime live: control {action} failed: {error_message} ({error:?})")
                .as_str(),
            "viewer runtime live control failed",
        );
        self.pause_auto_play(session);
        session.next_play_step_at = None;
        self.set_latest_player_gameplay_feedback(Self::make_player_gameplay_feedback(
            action,
            "blocked",
            effect,
            Some("advance the live world".to_string()),
            None,
            Some(error_message.clone()),
            Some(hint),
            delta_logical_time,
            delta_event_seq,
        ));
        if let Some(request_id) = request_id {
            let ack = ControlCompletionAck {
                request_id,
                status: ControlCompletionStatus::Blocked,
                delta_logical_time,
                delta_event_seq,
                error_code: Some(error_code),
                error_message: Some(error_message),
            };
            send_response(writer, &ViewerResponse::ControlCompletionAck { ack })?;
        }
        if emit_snapshot && session.explicitly_subscribed_to(ViewerStream::Snapshot) {
            let snapshot = self.compat_snapshot_for_session(session);
            send_response(writer, &ViewerResponse::Snapshot { snapshot })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{EffectReceipt, WorldEvent};

    fn completed_intent(receipt_ref: Option<&str>) -> AgentIntentV2 {
        AgentIntentV2 {
            schema_version: 2,
            agent_id: "agent-0".to_string(),
            intent_id: "agent-intent-v2:test".to_string(),
            kind: "agent_chat".to_string(),
            summary: "Stabilize power".to_string(),
            target_id: None,
            effect_intent_id: Some("effect-intent-v2:test".to_string()),
            intent_tick: Some(7),
            world_id: Some("live-runtime-test".to_string()),
            reorg_epoch: Some(2),
            authority_scope: Some("player_agent_chat".to_string()),
            status: "completed".to_string(),
            source: "player".to_string(),
            logical_time: 8,
            event_seq: 4,
            updated_at: 8,
            receipt_ref: receipt_ref.map(ToOwned::to_owned),
            reason_code: None,
            reason_summary: None,
            replaced_by: None,
            actor_id: "player-0".to_string(),
            request_digest: "digest".to_string(),
        }
    }

    #[test]
    fn completed_projection_requires_matching_committed_receipt_event() {
        let intent = completed_intent(Some("world-event:3"));
        let journal = crate::runtime::Journal {
            events: vec![WorldEvent {
                id: 3,
                time: 8,
                caused_by: None,
                body: RuntimeWorldEventBody::ReceiptAppended(EffectReceipt {
                    birth_era: None,
                    world: None,
                    intent_payload_digest: None,
                    intent_id: "effect-intent-v2:test".to_string(),
                    status: "ok".to_string(),
                    payload: serde_json::json!({"ok": true}),
                    cost_cents: None,
                    signature: None,
                }),
            }],
        };
        assert_eq!(
            committed_receipt_tuple(&intent, &journal),
            Some(serde_json::json!({
                "intent_id": "agent-intent-v2:test",
                "world_id": "live-runtime-test",
                "reorg_epoch": 2,
                "logical_time": 8,
                "event_seq": "4",
                "receipt_id": "world-event:3",
            }))
        );

        let mut mismatched = journal.clone();
        mismatched.events[0].id = 9;
        assert!(committed_receipt_tuple(&intent, &mismatched).is_none());
        assert!(committed_receipt_tuple(&completed_intent(None), &journal).is_none());
    }

    #[test]
    fn player_status_projection_has_no_legacy_acceptance_aliases() {
        assert_eq!(canonical_player_intent_status("accepted"), Some("accepted"));
        assert_eq!(
            canonical_player_intent_status("accepted_new"),
            Some("accepted")
        );
        assert_eq!(
            canonical_player_intent_status("reprioritized"),
            Some("accepted")
        );
        assert_eq!(
            canonical_player_intent_status("resume_required"),
            Some("blocked")
        );
    }
}
