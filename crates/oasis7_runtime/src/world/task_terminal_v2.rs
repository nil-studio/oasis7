//! New terminal authority; old V1 payloads/domains and result maps are unchanged.
use super::World;
use crate::service_admission::{self as admission, SignedReadRequest};
use oasis7_agent_api::game::*;
use oasis7_world_state::{TaskTerminalIndexV2, TaskTerminalRecordV2};
use std::collections::BTreeMap;
fn deny(reason: impl Into<String>) -> crate::WorldError {
    crate::WorldError::DistributedValidationFailed {
        reason: reason.into(),
    }
}
fn hash<T: serde::Serialize>(value: &T) -> Result<String, String> {
    oasis7_wasm_abi::canonical_hash(value).map_err(|e| e.to_string())
}
fn signature(record: &TaskTerminalRecordV2) -> Result<(), String> {
    oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
        GAME_TASK_CLEANUP_DOMAIN_V2,
        &record.original.request,
        &record.original.subject_public_key,
        &record.original.signature_hex,
    )?;
    let p = &record.original.request;
    p.validate()?;
    let c = &record.closed;
    p.permitted_branches.exact_initial_active.validate()?;
    if record.original.subject_public_key
        != p.permitted_branches.exact_initial_active.owner_public_key
        || c.close_original_digest != hash(&record.original)?
        || record.committed_at_unix_ms > p.cleanup_until_ms.0
    {
        return Err("terminal original or deadline mismatch".into());
    }
    match &c.selected_predicate {
        GameTaskClosePredicateV2::InitialActive => {
            if c.actual_before_fence != p.permitted_branches.exact_initial_active
                || record.stop_predecessor_canonical_cbor.is_some()
            {
                return Err("terminal initial predicate mismatch".into());
            }
        }
        GameTaskClosePredicateV2::OwnStopSuspended(reference) => {
            let intent = p
                .permitted_branches
                .own_stop_intent
                .as_ref()
                .ok_or("OwnStop was not allowed")?;
            let bytes = record
                .stop_predecessor_canonical_cbor
                .as_ref()
                .ok_or("guarded Stop full witness missing")?;
            let stopped: oasis7_world_state::TaskStopRecordV2 =
                serde_cbor::from_slice(bytes).map_err(|_| "invalid guarded Stop witness codec")?;
            if serde_cbor::to_vec(&stopped).map_err(|e| e.to_string())? != *bytes {
                return Err("guarded Stop witness is not canonical typed bytes".into());
            }
            super::task_stop_v2::validate_record(&stopped)?;
            let stop = &stopped.original;
            if stop.request.initial_fence != p.permitted_branches.exact_initial_active
                || stop.subject_public_key != record.original.subject_public_key
                || stop.request.client_operation_id != intent.operation_id
                || stop.request.custody_account_id != p.custody_account_id
                || stop.request.termination_plan_digest != p.termination_plan_digest
                || stop.request.target_executor_public_key != p.signer_public_key
                || stop.request.target_executor_generation != p.signer_generation
                || stop.request.target_executor_nonce != p.signer_nonce
                || hash(&(
                    GAME_TASK_STOP_DOMAIN_V2,
                    &stop.subject_public_key,
                    &stop.request,
                ))? != intent.intent_digest
                || stopped.committed_height > record.committed_height
                || reference.profile != intent.profile
                || reference.operation_id != stop.request.client_operation_id
                || reference.original_digest != hash(stop)?
                || reference.canonical_result_digest != stopped.result_core_digest
                || reference.post_fence != GameTaskFenceV1::from_control(&stopped.after_control)
                || reference.post_fence != c.actual_before_fence
            {
                return Err("terminal guarded Stop lineage mismatch".into());
            }
        }
    }
    Ok(())
}
/// Restore validates full original/witness independently of live current state.
pub(crate) fn validate_index(world: &World) -> Result<(), String> {
    if let Some(index) = &world.capability_revocation_state.task_terminals_v2 {
        index.validate()?;
        for record in index.records.values() {
            signature(record)?;
            let f = &record.closed.initial_fence;
            let current = world
                .capability_revocation_state
                .agent_signer_delegations
                .get(&f.agent_id)
                .ok_or("terminal permanent signer floor missing")?;
            if current.generation < record.closed.revoked_generation.0
                || current.nonce < record.closed.permanent_nonce_high_water.0
                || (current.generation == record.closed.revoked_generation.0
                    && (!current.revoked
                        || current.delegate_public_key != record.closed.revoked_signer_public_key))
            {
                return Err("terminal permanent signer floor regressed".into());
            }
        }
    }
    Ok(())
}
pub(super) fn project(
    world: &World,
    state: &mut oasis7_world_state::CapabilityRevocationState,
    record: &TaskTerminalRecordV2,
) -> Result<(), crate::WorldError> {
    signature(record).map_err(deny)?;
    let p = &record.original.request;
    let f = &p.permitted_branches.exact_initial_active;
    if let Some(old) = state
        .task_terminals_v2
        .as_ref()
        .and_then(|i| i.records.get(&f.task_id))
    {
        return if old == record {
            Ok(())
        } else {
            Err(deny("immutable task terminal conflict"))
        };
    }
    let expected = prepare(
        world,
        &record.original,
        record.committed_height,
        record.committed_at_unix_ms,
    )
    .map_err(deny)?;
    if expected != *record {
        return Err(deny("task terminal derived record mismatch"));
    }
    let delegation = state
        .agent_signer_delegations
        .get_mut(&f.agent_id)
        .ok_or_else(|| deny("terminal signer missing"))?;
    delegation.generation = p.revoke_generation.0;
    delegation.nonce = p.revoke_nonce.0;
    delegation.revoked = true;
    let index = state
        .task_terminals_v2
        .get_or_insert_with(|| TaskTerminalIndexV2 {
            schema_revision: 2,
            records: BTreeMap::new(),
        });
    index.records.insert(f.task_id.clone(), record.clone());
    index.validate().map_err(deny)
}
fn prepare(
    world: &World,
    original: &SignedReadRequest<GameTaskRevokeAndCloseV2>,
    height: u64,
    at: u64,
) -> Result<TaskTerminalRecordV2, String> {
    oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
        GAME_TASK_CLEANUP_DOMAIN_V2,
        &original.request,
        &original.subject_public_key,
        &original.signature_hex,
    )?;
    let p = &original.request;
    p.validate()?;
    let initial = &p.permitted_branches.exact_initial_active;
    initial.validate()?;
    if p.api_revision != 2
        || initial.task_status != GameTaskStatusV1::Active
        || at > p.cleanup_until_ms.0
        || p.cleanup_until_ms.0 == 0
        || admission::agent_authority::owner_public_key(world, &initial.agent_id)?
            != original.subject_public_key
    {
        return Err("terminal owner or deadline unavailable".into());
    }
    let identity = world
        .current_agent_identity(&initial.agent_id)
        .ok_or("terminal identity missing")?;
    let runtime = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if initial.owner_binding != identity.owner_binding
        || initial.owner_generation.0 != identity.generation
        || initial.world_id != runtime.world_id
        || initial.branch_id != runtime.branch_id
        || initial.reorg_epoch.0 != runtime.reorg_epoch
        || original.subject_public_key != initial.owner_public_key
        || world
            .chain_resource_manifest()
            .genesis_ref
            .as_ref()
            .is_none_or(|g| g != &initial.genesis_digest)
    {
        return Err("terminal current owner/World mismatch".into());
    }
    let current = admission::game_task::current_control_before_terminal(world, &initial.agent_id)?
        .ok_or("terminal current task unavailable")?;
    let current = GameTaskFenceV1::from_control(&current);
    let delegation = world
        .capability_revocation_state
        .agent_signer_delegations
        .get(&initial.agent_id)
        .ok_or("terminal signer unavailable")?;
    if delegation.revoked
        || delegation.delegate_public_key != p.signer_public_key
        || delegation.generation != p.signer_generation.0
        || delegation.nonce != p.signer_nonce.0
        || delegation.owner_binding != initial.owner_binding
        || delegation.agent_identity_generation != initial.owner_generation.0
        || delegation.world_id != initial.world_id
        || delegation.branch_id != initial.branch_id
        || initial.executor_binding.as_ref().is_none_or(|binding| {
            binding.executor_public_key != p.signer_public_key
                || binding.delegation_generation.0 != p.signer_generation.0
        })
        || p.revoke_generation.0 <= delegation.generation
        || p.revoke_nonce.0 <= delegation.nonce
    {
        return Err("terminal signer or checked high-water mismatch".into());
    }
    let (selected, witness) = if current == *initial {
        (GameTaskClosePredicateV2::InitialActive, None)
    } else {
        let intent = p
            .permitted_branches
            .own_stop_intent
            .as_ref()
            .ok_or("signed conditions only allow exact InitialActive")?;
        let mut found = None;
        if let Some(index) = &world.capability_revocation_state.task_stops_v2 {
            for stopped in index.records.values() {
                let stop = &stopped.original;
                if stop.request.client_operation_id == intent.operation_id
                    && hash(&(
                        GAME_TASK_STOP_DOMAIN_V2,
                        &stop.subject_public_key,
                        &stop.request,
                    ))? == intent.intent_digest
                    && found.replace(stopped).is_some()
                {
                    return Err("ambiguous guarded Stop witness".into());
                }
            }
        }
        let stopped = found.ok_or("exact guarded Stop committed witness unavailable")?;
        let bytes = serde_cbor::to_vec(stopped).map_err(|e| e.to_string())?;
        (
            GameTaskClosePredicateV2::OwnStopSuspended(Box::new(GameOwnStopReferenceV2 {
                profile: GameStopIntentProfileV2::GuardedTaskStopV2,
                operation_id: intent.operation_id.clone(),
                original_digest: hash(&stopped.original)?,
                canonical_result_digest: stopped.result_core_digest.clone(),
                post_fence: current.clone(),
            })),
            Some(bytes),
        )
    };
    let record = TaskTerminalRecordV2 {
        schema_revision: 2,
        original: original.clone(),
        committed_height: height,
        committed_at_unix_ms: at,
        stop_predecessor_canonical_cbor: witness,
        closed: GameTaskClosedV2 {
            schema_revision: 2,
            close_operation_id: p.client_operation_id.clone(),
            close_original_digest: hash(original)?,
            termination_plan_digest: p.termination_plan_digest.clone(),
            initial_fence: initial.clone(),
            actual_before_fence: current,
            selected_predicate: selected,
            revoked_signer_public_key: p.signer_public_key.clone(),
            revoked_generation: p.revoke_generation,
            permanent_nonce_high_water: p.revoke_nonce,
        },
    };
    signature(&record)?;
    Ok(record)
}
impl World {
    /// Exact permanent task-ID lookup never depends on evictable V1 results.
    pub fn closed_game_task_v2(&self, task_id: &str) -> Result<Option<GameTaskClosedV2>, String> {
        validate_index(self)?;
        Ok(self
            .capability_revocation_state
            .task_terminals_v2
            .as_ref()
            .and_then(|index| index.records.get(task_id))
            .map(|record| record.closed.clone()))
    }
    /// Typed current lifecycle projection; Closed has no executor binding.
    pub fn current_game_task_state_v2(
        &self,
        agent: &str,
    ) -> Result<Option<GameTaskStateV2>, String> {
        let Some(control) = admission::game_task::current_control_before_terminal(self, agent)?
        else {
            return Ok(None);
        };
        if let Some(record) = self
            .capability_revocation_state
            .task_terminals_v2
            .as_ref()
            .and_then(|index| index.records.get(&control.task_id))
        {
            return Ok(Some(GameTaskStateV2::Closed(Box::new(
                record.closed.clone(),
            ))));
        }
        Ok(Some(GameTaskStateV2::Open(Box::new(
            GameTaskFenceV1::from_control(&control),
        ))))
    }
    /// Canonical execution only: the clock comes from the begun verified tick,
    /// never caller time, an old committed header or logical time.
    pub fn apply_task_revoke_and_close_v2(
        &mut self,
        original: &SignedReadRequest<GameTaskRevokeAndCloseV2>,
    ) -> Result<TaskTerminalRecordV2, String> {
        let (context, _) = self
            .formal_execution_tick
            .as_ref()
            .ok_or("terminal requires begun formal committed context")?;
        let at = u64::try_from(context.committed_at_unix_ms)
            .map_err(|_| "terminal committed clock is negative")?;
        let record = prepare(self, original, context.height, at)?;
        let mut transaction = self.clone();
        admission::game_task::retire_task_contexts(
            &mut transaction,
            &original
                .request
                .permitted_branches
                .exact_initial_active
                .agent_id,
            &[],
        )?;
        transaction
            .append_capability_authorization_event_batch(vec![
                crate::CapabilityAuthorizationEvent::TaskTerminalRecordedV2 {
                    record: Box::new(record.clone()),
                },
            ])
            .map_err(|e| format!("{e:?}"))?;
        *self = transaction;
        Ok(record)
    }
}

#[cfg(test)]
#[path = "task_terminal_v2_signature_tests.rs"]
mod signature_tests;
