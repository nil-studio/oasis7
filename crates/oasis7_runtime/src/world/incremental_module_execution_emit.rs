//! Concrete ModuleEmitted owner application: original visual parser, actual
//! anchor points, actual visual delta and canonical WorldEvent in one txn.
use super::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_module_execution_event::{
        ModuleOutputEventContextV1, append_module_output_event_v1,
    },
    incremental_module_execution_visual_anchor::{
        RootModuleVisualAnchorErrorV1, check_root_module_visual_anchor_v1,
    },
    incremental_module_execution_visual_decode::{
        RootModuleVisualDecodeErrorV1, RootModuleVisualDecodeWorkV1,
        decode_root_module_visual_emit_v1,
    },
    incremental_module_invocation::PreparedRootModuleInvocationV1,
};
use oasis7_agent_api::module_visual::ModuleVisualMutation;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, RootSetV1},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_wasm_abi::ModuleEmitEvent;
use oasis7_world_state::{
    incremental_agent_authority_records::AgentAuthorityFamilyQualificationV1,
    incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_module_marketplace::{
        ModuleMarketplaceFamilyQualificationV1, ModuleVisualOverlayErrorV1,
        ModuleVisualOverlayWorkV1, change_module_visual_overlay_v1,
    },
};
#[derive(Debug)]
pub enum RootModuleEmitErrorV1 {
    Value(ValueTreeError),
    Anchor(RootModuleVisualAnchorErrorV1),
    Decode(RootModuleVisualDecodeErrorV1),
    Visual(ModuleVisualOverlayErrorV1),
    Body(BodyMutationError),
}
impl From<ValueTreeError> for RootModuleEmitErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<ExecutionError> for RootModuleEmitErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Body(e.into())
    }
}
impl From<BodyMutationError> for RootModuleEmitErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Real source, owner families, independent write/original contexts and finite work stay explicit."
)]
pub fn contribute_root_module_emit_v1<M, R, E, C, V, EW, EC, F, W, WC>(
    selected: &PreparedRootModuleInvocationV1,
    event: &ModuleEmitEvent,
    now: u64,
    roots: &RootSetV1,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    authority: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
    marketplace: &ReservedValue<ModuleMarketplaceFamilyQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, WC>,
    events: &mut ModuleOutputEventContextV1<'_, V, EW, EC, F>,
    decode: &mut RootModuleVisualDecodeWorkV1,
    body_scan: &mut StateScanBudget,
    cells: &mut u64,
    max_owner_bytes: usize,
    visual: &mut ModuleVisualOverlayWorkV1,
    live: &ExecutionFence,
) -> Result<(), RootModuleEmitErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    V: ValueNodeReader,
    EW: ValueNodeWriter,
    EC: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    WC: FnMut() -> bool,
{
    let values = &mut ctx.values;
    let keys = &mut *ctx.key_work;
    let error = &mut ctx.take_state_error;
    let cancelled = &mut ctx.cancelled;
    let semantic_bytes = ctx.semantic_bytes;
    let record_bytes = ctx.record_bytes;
    ctx.transaction.contribute(|tx| {
        tx.validate_transaction_identity(selected.guard())?;
        tx.validate_guard(&tx.guard(), live)?;
        if selected.module_id() != event.module_id
            || selected.parent_roots_digest() != &tx.parent_fence().parent_roots
            || !values.memory.same_pool(events.mutation.memory)
            || !values.memory.same_pool(write.memory)
        {
            return Err(ValueTreeError::Invalid("module emit selected owner binding").into());
        }
        events.world_events.validate(tx)?;
        super::incremental_clock::check_clock(now, tx, &mut events.mutation)?;
        let parsed =
            decode_root_module_visual_emit_v1(event, values.memory, decode, &mut *cancelled)
                .map_err(RootModuleEmitErrorV1::Decode)?;
        let mut local = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: ValueReadContext {
                source: values.source,
                memory: values.memory,
                class: values.class,
                io: &mut *values.io,
            },
            key_work: keys,
            semantic_bytes,
            record_bytes,
            take_state_error: &mut *error,
            cancelled: &mut *cancelled,
        };
        if let Some(mutation) = &*parsed {
            if let ModuleVisualMutation::Upsert(entity) = mutation {
                check_root_module_visual_anchor_v1(
                    &entity.anchor,
                    now,
                    agents,
                    authority,
                    state,
                    &mut local,
                    body_scan,
                    cells,
                    max_owner_bytes,
                )
                .map_err(RootModuleEmitErrorV1::Anchor)?;
            }
            let guard = local.transaction.guard();
            change_module_visual_overlay_v1(
                &event.module_id,
                mutation,
                roots,
                marketplace,
                state,
                &mut local,
                write,
                visual,
                &guard,
                live,
            )
            .map_err(RootModuleEmitErrorV1::Visual)?;
        }
        // parsed's actual admission remains alive through the event payload
        // clone and canonical producer, including parser/owner temporary peaks.
        append_module_output_event_v1(
            now,
            crate::WorldEventBody::ModuleEmitted(event.clone()),
            local.transaction,
            events,
        )?;
        if let Some(error) = (local.take_state_error)() {
            return Err(error.into());
        }
        if (local.cancelled)() || (events.mutation.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        local
            .transaction
            .validate_guard(&local.transaction.guard(), live)?;
        Ok(())
    })
}
