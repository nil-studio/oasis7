//! Signed local owner use cases. The host supplies its current registered-key
//! policy by borrow; signed wire data never becomes a reusable credential.
use super::*;
use crate::{
    AgentIntentAuthorityContext, AgentIntentReplayDisposition, AgentOwnerControlKindV1,
    AgentOwnerControlV1,
};
use oasis7_proto::agency_control::{
    AGENCY_CONTROL_REQUEST_TYPE, AgencyControlOperation, AgencyControlRequest,
};
use oasis7_proto::player_auth::{
    verify_agency_control_auth_proof, verify_agent_chat_auth_proof_with_authority,
};
use oasis7_proto::viewer::AgentChatRequest;

/// Trusted process-local registration/session policy for a single call.
/// Implementations must consult the application's actual current registry and
/// Agent binding; callers cannot select it through a signed network request.
pub trait OwnerControlHostAuthority {
    fn validate_registered_owner(
        &self,
        player_id: &str,
        public_key: &str,
        agent_id: &str,
        world_id: &str,
        reorg_epoch: u64,
    ) -> Result<(), String>;
}
fn denied(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
fn control_from_request(request: &AgencyControlRequest) -> Result<AgentOwnerControlV1, WorldError> {
    let (control_id, intent_id, request_digest, grant_id, expected_grant_revision, kind) =
        match &request.command {
            AgencyControlOperation::OverridePendingIntent {
                control_id,
                intent_id,
                request_digest,
                grant_id,
                expected_grant_revision,
            } => (
                control_id,
                intent_id,
                request_digest,
                Some(grant_id.clone()),
                Some(*expected_grant_revision),
                AgentOwnerControlKindV1::Override,
            ),
            AgencyControlOperation::InterruptPendingIntent {
                control_id,
                intent_id,
                request_digest,
                grant_id,
                expected_grant_revision,
            } => (
                control_id,
                intent_id,
                request_digest,
                grant_id.clone(),
                *expected_grant_revision,
                AgentOwnerControlKindV1::Interrupt,
            ),
            _ => return Err(denied("owner control operation required")),
        };
    if control_id.trim().is_empty()
        || control_id.len() > 256
        || intent_id.trim().is_empty()
        || request_digest.trim().is_empty()
        || request_digest.len() > 256
    {
        return Err(denied("owner_control_identity_invalid"));
    }
    if grant_id.as_deref().is_some_and(|id| id.trim().is_empty())
        || grant_id.is_some() != expected_grant_revision.is_some()
    {
        return Err(denied("owner_control_grant_invalid"));
    }
    Ok(AgentOwnerControlV1 {
        control_id: control_id.clone(),
        agent_id: request.agent_id.clone(),
        intent_id: intent_id.clone(),
        request_digest: request_digest.clone(),
        grant_id,
        expected_grant_revision,
        kind,
    })
}
impl World {
    pub(super) fn authenticate_local_agency_request(
        &self,
        request: &AgencyControlRequest,
        host: &dyn OwnerControlHostAuthority,
    ) -> Result<String, WorldError> {
        if request.frame_type != AGENCY_CONTROL_REQUEST_TYPE
            || request.request_id.trim().is_empty()
            || request.request_id.len() > 256
        {
            return Err(denied("invalid_request"));
        }
        let command = serde_json::to_value(&request.command)?;
        let proof = request
            .auth
            .as_ref()
            .ok_or_else(|| denied("auth_proof_required"))?;
        let verified = verify_agency_control_auth_proof(
            &request.request_id,
            &request.player_id,
            &request.public_key,
            &request.agent_id,
            &request.world_id,
            request.reorg_epoch,
            &command,
            proof,
        )
        .map_err(denied)?;
        host.validate_registered_owner(
            &verified.player_id,
            &verified.public_key,
            &request.agent_id,
            &request.world_id,
            request.reorg_epoch,
        )
        .map_err(denied)?;
        let owner = self
            .current_agent_owner(&request.agent_id)
            .ok_or_else(|| denied("owner_source_unavailable"))?;
        if owner.player_id != verified.player_id {
            return Err(denied("agent_control_forbidden"));
        }
        Ok(verified.player_id)
    }
    /// Reverify the complete signed command before reading an exact owner retry.
    pub fn signed_owner_control_replay(
        &self,
        request: &AgencyControlRequest,
        host: &dyn OwnerControlHostAuthority,
    ) -> Result<Option<(AgentOwnerControlV1, AgentIntentReplayDisposition)>, WorldError> {
        let player = self.authenticate_local_agency_request(request, host)?;
        let control = control_from_request(request)?;
        self.agent_owner_control_replay(&player, &control.control_id)
    }
    /// Apply the exact control encoded in the signed request, never host-supplied derived fields.
    pub fn apply_signed_owner_control(
        &mut self,
        request: &AgencyControlRequest,
        host: &dyn OwnerControlHostAuthority,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        let player = self.authenticate_local_agency_request(request, host)?;
        self.bind_agent_owner_control(&player, control_from_request(request)?)
    }
    /// Legacy local chat still uses the host's current registration/binding policy.
    pub fn record_signed_local_agent_chat(
        &mut self,
        request: &AgentChatRequest,
        host: &dyn OwnerControlHostAuthority,
    ) -> Result<AgentIntentRecordOutcome, WorldError> {
        let proof = request
            .auth
            .as_ref()
            .ok_or_else(|| denied("auth_proof_required"))?;
        let world_id = request
            .world_id
            .as_deref()
            .ok_or_else(|| denied("agent_chat authority envelope is required"))?;
        let reorg = request
            .reorg_epoch
            .ok_or_else(|| denied("agent_chat authority envelope is required"))?;
        let verified = verify_agent_chat_auth_proof_with_authority(
            request,
            proof,
            world_id,
            reorg,
            "player_agent_chat",
        )
        .map_err(denied)?;
        host.validate_registered_owner(
            &verified.player_id,
            &verified.public_key,
            &request.agent_id,
            world_id,
            reorg,
        )
        .map_err(denied)?;
        if self.state.is_manufactured_agent(&request.agent_id) {
            let owner = self
                .current_agent_owner(&request.agent_id)
                .ok_or_else(|| denied("owner_source_unavailable"))?;
            if owner.player_id != verified.player_id {
                return Err(denied("agent_control_forbidden"));
            }
        } else if self
            .state
            .starter_oc_claims
            .get(&request.agent_id)
            .is_some_and(|owner| owner.player_id != verified.player_id)
        {
            return Err(denied("agent_control_forbidden"));
        }
        let intent_seq = request.intent_seq.unwrap_or(verified.nonce);
        if intent_seq == 0 || intent_seq != verified.nonce {
            return Err(denied("intent_seq must match auth nonce"));
        }
        self.record_agent_chat_intent_with_authority(
            &verified.player_id,
            &request.agent_id,
            intent_seq,
            &request.message,
            AgentIntentAuthorityContext {
                intent_tick: request.intent_tick,
                world_id: request.world_id.clone(),
                reorg_epoch: request.reorg_epoch,
                authority_scope: request.authority_scope.clone(),
                replaces_intent_id: request.replaces_intent_id.clone(),
            },
        )
    }
}
