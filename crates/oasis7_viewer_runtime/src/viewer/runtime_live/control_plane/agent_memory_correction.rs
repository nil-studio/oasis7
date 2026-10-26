use super::super::agency_control::{
    AgencyControlOperation, AgencyControlRequest, AgencyControlResponse,
};
use crate::runtime::{AgentOwnerControlKindV1, AgentOwnerControlV1};
use crate::viewer::ViewerRuntimeLiveServer;
use crate::viewer::auth::verify_agency_control_auth_proof;
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

/// One-call adapter over the actual live host registration and Agent binding.
pub(super) struct CurrentOwnerAuthority<'a> {
    pub(super) server: &'a ViewerRuntimeLiveServer,
}
impl crate::runtime::OwnerControlHostAuthority for CurrentOwnerAuthority<'_> {
    fn validate_registered_owner(
        &self,
        player_id: &str,
        public_key: &str,
        agent_id: &str,
        world_id: &str,
        reorg_epoch: u64,
    ) -> Result<(), String> {
        if world_id != self.server.config.world_id || reorg_epoch != self.server.reorg_epoch {
            return Err("request world or reorg epoch is no longer current".into());
        }
        self.server
            .session_policy
            .validate_known_session_key(player_id, public_key)?;
        super::ensure_agent_player_access_runtime(
            &self.server.world,
            &self.server.llm_sidecar,
            agent_id,
            player_id,
            Some(public_key),
        )
        .map_err(|error| error.message)
    }
}

impl ViewerRuntimeLiveServer {
    #[cfg(test)]
    pub(in crate::viewer::runtime_live) fn current_owner_authority(
        &self,
    ) -> impl crate::runtime::OwnerControlHostAuthority + '_ {
        CurrentOwnerAuthority { server: self }
    }
}

fn agency_error(request_id: &str, code: &str, message: impl Into<String>) -> AgencyControlResponse {
    AgencyControlResponse::error(request_id, code, message)
}

fn operation_payload(command: &AgencyControlOperation) -> Result<Value, String> {
    serde_json::to_value(command)
        .map_err(|error| format!("agency operation serialization failed: {error}"))
}

fn action_kind_valid(kind: &str) -> bool {
    matches!(kind, "MoveAgent" | "TransferMaterial" | "ScheduleRecipe")
}

impl ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn handle_agency_control_request(
        &mut self,
        request: AgencyControlRequest,
    ) -> AgencyControlResponse {
        let request_id = request.request_id.as_str();
        if self.chain_link_enabled()
            && matches!(
                &request.command,
                AgencyControlOperation::InstallDelegation { .. }
                    | AgencyControlOperation::RevokeDelegation { .. }
                    | AgencyControlOperation::OverridePendingIntent { .. }
                    | AgencyControlOperation::InterruptPendingIntent { .. }
            )
        {
            return agency_error(
                request_id,
                "canonical_agency_control_unavailable",
                "world service has no registered canonical owner-control capability",
            );
        }
        if request.frame_type != super::super::agency_control::AGENCY_CONTROL_REQUEST_TYPE
            || request_id.trim().is_empty()
            || request_id.len() > 256
        {
            return agency_error(request_id, "invalid_request", "request identity is invalid");
        }
        if self.resolve_authoritative_recovery_write_fence().is_err()
            || self.authoritative_recovery_write_fence.is_some()
        {
            return agency_error(
                request_id,
                "runtime_recovery_fenced",
                "agency controls are unavailable while Runtime recovery is unresolved",
            );
        }
        if request.world_id != self.config.world_id || request.reorg_epoch != self.reorg_epoch {
            return agency_error(
                request_id,
                "stale_world_position",
                "request world or reorg epoch is no longer current",
            );
        }
        let Some(proof) = request.auth.as_ref() else {
            return agency_error(
                request_id,
                "auth_proof_required",
                "agency control requires owner auth proof",
            );
        };
        let Ok(command_value) = operation_payload(&request.command) else {
            return agency_error(
                request_id,
                "invalid_request",
                "agency operation cannot be serialized",
            );
        };
        let verified = match verify_agency_control_auth_proof(
            request_id,
            request.player_id.as_str(),
            request.public_key.as_str(),
            request.agent_id.as_str(),
            request.world_id.as_str(),
            request.reorg_epoch,
            &command_value,
            proof,
        ) {
            Ok(verified) => verified,
            Err(message) => return agency_error(request_id, "auth_proof_invalid", message),
        };
        if let Err(message) = self
            .session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
        {
            return agency_error(request_id, "player_session_invalid", message);
        }
        if super::ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            request.agent_id.as_str(),
            verified.player_id.as_str(),
            Some(verified.public_key.as_str()),
        )
        .is_err()
        {
            return agency_error(
                request_id,
                "agent_control_forbidden",
                "this player is not the bound owner of the requested Agent",
            );
        }
        let Some(claim) = self.world.current_agent_owner(&request.agent_id) else {
            return agency_error(
                request_id,
                "owner_source_unavailable",
                "the Agent has no current Starter OC owner source",
            );
        };
        if claim.player_id != verified.player_id {
            return agency_error(
                request_id,
                "agent_control_forbidden",
                "this player is not the current Starter OC owner",
            );
        }
        let owner_id = claim.player_id.clone();
        if let Err(message) = self
            .llm_sidecar
            .validate_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
        {
            // Exact idempotent mutations are checked before nonce consumption
            // by their operation handlers below; every new request remains
            // monotonic and cannot replay a previous signed payload.
            if !matches!(request.command, AgencyControlOperation::Inspect) {
                return self
                    .handle_agency_mutation_idempotency(&request, &owner_id)
                    .unwrap_or_else(|| agency_error(request_id, "auth_nonce_replay", message));
            }
            return agency_error(request_id, "auth_nonce_replay", message);
        }

        match request.command.clone() {
            AgencyControlOperation::Inspect => {
                self.inspect_agent_agency(&request, &command_value, &verified)
            }
            AgencyControlOperation::CorrectMemory {
                correction_id,
                target_memory_id,
                expected_revision,
                replacement_summary,
            } => self.correct_agent_memory_for_owner(
                &request,
                correction_id,
                target_memory_id,
                expected_revision,
                replacement_summary,
                &verified,
            ),
            AgencyControlOperation::InstallDelegation { .. }
            | AgencyControlOperation::RevokeDelegation { .. } => {
                self.apply_delegation_for_owner(&request, &verified)
            }
            AgencyControlOperation::OverridePendingIntent {
                control_id,
                intent_id,
                request_digest,
                grant_id,
                expected_grant_revision,
            } => self.apply_owner_pending_intent_control(
                &request,
                &verified,
                control_id,
                intent_id,
                request_digest,
                Some(grant_id),
                Some(expected_grant_revision),
                AgentOwnerControlKindV1::Override,
            ),
            AgencyControlOperation::InterruptPendingIntent {
                control_id,
                intent_id,
                request_digest,
                grant_id,
                expected_grant_revision,
            } => self.apply_owner_pending_intent_control(
                &request,
                &verified,
                control_id,
                intent_id,
                request_digest,
                grant_id,
                expected_grant_revision,
                AgentOwnerControlKindV1::Interrupt,
            ),
        }
    }

    fn handle_agency_mutation_idempotency(
        &mut self,
        request: &AgencyControlRequest,
        owner_id: &str,
    ) -> Option<AgencyControlResponse> {
        match &request.command {
            AgencyControlOperation::CorrectMemory {
                correction_id,
                target_memory_id,
                expected_revision,
                replacement_summary,
            } => self
                .llm_sidecar
                .agent_memory_corrections(request.agent_id.as_str())
                .into_iter()
                .find(|row| row.correction_id == *correction_id)
                .map(|row| {
                    if row.agent_id == request.agent_id
                        && row.target_memory_id == *target_memory_id
                        && row.expected_revision == *expected_revision
                        && row.replacement_summary
                            == replacement_summary.nfc().collect::<String>().trim()
                    {
                        AgencyControlResponse::success(
                            request.request_id.clone(),
                            json!({"correction": row, "idempotent_replay": true}),
                        )
                    } else {
                        agency_error(
                            &request.request_id,
                            "correction_identity_reused",
                            "correction id already belongs to a different payload",
                        )
                    }
                }),
            AgencyControlOperation::InstallDelegation { .. }
            | AgencyControlOperation::RevokeDelegation { .. } => {
                use crate::runtime::{DelegationControlStage, DelegationReplayMatch};
                match self.world.signed_delegation_control_replay(
                    request,
                    &CurrentOwnerAuthority { server: self },
                ) {
                    Ok(DelegationReplayMatch::ExactReplay(authorization)) => {
                        Some(AgencyControlResponse::success(
                            request.request_id.clone(),
                            json!({"authorization": authorization, "idempotent_replay": true}),
                        ))
                    }
                    Ok(DelegationReplayMatch::IdentityConflict) => Some(agency_error(
                        &request.request_id,
                        "grant_identity_conflict",
                        "grant id already belongs to a different payload",
                    )),
                    // Preserve the old nonce branch's read-failure/no-match behavior.
                    Ok(DelegationReplayMatch::NoMatch) => None,
                    Err(error) if error.stage == DelegationControlStage::Read => None,
                    Err(error) => Some(agency_error(
                        &request.request_id,
                        "agent_control_forbidden",
                        format!("{:?}", error.source),
                    )),
                }
            }
            AgencyControlOperation::OverridePendingIntent {
                control_id,
                intent_id,
                request_digest,
                grant_id,
                expected_grant_revision,
            } => self.replay_owner_control(
                request,
                owner_id,
                control_id,
                intent_id,
                request_digest,
                Some(grant_id),
                Some(*expected_grant_revision),
                AgentOwnerControlKindV1::Override,
            ),
            AgencyControlOperation::InterruptPendingIntent {
                control_id,
                intent_id,
                request_digest,
                grant_id,
                expected_grant_revision,
            } => self.replay_owner_control(
                request,
                owner_id,
                control_id,
                intent_id,
                request_digest,
                grant_id.as_ref(),
                *expected_grant_revision,
                AgentOwnerControlKindV1::Interrupt,
            ),
            AgencyControlOperation::Inspect => None,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "The signed owner control binds exact intent, grant revision, and command identity"
    )]
    fn replay_owner_control(
        &self,
        request: &AgencyControlRequest,
        owner_id: &str,
        control_id: &str,
        intent_id: &str,
        request_digest: &str,
        grant_id: Option<&String>,
        expected_grant_revision: Option<u64>,
        kind: AgentOwnerControlKindV1,
    ) -> Option<AgencyControlResponse> {
        let control = AgentOwnerControlV1 {
            control_id: control_id.to_string(),
            agent_id: request.agent_id.clone(),
            intent_id: intent_id.to_string(),
            request_digest: request_digest.to_string(),
            grant_id: grant_id.cloned(),
            expected_grant_revision,
            kind,
        };
        let _ = owner_id;
        match self
            .world
            .signed_owner_control_replay(request, &CurrentOwnerAuthority { server: self })
        {
            Ok(Some((existing, disposition))) if existing == control => {
                Some(AgencyControlResponse::success(
                    request.request_id.clone(),
                    json!({
                        "control_id": control_id,
                        "intent": disposition,
                        "control_kind": match kind {
                            AgentOwnerControlKindV1::Override => "override",
                            AgentOwnerControlKindV1::Interrupt => "interrupt",
                        },
                        "result": "already_recorded",
                        "idempotent_replay": true
                    }),
                ))
            }
            Ok(Some(_)) => Some(agency_error(
                &request.request_id,
                "owner_control_identity_conflict",
                "control id already belongs to a different exact command",
            )),
            Ok(None) | Err(_) => None,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "The control record binds exact intent, grant revision, actor, and request"
    )]
    fn apply_owner_pending_intent_control(
        &mut self,
        request: &AgencyControlRequest,
        verified: &crate::viewer::VerifiedPlayerAuth,
        control_id: String,
        intent_id: String,
        request_digest: String,
        grant_id: Option<String>,
        expected_grant_revision: Option<u64>,
        kind: AgentOwnerControlKindV1,
    ) -> AgencyControlResponse {
        if control_id.trim().is_empty()
            || control_id.len() > 256
            || intent_id.trim().is_empty()
            || request_digest.trim().is_empty()
            || request_digest.len() > 256
        {
            return agency_error(
                &request.request_id,
                "owner_control_identity_invalid",
                "owner control requires bounded control, intent, and request identities",
            );
        }
        if grant_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
            || grant_id.is_some() != expected_grant_revision.is_some()
        {
            return agency_error(
                &request.request_id,
                "owner_control_grant_invalid",
                "grant id and expected revision must be supplied together",
            );
        }
        if kind == AgentOwnerControlKindV1::Override && grant_id.is_none() {
            return agency_error(
                &request.request_id,
                "owner_control_grant_required",
                "pending intent override requires an exact delegation grant revision",
            );
        }
        let mut candidate = self.world.clone();
        let disposition = match candidate
            .apply_signed_owner_control(request, &CurrentOwnerAuthority { server: self })
        {
            Ok(disposition) => disposition,
            Err(error) => {
                return agency_error(
                    &request.request_id,
                    "owner_control_rejected",
                    format!("{error:?}"),
                );
            }
        };
        if let Err(error) = self
            .llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
        {
            return agency_error(&request.request_id, "auth_nonce_replay", error);
        }
        self.world = candidate;
        AgencyControlResponse::success(
            request.request_id.clone(),
            json!({
                "control_id": control_id,
                "intent": disposition,
                "control_kind": match kind { AgentOwnerControlKindV1::Override => "override", AgentOwnerControlKindV1::Interrupt => "interrupt" },
                "idempotent_replay": false,
            }),
        )
    }

    fn inspect_agent_agency(
        &mut self,
        request: &AgencyControlRequest,
        _command: &Value,
        verified: &crate::viewer::VerifiedPlayerAuth,
    ) -> AgencyControlResponse {
        if let Err(error) = self
            .llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
        {
            return agency_error(&request.request_id, "auth_nonce_replay", error);
        }
        let snapshot = self.compat_snapshot(Some(verified.player_id.as_str()));
        let current_intent_id = snapshot
            .player_gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.primary_intent.as_ref())
            .and_then(|intent| intent.intent_id.clone());
        let agency = super::super::player_agency_projection::project_player_agency_read_model(
            &self.world,
            &self.llm_sidecar,
            request.agent_id.as_str(),
            current_intent_id.as_deref(),
        );
        AgencyControlResponse::success(
            request.request_id.clone(),
            json!({
                "world_id": self.config.world_id,
                "reorg_epoch": self.reorg_epoch,
                "agent_id": request.agent_id,
                "current_intent_id": current_intent_id,
                "player_gameplay": snapshot.player_gameplay,
                "agency": agency,
            }),
        )
    }

    fn correct_agent_memory_for_owner(
        &mut self,
        request: &AgencyControlRequest,
        correction_id: String,
        target_memory_id: String,
        expected_revision: u64,
        replacement_summary: String,
        verified: &crate::viewer::VerifiedPlayerAuth,
    ) -> AgencyControlResponse {
        if correction_id.trim().is_empty() || correction_id.len() > 256 {
            return agency_error(
                &request.request_id,
                "correction_identity_invalid",
                "correction id is invalid",
            );
        }
        let Some(context) = self
            .llm_sidecar
            .agent_referenced_memory_context(request.agent_id.as_str())
        else {
            return agency_error(
                &request.request_id,
                "memory_context_unavailable",
                "no owner-readable private memory context is available",
            );
        };
        if context.get("used_for_decision").and_then(Value::as_bool) != Some(true)
            || context.get("current_use").and_then(Value::as_str)
                != Some("committed_decision_context")
        {
            return agency_error(
                &request.request_id,
                "memory_context_not_committed",
                "corrections require a private memory context used by a committed decision",
            );
        }
        let referenced = context
            .get("sources")
            .and_then(Value::as_array)
            .is_some_and(|sources| {
                sources.iter().any(|source| {
                    source.get("memory_id").and_then(Value::as_str)
                        == Some(target_memory_id.as_str())
                })
            });
        if !referenced {
            return agency_error(
                &request.request_id,
                "memory_target_not_referenced",
                "target memory is not in the Agent's committed retrieval context",
            );
        }
        let correction = match self.llm_sidecar.correct_agent_memory(
            request.agent_id.as_str(),
            correction_id,
            target_memory_id,
            expected_revision,
            replacement_summary,
        ) {
            Ok(correction) => correction,
            Err(message) => {
                return agency_error(&request.request_id, "memory_correction_rejected", message);
            }
        };
        if let Err(message) = self
            .llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
        {
            return agency_error(&request.request_id, "auth_nonce_replay", message);
        }
        AgencyControlResponse::success(
            request.request_id.clone(),
            json!({"correction": correction}),
        )
    }

    fn apply_delegation_for_owner(
        &mut self,
        request: &AgencyControlRequest,
        verified: &crate::viewer::VerifiedPlayerAuth,
    ) -> AgencyControlResponse {
        use crate::runtime::{DelegationControlDisposition, DelegationControlStage};
        // This is the local Viewer issuance profile, deliberately narrower than
        // signed Runtime and storage. Old nonces never enter this fresh path.
        if let AgencyControlOperation::InstallDelegation { action_kinds, .. } = &request.command {
            if action_kinds.iter().any(|kind| !action_kind_valid(kind)) {
                return agency_error(
                    &request.request_id,
                    "grant_scope_invalid",
                    "grant identity, object, period, or action scope is invalid or unsupported",
                );
            }
        }
        let mut candidate = self.world.clone();
        let outcome = match candidate
            .apply_signed_delegation_control(request, &CurrentOwnerAuthority { server: self })
        {
            Ok(outcome) => outcome,
            Err(error) => {
                let install = matches!(
                    request.command,
                    AgencyControlOperation::InstallDelegation { .. }
                );
                let (code, message) = match error.stage {
                    DelegationControlStage::Scope => ("grant_scope_invalid",
                        "grant identity, object, period, or action scope is invalid or unsupported".to_string()),
                    DelegationControlStage::Read => ("runtime_agency_read_failed", format!("{:?}", error.source)),
                    DelegationControlStage::Identity => ("grant_identity_conflict", "grant id already exists with different terms".to_string()),
                    DelegationControlStage::Reservation => ("grant_scope_already_reserved",
                        "this source/object/action scope already has a grant history; changing identity or period cannot reset its allowance".to_string()),
                    DelegationControlStage::NotFound => ("grant_not_found", "delegation grant was not found".to_string()),
                    DelegationControlStage::Revision => ("grant_revision_stale", "delegation grant revision changed or is already revoked".to_string()),
                    DelegationControlStage::Authentication | DelegationControlStage::Apply => (
                        if install { "grant_install_rejected" } else { "grant_revoke_rejected" },
                        format!("{:?}", error.source)),
                };
                return agency_error(&request.request_id, code, message);
            }
        };
        if outcome.disposition == DelegationControlDisposition::Applied {
            self.world = candidate;
        }
        if let Err(error) = self
            .llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
        {
            return agency_error(&request.request_id, "auth_nonce_replay", error);
        }
        if outcome.disposition == DelegationControlDisposition::ExactReplay {
            return AgencyControlResponse::success(
                request.request_id.clone(),
                json!({"authorization": outcome.authorization, "idempotent_replay": true}),
            );
        }
        // Display only: no lifecycle decisions depend on this compatibility read.
        let rows = self
            .world
            .agent_delegation_authorizations(&request.agent_id)
            .unwrap_or_default();
        AgencyControlResponse::success(request.request_id.clone(), json!({"authorizations": rows}))
    }
}
