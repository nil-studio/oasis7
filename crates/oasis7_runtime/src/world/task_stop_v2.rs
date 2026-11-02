//! Guarded Stop uses actual begun committed UTC, new Owner domain and intrinsic history.
use super::World;
use crate::service_admission::{self as admission};
use oasis7_agent_api::game::*;
use oasis7_world_state::{TaskStopIndexV2, TaskStopRecordV2};
use std::collections::BTreeMap;
fn deny(reason: impl Into<String>) -> crate::WorldError {
    crate::WorldError::DistributedValidationFailed {
        reason: reason.into(),
    }
}
pub(crate) fn validate_record(record: &TaskStopRecordV2) -> Result<(), String> {
    record.validate()?;
    oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
        GAME_TASK_STOP_DOMAIN_V2,
        &record.original.request,
        &record.original.subject_public_key,
        &record.original.signature_hex,
    )
}
pub(crate) fn validate_index(world: &World) -> Result<(), String> {
    if let Some(index) = &world.capability_revocation_state.task_stops_v2 {
        index.validate()?;
        for record in index.records.values() {
            validate_record(record)?;
        }
    }
    Ok(())
}
fn prepare(
    world: &World,
    original: &SignedGameTaskStopV2,
    height: u64,
    at: u64,
) -> Result<TaskStopRecordV2, String> {
    original.validate_shape()?;
    oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
        GAME_TASK_STOP_DOMAIN_V2,
        &original.request,
        &original.subject_public_key,
        &original.signature_hex,
    )?;
    let p = &original.request;
    let f = &p.initial_fence;
    if at >= p.execute_until_ms.0 {
        return Err("guarded Stop original expired at canonical execution".into());
    }
    admission::game_task::ensure_task_not_closed(world, &f.task_id)?;
    if admission::agent_authority::owner_public_key(world, &f.agent_id)?
        != original.subject_public_key
    {
        return Err("guarded Stop current Owner mismatch".into());
    }
    let identity = world
        .current_agent_identity(&f.agent_id)
        .ok_or("guarded Stop identity missing")?;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if f.owner_binding != identity.owner_binding
        || f.owner_generation.0 != identity.generation
        || f.world_id != binding.world_id
        || f.branch_id != binding.branch_id
        || f.reorg_epoch.0 != binding.reorg_epoch
        || world.chain_resource_manifest().genesis_ref.as_ref() != Some(&f.genesis_digest)
    {
        return Err("guarded Stop owner/history scope changed".into());
    }
    let delegation = world
        .capability_revocation_state
        .agent_signer_delegations
        .get(&f.agent_id)
        .ok_or("guarded Stop delegation missing")?;
    if delegation.revoked
        || delegation.delegate_public_key != p.target_executor_public_key
        || delegation.generation != p.target_executor_generation.0
        || delegation.nonce != p.target_executor_nonce.0
        || delegation.owner_binding != f.owner_binding
        || delegation.agent_identity_generation != f.owner_generation.0
        || delegation.world_id != f.world_id
        || delegation.branch_id != f.branch_id
    {
        return Err("guarded Stop actual signer lineage changed".into());
    }
    let before = admission::game_task::current_control_before_terminal(world, &f.agent_id)?
        .ok_or("guarded Stop current task missing")?;
    let after = p.derive_post_control(&before)?;
    let mut record = TaskStopRecordV2 {
        schema_revision: 2,
        original: original.clone(),
        before_control: before,
        after_control: after,
        committed_height: height,
        committed_at_unix_ms: at,
        result_core_digest: String::new(),
    };
    record.result_core_digest = record.compute_result_core_digest()?;
    validate_record(&record)?;
    Ok(record)
}
pub(super) fn project(
    world: &World,
    state: &mut oasis7_world_state::CapabilityRevocationState,
    record: &TaskStopRecordV2,
) -> Result<(), crate::WorldError> {
    validate_record(record).map_err(deny)?;
    let key = record.operation_key().map_err(deny)?;
    if let Some(old) = state
        .task_stops_v2
        .as_ref()
        .and_then(|i| i.records.get(&key))
    {
        return if old == record {
            Ok(())
        } else {
            Err(deny("immutable guarded Stop original conflict"))
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
        return Err(deny("guarded Stop canonical record mismatch"));
    }
    let index = state.task_stops_v2.get_or_insert_with(|| TaskStopIndexV2 {
        schema_revision: 2,
        records: BTreeMap::new(),
    });
    index.records.insert(key, record.clone());
    index.validate().map_err(deny)
}
impl World {
    /// Pure historical lookup. It never refreshes an original or grants a new dispatch.
    pub fn guarded_task_stop_v2(
        &self,
        original: &SignedGameTaskStopV2,
    ) -> Result<Option<TaskStopRecordV2>, String> {
        validate_index(self)?;
        Ok(self
            .capability_revocation_state
            .task_stops_v2
            .as_ref()
            .and_then(|i| i.records.values().find(|r| &r.original == original))
            .cloned())
    }
    /// No caller timestamp: only the private context installed by actual qualified execution.
    pub fn apply_guarded_task_stop_v2(
        &mut self,
        original: &SignedGameTaskStopV2,
    ) -> Result<TaskStopRecordV2, String> {
        let (context, _) = self
            .formal_execution_tick
            .as_ref()
            .ok_or("guarded Stop requires begun formal committed context")?;
        let at = u64::try_from(context.committed_at_unix_ms)
            .map_err(|_| "negative committed Stop clock")?;
        let record = prepare(self, original, context.height, at)?;
        let key = record.operation_key()?;
        if self
            .capability_revocation_state
            .task_stops_v2
            .as_ref()
            .is_some_and(|i| i.records.contains_key(&key))
        {
            return Err("guarded Stop already recorded; use exact known lookup".into());
        }
        let mut transaction = self.clone();
        admission::game_task::retire_task_contexts(
            &mut transaction,
            &original.request.initial_fence.agent_id,
            &[],
        )?;
        transaction
            .append_capability_authorization_event_batch(vec![
                crate::CapabilityAuthorizationEvent::TaskStoppedV2 {
                    record: Box::new(record.clone()),
                },
            ])
            .map_err(|e| format!("{e:?}"))?;
        *self = transaction;
        Ok(record)
    }
}

impl World {
    /// No cold reader: only complete already resident predecessors qualify.
    pub fn resident_guarded_task_history(
        &self,
    ) -> Result<crate::service_admission::game_task::ResidentTaskHistory<'_>, String> {
        self.verify_capability_authorization_root()
            .map_err(|e| format!("{e:?}"))?;
        crate::service_admission::game_task::resident_task_history(self)
    }
}
