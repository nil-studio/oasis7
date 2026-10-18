use super::*;

impl ViewerRuntimeLiveServer {
    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn release_hosted_session(
        &mut self,
        request: crate::viewer::HostedSessionReleaseRequest,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        if !crate::viewer::hosted_identity_v2_enabled() {
            return Err(recovery_error(
                "hosted_identity_v2_required",
                "hosted release requires identity v2",
                None,
                None,
                None,
            ));
        }
        let context =
            crate::viewer::auth::verify_hosted_release_grant(&request.authorization_grant)
                .map_err(|_| {
                    recovery_error(
                        "hosted_authorization_invalid",
                        "hosted release authorization rejected",
                        None,
                        None,
                        None,
                    )
                })?;
        if self
            .session_policy
            .active_session_public_key(&context.player_id)
            != Some(context.session_public_key.as_str())
            || self.session_policy.session_epoch(&context.player_id) != context.world_session_epoch
        {
            return Err(recovery_error(
                "state_changed",
                "hosted world control changed",
                None,
                Some(context.player_id),
                None,
            ));
        }
        self.session_policy
            .validate_authorization_registration(&context, context.world_session_epoch)
            .map_err(|message| {
                self.recovery_error_from_session_policy(
                    message,
                    context.player_id.clone(),
                    Some(context.session_public_key.clone()),
                )
            })?;
        self.revoke_session_key(AuthoritativeSessionRevokeRequest {
            player_id: context.player_id,
            session_pubkey: Some(context.session_public_key),
            revoke_reason: "account_world_release".into(),
            revoked_by: Some("hosted_identity_v2".into()),
        })
    }
    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn handle_reconnect_sync(
        &mut self,
        request: AuthoritativeReconnectSyncRequest,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        let player_id = request.player_id.trim().to_string();
        if player_id.is_empty() {
            return Err(recovery_error(
                "player_id_required",
                "reconnect_sync requires non-empty player_id",
                None,
                None,
                None,
            ));
        }

        let mut session_pubkey = request
            .session_pubkey
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let mut session_epoch = None;
        if let Some(pubkey) = session_pubkey.as_deref() {
            let epoch = match self
                .session_policy
                .validate_known_session_key(player_id.as_str(), pubkey)
            {
                Ok(epoch) => epoch,
                Err(message) => {
                    return Err(self.recovery_error_from_session_policy(
                        message,
                        player_id.clone(),
                        Some(pubkey.to_string()),
                    ));
                }
            };
            session_epoch = Some(epoch);
        }
        if session_pubkey.is_none() && crate::viewer::hosted_identity_v2_enabled() {
            // Public key/epoch are fencing metadata, never a bearer capability.
            // Identity's loopback adapter needs the actual CAS version even
            // after its prior short authorization has expired.
            session_pubkey = self
                .session_policy
                .active_session_public_key(&player_id)
                .map(str::to_owned);
            session_epoch = Some(self.session_policy.session_epoch(&player_id));
        }

        let cursor = self.current_recovery_cursor().map_err(|err| {
            recovery_error(
                "cursor_compute_failed",
                format!("{err:?}"),
                None,
                Some(player_id.clone()),
                session_pubkey.clone(),
            )
        })?;
        let stable_cursor = self
            .stable_checkpoints
            .back()
            .map(|entry| entry.log_cursor)
            .unwrap_or(0);

        let mut reasons = Vec::new();
        if request
            .expected_reorg_epoch
            .is_some_and(|epoch| epoch != self.reorg_epoch)
        {
            reasons.push(format!(
                "expected_reorg_epoch mismatch (client={}, server={})",
                request.expected_reorg_epoch.unwrap_or_default(),
                self.reorg_epoch
            ));
        }
        if let Some(last_known_cursor) = request.last_known_log_cursor {
            if last_known_cursor > cursor.log_cursor {
                reasons.push(format!(
                    "client cursor {} is ahead of server cursor {}",
                    last_known_cursor, cursor.log_cursor
                ));
            }
            if last_known_cursor < stable_cursor {
                reasons.push(format!(
                    "client cursor {} is behind stable cursor {}",
                    last_known_cursor, stable_cursor
                ));
            }
        }
        let message = if reasons.is_empty() {
            Some("delta_replay_allowed".to_string())
        } else {
            Some(format!("snapshot_reload_required: {}", reasons.join("; ")))
        };
        let bound_agent_id = self
            .llm_sidecar
            .bound_agent_for_player(player_id.as_str())
            .map(ToOwned::to_owned);
        let binding_epoch = bound_agent_id
            .as_deref()
            .map(|agent_id| self.prompt_control_authority.binding_epoch(agent_id));

        Ok(AuthoritativeRecoveryAck {
            status: AuthoritativeRecoveryStatus::CatchUpReady,
            reorg_epoch: self.reorg_epoch,
            snapshot_height: cursor.snapshot_height,
            snapshot_hash: cursor.snapshot_hash,
            log_cursor: cursor.log_cursor,
            stable_batch_id: cursor.stable_batch_id,
            player_id: Some(player_id),
            agent_id: bound_agent_id,
            session_pubkey,
            replaced_by_pubkey: None,
            session_epoch,
            binding_epoch,
            message,
            revoke_reason: None,
            revoked_by: None,
            rollback_receipt: None,
            acknowledged_at_tick: self.world.state().time,
        })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn revoke_session_key(
        &mut self,
        request: AuthoritativeSessionRevokeRequest,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        let player_id = request.player_id.trim().to_string();
        if player_id.is_empty() {
            return Err(recovery_error(
                "player_id_required",
                "revoke_session requires non-empty player_id",
                None,
                None,
                None,
            ));
        }

        let mutation_snapshot = self.session_mutation_snapshot();
        let (revoked_pubkey, session_epoch) = self
            .session_policy
            .revoke_session(player_id.as_str(), request.session_pubkey.as_deref())
            .map_err(|message| {
                recovery_error(
                    map_session_policy_error_code(message.as_str()),
                    message,
                    None,
                    Some(player_id.clone()),
                    request.session_pubkey.clone(),
                )
            })?;
        let revoke_metadata = RuntimeSessionRevokeMetadata {
            revoke_reason: normalize_optional_string(Some(request.revoke_reason.as_str())),
            revoked_by: normalize_optional_string(request.revoked_by.as_deref()),
        };
        self.record_session_revoke_metadata(
            player_id.as_str(),
            revoked_pubkey.as_str(),
            revoke_metadata.clone(),
        );
        self.clear_player_auth_runtime_state(player_id.as_str());
        self.apply_session_revoke_binding(player_id.as_str(), revoked_pubkey.as_str());

        let cursor = self.current_recovery_cursor().map_err(|err| {
            recovery_error(
                "cursor_compute_failed",
                format!("{err:?}"),
                None,
                Some(player_id.clone()),
                Some(revoked_pubkey.clone()),
            )
        })?;
        let ack = AuthoritativeRecoveryAck {
            status: AuthoritativeRecoveryStatus::SessionRevoked,
            reorg_epoch: self.reorg_epoch,
            snapshot_height: cursor.snapshot_height,
            snapshot_hash: cursor.snapshot_hash,
            log_cursor: cursor.log_cursor,
            stable_batch_id: cursor.stable_batch_id,
            player_id: Some(player_id),
            agent_id: None,
            session_pubkey: Some(revoked_pubkey),
            replaced_by_pubkey: None,
            session_epoch: Some(session_epoch),
            binding_epoch: None,
            message: Some(request.revoke_reason.trim().to_string()),
            revoke_reason: revoke_metadata.revoke_reason,
            revoked_by: revoke_metadata.revoked_by,
            rollback_receipt: None,
            acknowledged_at_tick: self.world.state().time,
        };
        if let Err(error) = self.persist_current_recovery_generation(&ack) {
            self.restore_session_mutation_snapshot(mutation_snapshot);
            return Err(error);
        }
        Ok(ack)
    }

    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn rotate_session_key(
        &mut self,
        request: AuthoritativeSessionRotateRequest,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        let player_id = request.player_id.trim().to_string();
        if player_id.is_empty() {
            return Err(recovery_error(
                "player_id_required",
                "rotate_session requires non-empty player_id",
                None,
                None,
                None,
            ));
        }

        let mutation_snapshot = self.session_mutation_snapshot();
        let session_epoch = self
            .session_policy
            .rotate_session(
                player_id.as_str(),
                request.old_session_pubkey.as_str(),
                request.new_session_pubkey.as_str(),
            )
            .map_err(|message| {
                recovery_error(
                    map_session_policy_error_code(message.as_str()),
                    message,
                    None,
                    Some(player_id.clone()),
                    Some(request.old_session_pubkey.clone()),
                )
            })?;
        let rotation_metadata = RuntimeSessionRevokeMetadata {
            revoke_reason: normalize_optional_string(Some(request.rotate_reason.as_str())),
            revoked_by: normalize_optional_string(request.rotated_by.as_deref()),
        };
        self.record_session_revoke_metadata(
            player_id.as_str(),
            request.old_session_pubkey.as_str(),
            rotation_metadata.clone(),
        );
        self.clear_player_auth_runtime_state(player_id.as_str());
        self.apply_session_rotate_binding(
            player_id.as_str(),
            request.old_session_pubkey.as_str(),
            request.new_session_pubkey.as_str(),
        );

        let cursor = self.current_recovery_cursor().map_err(|err| {
            recovery_error(
                "cursor_compute_failed",
                format!("{err:?}"),
                None,
                Some(player_id.clone()),
                Some(request.old_session_pubkey.clone()),
            )
        })?;
        let ack = AuthoritativeRecoveryAck {
            status: AuthoritativeRecoveryStatus::SessionRotated,
            reorg_epoch: self.reorg_epoch,
            snapshot_height: cursor.snapshot_height,
            snapshot_hash: cursor.snapshot_hash,
            log_cursor: cursor.log_cursor,
            stable_batch_id: cursor.stable_batch_id,
            player_id: Some(player_id),
            agent_id: self
                .llm_sidecar
                .bound_agent_for_player(request.player_id.as_str())
                .map(ToOwned::to_owned),
            session_pubkey: Some(request.old_session_pubkey),
            replaced_by_pubkey: Some(request.new_session_pubkey),
            session_epoch: Some(session_epoch),
            binding_epoch: None,
            message: Some(request.rotate_reason.trim().to_string()),
            revoke_reason: rotation_metadata.revoke_reason,
            revoked_by: rotation_metadata.revoked_by,
            rollback_receipt: None,
            acknowledged_at_tick: self.world.state().time,
        };
        if let Err(error) = self.persist_current_recovery_generation(&ack) {
            self.restore_session_mutation_snapshot(mutation_snapshot);
            return Err(error);
        }
        Ok(ack)
    }
}
