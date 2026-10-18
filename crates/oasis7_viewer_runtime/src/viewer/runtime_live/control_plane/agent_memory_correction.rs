use super::super::agency_control::{
    AgencyControlOperation, AgencyControlRequest, AgencyControlResponse,
};
use crate::runtime::{AgentDelegationGrantV1, AgentOwnerControlKindV1, AgentOwnerControlV1};
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
        let Some(claim) = self
            .world
            .state()
            .starter_oc_claims
            .get(&request.agent_id)
            .cloned()
        else {
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
        let source_id = format!("starter_claim:{}:{}", request.agent_id, claim.claimed_at);
        if let Err(message) = self
            .llm_sidecar
            .validate_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
        {
            // Exact idempotent mutations are checked before nonce consumption
            // by their operation handlers below; every new request remains
            // monotonic and cannot replay a previous signed payload.
            if !matches!(request.command, AgencyControlOperation::Inspect) {
                return self
                    .handle_agency_mutation_idempotency(&request, &owner_id, &source_id)
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
            AgencyControlOperation::InstallDelegation {
                grant_id,
                object_id,
                action_kinds,
                period_id,
                valid_from_tick,
                valid_until_tick,
                limit_units,
            } => self.install_agent_delegation_for_owner(
                &request,
                &owner_id,
                &source_id,
                grant_id,
                object_id,
                action_kinds,
                period_id,
                valid_from_tick,
                valid_until_tick,
                limit_units,
                &verified,
            ),
            AgencyControlOperation::RevokeDelegation {
                grant_id,
                expected_revision,
            } => self.revoke_agent_delegation_for_owner(
                &request,
                grant_id,
                expected_revision,
                &verified,
            ),
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
        source_id: &str,
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
            AgencyControlOperation::InstallDelegation { grant_id, .. } => self
                .world
                .agent_delegation_authorizations(request.agent_id.as_str())
                .ok()
                .and_then(|rows| rows.into_iter().find(|row| row.grant.grant_id == *grant_id))
                .map(|row| {
                    let expected = grant_from_request(request, owner_id, source_id);
                    if expected.is_ok_and(|expected| expected == row.grant) {
                        AgencyControlResponse::success(
                            request.request_id.clone(),
                            json!({"authorization": row, "idempotent_replay": true}),
                        )
                    } else {
                        agency_error(
                            &request.request_id,
                            "grant_identity_conflict",
                            "grant id already belongs to a different payload",
                        )
                    }
                }),
            AgencyControlOperation::RevokeDelegation {
                grant_id,
                expected_revision,
            } => self
                .world
                .agent_delegation_authorizations(request.agent_id.as_str())
                .ok()
                .and_then(|rows| rows.into_iter().find(|row| row.grant.grant_id == *grant_id))
                .and_then(|row| {
                    (row.grant.revoked && row.grant.revision == expected_revision.saturating_add(1))
                        .then(|| {
                            AgencyControlResponse::success(
                                request.request_id.clone(),
                                json!({"authorization": row, "idempotent_replay": true}),
                            )
                        })
                }),
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

    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the exact authenticated owner grant fields explicit at the control boundary"
    )]
    fn install_agent_delegation_for_owner(
        &mut self,
        request: &AgencyControlRequest,
        owner_id: &str,
        source_id: &str,
        grant_id: String,
        object_id: String,
        action_kinds: Vec<String>,
        period_id: String,
        valid_from_tick: u64,
        valid_until_tick: u64,
        limit_units: u64,
        verified: &crate::viewer::VerifiedPlayerAuth,
    ) -> AgencyControlResponse {
        if grant_id.trim().is_empty()
            || object_id.trim().is_empty()
            || period_id.trim().is_empty()
            || action_kinds.is_empty()
            || action_kinds.iter().any(|kind| !action_kind_valid(kind))
            || action_kinds
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != action_kinds.len()
        {
            return agency_error(
                &request.request_id,
                "grant_scope_invalid",
                "grant identity, object, period, or action scope is invalid or unsupported",
            );
        }
        let grant = AgentDelegationGrantV1 {
            grant_id,
            source_id: source_id.to_string(),
            issuer_id: owner_id.to_string(),
            owner_id: owner_id.to_string(),
            organization_id: None,
            agent_id: request.agent_id.clone(),
            object_id,
            action_kinds,
            revision: 1,
            period_id,
            valid_from_tick,
            valid_until_tick,
            limit_units,
            resource_kind: "electricity".to_string(),
            revoked: false,
        };
        let existing = match self
            .world
            .agent_delegation_authorizations(request.agent_id.as_str())
        {
            Ok(rows) => rows,
            Err(error) => {
                return agency_error(
                    &request.request_id,
                    "runtime_agency_read_failed",
                    format!("{error:?}"),
                );
            }
        };
        if let Some(row) = existing
            .iter()
            .find(|row| row.grant.grant_id == grant.grant_id)
        {
            if row.grant == grant {
                if let Err(error) = self
                    .llm_sidecar
                    .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
                {
                    return agency_error(&request.request_id, "auth_nonce_replay", error);
                }
                return AgencyControlResponse::success(
                    request.request_id.clone(),
                    json!({"authorization": row, "idempotent_replay": true}),
                );
            }
            return agency_error(
                &request.request_id,
                "grant_identity_conflict",
                "grant id already exists with different terms",
            );
        }
        if existing.iter().any(|row| {
            row.grant.object_id == grant.object_id
                && row
                    .grant
                    .action_kinds
                    .iter()
                    .any(|kind| grant.action_kinds.contains(kind))
        }) {
            return agency_error(
                &request.request_id,
                "grant_scope_already_reserved",
                "this source/object/action scope already has a grant history; changing identity or period cannot reset its allowance",
            );
        }
        let mut candidate = self.world.clone();
        if let Err(error) = candidate
            .apply_signed_delegation_control(request, &CurrentOwnerAuthority { server: self })
        {
            return agency_error(
                &request.request_id,
                "grant_install_rejected",
                format!("{error:?}"),
            );
        }
        self.world = candidate;
        if let Err(error) = self
            .llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
        {
            return agency_error(&request.request_id, "auth_nonce_replay", error);
        }
        let rows = self
            .world
            .agent_delegation_authorizations(request.agent_id.as_str())
            .unwrap_or_default();
        AgencyControlResponse::success(request.request_id.clone(), json!({"authorizations": rows}))
    }

    fn revoke_agent_delegation_for_owner(
        &mut self,
        request: &AgencyControlRequest,
        grant_id: String,
        expected_revision: u64,
        verified: &crate::viewer::VerifiedPlayerAuth,
    ) -> AgencyControlResponse {
        let mut rows = match self
            .world
            .agent_delegation_authorizations(request.agent_id.as_str())
        {
            Ok(rows) => rows,
            Err(error) => {
                return agency_error(
                    &request.request_id,
                    "runtime_agency_read_failed",
                    format!("{error:?}"),
                );
            }
        };
        let Some(index) = rows.iter().position(|row| row.grant.grant_id == grant_id) else {
            return agency_error(
                &request.request_id,
                "grant_not_found",
                "delegation grant was not found",
            );
        };
        let current = rows.remove(index);
        if current.grant.revoked && current.grant.revision == expected_revision.saturating_add(1) {
            if let Err(error) = self
                .llm_sidecar
                .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
            {
                return agency_error(&request.request_id, "auth_nonce_replay", error);
            }
            return AgencyControlResponse::success(
                request.request_id.clone(),
                json!({"authorization": current, "idempotent_replay": true}),
            );
        }
        if current.grant.revision != expected_revision || current.grant.revoked {
            return agency_error(
                &request.request_id,
                "grant_revision_stale",
                "delegation grant revision changed or is already revoked",
            );
        }
        let mut candidate = self.world.clone();
        if let Err(error) = candidate
            .apply_signed_delegation_control(request, &CurrentOwnerAuthority { server: self })
        {
            return agency_error(
                &request.request_id,
                "grant_revoke_rejected",
                format!("{error:?}"),
            );
        }
        self.world = candidate;
        if let Err(error) = self
            .llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
        {
            return agency_error(&request.request_id, "auth_nonce_replay", error);
        }
        let result = self
            .world
            .agent_delegation_authorizations(request.agent_id.as_str())
            .unwrap_or_default();
        AgencyControlResponse::success(
            request.request_id.clone(),
            json!({"authorizations": result}),
        )
    }
}

fn grant_from_request(
    request: &AgencyControlRequest,
    owner_id: &str,
    source_id: &str,
) -> Result<AgentDelegationGrantV1, String> {
    let AgencyControlOperation::InstallDelegation {
        grant_id,
        object_id,
        action_kinds,
        period_id,
        valid_from_tick,
        valid_until_tick,
        limit_units,
    } = &request.command
    else {
        return Err("not install delegation".to_string());
    };
    Ok(AgentDelegationGrantV1 {
        grant_id: grant_id.clone(),
        source_id: source_id.to_string(),
        issuer_id: owner_id.to_string(),
        owner_id: owner_id.to_string(),
        organization_id: None,
        agent_id: request.agent_id.clone(),
        object_id: object_id.clone(),
        action_kinds: action_kinds.clone(),
        revision: 1,
        period_id: period_id.clone(),
        valid_from_tick: *valid_from_tick,
        valid_until_tick: *valid_until_tick,
        limit_units: *limit_units,
        resource_kind: "electricity".to_string(),
        revoked: false,
    })
}
