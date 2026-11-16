//! Read-only original event allocator preview under the existing Common.
//! This returns metadata only: it neither allocates an event nor mints a
//! post-write provenance capability, execution receipt or completed tick.
use crate::world::incremental_body::{BodyMutationContext, BodyMutationError};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1;
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("normal event allocator preview")
}
fn preview_allocator_v1<M, R, E, C>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    allocator: crate::world::incremental_allocators::RuntimeAllocatorV1,
) -> Result<u64, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    if !ctx.transaction.uses_budget(common.memory())
        || !ctx.values.memory.same_pool(common.memory())
        || ctx.values.class != MemoryClass::Ordinary
    {
        return Err(invalid());
    }
    common
        .consume_steps(1)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let (memory, meter, stop) = common.parts();
    let child = meter
        .begin_child()
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let original = (
        ctx.values.io.nodes_remaining,
        ctx.values.io.read_bytes_remaining,
        ctx.values.io.write_bytes_remaining,
    );
    ctx.values.io.nodes_remaining = ctx.values.io.nodes_remaining.min(child.remaining_steps());
    ctx.values.io.read_bytes_remaining = ctx
        .values
        .io
        .read_bytes_remaining
        .min(child.remaining_read_bytes() / 2);
    ctx.values.io.write_bytes_remaining = ctx
        .values
        .io
        .write_bytes_remaining
        .min(child.remaining_read_bytes() / 2);
    let before = (
        ctx.values.io.nodes_remaining,
        ctx.values.io.read_bytes_remaining,
        ctx.values.io.write_bytes_remaining,
    );
    let result = {
        let mut cancel = || stop() || (ctx.cancelled)();
        let mut discard = crate::world::incremental_body::Discard;
        let mut context = BodyMutationContext {
            sink: &mut discard,
            memory,
            io: &mut *ctx.values.io,
            cancelled: &mut cancel,
        };
        crate::World::read_iws_allocator(
            allocator,
            ctx.transaction,
            ctx.values.source,
            &mut context,
        )
    };
    let after = (
        ctx.values.io.nodes_remaining,
        ctx.values.io.read_bytes_remaining,
        ctx.values.io.write_bytes_remaining,
    );
    ctx.values.io.nodes_remaining = original.0.saturating_sub(before.0.saturating_sub(after.0));
    ctx.values.io.read_bytes_remaining =
        original.1.saturating_sub(before.1.saturating_sub(after.1));
    ctx.values.io.write_bytes_remaining =
        original.2.saturating_sub(before.2.saturating_sub(after.2));
    let accounting = (|| {
        let steps = before.0.checked_sub(after.0).ok_or_else(invalid)?;
        let read = before.1.checked_sub(after.1).ok_or_else(invalid)?;
        let write = before.2.checked_sub(after.2).ok_or_else(invalid)?;
        child
            .consume(steps, read.checked_add(write).ok_or_else(invalid)?)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))
    })();
    if let Err(error) = accounting {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
        return match result {
            Err(first) => Err(first),
            Ok(_) => Err(error),
        };
    }
    child.finish();
    let sequence = result?;
    use crate::world::incremental_allocators::RuntimeAllocatorV1;
    match allocator {
        RuntimeAllocatorV1::Event => {
            Ok(crate::World::preview_next_event_id(sequence.next, sequence.era).0)
        }
        RuntimeAllocatorV1::Proposal => {
            Ok(crate::World::preview_next_proposal_id(sequence.next, sequence.era).0)
        }
        _ => Err(invalid()),
    }
}

macro_rules! allocator_preview {
    ($name:ident,$kind:ident) => {
        pub(in crate::world) fn $name<M, R, E, C>(
            ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
            common: &mut FormalBoundedContextV1<'_>,
        ) -> Result<u64, BodyMutationError>
        where
            M: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
        {
            preview_allocator_v1(
                ctx,
                common,
                crate::world::incremental_allocators::RuntimeAllocatorV1::$kind,
            )
        }
    };
}
allocator_preview!(preview_current_normal_event_id_v1, Event);
allocator_preview!(preview_current_normal_proposal_id_v1, Proposal);
