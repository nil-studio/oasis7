//! Current task-fenced executor gateway. Owner controls are not representable.
use super::{
    reject,
    service::{IdentityService, audit, consume_quota, session_key, valid_session},
    store::StoreError,
};
use oasis7_agent_api::game::*;
use oasis7_client_api::{
    world_service::{
        entry_executor::*, task_context::*, task_factory_quote::*, task_recipe_quote::*, *,
    },
    world_service_wire::{SchedulerOperationV1, WorldServicePayloadV1},
};
use serde_json::Value;
const MAX_EXECUTOR_REPLY: usize = 4 * 1024 * 1024;
#[cfg(test)]
#[path = "entry_executor_control_tests.rs"]
mod tests;

impl IdentityService {
    pub(super) fn entry_executor_route(
        &mut self,
        method: &str,
        path: &str,
        sid: Option<&str>,
        body: &Value,
        now: u64,
    ) -> Option<Result<Value, StoreError>> {
        if path != "/entry-executor/control" {
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
                .filter(|e| e.executor_control_enabled)
                .ok_or_else(|| reject("executor_control_unavailable"))?;
            let bytes = serde_json::to_vec(body).map_err(|_| reject("invalid_request"))?;
            if bytes.len() > 65536 {
                return Err(reject("invalid_request"));
            }
            let request: EntryExecutorControlRequestV1 =
                parse_request(&bytes).map_err(|_| reject("invalid_request"))?;
            let access = &request.access;
            let fence = &access.fence;
            fence.validate().map_err(|_| reject("invalid_request"))?;
            if access.read.request.world != entry.trust.world
                || fence.world_id != entry.trust.world.world_id
                || fence.genesis_digest != entry.trust.world.genesis_digest
                || access.read.request.scope_id != format!("agent:{}", fence.agent_id)
                || access.read.request.fixed_commit.is_some()
                || access.read.request.observation_nonce.is_none()
                || access.read.subject_public_key == fence.owner_public_key
            {
                return Err(reject("executor_context_mismatch"));
            }
            let resource = request_digest(
                "entry-executor-resource",
                &(
                    fence.world_id.as_str(),
                    fence.genesis_digest.as_str(),
                    fence.agent_id.as_str(),
                    fence.task_id.as_str(),
                    fence.owner_generation,
                    fence.executor_epoch,
                ),
            )
            .map_err(|_| reject("invalid_request"))?;
            self.store.transaction(|tx| {
                consume_quota(
                    tx,
                    "entry-executor-account",
                    &account.account_id,
                    now,
                    60_000,
                    240,
                )?;
                audit(tx, &account, "entry_executor_request", now)
            })?;
            let reply = entry.forward_bounded(
                VIEW_JSON_PATH,
                VIEW_JSON_PATH,
                &access.read,
                &access.read,
                MAX_EXECUTOR_REPLY,
            )?;
            let response: SignedServiceResponse<ReadWorldViewResponse<Value>> =
                serde_json::from_value(reply["world_response"].clone())
                    .map_err(|_| reject("executor_control_unavailable"))?;
            response
                .payload
                .validate(&access.read.request)
                .map_err(|_| reject("executor_control_unavailable"))?;
            let current: GameTaskControlV1 =
                serde_json::from_value(response.payload.view["game_task_control"].clone())
                    .map_err(|_| reject("executor_context_mismatch"))?;
            let actor = access.read.subject_public_key.as_str();
            if GameTaskFenceV1::from_control(&current) != *fence
                || current.task_status != GameTaskStatusV1::Active
                || current
                    .executor_binding
                    .as_ref()
                    .is_none_or(|b| b.executor_public_key != actor)
                || current.owner_public_key == actor
                || current.branch_id != response.payload.version.commit.binding.branch_id
                || current.reorg_epoch.0 != response.payload.version.commit.binding.reorg_generation
            {
                return Err(reject("executor_context_mismatch"));
            }
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
                )?;
                // Only a canonically authorized actor may spend this resource's
                // bucket. A different account cannot exhaust it with forged reads.
                consume_quota(
                    tx,
                    "entry-executor-resource",
                    &resource,
                    super::now_ms(),
                    60_000,
                    120,
                )
            })?;
            // A closed typed dispatcher prevents escalation into owner, account,
            // arbitrary CAS or operator routes even with a valid executor key.
            match &request.operation {
                EntryExecutorOperationV1::ReadView(q) => {
                    check_read(q, actor, fence, &entry.trust.world)?;
                    entry.forward_bounded(VIEW_JSON_PATH, VIEW_JSON_PATH, q, q, MAX_EXECUTOR_REPLY)
                }
                EntryExecutorOperationV1::Describe(q) => {
                    if q.subject_public_key != actor
                        || q.request.expected_world != entry.trust.world
                        || q.request.trust_config_ref != entry.trust.service_public_key
                    {
                        return Err(reject("executor_context_mismatch"));
                    }
                    verify_read_request(DESCRIBE_PATH, q)
                        .map_err(|_| reject("authentication_failed"))?;
                    entry.forward_bounded(DESCRIBE_PATH, DESCRIBE_PATH, q, q, MAX_EXECUTOR_REPLY)
                }
                EntryExecutorOperationV1::Submit(q) => {
                    q.validate().map_err(|_| reject("invalid_request"))?;
                    check_payload(&q.signed_payload, actor, fence, false)?;
                    let actual = oasis7_client_api::world_service_correlation::derive_correlation(
                        entry.trust.world.clone(),
                        &q.signed_payload,
                    )
                    .map_err(|_| reject("invalid_request"))?;
                    if actual != q.correlation {
                        return Err(reject("executor_context_mismatch"));
                    }
                    entry.forward_bounded(SUBMIT_PATH, SUBMIT_PATH, q, q, MAX_EXECUTOR_REPLY)
                }
                EntryExecutorOperationV1::Lookup(q) => {
                    q.request
                        .validate()
                        .map_err(|_| reject("invalid_request"))?;
                    check_payload(&q.original, actor, fence, true)?;
                    let actual = oasis7_client_api::world_service_correlation::derive_correlation(
                        entry.trust.world.clone(),
                        &q.original,
                    )
                    .map_err(|_| reject("invalid_request"))?;
                    if actual.key != q.request.key {
                        return Err(reject("executor_context_mismatch"));
                    }
                    let wrapper = EntryExecutorLookupRequestV1 {
                        access: access.clone(),
                        lookup: q.clone(),
                    };
                    entry.forward_bounded(
                        EXECUTOR_LOOKUP_PATH,
                        LOOKUP_PATH,
                        &wrapper,
                        q,
                        MAX_EXECUTOR_REPLY,
                    )
                }
                EntryExecutorOperationV1::PrepareContext(q) => {
                    verify_prepare(q).map_err(|_| reject("authentication_failed"))?;
                    check_actor_fence(&q.subject_public_key, &q.request.fence, actor, fence)?;
                    entry.forward_bounded(
                        TASK_CONTEXT_PREPARE_PATH,
                        TASK_CONTEXT_PREPARE_PATH,
                        q,
                        q,
                        MAX_EXECUTOR_REPLY,
                    )
                }
                EntryExecutorOperationV1::ReadContext(q) => {
                    verify_get(q).map_err(|_| reject("authentication_failed"))?;
                    check_actor_fence(
                        &q.subject_public_key,
                        &q.request.context.fence,
                        actor,
                        fence,
                    )?;
                    entry.forward_bounded(
                        TASK_CONTEXT_READ_PATH,
                        TASK_CONTEXT_READ_PATH,
                        q,
                        q,
                        MAX_EXECUTOR_REPLY,
                    )
                }
                EntryExecutorOperationV1::GoalSnapshot(q) => {
                    verify_snapshot_request(q).map_err(|_| reject("authentication_failed"))?;
                    check_actor_fence(&q.subject_public_key, &q.request.fence, actor, fence)?;
                    entry.forward_bounded(
                        TASK_SNAPSHOT_PATH,
                        TASK_SNAPSHOT_PATH,
                        q,
                        q,
                        MAX_EXECUTOR_REPLY,
                    )
                }
                EntryExecutorOperationV1::RecipeQuote(q) => {
                    verify_recipe_quote_request(q).map_err(|_| reject("authentication_failed"))?;
                    check_actor_fence(
                        &q.subject_public_key,
                        &q.request.context_proof.request.context.fence,
                        actor,
                        fence,
                    )?;
                    entry.forward_bounded(
                        TASK_RECIPE_QUOTE_PATH,
                        TASK_RECIPE_QUOTE_PATH,
                        q,
                        q,
                        MAX_EXECUTOR_REPLY,
                    )
                }
                EntryExecutorOperationV1::FactoryQuote(q) => {
                    verify_factory_quote_request(q).map_err(|_| reject("authentication_failed"))?;
                    check_actor_fence(
                        &q.subject_public_key,
                        &q.request.context_proof.request.context.fence,
                        actor,
                        fence,
                    )?;
                    entry.forward_bounded(
                        TASK_FACTORY_QUOTE_PATH,
                        TASK_FACTORY_QUOTE_PATH,
                        q,
                        q,
                        MAX_EXECUTOR_REPLY,
                    )
                }
            }
        })();
        Some(result.and_then(|v| {
            self.session(sid, super::now_ms())?;
            Ok(v)
        }))
    }
}
fn check_actor_fence(
    public: &str,
    actual: &GameTaskFenceV1,
    actor: &str,
    expected: &GameTaskFenceV1,
) -> Result<(), StoreError> {
    if public != actor || actual != expected {
        return Err(reject("executor_context_mismatch"));
    }
    Ok(())
}
fn check_read(
    q: &SignedReadRequest<ReadWorldViewRequest>,
    actor: &str,
    fence: &GameTaskFenceV1,
    world: &WorldIdentity,
) -> Result<(), StoreError> {
    if q.subject_public_key != actor
        || q.request.world != *world
        || q.request.scope_id != format!("agent:{}", fence.agent_id)
        || q.request.fixed_commit.is_some()
    {
        return Err(reject("executor_context_mismatch"));
    }
    verify_read_request(VIEW_JSON_PATH, q).map_err(|_| reject("authentication_failed"))
}
fn check_payload(
    payload: &WorldServicePayloadV1,
    actor: &str,
    fence: &GameTaskFenceV1,
    historical: bool,
) -> Result<(), StoreError> {
    let generation = fence
        .executor_binding
        .as_ref()
        .ok_or_else(|| reject("executor_context_mismatch"))?
        .delegation_generation
        .0;
    match payload {
        WorldServicePayloadV1::Cognition(s) => {
            if (!historical && s.request.delegation_generation != generation)
                || s.request.request.agent_id != fence.agent_id
                || s.request.task_fence.is_none()
                || s.request.causal_proposal.is_some()
            {
                return Err(reject("executor_context_mismatch"));
            }
            check_payload_fence(
                &s.subject_public_key,
                s.request.task_fence.as_ref().unwrap(),
                actor,
                fence,
                historical,
            )
        }
        WorldServicePayloadV1::Scheduler(s) => {
            if s.subject_public_key != actor
                || s.request.agent_id != fence.agent_id
                || (!historical && s.request.delegation_generation != generation)
            {
                return Err(reject("executor_context_mismatch"));
            }
            let context = match &s.request.operation {
                SchedulerOperationV1::TaskProviderPrefix { context }
                | SchedulerOperationV1::TaskProviderFailure { context, .. } => context,
                _ => return Err(reject("executor_operation_not_allowed")),
            };
            check_payload_fence(
                &s.subject_public_key,
                &context.fence,
                actor,
                fence,
                historical,
            )
        }
        _ => Err(reject("executor_operation_not_allowed")),
    }
}
fn check_payload_fence(
    public: &str,
    old: &GameTaskFenceV1,
    actor: &str,
    current: &GameTaskFenceV1,
    historical: bool,
) -> Result<(), StoreError> {
    if !historical {
        return check_actor_fence(public, old, actor, current);
    }
    if public != actor
        || old.world_id != current.world_id
        || old.genesis_digest != current.genesis_digest
        || old.agent_id != current.agent_id
        || old.task_id != current.task_id
        || old.owner_binding != current.owner_binding
        || old.owner_generation != current.owner_generation
        || old.owner_public_key != current.owner_public_key
    {
        return Err(reject("executor_context_mismatch"));
    }
    Ok(())
}
