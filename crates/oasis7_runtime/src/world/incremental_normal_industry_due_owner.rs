//! Same-Core bridge to the existing complete Current agent-owner resolver.
//! No independent meter, balance mirror, World snapshot or authority callback.
use super::*;
use crate::world::incremental_industrial_eligibility::{
    CurrentStateOwnerSourcesV1, IndustrialEligibilityErrorV1, IndustrialEligibilityWorkV1,
    OwnedCurrentIndustrialStateOwnerV1, resolve_current_state_owner_v1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
use std::cell::RefCell;
pub(super) enum DueOwnerReadErrorV1 {
    Bounded(BodyMutationError),
    Source {
        error: IndustrialEligibilityErrorV1,
        _fee: MemoryReservation,
    },
}
impl From<BodyMutationError> for DueOwnerReadErrorV1 {
    fn from(error: BodyMutationError) -> Self {
        Self::Bounded(error)
    }
}
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("due Current owner work bridge").into()
}

fn lanes<M, P, R, V, E, C, PE, PC>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialEligibilityWorkV1,
    ceiling: Option<(u64, u64)>,
) -> Result<([u64; 9], u64, [u64; 8]), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let steps = [
        &mut ctx.values.io.nodes_remaining,
        &mut parent.values.io.nodes_remaining,
        &mut work.read.cells_remaining,
        &mut work.scan.nodes_remaining,
        &mut work.scan.leaves_remaining,
        &mut work.typed.json.cells_remaining,
        &mut work.typed.json.json_nodes_remaining,
        &mut work.revocation.json.cells_remaining,
        &mut work.revocation.json.json_nodes_remaining,
    ];
    if let Some((steps_cap, _)) = ceiling {
        for value in steps {
            *value = (*value).min(steps_cap);
        }
    }
    if let Some((steps_cap, _)) = ceiling {
        work.pending_entries_remaining = work
            .pending_entries_remaining
            .min(usize::try_from(steps_cap).unwrap_or(usize::MAX));
    }
    let bytes = [
        &mut ctx.values.io.read_bytes_remaining,
        &mut ctx.values.io.write_bytes_remaining,
        &mut parent.values.io.read_bytes_remaining,
        &mut parent.values.io.write_bytes_remaining,
        &mut ctx.key_work.bytes_remaining,
        &mut parent.key_work.bytes_remaining,
        &mut work.read.comparison_bytes_remaining,
        &mut work.scan.read_bytes_remaining,
    ];
    if let Some((_, byte_cap)) = ceiling {
        for value in bytes {
            *value = (*value).min(byte_cap);
        }
    }
    Ok((
        [
            ctx.values.io.nodes_remaining,
            parent.values.io.nodes_remaining,
            work.read.cells_remaining,
            work.scan.nodes_remaining,
            work.scan.leaves_remaining,
            work.typed.json.cells_remaining,
            work.typed.json.json_nodes_remaining,
            work.revocation.json.cells_remaining,
            work.revocation.json.json_nodes_remaining,
        ],
        u64::try_from(work.pending_entries_remaining).map_err(|_| limited())?,
        [
            ctx.values.io.read_bytes_remaining,
            ctx.values.io.write_bytes_remaining,
            parent.values.io.read_bytes_remaining,
            parent.values.io.write_bytes_remaining,
            ctx.key_work.bytes_remaining,
            parent.key_work.bytes_remaining,
            work.read.comparison_bytes_remaining,
            work.scan.read_bytes_remaining,
        ],
    ))
}
fn delta(
    before: ([u64; 9], u64, [u64; 8]),
    after: ([u64; 9], u64, [u64; 8]),
) -> Result<(u64, u64), BodyMutationError> {
    let mut steps = before.1.checked_sub(after.1).ok_or_else(limited)?;
    for (a, b) in before.0.into_iter().zip(after.0) {
        steps = steps
            .checked_add(a.checked_sub(b).ok_or_else(limited)?)
            .ok_or_else(limited)?;
    }
    let mut bytes = 0u64;
    for (a, b) in before.2.into_iter().zip(after.2) {
        bytes = bytes
            .checked_add(a.checked_sub(b).ok_or_else(limited)?)
            .ok_or_else(limited)?;
    }
    Ok((steps, bytes))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_due_owner_v1<M, P, R, V, E, C, PE, PC>(
    agent: &str,
    sources: &CurrentStateOwnerSourcesV1<'_, '_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialEligibilityWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<Option<OwnedCurrentIndustrialStateOwnerV1>>, DueOwnerReadErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    if !ctx.values.memory.same_pool(common.memory())
        || !parent.values.memory.same_pool(common.memory())
        || ctx.values.class != oasis7_proto::memory_budget::MemoryClass::Ordinary
        || parent.values.class != oasis7_proto::memory_budget::MemoryClass::Ordinary
    {
        return Err(BodyMutationError::Invalid("due owner Current scope").into());
    }
    common
        .charge(1, u64::try_from(agent.len()).map_err(|_| limited())?)
        .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    // Each fixed resolver branch observes a bounded collection of actual root
    // bindings. The transient phase is retained across both decode and errors.
    let paths = oasis7_world_state::incremental_world_native::world_native_field_descriptors_v1()
        .map_err(BodyMutationError::from)?
        .len()
        .checked_add(16)
        .ok_or_else(limited)?;
    let root_bytes = sources
        .roots
        .world_id
        .len()
        .checked_add(1024 + 6 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES)
        .and_then(|n| n.checked_mul(paths))
        .ok_or_else(limited)?;
    common
        .charge(
            u64::try_from(paths).map_err(|_| limited())?,
            u64::try_from(root_bytes).map_err(|_| limited())?,
        )
        .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    let phase_bytes = sources
        .roots
        .world_id
        .len()
        .checked_mul(12)
        .and_then(|n| n.checked_add(4096 + std::mem::size_of::<DueOwnerReadErrorV1>()))
        .ok_or_else(limited)?;
    let phase = common
        .memory()
        .reserve(
            phase_bytes,
            oasis7_proto::memory_budget::MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)
        .map_err(BodyMutationError::from)?;
    let (_, meter, stop) = common.parts();
    let child = meter
        .begin_child()
        .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    let before = lanes(
        ctx,
        parent,
        work,
        Some((
            child.remaining_steps() / 10,
            child.remaining_read_bytes() / 8,
        )),
    )?;
    let result = {
        let cancellation = RefCell::new(stop);
        let mut local_stop = || (*cancellation.borrow_mut())() || (ctx.cancelled)();
        let mut parent_stop = || (*cancellation.borrow_mut())() || (parent.cancelled)();
        let mut scoped = WorldFieldOverlayContextV1 {
            transaction: &mut *ctx.transaction,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut local_stop,
        };
        let mut scoped_parent = WorldFieldReadContextV1 {
            root: parent.root,
            state: parent.state,
            values: ValueReadContext {
                source: parent.values.source,
                memory: parent.values.memory,
                class: parent.values.class,
                io: &mut *parent.values.io,
            },
            key_work: &mut *parent.key_work,
            semantic_bytes: parent.semantic_bytes,
            record_bytes: parent.record_bytes,
            take_state_error: &mut parent.take_state_error,
            cancelled: &mut parent_stop,
        };
        let guard = scoped.transaction.guard();
        resolve_current_state_owner_v1(
            sources,
            agent,
            &mut scoped,
            &mut scoped_parent,
            work,
            &guard,
            live,
        )
    };
    let after = lanes(ctx, parent, work, None)?;
    let accounted = delta(before, after).and_then(|(steps, bytes)| {
        child
            .consume(steps, bytes)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))
    });
    // An increased counter cannot be settled, even when the original read also
    // failed: Drop exhausts both balances. Do not finish/refund that lease.
    if let Err(accounting) = accounted {
        return match result {
            Err(error) => Err(DueOwnerReadErrorV1::Source { error, _fee: phase }),
            Ok(_) => Err(accounting.into()),
        };
    }
    child.finish();
    match result {
        Err(error) => Err(DueOwnerReadErrorV1::Source { error, _fee: phase }),
        Ok(value) => {
            common
                .consume_steps(0)
                .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
            Ok(value)
        }
    }
}

/// Original industrial-owner tuple, hashed by the existing unique JSON sink.
/// None preserves the original missing/empty authority business predicate.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_due_owner_binding_v1<M, P, R, V, E, C, PE, PC>(
    agent: &str,
    sources: &CurrentStateOwnerSourcesV1<'_, '_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialEligibilityWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<String>>, DueOwnerReadErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let owner = read_current_due_owner_v1(agent, sources, ctx, parent, work, common, live)?;
    let Some(owner) = owner.as_ref() else {
        return Ok(None);
    };
    let bytes = owner
        .player_id()
        .len()
        .checked_add(owner.source_id().len())
        .and_then(|n| n.checked_add(owner.public_key().map_or(0, str::len)))
        .ok_or_else(limited)?;
    common
        .charge(1, u64::try_from(bytes).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    if owner.player_id().is_empty()
        || owner.source_id().is_empty()
        || owner.public_key().is_none_or(str::is_empty)
    {
        return Ok(None);
    }
    let digest = crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(
        &(
            owner.primary_agent_id(),
            owner.player_id(),
            owner.public_key(),
            owner.source_id(),
            owner.generation(),
        ),
        common,
    )?;
    Ok(Some(digest))
}
