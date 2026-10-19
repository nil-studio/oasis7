use super::*;

impl ViewerRuntimeLiveServer {
    pub(super) fn set_latest_player_gameplay_feedback(
        &mut self,
        feedback: PlayerGameplayRecentFeedback,
    ) {
        // Only world progress/readiness producers write the shared layer.
        // Signed request feedback belongs in set_verified_player_gameplay_feedback.
        self.set_latest_player_gameplay_feedback_with_causality(feedback, None);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Gameplay feedback fields map directly to the stable player-facing DTO"
    )]
    pub(super) fn make_player_gameplay_feedback(
        action: impl Into<String>,
        stage: impl Into<String>,
        effect: impl Into<String>,
        intent_summary: Option<String>,
        target_agent_id: Option<String>,
        reason: Option<String>,
        hint: Option<String>,
        delta_logical_time: u64,
        delta_event_seq: u64,
    ) -> PlayerGameplayRecentFeedback {
        PlayerGameplayRecentFeedback {
            action: action.into(),
            stage: stage.into(),
            effect: effect.into(),
            intent_summary,
            target_agent_id,
            reason,
            hint,
            delta_logical_time,
            delta_event_seq,
        }
    }

    pub(super) fn set_latest_player_gameplay_feedback_with_causality(
        &mut self,
        feedback: PlayerGameplayRecentFeedback,
        causality: Option<PlayerGameplayCausalitySignal>,
    ) {
        if feedback.delta_logical_time > 0 || feedback.delta_event_seq > 0 {
            self.confirm_player_gameplay_progress();
        }
        self.latest_player_gameplay_causality = causality;
        self.latest_player_gameplay_feedback = Some(feedback);
    }

    pub(super) fn record_chain_sync_failure(&mut self, error: &ViewerRuntimeLiveServerError) {
        let reason = match error {
            ViewerRuntimeLiveServerError::Serde(message) => message.clone(),
            ViewerRuntimeLiveServerError::Runtime(err) => format!("{err:?}"),
            ViewerRuntimeLiveServerError::Init(message) => message.clone(),
            ViewerRuntimeLiveServerError::Io(err) => err.to_string(),
        };
        let hint = if reason.contains("execution world is not ready") {
            "wait for the execution world persistence files to appear, or restart/repair the chain runtime bootstrap before refreshing gameplay"
                .to_string()
        } else {
            "repair the chain runtime sync path, then refresh gameplay to confirm the committed world is available"
                .to_string()
        };
        self.set_latest_player_gameplay_feedback(Self::make_player_gameplay_feedback(
            "chain_sync",
            "blocked",
            "committed runtime sync failed before the viewer could observe new world state",
            Some("refresh committed world state".to_string()),
            None,
            Some(reason),
            Some(hint),
            0,
            0,
        ));
    }

    pub(super) fn gameplay_action_rejection_feedback(
        error: &GameplayActionError,
    ) -> PlayerGameplayRecentFeedback {
        let action_id = error.action_id.as_deref().unwrap_or("unknown");
        let hint = gameplay_action_rejection_hint(error.code.as_str());
        Self::make_player_gameplay_feedback(
            format!("gameplay_action:{action_id}"),
            if error.code == "world_service_outcome_unknown" {
                "unknown"
            } else {
                "rejected"
            },
            format!("gameplay action {action_id}: {}", error.message),
            Some(format!("submit gameplay action {action_id}")),
            error.target_agent_id.clone(),
            Some(format!("{}: {}", error.code, error.message)),
            Some(hint.to_string()),
            0,
            0,
        )
    }

    pub(super) fn clear_chain_sync_failure_feedback(&mut self) {
        if self
            .latest_player_gameplay_feedback
            .as_ref()
            .is_some_and(|feedback| feedback.action == "chain_sync")
        {
            self.latest_player_gameplay_feedback = None;
        }
    }

    pub(super) fn confirm_player_gameplay_progress(&mut self) {
        self.confirmed_player_gameplay_progress_time = Some(self.world.state().time);
    }
}

fn gameplay_action_rejection_hint(code: &str) -> &'static str {
    match code {
        "chain_submit_unavailable" | "chain_submit_failed" => {
            "restore the chain gameplay submission path, then retry"
        }
        "llm_mode_required"
        | "llm_init_failed"
        | "provider_config_invalid"
        | "provider_config_missing"
        | "provider_unreachable"
        | "runtime_sync_unavailable"
        | "runtime_io_failed"
        | "runtime_sync_decode_failed"
        | "runtime_init_failed"
        | "runtime_step_failed" => "restore or wait for runtime and provider readiness, then retry",
        "auth_proof_required"
        | "auth_nonce_replay"
        | "invalid_first_agent_claim_target"
        | "first_agent_already_bound"
        | "player_agent_binding_required"
        | "actor_agent_required"
        | "actor_agent_mismatch"
        | "player_bind_failed" => "correct the rejected request before retrying",
        _ => "inspect the rejection details before retrying",
    }
}
