//! Fact archive and current delivery are separate typed evidence, never a grant.
use oasis7_agent_api::game::DecimalU64;
use oasis7_client_api::world_service::{CommitRef, task_cleanup_v2::*};
use oasis7_runtime::World;
pub fn observe(
    world: &World,
    commit: &CommitRef,
    historical: Option<(&World, &CommitRef)>,
    request: &TaskCleanupLookupV2,
) -> Result<TaskCleanupObservationV2, String> {
    request.submitted.verify_owner_signature()?;
    request.validate()?;
    commit.validate().map_err(|e| e.to_string())?;
    if commit.world != request.submitted.expected_world {
        return Err("cleanup observation World mismatch".into());
    }
    let mut response = TaskCleanupObservationV2 {
        api_revision: 2,
        world: commit.world.clone(),
        original_digest: request.submitted.original_digest()?,
        observation_nonce: request.observation_nonce.clone(),
        delivery_commit: commit.clone(),
        outcome: TaskCleanupOutcomeV2::Unknown,
    };
    let p = &request.submitted.original.request;
    let f = &p.permitted_branches.exact_initial_active;
    let Some(record) = world
        .capability_revocation_state()
        .task_terminals_v2
        .as_ref()
        .and_then(|index| index.records.get(&f.task_id))
    else {
        return Ok(response);
    };
    if record.original != request.submitted.original {
        return Ok(response);
    }
    let closed = world
        .closed_game_task_v2(&f.task_id)?
        .ok_or("terminal missing from pinned World")?;
    let Some((historical_world, closed_at_commit)) = historical else {
        response.outcome = TaskCleanupOutcomeV2::HistoryUnavailable;
        return Ok(response);
    };
    closed_at_commit.validate().map_err(|e| e.to_string())?;
    let historical_record = historical_world
        .capability_revocation_state()
        .task_terminals_v2
        .as_ref()
        .and_then(|index| index.records.get(&f.task_id))
        .ok_or("historical terminal witness missing")?;
    historical_world
        .closed_game_task_v2(&f.task_id)?
        .ok_or("historical Closed state missing")?;
    if historical_record != record
        || closed_at_commit.world != commit.world
        || closed_at_commit.binding.branch_id != f.branch_id
        || closed_at_commit.binding.reorg_generation != f.reorg_epoch.0
    {
        return Err("historical cleanup commit/witness mismatch".into());
    }
    let historical_header = &historical_world
        .latest_tick_consensus_record()
        .ok_or("historical cleanup tick context missing")?
        .block
        .header;
    if historical_header.chain_height != Some(record.committed_height)
        || historical_header
            .committed_at_unix_ms
            .and_then(|at| u64::try_from(at).ok())
            != Some(record.committed_at_unix_ms)
    {
        return Err("historical cleanup clock/height differs from canonical tick".into());
    }
    let fact = Box::new(TaskCleanupFactV2 {
        closed,
        committed_height: DecimalU64(record.committed_height),
        committed_at_unix_ms: DecimalU64(record.committed_at_unix_ms),
        closed_at_commit: closed_at_commit.clone(),
        stop_predecessor_canonical_cbor: record.stop_predecessor_canonical_cbor.clone(),
    });
    let delivery = current_delivery(world, commit, request)?;
    response.outcome = TaskCleanupOutcomeV2::KnownClosed { fact, delivery };
    response.validate(request)?;
    Ok(response)
}
fn current_delivery(
    world: &World,
    commit: &CommitRef,
    request: &TaskCleanupLookupV2,
) -> Result<TaskCleanupDeliveryV2, String> {
    let p = &request.submitted.original.request;
    let f = &p.permitted_branches.exact_initial_active;
    let owner =
        oasis7_runtime::service_admission::agent_authority::owner_public_key(world, &f.agent_id);
    let identity = world.current_agent_identity(&f.agent_id);
    let (Ok(owner), Some(identity)) = (owner, identity) else {
        return Ok(TaskCleanupDeliveryV2::Drift(
            TaskCleanupDriftV2::OwnerOrLineage,
        ));
    };
    if owner != f.owner_public_key
        || identity.owner_binding != f.owner_binding
        || identity.generation != f.owner_generation.0
        || commit.binding.branch_id != f.branch_id
        || commit.binding.reorg_generation != f.reorg_epoch.0
    {
        return Ok(TaskCleanupDeliveryV2::Drift(
            TaskCleanupDriftV2::OwnerOrLineage,
        ));
    }
    let signer = world
        .capability_revocation_state()
        .agent_signer_delegations
        .get(&f.agent_id)
        .ok_or("terminal current signer missing")?;
    if !signer.revoked
        || signer.delegate_public_key != p.signer_public_key
        || signer.generation != p.revoke_generation.0
        || signer.nonce != p.revoke_nonce.0
    {
        return Ok(TaskCleanupDeliveryV2::Drift(
            TaskCleanupDriftV2::SignerOrHighWater,
        ));
    }
    Ok(TaskCleanupDeliveryV2::ExactClosed(TaskCleanupCurrentV2 {
        current_owner_public_key: owner,
        current_owner_binding: identity.owner_binding.clone(),
        current_owner_generation: DecimalU64(identity.generation),
        current_revoked_signer_public_key: signer.delegate_public_key.clone(),
        current_revoked_generation: DecimalU64(signer.generation),
        current_nonce_high_water: DecimalU64(signer.nonce),
    }))
}
/// Only an actual complete second pin may reach this method at the Node adapter.
/// Historical facts remain archivable after delivery-current drift.
pub fn before_delivery(
    mut response: TaskCleanupObservationV2,
    actual: &CommitRef,
) -> Result<TaskCleanupObservationV2, String> {
    actual.validate().map_err(|e| e.to_string())?;
    if actual.world != response.world {
        return Err("cleanup delivery World changed".into());
    }
    if actual != &response.delivery_commit {
        response.delivery_commit = actual.clone();
        match &mut response.outcome {
            TaskCleanupOutcomeV2::KnownClosed { delivery, .. } => {
                *delivery = TaskCleanupDeliveryV2::Drift(TaskCleanupDriftV2::DeliveryCommit);
            }
            _ => {
                response.outcome =
                    TaskCleanupOutcomeV2::UnresolvedCurrentDrift(TaskCleanupDriftV2::DeliveryCommit)
            }
        }
    }
    Ok(response)
}
#[cfg(test)]
#[path = "task_cleanup_v2_observation_tests.rs"]
mod tests;
