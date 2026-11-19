//! Runtime-owned signed delegation identity, lifecycle and read-only retries.
use super::{OwnerControlHostAuthority, World};
use crate::{AgentDelegationAuthorizationV1, AgentDelegationGrantV1, WorldError};
use oasis7_proto::agency_control::{AgencyControlOperation, AgencyControlRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegationControlOperation {
    Install,
    Revoke,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegationControlDisposition {
    Applied,
    ExactReplay,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegationControlOutcome {
    pub operation: DelegationControlOperation,
    pub disposition: DelegationControlDisposition,
    pub authorization: AgentDelegationAuthorizationV1,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegationReplayMatch {
    ExactReplay(Box<AgentDelegationAuthorizationV1>),
    IdentityConflict,
    NoMatch,
}
/// Stages let adapters preserve their existing codes without inspecting Debug text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegationControlStage {
    Authentication,
    Read,
    Scope,
    Identity,
    Reservation,
    NotFound,
    Revision,
    Apply,
}
#[derive(Debug)]
pub struct DelegationControlError {
    pub stage: DelegationControlStage,
    pub source: WorldError,
}
impl DelegationControlError {
    fn at(stage: DelegationControlStage, source: WorldError) -> Self {
        Self { stage, source }
    }
    fn denied(stage: DelegationControlStage, reason: &str) -> Self {
        Self::at(
            stage,
            WorldError::ResourceBalanceInvalid {
                reason: reason.into(),
            },
        )
    }
}

/// This construction deliberately does not validate the fresh-install profile:
/// an old nonce must match historical identity before any fresh-only checks.
fn grant_from_command(
    agent_id: &str,
    command: &AgencyControlOperation,
    player: &str,
    source: &str,
) -> Option<AgentDelegationGrantV1> {
    let AgencyControlOperation::InstallDelegation {
        grant_id,
        object_id,
        action_kinds,
        period_id,
        valid_from_tick,
        valid_until_tick,
        limit_units,
    } = command
    else {
        return None;
    };
    Some(AgentDelegationGrantV1 {
        grant_id: grant_id.clone(),
        source_id: source.into(),
        issuer_id: player.into(),
        owner_id: player.into(),
        organization_id: None,
        agent_id: agent_id.into(),
        object_id: object_id.clone(),
        action_kinds: action_kinds.clone(),
        revision: 1,
        period_id: period_id.clone(),
        valid_from_tick: *valid_from_tick,
        valid_until_tick: *valid_until_tick,
        limit_units: *limit_units,
        resource_kind: "electricity".into(),
        revoked: false,
    })
}
fn history_match(
    command: &AgencyControlOperation,
    grant: Option<&AgentDelegationGrantV1>,
    rows: &[AgentDelegationAuthorizationV1],
) -> DelegationReplayMatch {
    match command {
        AgencyControlOperation::InstallDelegation { grant_id, .. } => {
            match rows.iter().find(|row| row.grant.grant_id == *grant_id) {
                Some(row) if Some(&row.grant) == grant => {
                    DelegationReplayMatch::ExactReplay(Box::new(row.clone()))
                }
                Some(_) => DelegationReplayMatch::IdentityConflict,
                None => DelegationReplayMatch::NoMatch,
            }
        }
        AgencyControlOperation::RevokeDelegation {
            grant_id,
            expected_revision,
        } => rows
            .iter()
            .find(|row| {
                row.grant.grant_id == *grant_id
                    && row.grant.revoked
                    && row.grant.revision == expected_revision.saturating_add(1)
            })
            .map(|row| DelegationReplayMatch::ExactReplay(Box::new(row.clone())))
            .unwrap_or(DelegationReplayMatch::NoMatch),
        _ => DelegationReplayMatch::NoMatch,
    }
}
impl World {
    fn delegation_request_grant(
        &self,
        request: &AgencyControlRequest,
        host: &dyn OwnerControlHostAuthority,
    ) -> Result<Option<AgentDelegationGrantV1>, DelegationControlError> {
        let player = self
            .authenticate_local_agency_request(request, host)
            .map_err(|e| DelegationControlError::at(DelegationControlStage::Authentication, e))?;
        let owner = self.current_agent_owner(&request.agent_id).ok_or_else(|| {
            DelegationControlError::denied(
                DelegationControlStage::Authentication,
                "owner_source_unavailable",
            )
        })?;
        Ok(grant_from_command(
            &request.agent_id,
            &request.command,
            &player,
            &owner.source_id,
        ))
    }
    /// Reverify current signature, registration/binding and owner on every read.
    /// No mutation, persistence or host nonce consumption is possible here.
    pub fn signed_delegation_control_replay(
        &self,
        request: &AgencyControlRequest,
        host: &dyn OwnerControlHostAuthority,
    ) -> Result<DelegationReplayMatch, DelegationControlError> {
        let grant = self.delegation_request_grant(request, host)?;
        let rows = self
            .agent_delegation_authorizations(&request.agent_id)
            .map_err(|e| DelegationControlError::at(DelegationControlStage::Read, e))?;
        Ok(history_match(&request.command, grant.as_ref(), &rows))
    }
    /// The host admits a fresh nonce; Runtime owns the sole lifecycle decision.
    pub fn apply_signed_delegation_control(
        &mut self,
        request: &AgencyControlRequest,
        host: &dyn OwnerControlHostAuthority,
    ) -> Result<DelegationControlOutcome, DelegationControlError> {
        let grant = self.delegation_request_grant(request, host)?;
        self.apply_authenticated_delegation_control(&request.agent_id, &request.command, grant)
    }

    /// Formal service path: verify the owner signature and complete current
    /// task fence before sharing the local path's sole grant lifecycle.
    pub fn apply_native_delegation_control<'a>(
        &mut self,
        signed: &crate::service_admission::SignedReadRequest<
            oasis7_agent_api::game::GameNativeDelegationControlV1,
        >,
        staged: impl IntoIterator<Item = &'a crate::service_admission::CanonicalIntentResultV1>,
    ) -> Result<serde_json::Value, String> {
        use oasis7_agent_api::game::GameNativeDelegationOperationV1 as Native;
        crate::service_admission::native_delegation::validate_owner_staged(self, signed, staged)?;
        let agent_id = &signed.request.fence.agent_id;
        let command = match &signed.request.operation {
            Native::Install {
                grant_id,
                object_id,
                action_kinds,
                period_id,
                valid_from_tick,
                valid_until_tick,
                limit_units,
            } => AgencyControlOperation::InstallDelegation {
                grant_id: grant_id.clone(),
                object_id: object_id.clone(),
                action_kinds: action_kinds.clone(),
                period_id: period_id.clone(),
                valid_from_tick: valid_from_tick.0,
                valid_until_tick: valid_until_tick.0,
                limit_units: limit_units.0,
            },
            Native::Revoke {
                grant_id,
                expected_revision,
            } => AgencyControlOperation::RevokeDelegation {
                grant_id: grant_id.clone(),
                expected_revision: expected_revision.0,
            },
        };
        let owner = self
            .current_agent_owner(agent_id)
            .ok_or("owner source unavailable")?;
        let grant = grant_from_command(agent_id, &command, &owner.player_id, &owner.source_id);
        // This World is the canonical block candidate. Local owner controls
        // persist before acknowledgement, but a staged formal grant must not
        // publish a compatibility snapshot ahead of its canonical result.
        let persistence_dir = self.persistence_dir.replace(None);
        let outcome = self.apply_authenticated_delegation_control(agent_id, &command, grant);
        self.persistence_dir.replace(persistence_dir);
        let outcome = outcome
            .map_err(|e| format!("native delegation {:?}: {:?}", e.stage, e.source))?;
        Ok(serde_json::json!({
            "kind": "game_native_delegation",
            "schema_revision": 1,
            "operation": match outcome.operation {
                DelegationControlOperation::Install => "install",
                DelegationControlOperation::Revoke => "revoke",
            },
            "disposition": match outcome.disposition {
                DelegationControlDisposition::Applied => "applied",
                DelegationControlDisposition::ExactReplay => "exact_replay",
            },
            "authorization": outcome.authorization,
        }))
    }

    fn apply_authenticated_delegation_control(
        &mut self,
        agent_id: &str,
        command: &AgencyControlOperation,
        grant: Option<AgentDelegationGrantV1>,
    ) -> Result<DelegationControlOutcome, DelegationControlError> {
        use DelegationControlStage as Stage;
        let operation = match command {
            AgencyControlOperation::InstallDelegation { .. } => DelegationControlOperation::Install,
            AgencyControlOperation::RevokeDelegation { .. } => DelegationControlOperation::Revoke,
            _ => {
                return Err(DelegationControlError::denied(
                    Stage::Scope,
                    "delegation control operation required",
                ));
            }
        };
        if let Some(grant) = &grant
            && (grant.grant_id.trim().is_empty()
                || grant.object_id.trim().is_empty()
                || grant.period_id.trim().is_empty()
                || grant.action_kinds.is_empty()
                || grant.action_kinds.iter().any(|kind| {
                    !matches!(
                        kind.as_str(),
                        "MoveAgent"
                            | "TransferMaterial"
                            | "BuildFactory"
                            | "ScheduleRecipe"
                            | "WorldRule"
                    )
                })
                || grant
                    .action_kinds
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != grant.action_kinds.len())
        {
            return Err(DelegationControlError::denied(
                Stage::Scope,
                "grant_scope_invalid",
            ));
        }
        let rows = self
            .agent_delegation_authorizations(agent_id)
            .map_err(|e| DelegationControlError::at(Stage::Read, e))?;
        match history_match(command, grant.as_ref(), &rows) {
            DelegationReplayMatch::ExactReplay(authorization) => {
                return Ok(DelegationControlOutcome {
                    operation,
                    disposition: DelegationControlDisposition::ExactReplay,
                    authorization: *authorization,
                });
            }
            DelegationReplayMatch::IdentityConflict => {
                return Err(DelegationControlError::denied(
                    Stage::Identity,
                    "grant_identity_conflict",
                ));
            }
            DelegationReplayMatch::NoMatch => {}
        }
        let grant = match grant {
            Some(grant) => {
                if rows.iter().any(|row| {
                    row.grant.source_id == grant.source_id
                        && !row.grant.revoked
                        && row.grant.object_id == grant.object_id
                        && row
                            .grant
                            .action_kinds
                            .iter()
                            .any(|kind| grant.action_kinds.contains(kind))
                }) {
                    return Err(DelegationControlError::denied(
                        Stage::Reservation,
                        "grant_scope_already_reserved",
                    ));
                }
                grant
            }
            None => {
                let AgencyControlOperation::RevokeDelegation {
                    grant_id,
                    expected_revision,
                } = command
                else {
                    unreachable!()
                };
                let mut current = rows
                    .into_iter()
                    .find(|row| row.grant.grant_id == *grant_id)
                    .ok_or_else(|| {
                        DelegationControlError::denied(Stage::NotFound, "grant_not_found")
                    })?
                    .grant;
                if current.revision != *expected_revision || current.revoked {
                    return Err(DelegationControlError::denied(
                        Stage::Revision,
                        "grant_revision_stale",
                    ));
                }
                current.revision = current.revision.saturating_add(1);
                current.revoked = true;
                current
            }
        };
        let authorization = self
            .install_agent_delegation_with_authorization(grant)
            .map_err(|e| DelegationControlError::at(Stage::Apply, e))?;
        Ok(DelegationControlOutcome {
            operation,
            disposition: DelegationControlDisposition::Applied,
            authorization,
        })
    }
}
