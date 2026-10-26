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
    ExactReplay(AgentDelegationAuthorizationV1),
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
fn grant_from_request(
    request: &AgencyControlRequest,
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
    } = &request.command
    else {
        return None;
    };
    Some(AgentDelegationGrantV1 {
        grant_id: grant_id.clone(),
        source_id: source.into(),
        issuer_id: player.into(),
        owner_id: player.into(),
        organization_id: None,
        agent_id: request.agent_id.clone(),
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
                    DelegationReplayMatch::ExactReplay(row.clone())
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
            .map(|row| DelegationReplayMatch::ExactReplay(row.clone()))
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
        Ok(grant_from_request(request, &player, &owner.source_id))
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
        use DelegationControlStage as Stage;
        let grant = self.delegation_request_grant(request, host)?;
        let operation = match &request.command {
            AgencyControlOperation::InstallDelegation { .. } => DelegationControlOperation::Install,
            AgencyControlOperation::RevokeDelegation { .. } => DelegationControlOperation::Revoke,
            _ => {
                return Err(DelegationControlError::denied(
                    Stage::Scope,
                    "delegation control operation required",
                ));
            }
        };
        if let Some(grant) = &grant {
            if grant.grant_id.trim().is_empty()
                || grant.object_id.trim().is_empty()
                || grant.period_id.trim().is_empty()
                || grant.action_kinds.is_empty()
                || grant.action_kinds.iter().any(|kind| {
                    !matches!(
                        kind.as_str(),
                        "MoveAgent" | "TransferMaterial" | "ScheduleRecipe" | "WorldRule"
                    )
                })
                || grant
                    .action_kinds
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != grant.action_kinds.len()
            {
                return Err(DelegationControlError::denied(
                    Stage::Scope,
                    "grant_scope_invalid",
                ));
            }
        }
        let rows = self
            .agent_delegation_authorizations(&request.agent_id)
            .map_err(|e| DelegationControlError::at(Stage::Read, e))?;
        match history_match(&request.command, grant.as_ref(), &rows) {
            DelegationReplayMatch::ExactReplay(authorization) => {
                return Ok(DelegationControlOutcome {
                    operation,
                    disposition: DelegationControlDisposition::ExactReplay,
                    authorization,
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
                } = &request.command
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
