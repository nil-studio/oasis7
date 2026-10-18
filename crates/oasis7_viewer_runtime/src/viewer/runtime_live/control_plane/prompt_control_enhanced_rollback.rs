use super::*;

impl ViewerRuntimeLiveServer {
    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn handle_enhanced_prompt_rollback(
        &mut self,
        request: PromptControlRollbackRequest,
    ) -> Result<PromptControlAck, PromptControlError> {
        let operation = PromptControlOperation::Rollback;
        let request_id = match request.request_id.as_deref().map(str::trim) {
            Some(value) if !value.is_empty() && value.len() <= 128 => value.to_string(),
            Some(value) if value.len() > 128 => {
                return Err(prompt_control_enhanced_error(
                    "prompt_control_request_id_invalid",
                    "prompt_control request_id exceeds 128 UTF-8 bytes",
                    None,
                    operation,
                    false,
                    None,
                    None,
                    PromptControlResultStatus::Rejected,
                ));
            }
            _ => {
                return Err(prompt_control_enhanced_error(
                    "prompt_control_request_id_required",
                    "prompt_control request_id is required",
                    None,
                    operation,
                    false,
                    None,
                    None,
                    PromptControlResultStatus::Rejected,
                ));
            }
        };
        let agent_id = request.agent_id.trim().to_string();
        let player_id =
            normalize_required_player_id(request.player_id.as_str(), agent_id.as_str())?;
        if !self.llm_sidecar.is_llm_mode() {
            return Err(prompt_control_enhanced_error(
                "llm_mode_required",
                "prompt_control rollback requires runtime live server running with --llm",
                Some(request_id),
                operation,
                false,
                Some(agent_id.clone()),
                Some(player_id),
                PromptControlResultStatus::Blocked,
            ));
        }
        if !self.llm_sidecar.supports_prompt_control_result() {
            return Err(prompt_control_enhanced_error(
                "agent_provider_prompt_control_unsupported",
                "prompt_control is not supported when runtime live uses ProviderBacked(Local HTTP)",
                Some(request_id.clone()),
                operation,
                false,
                Some(agent_id),
                Some(player_id.clone()),
                PromptControlResultStatus::Rejected,
            ));
        }
        let Some(auth) = request.auth.as_ref() else {
            return Err(prompt_control_enhanced_error(
                "auth_required",
                "prompt_control rollback requires auth proof",
                Some(request_id),
                operation,
                false,
                None,
                None,
                PromptControlResultStatus::Blocked,
            ));
        };
        let verified = verify_prompt_control_rollback_auth_proof(&request, auth).map_err(|_| {
            prompt_control_enhanced_error(
                "auth_invalid",
                "prompt_control authentication failed",
                Some(request_id.clone()),
                operation,
                false,
                None,
                None,
                PromptControlResultStatus::Blocked,
            )
        })?;
        let expected_authority_epoch = request
            .expected_authority_epoch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                prompt_control_enhanced_error(
                    "expected_authority_epoch_required",
                    "prompt_control expected_authority_epoch is required",
                    Some(request_id.clone()),
                    operation,
                    false,
                    None,
                    None,
                    PromptControlResultStatus::Rejected,
                )
            })?;
        if expected_authority_epoch != self.prompt_control_authority.authority_epoch
            && self
                .llm_sidecar
                .bound_agent_for_player(player_id.as_str())
                .is_none()
            && !self
                .session_revoke_metadata
                .contains_key(&session_revoke_metadata_key(
                    player_id.as_str(),
                    verified.public_key.as_str(),
                ))
            && self
                .llm_sidecar
                .agent_player_bindings
                .get(agent_id.as_str())
                .is_none_or(|bound_player| bound_player == player_id.as_str())
        {
            return Err(prompt_control_result_unknown_error(&request_id));
        }
        let current_session_epoch = self
            .session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|_| {
                prompt_control_control_lost_error(
                    &request_id,
                    operation,
                    false,
                    Some(agent_id.clone()),
                )
            })?;
        if request.session_epoch != Some(current_session_epoch) {
            return Err(prompt_control_control_lost_error(
                &request_id,
                operation,
                false,
                Some(agent_id.clone()),
            ));
        }
        let current_binding_epoch = self
            .prompt_control_authority
            .binding_epoch(agent_id.as_str());
        if request.binding_epoch != Some(current_binding_epoch) {
            return Err(prompt_control_control_lost_error(
                &request_id,
                operation,
                false,
                Some(agent_id.clone()),
            ));
        }
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|error| {
            prompt_control_access_error(
                error,
                &request_id,
                operation,
                false,
                agent_id.as_str(),
                player_id.as_str(),
            )
        })?;
        if expected_authority_epoch != self.prompt_control_authority.authority_epoch {
            return Err(prompt_control_result_unknown_error(&request_id));
        }
        let expected_version = request.expected_version.ok_or_else(|| {
            prompt_control_enhanced_error(
                "expected_version_required",
                "prompt_control expected_version is required",
                Some(request_id.clone()),
                operation,
                false,
                Some(agent_id.clone()),
                Some(player_id.clone()),
                PromptControlResultStatus::Rejected,
            )
        })?;
        let updated_by = request.updated_by.as_deref().map(str::trim).unwrap_or("");
        if updated_by.is_empty() || updated_by != player_id {
            return Err(prompt_control_enhanced_error(
                "updated_by_required",
                "mutating prompt_control requests require updated_by matching player_id",
                Some(request_id),
                operation,
                false,
                Some(agent_id),
                Some(player_id),
                PromptControlResultStatus::Rejected,
            ));
        }
        let identity = normalize_prompt_control_operation_identity(
            "rollback",
            false,
            agent_id.as_str(),
            player_id.as_str(),
            request.session_epoch,
            request.binding_epoch,
            Some(expected_authority_epoch),
            request.expected_version,
            &None,
            &None,
            &None,
            Some(request.to_version),
            request.updated_by.as_deref(),
        )
        .map_err(|message| {
            prompt_control_enhanced_error(
                "prompt_control_identity_invalid",
                message.as_str(),
                Some(request_id.clone()),
                operation,
                false,
                Some(agent_id.clone()),
                Some(player_id.clone()),
                PromptControlResultStatus::Rejected,
            )
        })?;
        let operation_digest = prompt_control_operation_digest(&identity);
        match self.prompt_control_authority.result_ledger.lookup(
            self.prompt_control_authority.authority_epoch.as_str(),
            player_id.as_str(),
            request_id.as_str(),
            operation_digest.as_str(),
        ) {
            PromptControlLedgerLookup::Replay(PromptControlLedgerReceipt::Ack(mut ack)) => {
                ack.idempotent_replay = true;
                return Ok(ack);
            }
            PromptControlLedgerLookup::Replay(PromptControlLedgerReceipt::Error(mut error)) => {
                error.idempotent_replay = true;
                return Err(error);
            }
            PromptControlLedgerLookup::Conflict => {
                return Err(prompt_control_enhanced_error(
                    "request_id_conflict",
                    "request_id was already used for a different prompt operation",
                    Some(request_id),
                    operation,
                    false,
                    None,
                    None,
                    PromptControlResultStatus::Rejected,
                ));
            }
            PromptControlLedgerLookup::Missing => {}
        }
        if self.prompt_control_authority.result_ledger.is_full() {
            return Err(prompt_control_result_cache_full_error(
                Some(request_id),
                operation,
                false,
            ));
        }
        if self.hosted_public_join_mode() {
            self.verify_hosted_prompt_control_rollback_strong_auth(&request)
                .map_err(|error| {
                    prompt_control_enhanced_error(
                        if error.code == "strong_auth_required" {
                            "auth_required"
                        } else {
                            "auth_invalid"
                        },
                        "prompt_control authentication failed",
                        Some(request_id.clone()),
                        operation,
                        false,
                        None,
                        None,
                        PromptControlResultStatus::Blocked,
                    )
                })?;
        }
        self.prepare_hosted_local_mock_prompt_context_after_authorization(
            request_id.as_str(),
            operation,
            false,
        )
        .map_err(|error| *error)?;
        let current = self
            .current_prompt_profile(agent_id.as_str())
            .map_err(|_| {
                prompt_control_enhanced_error(
                    "agent_not_found",
                    "prompt control target Agent was not found",
                    Some(request_id.clone()),
                    operation,
                    false,
                    Some(agent_id.clone()),
                    Some(player_id.clone()),
                    PromptControlResultStatus::Rejected,
                )
            })?;
        if current.version != expected_version {
            let mut stale = prompt_control_enhanced_error(
                "version_conflict",
                "prompt control expected_version does not match current version",
                Some(request_id.clone()),
                operation,
                false,
                Some(agent_id.clone()),
                Some(player_id.clone()),
                PromptControlResultStatus::Stale,
            );
            stale.authority_epoch = Some(self.prompt_control_authority.authority_epoch.clone());
            stale.session_epoch = request.session_epoch;
            stale.binding_epoch = request.binding_epoch;
            stale.expected_version = Some(expected_version);
            stale.current_version = Some(current.version);
            stale.version = Some(current.version);
            stale.value_visibility = Some(PromptControlValueVisibility::LatestAllowed);
            stale.next_step = Some("refresh_prompt_version".to_string());
            stale.operation_digest = Some(operation_digest.clone());
            self.prompt_control_authority
                .result_ledger
                .validate_error(&stale)
                .map_err(|error| prompt_control_ledger_error(error, &request_id))?;
            self.llm_sidecar
                .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
                .map_err(|_message| {
                    prompt_control_enhanced_error(
                        "auth_invalid",
                        "prompt control authentication failed",
                        Some(request_id.clone()),
                        operation,
                        false,
                        None,
                        None,
                        PromptControlResultStatus::Blocked,
                    )
                })?;
            self.prompt_control_authority
                .result_ledger
                .insert_error(
                    self.prompt_control_authority.authority_epoch.as_str(),
                    player_id.as_str(),
                    request_id.as_str(),
                    operation_digest,
                    stale.clone(),
                )
                .map_err(|error| prompt_control_ledger_error(error, &request_id))?;
            return Err(stale);
        }
        let target = if request.to_version == 0 {
            AgentPromptProfile::for_agent(agent_id.clone())
        } else {
            let Some(target) =
                self.lookup_prompt_profile_version(agent_id.as_str(), request.to_version)
            else {
                let target_error = prompt_control_enhanced_error(
                    "target_version_not_found",
                    "prompt control rollback target version was not found",
                    Some(request_id.clone()),
                    operation,
                    false,
                    Some(agent_id.clone()),
                    Some(player_id.clone()),
                    PromptControlResultStatus::Rejected,
                );
                let recorded = self.record_enhanced_prompt_control_error(
                    verified.player_id.as_str(),
                    verified.nonce,
                    player_id.as_str(),
                    request_id.as_str(),
                    operation_digest.clone(),
                    target_error,
                    false,
                )?;
                return Err(recorded);
            };
            target
        };
        let mut candidate = current.clone();
        candidate.system_prompt_override = target.system_prompt_override;
        candidate.short_term_goal_override = target.short_term_goal_override;
        candidate.long_term_goal_override = target.long_term_goal_override;
        let applied_fields = changed_prompt_fields_runtime(&current, &candidate);
        if applied_fields.is_empty() {
            let noop_error = prompt_control_enhanced_error(
                "rollback_noop",
                "prompt control rollback would not change the current profile",
                Some(request_id.clone()),
                operation,
                false,
                Some(agent_id.clone()),
                Some(player_id.clone()),
                PromptControlResultStatus::Rejected,
            );
            let recorded = self.record_enhanced_prompt_control_error(
                verified.player_id.as_str(),
                verified.nonce,
                player_id.as_str(),
                request_id.as_str(),
                operation_digest.clone(),
                noop_error,
                false,
            )?;
            return Err(recorded);
        }
        candidate.version = current.version.saturating_add(1);
        candidate.updated_at_tick = self.world.state().time;
        candidate.updated_by = player_id.clone();
        let digest = prompt_profile_digest_runtime(&candidate);
        let mut ack = PromptControlAck {
            request_id: Some(request_id.clone()),
            authority_epoch: Some(self.prompt_control_authority.authority_epoch.clone()),
            agent_id: agent_id.clone(),
            operation,
            preview: false,
            status: Some(PromptControlResultStatus::Applied),
            player_id: Some(player_id.clone()),
            session_epoch: request.session_epoch,
            binding_epoch: request.binding_epoch,
            expected_version: Some(expected_version),
            version: candidate.version,
            updated_at_tick: candidate.updated_at_tick,
            applied_fields: applied_fields.clone(),
            digest: digest.clone(),
            value_visibility: Some(PromptControlValueVisibility::LatestAllowed),
            applied_scope: Some(PromptControlApplicationScope::RuntimeInstance),
            persistence_scope: Some(PromptControlApplicationScope::None),
            sync_scope: Some(PromptControlApplicationScope::None),
            reason_code: Some("rolled_back".to_string()),
            next_step: Some("continue_runtime".to_string()),
            operation_digest: Some(operation_digest.clone()),
            idempotent_replay: false,
            mutation_count: Some(1),
            rolled_back_to_version: Some(request.to_version),
        };
        self.prompt_control_authority
            .result_ledger
            .validate_receipt(&ack)
            .map_err(|error| prompt_control_ledger_error(error, &request_id))?;
        self.llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
            .map_err(|_message| {
                prompt_control_enhanced_error(
                    "auth_invalid",
                    "prompt control authentication failed",
                    Some(request_id.clone()),
                    operation,
                    false,
                    None,
                    None,
                    PromptControlResultStatus::Blocked,
                )
            })?;
        if self.prompt_control_authority.authority_epoch != expected_authority_epoch
            || self
                .prompt_control_authority
                .binding_epoch(agent_id.as_str())
                != current_binding_epoch
        {
            return Err(prompt_control_result_unknown_error(&request_id));
        }
        if let Err(message) = self.llm_sidecar.apply_prompt_profile_to_driver(&candidate) {
            let enqueue_error = PromptControlError {
                code: "prompt_override_enqueue_failed".to_string(),
                message,
                request_id: Some(request_id.clone()),
                authority_epoch: Some(self.prompt_control_authority.authority_epoch.clone()),
                operation: Some(operation),
                preview: Some(false),
                status: Some(PromptControlResultStatus::Blocked),
                value_visibility: Some(PromptControlValueVisibility::Hidden),
                agent_id: Some(agent_id.clone()),
                player_id: Some(player_id.clone()),
                expected_version: Some(expected_version),
                current_version: Some(current.version),
                reason_code: Some("prompt_override_enqueue_failed".to_string()),
                ..PromptControlError::default_legacy()
            };
            let recorded = self.record_enhanced_prompt_control_error(
                verified.player_id.as_str(),
                verified.nonce,
                player_id.as_str(),
                request_id.as_str(),
                operation_digest.clone(),
                enqueue_error,
                true,
            )?;
            return Err(recorded);
        }
        self.llm_sidecar.upsert_prompt_profile(candidate.clone());
        self.bind_agent_player_access(
            agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )?;
        self.enqueue_virtual_event(WorldEventKind::AgentPromptUpdated {
            profile: candidate.clone(),
            operation: PromptUpdateOperation::Rollback,
            applied_fields: applied_fields.clone(),
            digest: digest.clone(),
            rolled_back_to_version: Some(request.to_version),
        });
        self.record_primary_intent_from_short_term_goal(
            agent_id.as_str(),
            candidate.short_term_goal_override.as_deref(),
        );
        self.llm_sidecar.request_decision();
        self.prompt_control_authority
            .result_ledger
            .insert(
                self.prompt_control_authority.authority_epoch.as_str(),
                player_id.as_str(),
                request_id.as_str(),
                operation_digest,
                ack.clone(),
            )
            .map_err(|error| prompt_control_ledger_error(error, &request_id))?;
        ack.idempotent_replay = false;
        Ok(ack)
    }
}
