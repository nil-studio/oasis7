use super::*;

impl LiveWorld {
    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(crate) fn prompt_control_preview(
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
        ensure_agent_player_access(
            self.kernel(),
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let current = self.current_prompt_profile(request.agent_id.as_str())?;
        ensure_expected_prompt_version(
            request.agent_id.as_str(),
            current.version,
            request.expected_version,
        )?;

        let mut candidate = current.clone();
        apply_prompt_patch(&mut candidate, &request);
        let applied_fields = changed_prompt_fields(&current, &candidate);
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
            updated_at_tick: self.kernel.time(),
            applied_fields,
            digest: prompt_profile_digest(&candidate),
            rolled_back_to_version: None,
            ..PromptControlAck::default_legacy()
        })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(crate) fn prompt_control_apply(
        &mut self,
        request: PromptControlApplyRequest,
    ) -> Result<PromptControlAck, PromptControlError> {
        let player_id =
            normalize_required_player_id(request.player_id.as_str(), request.agent_id.as_str())?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        self.verify_and_consume_prompt_control_apply_auth(
            PromptControlAuthIntent::Apply,
            &request,
        )?;
        ensure_agent_player_access(
            self.kernel(),
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let current = self.current_prompt_profile(request.agent_id.as_str())?;
        ensure_expected_prompt_version(
            request.agent_id.as_str(),
            current.version,
            request.expected_version,
        )?;
        ensure_updated_by_matches_player(
            request.updated_by.as_deref(),
            player_id.as_str(),
            request.agent_id.as_str(),
        )?;

        let mut candidate = current.clone();
        apply_prompt_patch(&mut candidate, &request);
        let applied_fields = changed_prompt_fields(&current, &candidate);
        let digest = prompt_profile_digest(&candidate);

        if applied_fields.is_empty() {
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
        candidate.updated_at_tick = self.kernel.time();
        candidate.updated_by = player_id.clone();

        self.apply_prompt_profile_to_driver(&candidate)?;
        self.bind_agent_player_access(
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let digest = prompt_profile_digest(&candidate);
        self.kernel.apply_agent_prompt_profile_update(
            candidate.clone(),
            PromptUpdateOperation::Apply,
            applied_fields.clone(),
            digest.clone(),
            None,
        );

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
    pub(crate) fn prompt_control_rollback(
        &mut self,
        request: PromptControlRollbackRequest,
    ) -> Result<PromptControlAck, PromptControlError> {
        let player_id =
            normalize_required_player_id(request.player_id.as_str(), request.agent_id.as_str())?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        self.verify_and_consume_prompt_control_rollback_auth(&request)?;
        ensure_agent_player_access(
            self.kernel(),
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let current = self.current_prompt_profile(request.agent_id.as_str())?;
        ensure_expected_prompt_version(
            request.agent_id.as_str(),
            current.version,
            request.expected_version,
        )?;
        ensure_updated_by_matches_player(
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

        let mut candidate = current.clone();
        candidate.system_prompt_override = target.system_prompt_override;
        candidate.short_term_goal_override = target.short_term_goal_override;
        candidate.long_term_goal_override = target.long_term_goal_override;
        let applied_fields = changed_prompt_fields(&current, &candidate);
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
        candidate.updated_at_tick = self.kernel.time();
        candidate.updated_by = player_id.clone();

        self.apply_prompt_profile_to_driver(&candidate)?;
        self.bind_agent_player_access(
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let digest = prompt_profile_digest(&candidate);
        self.kernel.apply_agent_prompt_profile_update(
            candidate.clone(),
            PromptUpdateOperation::Rollback,
            applied_fields.clone(),
            digest.clone(),
            Some(request.to_version),
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

    pub(crate) fn agent_chat(
        &mut self,
        request: AgentChatRequest,
    ) -> Result<AgentChatAck, AgentChatError> {
        let player_id = request
            .player_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let Some(player_id) = player_id else {
            return Err(AgentChatError {
                code: "player_id_required".to_string(),
                message: "agent_chat requires non-empty player_id".to_string(),
                agent_id: Some(request.agent_id),
            });
        };
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        let message = request.message.trim().to_string();
        if message.is_empty() {
            return Err(AgentChatError {
                code: "empty_message".to_string(),
                message: "chat message cannot be empty".to_string(),
                agent_id: Some(request.agent_id),
            });
        }
        self.verify_and_consume_agent_chat_auth(&request)?;

        if matches!(self.driver, LiveDriver::Script(_)) {
            return Err(AgentChatError {
                code: "llm_mode_required".to_string(),
                message: "agent chat requires live server running with --llm".to_string(),
                agent_id: Some(request.agent_id),
            });
        }

        self.bind_agent_player_access_for_chat(
            request.agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        let runner = match &mut self.driver {
            LiveDriver::Llm(runner) => runner,
            LiveDriver::Script(_) => unreachable!("script mode handled above"),
        };
        let Some(agent) = runner.get_mut(request.agent_id.as_str()) else {
            return Err(AgentChatError {
                code: "agent_not_registered".to_string(),
                message: format!("agent {} is not registered in llm runner", request.agent_id),
                agent_id: Some(request.agent_id),
            });
        };
        if !agent
            .behavior
            .push_player_message(self.kernel.time(), message.as_str())
        {
            return Err(AgentChatError {
                code: "empty_message".to_string(),
                message: "chat message cannot be empty".to_string(),
                agent_id: Some(request.agent_id),
            });
        }
        Ok(AgentChatAck {
            auth_nonce: None,
            agent_id: request.agent_id,
            accepted_at_tick: self.kernel.time(),
            message_len: message.chars().count(),
            player_id: Some(player_id),
            intent_tick: request.intent_tick,
            intent_seq: request
                .intent_seq
                .or_else(|| request.auth.as_ref().map(|auth| auth.nonce)),
            idempotent_replay: false,
            intent_id: None,
            accepted_event_seq: None,
            status: None,
            receipt_ref: None,
            replaced_by: None,
        })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(crate) fn verify_and_consume_prompt_control_apply_auth(
        &mut self,
        intent: PromptControlAuthIntent,
        request: &PromptControlApplyRequest,
    ) -> Result<(), PromptControlError> {
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
        self.kernel
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
            .map_err(|message| PromptControlError {
                code: "auth_nonce_replay".to_string(),
                message,
                agent_id: Some(request.agent_id.clone()),
                current_version: self.current_prompt_version(request.agent_id.as_str()),
                ..PromptControlError::default_legacy()
            })?;
        Ok(())
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(crate) fn verify_and_consume_prompt_control_rollback_auth(
        &mut self,
        request: &PromptControlRollbackRequest,
    ) -> Result<(), PromptControlError> {
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
        self.kernel
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
            .map_err(|message| PromptControlError {
                code: "auth_nonce_replay".to_string(),
                message,
                agent_id: Some(request.agent_id.clone()),
                current_version: self.current_prompt_version(request.agent_id.as_str()),
                ..PromptControlError::default_legacy()
            })?;
        Ok(())
    }

    pub(crate) fn verify_and_consume_agent_chat_auth(
        &mut self,
        request: &AgentChatRequest,
    ) -> Result<(), AgentChatError> {
        let Some(auth) = request.auth.as_ref() else {
            return Err(AgentChatError {
                code: "auth_proof_required".to_string(),
                message: "agent_chat requires auth proof".to_string(),
                agent_id: Some(request.agent_id.clone()),
            });
        };
        let verified =
            verify_agent_chat_auth_proof(request, auth).map_err(|message| AgentChatError {
                code: map_auth_verify_error_code(message.as_str()).to_string(),
                message,
                agent_id: Some(request.agent_id.clone()),
            })?;
        self.kernel
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
            .map_err(|message| AgentChatError {
                code: "auth_nonce_replay".to_string(),
                message,
                agent_id: Some(request.agent_id.clone()),
            })?;
        Ok(())
    }

    pub(crate) fn current_prompt_version(&self, agent_id: &str) -> Option<u64> {
        self.kernel
            .model()
            .agent_prompt_profiles
            .get(agent_id)
            .map(|profile| profile.version)
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(crate) fn current_prompt_profile(
        &self,
        agent_id: &str,
    ) -> Result<AgentPromptProfile, PromptControlError> {
        if !self.kernel.model().agents.contains_key(agent_id) {
            return Err(PromptControlError {
                code: "agent_not_found".to_string(),
                message: format!("agent not found: {agent_id}"),
                agent_id: Some(agent_id.to_string()),
                current_version: None,
                ..PromptControlError::default_legacy()
            });
        }
        Ok(self
            .kernel
            .model()
            .agent_prompt_profiles
            .get(agent_id)
            .cloned()
            .unwrap_or_else(|| AgentPromptProfile::for_agent(agent_id.to_string())))
    }

    pub(crate) fn lookup_prompt_profile_version(
        &self,
        agent_id: &str,
        version: u64,
    ) -> Option<AgentPromptProfile> {
        if let Some(profile) = self.kernel.model().agent_prompt_profiles.get(agent_id)
            && profile.version == version
        {
            return Some(profile.clone());
        }
        self.kernel.journal().iter().rev().find_map(|event| {
            let oasis7_simulator::WorldEventKind::AgentPromptUpdated { profile, .. } = &event.kind
            else {
                return None;
            };
            if profile.agent_id == agent_id && profile.version == version {
                Some(profile.clone())
            } else {
                None
            }
        })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(crate) fn apply_prompt_profile_to_driver(
        &mut self,
        profile: &AgentPromptProfile,
    ) -> Result<(), PromptControlError> {
        match &mut self.driver {
            LiveDriver::Script(_) => Err(PromptControlError {
                code: "llm_mode_required".to_string(),
                message: "prompt_control requires live server running with --llm".to_string(),
                agent_id: Some(profile.agent_id.clone()),
                current_version: self
                    .kernel
                    .model()
                    .agent_prompt_profiles
                    .get(&profile.agent_id)
                    .map(|entry| entry.version),
                ..PromptControlError::default_legacy()
            }),
            LiveDriver::Llm(runner) => {
                let Some(agent) = runner.get_mut(profile.agent_id.as_str()) else {
                    return Err(PromptControlError {
                        code: "agent_not_registered".to_string(),
                        message: format!(
                            "agent {} is not registered in llm runner",
                            profile.agent_id
                        ),
                        agent_id: Some(profile.agent_id.clone()),
                        current_version: self
                            .kernel
                            .model()
                            .agent_prompt_profiles
                            .get(&profile.agent_id)
                            .map(|entry| entry.version),
                        ..PromptControlError::default_legacy()
                    });
                };
                agent.behavior.apply_prompt_overrides(
                    profile.system_prompt_override.clone(),
                    profile.short_term_goal_override.clone(),
                    profile.long_term_goal_override.clone(),
                );
                Ok(())
            }
        }
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(crate) fn bind_agent_player_access(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
    ) -> Result<(), PromptControlError> {
        ensure_agent_player_access(self.kernel(), agent_id, player_id, public_key)?;
        let needs_bind = self.kernel.player_binding_for_agent(agent_id).is_none()
            || (public_key.is_some()
                && self.kernel.public_key_binding_for_agent(agent_id).is_none());
        if needs_bind {
            self.kernel
                .bind_agent_player(agent_id, player_id, public_key)
                .map_err(|message| PromptControlError {
                    code: "player_bind_failed".to_string(),
                    message,
                    agent_id: Some(agent_id.to_string()),
                    current_version: self
                        .kernel
                        .model()
                        .agent_prompt_profiles
                        .get(agent_id)
                        .map(|profile| profile.version),
                    ..PromptControlError::default_legacy()
                })?;
        }
        Ok(())
    }

    pub(crate) fn bind_agent_player_access_for_chat(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
    ) -> Result<(), AgentChatError> {
        let mapped = ensure_agent_player_access(self.kernel(), agent_id, player_id, public_key)
            .map_err(|err| AgentChatError {
                code: "agent_control_forbidden".to_string(),
                message: err.message,
                agent_id: err.agent_id,
            });
        mapped?;
        let needs_bind = self.kernel.player_binding_for_agent(agent_id).is_none()
            || (public_key.is_some()
                && self.kernel.public_key_binding_for_agent(agent_id).is_none());
        if needs_bind {
            self.kernel
                .bind_agent_player(agent_id, player_id, public_key)
                .map_err(|message| AgentChatError {
                    code: "player_bind_failed".to_string(),
                    message,
                    agent_id: Some(agent_id.to_string()),
                })?;
        }
        Ok(())
    }
}
