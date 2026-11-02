//! Read-only exact result extraction from an actual full qualified publication.
//! This does not authorize admission, sign a Node fact, or create Current.
use crate::formal_read_service::LocalStopPublication;
use oasis7_agent_api::game::GameTaskStopRecordV2;
use oasis7_client_api::world_service::{CommitRef, task_mutation_qualified_v2::*};
pub fn exact_result(
    publication: &LocalStopPublication,
    submitted: &TaskMutationSubmittedV2,
    execution: &TaskMutationExecutionV2,
) -> Result<(CommitRef, TaskMutationQualifiedResultV2), String> {
    submitted.validate_scope()?;
    let world = publication.world();
    let commit = publication.commit();
    if &commit.world != submitted.world() || world.state().time != execution.height.0 {
        return Err("mutation result is not the exact qualified Runtime cut".into());
    }
    let header = &world
        .latest_tick_consensus_record()
        .ok_or("qualified mutation tick missing")?
        .block
        .header;
    if header.chain_height != Some(execution.height.0)
        || header.chain_slot != Some(execution.slot.0)
        || header.chain_epoch != Some(execution.epoch.0)
        || header.node_block_hash.as_deref() != Some(execution.node_block_hash.as_str())
        || header.action_root.as_deref() != Some(execution.action_root.as_str())
        || header
            .committed_at_unix_ms
            .and_then(|at| u64::try_from(at).ok())
            != Some(execution.committed_at_unix_ms.0)
    {
        return Err("mutation execution does not match qualified Runtime header".into());
    }
    let result = match submitted {
        TaskMutationSubmittedV2::GuardedStop(q) => {
            let record = world
                .guarded_task_stop_v2(&q.original)?
                .ok_or("qualified Stop record missing")?;
            record.validate()?;
            if record.original != q.original
                || record.committed_height != execution.height.0
                || record.committed_at_unix_ms != execution.committed_at_unix_ms.0
            {
                return Err("qualified Stop differs from exact committed original".into());
            }
            TaskMutationQualifiedResultV2::GuardedStop {
                record: Box::new(record),
            }
        }
        TaskMutationSubmittedV2::RevokeAndClose(q) => {
            let id = &q
                .original
                .request
                .permitted_branches
                .exact_initial_active
                .task_id;
            let record = world
                .capability_revocation_state()
                .task_terminals_v2
                .as_ref()
                .and_then(|index| index.records.get(id))
                .ok_or("qualified Closed record missing")?;
            if record.original != q.original
                || record.committed_height != execution.height.0
                || record.committed_at_unix_ms != execution.committed_at_unix_ms.0
            {
                return Err("qualified Closed differs from exact committed original".into());
            }
            let closed = world
                .closed_game_task_v2(id)?
                .ok_or("qualified Closed state missing")?;
            let stop_predecessor = record
                .stop_predecessor_canonical_cbor
                .as_ref()
                .map(|bytes| -> Result<Box<GameTaskStopRecordV2>, String> {
                    let stop: GameTaskStopRecordV2 =
                        serde_cbor::from_slice(bytes).map_err(|error| error.to_string())?;
                    stop.validate()?;
                    if serde_cbor::to_vec(&stop).map_err(|error| error.to_string())? != *bytes {
                        return Err("Closed predecessor is not canonical original record".into());
                    }
                    Ok(Box::new(stop))
                })
                .transpose()?;
            TaskMutationQualifiedResultV2::RevokeAndClose {
                closed: Box::new(closed),
                stop_predecessor,
            }
        }
    };
    Ok((commit.clone(), result))
}
