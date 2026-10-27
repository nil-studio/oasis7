//! Authenticated TLS owner gateway. No arbitrary routing, operator signing or
//! executor proposals. Upstream private listeners remain loopback-only.
use super::{
    reject,
    service::{IdentityService, audit, consume_quota, session_key, valid_session},
    store::StoreError,
};
use oasis7_agent_api::game::*;
use oasis7_client_api::{
    world_service::{entry_control::*, participant_discovery::*, private_goal_read::*, *},
    world_service_wire::WorldServicePayloadV1,
};
use serde_json::Value;
#[cfg(test)]
#[path = "entry_owner_control_tests.rs"]
mod tests;

impl IdentityService {
    pub(super) fn entry_owner_route(
        &mut self,
        method: &str,
        path: &str,
        sid: Option<&str>,
        body: &Value,
        now: u64,
    ) -> Option<Result<Value, StoreError>> {
        if path != "/entry-owner/control" {
            return None;
        }
        let result = (|| {
            if method != "POST" {
                return Err(reject("method_not_allowed"));
            }
            let (account, _) = self.session(sid, now)?;
            let entry = self
                .participant_entry
                .as_ref()
                .filter(|e| e.owner_control_enabled)
                .ok_or_else(|| reject("owner_control_unavailable"))?;
            let bytes = serde_json::to_vec(body).map_err(|_| reject("invalid_request"))?;
            if bytes.len() > 65536 {
                return Err(reject("invalid_request"));
            }
            let request: EntryOwnerControlRequestV1 =
                parse_request(&bytes).map_err(|_| reject("invalid_request"))?;
            self.check_participant_subject(
                &request.access.owner.request,
                &account.account_id,
                entry,
            )?;
            self.store.transaction(|tx| {
                consume_quota(tx, "entry-owner", &account.account_id, now, 60_000, 120)?;
                audit(tx, &account, "entry_owner_request", now)
            })?;
            let reply = entry.forward_owner_view(&request.access)?;
            let (owner, task, commit) = owner_observation(
                &request.access,
                &reply,
                matches!(&request.operation, EntryOwnerOperationV1::LookupExecutor(_)),
            )?;
            if owner.public_key != request.access.owner.subject_public_key
                || owner.world_id != entry.trust.world.world_id
                || owner.canonical_authority.branch_id != commit.binding.branch_id
                || owner.reorg_epoch != commit.binding.reorg_generation
            {
                return Err(reject("owner_control_context_mismatch"));
            }
            // Owner observation may block on another process. Revocation while
            // it was in flight must prevent the subsequent mutation/private read.
            let token = session_key(
                &self.config.session_key,
                sid.ok_or_else(|| reject("session_expired"))?,
            );
            self.store.transaction(|tx| {
                valid_session(
                    tx,
                    &token,
                    self.config.rp.identity_instance_epoch,
                    super::now_ms(),
                )
                .map(|_| ())
            })?;
            match &request.operation {
                EntryOwnerOperationV1::Submit(q) => {
                    q.validate().map_err(|_| reject("invalid_request"))?;
                    validate_payload(&q.signed_payload, &owner, &entry.trust.world)?;
                    let actual = oasis7_client_api::world_service_correlation::derive_correlation(
                        entry.trust.world.clone(),
                        &q.signed_payload,
                    )
                    .map_err(|_| reject("invalid_request"))?;
                    if actual != q.correlation {
                        return Err(reject("owner_control_context_mismatch"));
                    }
                    entry.forward_bound(SUBMIT_PATH, SUBMIT_PATH, q, q)
                }
                EntryOwnerOperationV1::Lookup(q) => {
                    q.request
                        .validate()
                        .map_err(|_| reject("invalid_request"))?;
                    validate_payload(&q.original, &owner, &entry.trust.world)?;
                    let actual = oasis7_client_api::world_service_correlation::derive_correlation(
                        entry.trust.world.clone(),
                        &q.original,
                    )
                    .map_err(|_| reject("invalid_request"))?;
                    if actual.key != q.request.key {
                        return Err(reject("owner_control_context_mismatch"));
                    }
                    entry.forward_bound(LOOKUP_PATH, LOOKUP_PATH, q, q)
                }
                EntryOwnerOperationV1::LookupExecutor(q) => {
                    q.request
                        .validate()
                        .map_err(|_| reject("invalid_request"))?;
                    let control = task
                        .as_ref()
                        .ok_or_else(|| reject("owner_control_context_mismatch"))?;
                    let fence = GameTaskFenceV1::from_control(control);
                    if fence.agent_id != owner.agent_id
                        || fence.world_id != entry.trust.world.world_id
                        || fence.genesis_digest != entry.trust.world.genesis_digest
                        || fence.owner_public_key != owner.public_key
                        || fence.owner_binding != owner.player_id
                        || fence.owner_generation.0
                            != owner.canonical_authority.agent_identity_generation
                        || fence.branch_id != owner.canonical_authority.branch_id
                        || fence.reorg_epoch.0 != owner.reorg_epoch
                    {
                        return Err(reject("owner_control_context_mismatch"));
                    }
                    super::entry_executor_control::check_payload(
                        &q.original,
                        &owner.public_key,
                        &fence,
                        true,
                    )?;
                    let actual = oasis7_client_api::world_service_correlation::derive_correlation(
                        entry.trust.world.clone(),
                        &q.original,
                    )
                    .map_err(|_| reject("invalid_request"))?;
                    if actual.key != q.request.key {
                        return Err(reject("owner_control_context_mismatch"));
                    }
                    let wrapper = oasis7_client_api::world_service::entry_executor::EntryExecutorLookupRequestV1 {
                        access: oasis7_client_api::world_service::entry_executor::EntryExecutorAccessV1 {
                            read: request.access.read.clone(), fence,
                        }, lookup: q.clone(),
                    };
                    entry.forward_bound(
                        oasis7_client_api::world_service::entry_executor::EXECUTOR_LOOKUP_PATH,
                        LOOKUP_PATH,
                        &wrapper,
                        q,
                    )
                }
                EntryOwnerOperationV1::PrepareGoal(q) => {
                    q.verify().map_err(|_| reject("authentication_failed"))?;
                    let r = &q.request;
                    if r.agent_id != owner.agent_id
                        || q.subject_public_key != owner.public_key
                        || r.world_id != entry.trust.world.world_id
                        || r.genesis_digest != entry.trust.world.genesis_digest
                        || r.branch_id != owner.canonical_authority.branch_id
                        || r.reorg_epoch.0 != owner.reorg_epoch
                        || r.owner_generation.0
                            != owner.canonical_authority.agent_identity_generation
                    {
                        return Err(reject("owner_control_context_mismatch"));
                    }
                    entry.forward_bound(GOAL_PREPARE_PATH, GOAL_PREPARE_PATH, q, q)
                }
                EntryOwnerOperationV1::ReadGoal(q) => {
                    verify_current_request(q).map_err(|_| reject("authentication_failed"))?;
                    if q.request.agent_id != owner.agent_id
                        || q.subject_public_key != owner.public_key
                        || q.request.world != entry.trust.world
                    {
                        return Err(reject("owner_control_context_mismatch"));
                    }
                    entry.forward_bound(GOAL_CURRENT_PATH, GOAL_CURRENT_PATH, q, q)
                }
            }
        })();
        Some(result.and_then(|value| {
            self.session(sid, super::now_ms())?;
            Ok(value)
        }))
    }
}
fn owner_observation(
    request: &ParticipantOwnerViewRequestV1,
    reply: &Value,
    needs_task: bool,
) -> Result<
    (
        oasis7_proto::viewer::CanonicalAgentOwnerFenceV1,
        Option<GameTaskControlV1>,
        CommitRef,
    ),
    StoreError,
> {
    match request.response_profile {
        Some(ParticipantOwnerViewProfileV1::OwnerSetupV1) => {
            let response: SignedServiceResponse<OwnerSetupObservationV1> =
                serde_json::from_value(reply["world_response"].clone())
                    .map_err(|_| reject("owner_control_unavailable"))?;
            response
                .payload
                .validate(request)
                .map_err(|_| reject("owner_control_unavailable"))?;
            let control = response.payload.control;
            Ok((
                control.view.canonical_agent_owner,
                control.view.game_task_control,
                control.version.commit,
            ))
        }
        Some(ParticipantOwnerViewProfileV1::OwnerControlV1) => {
            let response: SignedServiceResponse<ParticipantOwnerControlResponseV1> =
                serde_json::from_value(reply["world_response"].clone())
                    .map_err(|_| reject("owner_control_unavailable"))?;
            response
                .payload
                .validate(request)
                .map_err(|_| reject("owner_control_unavailable"))?;
            Ok((
                response.payload.view.canonical_agent_owner,
                response.payload.view.game_task_control,
                response.payload.version.commit,
            ))
        }
        None => {
            let response: SignedServiceResponse<ReadWorldViewResponse<Value>> =
                serde_json::from_value(reply["world_response"].clone())
                    .map_err(|_| reject("owner_control_unavailable"))?;
            response
                .payload
                .validate(&request.read.request)
                .map_err(|_| reject("owner_control_unavailable"))?;
            let owner =
                serde_json::from_value(response.payload.view["canonical_agent_owner"].clone())
                    .map_err(|_| reject("owner_control_unavailable"))?;
            // Legacy operations other than executor lookup never consumed task
            // projection fields. Preserve that contract for absent profile.
            let task = if needs_task {
                Some(
                    serde_json::from_value(response.payload.view["game_task_control"].clone())
                        .map_err(|_| reject("owner_control_context_mismatch"))?,
                )
            } else {
                None
            };
            Ok((owner, task, response.payload.version.commit))
        }
    }
}
fn validate_payload(
    payload: &WorldServicePayloadV1,
    owner: &oasis7_proto::viewer::CanonicalAgentOwnerFenceV1,
    world: &WorldIdentity,
) -> Result<(), StoreError> {
    let (agent, public, world_id, genesis, branch, generation) = match payload {
        WorldServicePayloadV1::GameStarterClaim(s) => (
            s.request.agent_id.as_str(),
            s.subject_public_key.as_str(),
            s.request.world_id.as_str(),
            Some(s.request.genesis_digest.as_str()),
            s.request.branch_id.as_str(),
            s.request.owner_generation.0,
        ),
        WorldServicePayloadV1::GameTaskCreate(s) => (
            s.request.agent_id.as_str(),
            s.subject_public_key.as_str(),
            s.request.world_id.as_str(),
            Some(s.request.genesis_digest.as_str()),
            s.request.branch_id.as_str(),
            s.request.owner_generation.0,
        ),
        WorldServicePayloadV1::GameGoalPublish(s) => (
            s.request.agent_id.as_str(),
            s.subject_public_key.as_str(),
            s.request.world_id.as_str(),
            Some(s.request.genesis_digest.as_str()),
            s.request.branch_id.as_str(),
            s.request.owner_generation.0,
        ),
        WorldServicePayloadV1::GameExecutorControl(s) => {
            if matches!(&s.request.operation,GameExecutorOperationV1::Bind{executor_public_key,..} if executor_public_key==&owner.public_key)
            {
                return Err(reject("owner_key_cannot_be_delegate"));
            }
            (
                s.request.fence.agent_id.as_str(),
                s.subject_public_key.as_str(),
                s.request.fence.world_id.as_str(),
                Some(s.request.fence.genesis_digest.as_str()),
                s.request.fence.branch_id.as_str(),
                s.request.fence.owner_generation.0,
            )
        }
        WorldServicePayloadV1::Delegation(s) => {
            if s.request.delegate_public_key == owner.public_key {
                return Err(reject("owner_key_cannot_be_delegate"));
            }
            (
                s.request.agent_id.as_str(),
                s.subject_public_key.as_str(),
                s.request.world_id.as_str(),
                None,
                s.request.branch_id.as_str(),
                s.request.agent_identity_generation,
            )
        }
        _ => return Err(reject("owner_control_operation_not_allowed")),
    };
    if agent != owner.agent_id
        || public != owner.public_key
        || world_id != world.world_id
        || genesis.is_some_and(|g| g != world.genesis_digest)
        || branch != owner.canonical_authority.branch_id
        || generation != owner.canonical_authority.agent_identity_generation
    {
        return Err(reject("owner_control_context_mismatch"));
    }
    Ok(())
}
