use super::*;

#[path = "prompt_control_enhanced_rollback.rs"]
mod prompt_control_enhanced_rollback;

impl ViewerRuntimeLiveServer {
    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(in crate::viewer::runtime_live) fn handle_prompt_control_for_protocol(
        &mut self,
        command: PromptControlCommand,
        negotiated: &crate::viewer::protocol::NegotiatedViewerProtocol,
    ) -> Result<PromptControlAck, PromptControlError> {
        let capability_selected =
            crate::viewer::protocol::viewer_protocol_supports_prompt_control_result(negotiated);
        let enhanced = match &command {
            PromptControlCommand::Preview { request } | PromptControlCommand::Apply { request } => {
                has_enhanced_prompt_identity_apply(request)
            }
            PromptControlCommand::Rollback { request } => {
                has_enhanced_prompt_identity_rollback(request)
            }
        };
        if capability_selected {
            let missing_field = match &command {
                PromptControlCommand::Preview { request }
                | PromptControlCommand::Apply { request } => {
                    Self::missing_enhanced_apply_field(request)
                }
                PromptControlCommand::Rollback { request } => {
                    Self::missing_enhanced_rollback_field(request)
                }
            };
            if let Some(field) = missing_field {
                let (request_id, operation, preview) = match &command {
                    PromptControlCommand::Preview { request } => (
                        request.request_id.clone(),
                        PromptControlOperation::Apply,
                        true,
                    ),
                    PromptControlCommand::Apply { request } => (
                        request.request_id.clone(),
                        PromptControlOperation::Apply,
                        false,
                    ),
                    PromptControlCommand::Rollback { request } => (
                        request.request_id.clone(),
                        PromptControlOperation::Rollback,
                        false,
                    ),
                };
                return Err(Self::prompt_control_field_required_error(
                    request_id, operation, preview, field,
                ));
            }
        } else if enhanced {
            return Err(PromptControlError {
                code: "prompt_control_capability_required".to_string(),
                message: "prompt_control result capability is required for enhanced requests"
                    .to_string(),
                ..PromptControlError::default_legacy()
            });
        } else {
            return self.handle_prompt_control(command);
        }
        match command {
            PromptControlCommand::Preview { request } => {
                self.handle_enhanced_prompt_apply(request, true)
            }
            PromptControlCommand::Apply { request } => {
                self.handle_enhanced_prompt_apply(request, false)
            }
            PromptControlCommand::Rollback { request } => {
                self.handle_enhanced_prompt_rollback(request)
            }
        }
    }

    fn missing_enhanced_apply_field(request: &PromptControlApplyRequest) -> Option<&'static str> {
        if request
            .request_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Some("request_id");
        }
        if request.session_epoch.is_none() {
            return Some("session_epoch");
        }
        if request.binding_epoch.is_none() {
            return Some("binding_epoch");
        }
        if request
            .expected_authority_epoch
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Some("expected_authority_epoch");
        }
        if request.expected_version.is_none() {
            return Some("expected_version");
        }
        None
    }

    fn missing_enhanced_rollback_field(
        request: &PromptControlRollbackRequest,
    ) -> Option<&'static str> {
        if request
            .request_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Some("request_id");
        }
        if request.session_epoch.is_none() {
            return Some("session_epoch");
        }
        if request.binding_epoch.is_none() {
            return Some("binding_epoch");
        }
        if request
            .expected_authority_epoch
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Some("expected_authority_epoch");
        }
        if request.expected_version.is_none() {
            return Some("expected_version");
        }
        None
    }

    fn prompt_control_field_required_error(
        request_id: Option<String>,
        operation: PromptControlOperation,
        preview: bool,
        field: &str,
    ) -> PromptControlError {
        prompt_control_enhanced_error(
            "prompt_control_field_required",
            &format!("prompt_control {field} is required when the result capability is selected"),
            request_id,
            operation,
            preview,
            None,
            None,
            PromptControlResultStatus::Rejected,
        )
    }

    fn prepare_hosted_local_mock_prompt_context_after_authorization(
        &mut self,
        request_id: &str,
        operation: PromptControlOperation,
        preview: bool,
    ) -> Result<(), Box<PromptControlError>> {
        if !(self.hosted_local_mock_test_lane_active && self.llm_sidecar.is_llm_mode()) {
            return Ok(());
        }
        self.llm_sidecar
            .prepare_hosted_local_mock_prompt_context(
                &mut self.world,
                &self.snapshot_config,
                self.config.world_id.as_str(),
            )
            .map_err(|error| {
                tracing::warn!(
                    error = %error,
                    "Hosted local-mock prompt context preparation blocked authorized prompt control"
                );
                let mut blocked = prompt_control_enhanced_error(
                    "prompt_control_runtime_context_unavailable",
                    &format!("Hosted local-mock prompt context preparation failed: {error}"),
                    Some(request_id.to_string()),
                    operation,
                    preview,
                    None,
                    None,
                    PromptControlResultStatus::Blocked,
                );
                blocked.value_visibility = Some(PromptControlValueVisibility::Hidden);
                blocked.reason_code =
                    Some("prompt_control_runtime_context_unavailable".to_string());
                blocked.next_step = Some("retry_after_runtime_resync".to_string());
                Box::new(blocked)
            })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    fn handle_enhanced_prompt_apply(
        &mut self,
        request: PromptControlApplyRequest,
        preview: bool,
    ) -> Result<PromptControlAck, PromptControlError> {
        let operation = PromptControlOperation::Apply;
        let request_id = match request.request_id.as_deref().map(str::trim) {
            Some(value) if !value.is_empty() && value.len() <= 128 => value.to_string(),
            Some(value) if value.len() > 128 => {
                return Err(prompt_control_enhanced_error(
                    "prompt_control_request_id_invalid",
                    "prompt_control request_id exceeds 128 UTF-8 bytes",
                    None,
                    operation,
                    preview,
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
                    preview,
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
                "prompt_control requires runtime live server running with --llm",
                Some(request_id),
                operation,
                preview,
                Some(agent_id.clone()),
                Some(player_id),
                PromptControlResultStatus::Blocked,
            ));
        }
        if !self.llm_sidecar.supports_prompt_control_result() {
            return Err(prompt_control_enhanced_error(
                "agent_provider_prompt_control_unsupported",
                "prompt_control is not supported when runtime live uses ProviderBacked(Local HTTP)",
                Some(request_id),
                operation,
                preview,
                Some(agent_id.clone()),
                Some(player_id),
                PromptControlResultStatus::Rejected,
            ));
        }
        let Some(auth) = request.auth.as_ref() else {
            return Err(prompt_control_enhanced_error(
                "auth_required",
                "prompt_control requires auth proof",
                Some(request_id),
                operation,
                preview,
                None,
                None,
                PromptControlResultStatus::Blocked,
            ));
        };
        let verified = verify_prompt_control_apply_auth_proof(
            if preview {
                PromptControlAuthIntent::Preview
            } else {
                PromptControlAuthIntent::Apply
            },
            &request,
            auth,
        )
        .map_err(|_| {
            prompt_control_enhanced_error(
                "auth_invalid",
                "prompt_control authentication failed",
                Some(request_id.clone()),
                operation,
                preview,
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
                    preview,
                    None,
                    None,
                    PromptControlResultStatus::Rejected,
                )
            })?;
        // A fresh authority has no live binding/session state to evaluate. In
        // that restart-only case preserve the redacted result_unknown fence;
        // when a binding or revoke record exists, continue so control_lost can
        // take precedence over the stale authority epoch.
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
                    preview,
                    Some(agent_id.clone()),
                )
            })?;
        if request.session_epoch != Some(current_session_epoch) {
            return Err(prompt_control_control_lost_error(
                &request_id,
                operation,
                preview,
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
                preview,
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
                preview,
                agent_id.as_str(),
                player_id.as_str(),
            )
        })?;
        // Session and binding loss are player-visible control loss. They must
        // win over an authority fence so stale requests cannot be relabeled as
        // result_unknown when the player no longer controls the Agent.
        if expected_authority_epoch != self.prompt_control_authority.authority_epoch {
            return Err(prompt_control_result_unknown_error(&request_id));
        }
        let expected_version = request.expected_version.ok_or_else(|| {
            prompt_control_enhanced_error(
                "expected_version_required",
                "prompt_control expected_version is required",
                Some(request_id.clone()),
                operation,
                preview,
                Some(agent_id.clone()),
                Some(player_id.clone()),
                PromptControlResultStatus::Rejected,
            )
        })?;
        if !preview {
            let updated_by = request.updated_by.as_deref().map(str::trim).unwrap_or("");
            if updated_by.is_empty() || updated_by != player_id {
                return Err(prompt_control_enhanced_error(
                    "updated_by_required",
                    "mutating prompt_control requests require updated_by matching player_id",
                    Some(request_id),
                    operation,
                    preview,
                    Some(agent_id),
                    Some(player_id),
                    PromptControlResultStatus::Rejected,
                ));
            }
        } else {
            ensure_updated_by_matches_player_runtime(
                request.updated_by.as_deref(),
                player_id.as_str(),
                agent_id.as_str(),
            )
            .map_err(|message| {
                prompt_control_enhanced_error(
                    "updated_by_invalid",
                    message.message.as_str(),
                    Some(request_id.clone()),
                    operation,
                    preview,
                    Some(agent_id.clone()),
                    Some(player_id.clone()),
                    PromptControlResultStatus::Rejected,
                )
            })?;
        }
        let identity = normalize_prompt_control_operation_identity(
            "apply",
            preview,
            agent_id.as_str(),
            player_id.as_str(),
            request.session_epoch,
            request.binding_epoch,
            Some(expected_authority_epoch),
            request.expected_version,
            &request.system_prompt_override,
            &request.short_term_goal_override,
            &request.long_term_goal_override,
            None,
            request.updated_by.as_deref(),
        )
        .map_err(|message| {
            prompt_control_enhanced_error(
                "prompt_control_identity_invalid",
                message.as_str(),
                Some(request_id.clone()),
                operation,
                preview,
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
                    preview,
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
                preview,
            ));
        }
        if self.hosted_public_join_mode() {
            self.verify_hosted_prompt_control_apply_strong_auth(
                if preview {
                    PromptControlAuthIntent::Preview
                } else {
                    PromptControlAuthIntent::Apply
                },
                &request,
            )
            .map_err(|error| {
                prompt_control_enhanced_error(
                    if error.code == "strong_auth_required" {
                        "auth_required"
                    } else {
                        "auth_invalid"
                    },
                    "prompt control authentication failed",
                    Some(request_id.clone()),
                    operation,
                    preview,
                    None,
                    None,
                    PromptControlResultStatus::Blocked,
                )
            })?;
        }
        self.prepare_hosted_local_mock_prompt_context_after_authorization(
            request_id.as_str(),
            operation,
            preview,
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
                    preview,
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
                preview,
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
                        preview,
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
        let mut candidate = current.clone();
        apply_prompt_patch_runtime(&mut candidate, &request);
        let applied_fields = changed_prompt_fields_runtime(&current, &candidate);
        let changed = !applied_fields.is_empty();
        let version = if changed {
            current.version.saturating_add(1)
        } else {
            current.version
        };
        if changed {
            candidate.version = version;
            candidate.updated_at_tick = self.world.state().time;
            candidate.updated_by = player_id.clone();
        }
        let digest = prompt_profile_digest_runtime(&candidate);
        let mut ack = PromptControlAck {
            request_id: Some(request_id.clone()),
            authority_epoch: Some(self.prompt_control_authority.authority_epoch.clone()),
            agent_id: agent_id.clone(),
            operation,
            preview,
            status: Some(if changed && !preview {
                PromptControlResultStatus::Applied
            } else {
                PromptControlResultStatus::Accepted
            }),
            player_id: Some(player_id.clone()),
            session_epoch: request.session_epoch,
            binding_epoch: request.binding_epoch,
            expected_version: Some(expected_version),
            version,
            updated_at_tick: candidate.updated_at_tick,
            applied_fields: applied_fields.clone(),
            digest: digest.clone(),
            value_visibility: Some(PromptControlValueVisibility::LatestAllowed),
            applied_scope: Some(if changed && !preview {
                PromptControlApplicationScope::RuntimeInstance
            } else {
                PromptControlApplicationScope::None
            }),
            persistence_scope: Some(PromptControlApplicationScope::None),
            sync_scope: Some(PromptControlApplicationScope::None),
            reason_code: Some(
                if preview && changed {
                    "preview_only"
                } else if changed {
                    "applied"
                } else {
                    "no_change"
                }
                .to_string(),
            ),
            next_step: Some(
                if preview && changed {
                    "confirm_apply"
                } else if changed {
                    "continue_runtime"
                } else {
                    "no_action_required"
                }
                .to_string(),
            ),
            operation_digest: Some(operation_digest.clone()),
            idempotent_replay: false,
            mutation_count: Some(u64::from(changed && !preview)),
            rolled_back_to_version: None,
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
                    preview,
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
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            agent_id.as_str(),
            player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|_| prompt_control_result_unknown_error(&request_id))?;
        if !preview && changed {
            if let Err(message) = self.llm_sidecar.apply_prompt_profile_to_driver(&candidate) {
                let enqueue_error = PromptControlError {
                    code: "prompt_override_enqueue_failed".to_string(),
                    message,
                    request_id: Some(request_id.clone()),
                    authority_epoch: Some(self.prompt_control_authority.authority_epoch.clone()),
                    operation: Some(operation),
                    preview: Some(preview),
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
                operation: PromptUpdateOperation::Apply,
                applied_fields: applied_fields.clone(),
                digest: digest.clone(),
                rolled_back_to_version: None,
            });
            if request.short_term_goal_override.is_some() {
                self.record_primary_intent_from_short_term_goal(
                    agent_id.as_str(),
                    candidate.short_term_goal_override.as_deref(),
                );
            }
            self.llm_sidecar.request_decision();
        }
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
