use super::*;

impl ViewerRuntimeLiveServer {
    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(in crate::viewer::runtime_live) fn handle_prompt_control(
        &mut self,
        command: PromptControlCommand,
    ) -> Result<PromptControlAck, PromptControlError> {
        if self.hosted_public_join_mode() {
            match &command {
                PromptControlCommand::Preview { request } => {
                    self.verify_hosted_prompt_control_apply_strong_auth(
                        PromptControlAuthIntent::Preview,
                        request,
                    )?;
                }
                PromptControlCommand::Apply { request } => {
                    self.verify_hosted_prompt_control_apply_strong_auth(
                        PromptControlAuthIntent::Apply,
                        request,
                    )?;
                }
                PromptControlCommand::Rollback { request } => {
                    self.verify_hosted_prompt_control_rollback_strong_auth(request)?;
                }
            }
        }
        if !self.llm_sidecar.is_llm_mode() {
            let (agent_id, message) = match command {
                PromptControlCommand::Preview { request }
                | PromptControlCommand::Apply { request } => (
                    request.agent_id,
                    "prompt_control requires runtime live server running with --llm".to_string(),
                ),
                PromptControlCommand::Rollback { request } => (
                    request.agent_id,
                    "prompt_control rollback requires runtime live server running with --llm"
                        .to_string(),
                ),
            };
            return Err(PromptControlError {
                code: "llm_mode_required".to_string(),
                message,
                agent_id: Some(agent_id.clone()),
                current_version: self.current_prompt_version(agent_id.as_str()),
                ..PromptControlError::default_legacy()
            });
        }
        if !self.llm_sidecar.supports_prompt_control() {
            let (agent_id, current_version) = match &command {
                PromptControlCommand::Preview { request }
                | PromptControlCommand::Apply { request } => (
                    request.agent_id.clone(),
                    self.current_prompt_version(request.agent_id.as_str()),
                ),
                PromptControlCommand::Rollback { request } => (
                    request.agent_id.clone(),
                    self.current_prompt_version(request.agent_id.as_str()),
                ),
            };
            return Err(PromptControlError {
                code: "agent_provider_prompt_control_unsupported".to_string(),
                message:
                    "prompt_control is not yet supported when runtime live uses ProviderBacked(Local HTTP)"
                        .to_string(),
                agent_id: Some(agent_id),
                current_version,
                ..PromptControlError::default_legacy()
            });
        }

        match command {
            PromptControlCommand::Preview { request } => self.prompt_control_preview(request),
            PromptControlCommand::Apply { request } => self.prompt_control_apply(request),
            PromptControlCommand::Rollback { request } => self.prompt_control_rollback(request),
        }
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    fn prompt_control_preview(
        &mut self,
        request: PromptControlApplyRequest,
    ) -> Result<PromptControlAck, PromptControlError> {
        let player_id =
            normalize_required_player_id(request.player_id.as_str(), request.agent_id.as_str())?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        self.verify_and_consume_prompt_control_apply_auth(
            PromptControlAuthIntent::Preview,
            &request,
        )?;
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let current = self.current_prompt_profile(request.agent_id.as_str())?;
        ensure_expected_prompt_version_runtime(
            request.agent_id.as_str(),
            current.version,
            request.expected_version,
        )?;

        let mut candidate = current.clone();
        apply_prompt_patch_runtime(&mut candidate, &request);
        let applied_fields = changed_prompt_fields_runtime(&current, &candidate);
        let preview_version = if applied_fields.is_empty() {
            current.version
        } else {
            current.version.saturating_add(1)
        };

        Ok(PromptControlAck {
            agent_id: request.agent_id,
            operation: PromptControlOperation::Apply,
            preview: true,
            version: preview_version,
            updated_at_tick: self.world.state().time,
            applied_fields,
            digest: prompt_profile_digest_runtime(&candidate),
            rolled_back_to_version: None,
            ..PromptControlAck::default_legacy()
        })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    fn prompt_control_apply(
        &mut self,
        request: PromptControlApplyRequest,
    ) -> Result<PromptControlAck, PromptControlError> {
        let player_id =
            normalize_required_player_id(request.player_id.as_str(), request.agent_id.as_str())?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        let verified = self.verify_and_consume_prompt_control_apply_auth(
            PromptControlAuthIntent::Apply,
            &request,
        )?;
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let current = self.current_prompt_profile(request.agent_id.as_str())?;
        ensure_expected_prompt_version_runtime(
            request.agent_id.as_str(),
            current.version,
            request.expected_version,
        )?;
        ensure_updated_by_matches_player_runtime(
            request.updated_by.as_deref(),
            player_id.as_str(),
            request.agent_id.as_str(),
        )?;

        let mut candidate = current.clone();
        apply_prompt_patch_runtime(&mut candidate, &request);
        let applied_fields = changed_prompt_fields_runtime(&current, &candidate);
        let digest = prompt_profile_digest_runtime(&candidate);
        if applied_fields.is_empty() {
            if request.short_term_goal_override.is_some() {
                self.record_primary_intent_from_short_term_goal(
                    request.agent_id.as_str(),
                    candidate.short_term_goal_override.as_deref(),
                );
            }
            return Ok(PromptControlAck {
                agent_id: request.agent_id,
                operation: PromptControlOperation::Apply,
                preview: false,
                version: current.version,
                updated_at_tick: current.updated_at_tick,
                applied_fields,
                digest,
                rolled_back_to_version: None,
                ..PromptControlAck::default_legacy()
            });
        }

        candidate.version = current.version.saturating_add(1);
        candidate.updated_at_tick = self.world.state().time;
        candidate.updated_by = player_id.clone();
        self.llm_sidecar
            .apply_prompt_profile_to_driver(&candidate)
            .map_err(|message| PromptControlError {
                code: "prompt_override_enqueue_failed".to_string(),
                message,
                agent_id: Some(request.agent_id.clone()),
                current_version: Some(current.version),
                ..PromptControlError::default_legacy()
            })?;
        self.llm_sidecar.upsert_prompt_profile(candidate.clone());
        self.bind_agent_player_access(
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let digest = prompt_profile_digest_runtime(&candidate);
        self.enqueue_virtual_event(WorldEventKind::AgentPromptUpdated {
            profile: candidate.clone(),
            operation: PromptUpdateOperation::Apply,
            applied_fields: applied_fields.clone(),
            digest: digest.clone(),
            rolled_back_to_version: None,
        });
        if request.short_term_goal_override.is_some() {
            self.record_primary_intent_from_short_term_goal(
                request.agent_id.as_str(),
                candidate.short_term_goal_override.as_deref(),
            );
        }
        self.llm_sidecar.request_decision();
        self.set_verified_player_gameplay_feedback(&verified, PlayerGameplayRecentFeedback {
            action: "prompt_control.apply".to_string(),
            stage: "completed_advanced".to_string(),
            effect: format!(
                "updated prompt guidance for {} to version {}",
                request.agent_id, candidate.version
            ),
            intent_summary: Some(format!("apply updated prompt guidance for {}", request.agent_id)),
            target_agent_id: Some(request.agent_id.clone()),
            reason: None,
            hint: Some(
                "continue the world and watch whether the new prompt guidance changes the agent's next decision"
                    .to_string(),
            ),
            delta_logical_time: 0,
            delta_event_seq: 0,
        });

        Ok(PromptControlAck {
            agent_id: request.agent_id,
            operation: PromptControlOperation::Apply,
            preview: false,
            version: candidate.version,
            updated_at_tick: candidate.updated_at_tick,
            applied_fields,
            digest,
            rolled_back_to_version: None,
            ..PromptControlAck::default_legacy()
        })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    fn prompt_control_rollback(
        &mut self,
        request: PromptControlRollbackRequest,
    ) -> Result<PromptControlAck, PromptControlError> {
        let player_id =
            normalize_required_player_id(request.player_id.as_str(), request.agent_id.as_str())?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        let verified = self.verify_and_consume_prompt_control_rollback_auth(&request)?;
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let current = self.current_prompt_profile(request.agent_id.as_str())?;
        ensure_expected_prompt_version_runtime(
            request.agent_id.as_str(),
            current.version,
            request.expected_version,
        )?;
        ensure_updated_by_matches_player_runtime(
            request.updated_by.as_deref(),
            player_id.as_str(),
            request.agent_id.as_str(),
        )?;

        let target = if request.to_version == 0 {
            AgentPromptProfile::for_agent(request.agent_id.clone())
        } else {
            self.lookup_prompt_profile_version(request.agent_id.as_str(), request.to_version)
                .ok_or_else(|| PromptControlError {
                    code: "target_version_not_found".to_string(),
                    message: format!(
                        "prompt profile version {} not found for {}",
                        request.to_version, request.agent_id
                    ),
                    agent_id: Some(request.agent_id.clone()),
                    current_version: Some(current.version),
                    ..PromptControlError::default_legacy()
                })?
        };
        let target_short_term_goal = target.short_term_goal_override.clone();
        let mut candidate = current.clone();
        candidate.system_prompt_override = target.system_prompt_override;
        candidate.short_term_goal_override = target.short_term_goal_override;
        candidate.long_term_goal_override = target.long_term_goal_override;
        let applied_fields = changed_prompt_fields_runtime(&current, &candidate);
        if applied_fields.is_empty() {
            return Err(PromptControlError {
                code: "rollback_noop".to_string(),
                message: format!(
                    "rollback target version {} yields no prompt changes for {}",
                    request.to_version, request.agent_id
                ),
                agent_id: Some(request.agent_id),
                current_version: Some(current.version),
                ..PromptControlError::default_legacy()
            });
        }

        candidate.version = current.version.saturating_add(1);
        candidate.updated_at_tick = self.world.state().time;
        candidate.updated_by = player_id.clone();
        self.llm_sidecar
            .apply_prompt_profile_to_driver(&candidate)
            .map_err(|message| PromptControlError {
                code: "prompt_override_enqueue_failed".to_string(),
                message,
                agent_id: Some(request.agent_id.clone()),
                current_version: Some(current.version),
                ..PromptControlError::default_legacy()
            })?;
        self.llm_sidecar.upsert_prompt_profile(candidate.clone());
        self.bind_agent_player_access(
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let digest = prompt_profile_digest_runtime(&candidate);
        self.enqueue_virtual_event(WorldEventKind::AgentPromptUpdated {
            profile: candidate.clone(),
            operation: PromptUpdateOperation::Rollback,
            applied_fields: applied_fields.clone(),
            digest: digest.clone(),
            rolled_back_to_version: Some(request.to_version),
        });
        self.record_primary_intent_from_short_term_goal(
            request.agent_id.as_str(),
            target_short_term_goal.as_deref(),
        );
        self.llm_sidecar.request_decision();
        self.set_verified_player_gameplay_feedback(
            &verified,
            PlayerGameplayRecentFeedback {
                action: "prompt_control.rollback".to_string(),
                stage: "completed_advanced".to_string(),
                effect: format!(
                    "rolled back prompt guidance for {} to base version {} via version {}",
                    request.agent_id, request.to_version, candidate.version
                ),
                intent_summary: Some(format!(
                    "roll back prompt guidance for {}",
                    request.agent_id
                )),
                target_agent_id: Some(request.agent_id.clone()),
                reason: None,
                hint: Some(
                    "continue the world and confirm the agent now follows the restored guidance"
                        .to_string(),
                ),
                delta_logical_time: 0,
                delta_event_seq: 0,
            },
        );

        Ok(PromptControlAck {
            agent_id: request.agent_id,
            operation: PromptControlOperation::Rollback,
            preview: false,
            version: candidate.version,
            updated_at_tick: candidate.updated_at_tick,
            applied_fields,
            digest,
            rolled_back_to_version: Some(request.to_version),
            ..PromptControlAck::default_legacy()
        })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    fn verify_and_consume_prompt_control_apply_auth(
        &mut self,
        intent: PromptControlAuthIntent,
        request: &PromptControlApplyRequest,
    ) -> Result<VerifiedPlayerAuth, PromptControlError> {
        let Some(auth) = request.auth.as_ref() else {
            return Err(PromptControlError {
                code: "auth_proof_required".to_string(),
                message: "prompt_control requires auth proof".to_string(),
                agent_id: Some(request.agent_id.clone()),
                current_version: self.current_prompt_version(request.agent_id.as_str()),
                ..PromptControlError::default_legacy()
            });
        };
        let verified =
            verify_prompt_control_apply_auth_proof(intent, request, auth).map_err(|message| {
                PromptControlError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    agent_id: Some(request.agent_id.clone()),
                    current_version: self.current_prompt_version(request.agent_id.as_str()),
                    ..PromptControlError::default_legacy()
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| PromptControlError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                agent_id: Some(request.agent_id.clone()),
                current_version: self.current_prompt_version(request.agent_id.as_str()),
                ..PromptControlError::default_legacy()
            })?;
        self.llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
            .map_err(|message| PromptControlError {
                code: "auth_nonce_replay".to_string(),
                message,
                agent_id: Some(request.agent_id.clone()),
                current_version: self.current_prompt_version(request.agent_id.as_str()),
                ..PromptControlError::default_legacy()
            })?;
        Ok(verified)
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn verify_hosted_prompt_control_apply_strong_auth(
        &self,
        intent: PromptControlAuthIntent,
        request: &PromptControlApplyRequest,
    ) -> Result<(), PromptControlError> {
        let Some(grant) = request.strong_auth_grant.as_ref() else {
            return Err(self.hosted_prompt_control_strong_auth_error(
                "strong_auth_required",
                request.agent_id.as_str(),
                "prompt_control requires hosted strong auth grant on hosted_public_join",
            ));
        };
        let signer_public_key =
            hosted_strong_auth_grant_public_key_from_env().map_err(|message| {
                self.hosted_prompt_control_strong_auth_error(
                    "strong_auth_required",
                    request.agent_id.as_str(),
                    message.as_str(),
                )
            })?;
        verify_hosted_prompt_control_apply_strong_auth_grant(
            intent,
            request,
            grant,
            signer_public_key.as_str(),
            hosted_strong_auth_now_unix_ms(),
        )
        .map_err(|message| {
            self.hosted_prompt_control_strong_auth_error(
                "strong_auth_grant_invalid",
                request.agent_id.as_str(),
                message.as_str(),
            )
        })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    fn verify_and_consume_prompt_control_rollback_auth(
        &mut self,
        request: &PromptControlRollbackRequest,
    ) -> Result<VerifiedPlayerAuth, PromptControlError> {
        let Some(auth) = request.auth.as_ref() else {
            return Err(PromptControlError {
                code: "auth_proof_required".to_string(),
                message: "prompt_control rollback requires auth proof".to_string(),
                agent_id: Some(request.agent_id.clone()),
                current_version: self.current_prompt_version(request.agent_id.as_str()),
                ..PromptControlError::default_legacy()
            });
        };
        let verified =
            verify_prompt_control_rollback_auth_proof(request, auth).map_err(|message| {
                PromptControlError {
                    code: map_auth_verify_error_code(message.as_str()).to_string(),
                    message,
                    agent_id: Some(request.agent_id.clone()),
                    current_version: self.current_prompt_version(request.agent_id.as_str()),
                    ..PromptControlError::default_legacy()
                }
            })?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| PromptControlError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                agent_id: Some(request.agent_id.clone()),
                current_version: self.current_prompt_version(request.agent_id.as_str()),
                ..PromptControlError::default_legacy()
            })?;
        self.llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
            .map_err(|message| PromptControlError {
                code: "auth_nonce_replay".to_string(),
                message,
                agent_id: Some(request.agent_id.clone()),
                current_version: self.current_prompt_version(request.agent_id.as_str()),
                ..PromptControlError::default_legacy()
            })?;
        Ok(verified)
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn verify_hosted_prompt_control_rollback_strong_auth(
        &self,
        request: &PromptControlRollbackRequest,
    ) -> Result<(), PromptControlError> {
        let Some(grant) = request.strong_auth_grant.as_ref() else {
            return Err(self.hosted_prompt_control_strong_auth_error(
                "strong_auth_required",
                request.agent_id.as_str(),
                "prompt_control rollback requires hosted strong auth grant on hosted_public_join",
            ));
        };
        let signer_public_key =
            hosted_strong_auth_grant_public_key_from_env().map_err(|message| {
                self.hosted_prompt_control_strong_auth_error(
                    "strong_auth_required",
                    request.agent_id.as_str(),
                    message.as_str(),
                )
            })?;
        verify_hosted_prompt_control_rollback_strong_auth_grant(
            request,
            grant,
            signer_public_key.as_str(),
            hosted_strong_auth_now_unix_ms(),
        )
        .map_err(|message| {
            self.hosted_prompt_control_strong_auth_error(
                "strong_auth_grant_invalid",
                request.agent_id.as_str(),
                message.as_str(),
            )
        })
    }

    fn hosted_prompt_control_strong_auth_error(
        &self,
        code: &str,
        agent_id: &str,
        message: &str,
    ) -> PromptControlError {
        PromptControlError {
            code: code.to_string(),
            message: message.to_string(),
            agent_id: Some(agent_id.to_string()),
            current_version: self.current_prompt_version(agent_id),
            ..PromptControlError::default_legacy()
        }
    }
}
