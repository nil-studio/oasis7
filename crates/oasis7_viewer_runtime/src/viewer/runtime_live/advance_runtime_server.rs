use super::*;

impl ViewerRuntimeLiveServer {
    pub(super) fn advance_runtime(
        &mut self,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
        action: &'static str,
        step_count: usize,
        request_id: Option<u64>,
        emit_while_paused: bool,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        if self.config.world_service.is_some() {
            // Play/Step changes playback orchestration only. Canonical time
            // advances exclusively at the service execution boundary.
            // Shared request preparation/periodic polling performs the I/O
            // outside the Viewer mutex. This path never executes a local tick.
            if let Some(request_id) = request_id {
                send_response(
                    writer,
                    &ViewerResponse::ControlCompletionAck {
                        ack: ControlCompletionAck {
                            request_id,
                            status: ControlCompletionStatus::TimeoutNoProgress,
                            delta_logical_time: 0,
                            delta_event_seq: 0,
                            error_code: None,
                            error_message: Some(
                                "canonical advancement is owned by the world service".into(),
                            ),
                        },
                    },
                )?;
            }
            return Ok(());
        }
        let baseline_logical_time = self.world.state().time;
        let baseline_event_seq = latest_runtime_event_seq(&self.world);
        let mut runtime_events_for_feedback = Vec::new();

        for _ in 0..step_count.max(1) {
            let iteration_logical_time = self.world.state().time;
            self.sync_runtime_wake_projection()?;
            if let Err(reason) = self
                .llm_sidecar
                .ensure_gameplay_ready(&self.world, &self.snapshot_config)
            {
                let (delta_logical_time, delta_event_seq) =
                    self.control_completion_delta(baseline_logical_time, baseline_event_seq);
                if self.tolerate_background_play_gameplay_block(
                    session,
                    writer,
                    action,
                    self.config.play_step_interval,
                    "runtime play loop hit a transient LLM access failure; will retry on the next play tick",
                    reason.clone(),
                    delta_logical_time,
                    delta_event_seq,
                )? {
                    return Ok(());
                }
                return self.block_gameplay_control(
                    session,
                    writer,
                    action,
                    "runtime play loop stopped because active LLM access is no longer available",
                    reason,
                    request_id,
                    delta_logical_time,
                    delta_event_seq,
                    true,
                );
            }
            let mut decision_trace: Option<AgentDecisionTrace> = None;
            // Provider-backed cognition commits are Runtime-atomic and may
            // append their terminal action event before the compatibility
            // tick below runs. Capture the complete journal delta so the
            // viewer still presents that authoritative event.
            let journal_start = self.world.journal().events.len();
            match self.config.decision_mode {
                ViewerLiveDecisionMode::Script => self.script.enqueue(&mut self.world),
                ViewerLiveDecisionMode::Llm => {
                    self.llm_sidecar.request_decision();
                    match self.enqueue_llm_action_from_sidecar() {
                        Ok(trace) => {
                            decision_trace = trace;
                        }
                        Err(trace) => {
                            if session.explicitly_subscribed_to(ViewerStream::Events) {
                                send_response(
                                    writer,
                                    &ViewerResponse::DecisionTrace {
                                        trace: trace.clone(),
                                    },
                                )?;
                            }
                            let (delta_logical_time, delta_event_seq) = self
                                .control_completion_delta(
                                    baseline_logical_time,
                                    baseline_event_seq,
                                );
                            let reason = trace.llm_error.clone().unwrap_or_else(|| {
                                "gameplay requires a configured and reachable LLM provider"
                                    .to_string()
                            });
                            let reason = append_decision_upstream_trace(reason, &trace);
                            if decision_trace_provider_error_retryable(&trace).unwrap_or(true)
                                && self.tolerate_background_play_gameplay_block(
                                    session,
                                    writer,
                                    action,
                                    self.config.play_step_interval,
                                    "runtime play loop hit a transient LLM decision failure; will retry on the next play tick",
                                    reason.clone(),
                                    delta_logical_time,
                                    delta_event_seq,
                                )?
                            {
                                return Ok(());
                            }
                            return self.block_gameplay_control(
                                session,
                                writer,
                                action,
                                "runtime play loop stopped because the LLM decision provider failed",
                                reason,
                                request_id,
                                delta_logical_time,
                                delta_event_seq,
                                true,
                            );
                        }
                    }
                }
            }
            if self.should_advance_compatibility_tick(iteration_logical_time)
                && let Err(error) = self.world.step()
            {
                let (delta_logical_time, delta_event_seq) =
                    self.control_completion_delta(baseline_logical_time, baseline_event_seq);
                return self.block_runtime_control(
                    session,
                    writer,
                    action,
                    "runtime step aborted because world advance failed",
                    ViewerRuntimeLiveServerError::Runtime(error),
                    request_id,
                    delta_logical_time,
                    delta_event_seq,
                    true,
                );
            }
            self.sync_runtime_wake_projection()?;
            session.transient_play_failures = 0;
            if self.world.state().time > baseline_logical_time
                || latest_runtime_event_seq(&self.world) > baseline_event_seq
            {
                self.confirm_player_gameplay_progress();
            }
            let new_events: Vec<_> = self.world.journal().events[journal_start..].to_vec();
            self.update_gameplay_feedback_from_runtime_events(&new_events);
            runtime_events_for_feedback.extend(new_events.iter().cloned());
            let mut mapped_events = Vec::new();
            for runtime_event in &new_events {
                let event = map_runtime_event(
                    runtime_event,
                    &self.snapshot_config,
                    self.seed_model.as_ref(),
                );
                if matches!(runtime_event.body, RuntimeWorldEventBody::Domain(_)) {
                    self.llm_sidecar
                        .notify_action_result_if_needed(runtime_event, event.clone());
                }
                self.llm_sidecar.notify_recipe_completion_with_binding(
                    runtime_event,
                    event.clone(),
                    self.world.current_cognition_runtime_binding().ok(),
                );
                mapped_events.push(event);
            }
            mapped_events.extend(self.pending_virtual_events.drain(..));
            let pending_batch = match self.register_authoritative_batch(mapped_events.as_slice()) {
                Ok(batch) => batch,
                Err(error) => {
                    let (delta_logical_time, delta_event_seq) =
                        self.control_completion_delta(baseline_logical_time, baseline_event_seq);
                    return self.block_runtime_control(
                        session,
                        writer,
                        action,
                        "runtime step aborted because authoritative batch registration failed",
                        error,
                        request_id,
                        delta_logical_time,
                        delta_event_seq,
                        true,
                    );
                }
            };
            let batch_finality_updates =
                match self.advance_authoritative_batch_finality(self.world.state().time) {
                    Ok(updates) => updates,
                    Err(error) => {
                        let (delta_logical_time, delta_event_seq) = self
                            .control_completion_delta(baseline_logical_time, baseline_event_seq);
                        return self.block_runtime_control(
                            session,
                            writer,
                            action,
                            "runtime step aborted because authoritative finality update failed",
                            error,
                            request_id,
                            delta_logical_time,
                            delta_event_seq,
                            true,
                        );
                    }
                };
            if let Some(trace) = decision_trace
                && session.explicitly_subscribed_to(ViewerStream::Events)
            {
                send_response(writer, &ViewerResponse::DecisionTrace { trace })?;
            }

            if session.explicitly_subscribed_to(ViewerStream::Events)
                && (emit_while_paused || session.playing)
            {
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

            if session.explicitly_subscribed_to(ViewerStream::Snapshot)
                && should_emit_runtime_advance_snapshot(session, action, emit_while_paused)
            {
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
        }

        if let Some(request_id) = request_id {
            let delta_logical_time = self
                .world
                .state()
                .time
                .saturating_sub(baseline_logical_time);
            let delta_event_seq =
                latest_runtime_event_seq(&self.world).saturating_sub(baseline_event_seq);
            let status = if delta_logical_time > 0 || delta_event_seq > 0 {
                ControlCompletionStatus::Advanced
            } else {
                ControlCompletionStatus::TimeoutNoProgress
            };
            let ack = ControlCompletionAck {
                request_id,
                status,
                delta_logical_time,
                delta_event_seq,
                error_code: None,
                error_message: None,
            };
            let feedback = player_gameplay_feedback_from_control_ack(
                &control_mode_for_action(action, step_count),
                &ack,
            );
            let causality =
                player_gameplay_causality_from_runtime_events(&runtime_events_for_feedback);
            self.set_latest_player_gameplay_feedback_with_causality(feedback, causality);
            if session.explicitly_subscribed_to(ViewerStream::Snapshot) {
                let snapshot = self.compat_snapshot_for_session(session);
                send_response(writer, &ViewerResponse::Snapshot { snapshot })?;
            }
            send_response(writer, &ViewerResponse::ControlCompletionAck { ack })?;
        }

        Ok(())
    }
}
