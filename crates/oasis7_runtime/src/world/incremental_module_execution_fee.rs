//! Fee owner plus exact charged Event in one atomic root-backed contribution.
use super::{
    incremental_body::BodyMutationError,
    incremental_module_execution_event::{
        ModuleOutputEventContextV1, append_module_output_event_v1,
    },
    incremental_module_fee_accounts::{
        PreparedRootModuleFeeV1, RootModuleFeeErrorV1, with_overlay,
    },
    incremental_module_fee_contribution::{
        RootModuleFeeWriteContextV1, contribute_root_module_fee_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionFence,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    incremental_field_overlay::WorldFieldOverlayContextV1,
};
/// Call immediately after actual prepare_root_module_fee_v1: that opaque
/// preparation's strict revision guard is intentionally not weakened. World
/// clock is checked after contribution, before Event publication, inside the
/// same outer contribute so a mismatch aborts all provisional balance writes.
#[expect(
    clippy::too_many_arguments,
    reason = "Real fee preparation, owner IO, same-transaction event original and live fence are independent obligations."
)]
pub fn contribute_root_module_fee_and_event_v1<M, R, E, C, W, WC, V, EW, EC, F>(
    prepared: &ReservedValue<PreparedRootModuleFeeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    now: u64,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut RootModuleFeeWriteContextV1<'_, W, WC>,
    events: &mut ModuleOutputEventContextV1<'_, V, EW, EC, F>,
    live: &ExecutionFence,
) -> Result<(), RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    WC: FnMut() -> bool,
    V: ValueNodeReader,
    EW: ValueNodeWriter,
    EC: FnMut() -> bool,
    F: FnMut(
        &mut dyn super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
{
    with_overlay(ctx, |ctx| {
        let event = prepared.event();
        let fee = event
            .module_id
            .len()
            .checked_add(event.trace_id.len())
            .and_then(|n| n.checked_add(event.payer_agent_id.len()))
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(8192 + std::mem::size_of::<crate::WorldEvent>()))
            .ok_or(ValueTreeError::ResourceLimited("module fee Event clone"))?;
        let _phase = ctx.values.memory.reserve(fee, MemoryClass::Ordinary)?;
        if !ctx.values.memory.same_pool(events.mutation.memory) {
            return Err(ValueTreeError::Invalid("module fee Event pool").into());
        }
        contribute_root_module_fee_v1(prepared, agents, now, ctx, write, live)?;
        super::incremental_clock::check_clock(now, ctx.transaction, &mut events.mutation)?;
        append_module_output_event_v1(
            now,
            crate::WorldEventBody::ModuleRuntimeCharged(event.clone()),
            ctx.transaction,
            events,
        )?;
        if (ctx.cancelled)() || (write.values.cancelled)() || (events.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        Ok(())
    })
}
