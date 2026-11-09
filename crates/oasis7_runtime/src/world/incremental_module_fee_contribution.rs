//! Atomic fee lowering using the original stock arithmetic and existing wires.
use super::{
    incremental_body::{BodyMutationContext, write_body_value},
    incremental_module_fee as core,
    incremental_module_fee_accounts::*,
};
use crate::ModuleRuntimeChargeEvent;
use oasis7_agent_api::{world_action::ResourceKind, world_resource::ResourceStock};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionFence,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_fields::{AgentFieldV1, AgentScalarRefV1},
    incremental_agent_subtree::{
        AgentSubtreeCellChangeV1, AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1,
        AgentSubtreeStructureQualificationV1, change_agent_subtree_scalar_v1,
    },
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_inventory_change::InventoryChangeRefV1,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_value_mutation::ValueMutationContextV1,
};
use std::cell::RefCell;

pub struct RootModuleFeeWriteContextV1<'a, W, C> {
    pub values: ValueMutationContextV1<'a, W, C>,
    pub key_work: &'a mut SemanticKeyBudget,
    pub member_limits: MemberRecordLimitsV1,
}
fn invalid() -> RootModuleFeeErrorV1 {
    ValueTreeError::Invalid("prepared module fee binding").into()
}

/// Contributes balances/treasury/activity only. The caller must append the exact
/// `prepared.event()` original/allocator/fact in the same outer transaction.
/// This return neither installs a World nor publishes a durable head.
pub fn contribute_root_module_fee_v1<M, R, E, C, W, CW>(
    prepared: &ReservedValue<PreparedRootModuleFeeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    now: u64,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut RootModuleFeeWriteContextV1<'_, W, CW>,
    live: &ExecutionFence,
) -> Result<(), RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
{
    with_overlay(ctx, |ctx| {
        ctx.transaction.validate_guard(&prepared.guard, live)?;
        if prepared.live != *live
            || ctx.values.class != MemoryClass::Ordinary
            || !prepared.uses_budget(ctx.values.memory)
            || !ctx.transaction.uses_budget(write.values.memory)
            || !agents.uses_budget(ctx.values.memory)
            || agents.allocation_class() != MemoryClass::Ordinary
            || agents.reserved_bytes() < 512
        {
            return Err(invalid());
        }
        if ctx
            .transaction
            .read(StateNamespaceV1::Business, prepared.owner_key.as_bytes())?
            != Some(prepared.owner_value)
        {
            return Err(invalid());
        }
        let channels = RefCell::new((&mut ctx.cancelled, &mut write.values.cancelled));
        let mut local = WorldFieldOverlayContextV1 {
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
            cancelled: || {
                let mut c = channels.borrow_mut();
                c.0() || c.1()
            },
        };
        let mut mutation = ValueMutationContextV1 {
            sink: &mut *write.values.sink,
            memory: write.values.memory,
            io: &mut *write.values.io,
            cancelled: || {
                let mut c = channels.borrow_mut();
                c.0() || c.1()
            },
        };
        finish(&mut local)?;
        let payer = &prepared.event.payer_agent_id;
        let current = [
            resource(payer, ResourceKind::Data, agents, &mut local)?,
            resource(payer, ResourceKind::Electricity, agents, &mut local)?,
        ];
        if current != prepared.balances {
            return Err(invalid());
        }
        apply_charge(
            &prepared.event,
            agents,
            now,
            &mut local,
            &mut mutation,
            write.key_work,
            write.member_limits,
        )?;
        finish(&mut local)?;
        if (mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
pub(super) fn apply_charge<M, R, E, C, W, CW>(
    charge: &ModuleRuntimeChargeEvent,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    now: u64,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut ValueMutationContextV1<'_, W, CW>,
    write_keys: &mut SemanticKeyBudget,
    member_limits: MemberRecordLimitsV1,
) -> Result<(), RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
{
    core::validate_amounts(charge).map_err(RootModuleFeeErrorV1::World)?;
    let _workspace = write.memory.reserve(32 << 10, MemoryClass::Ordinary)?;
    let payer = &charge.payer_agent_id;
    if balance(payer, AgentFieldV1::Identity, 0, agents, ctx)?.is_none() {
        return Err(RootModuleFeeErrorV1::World(
            crate::WorldError::AgentNotFound {
                agent_id: payer.clone(),
            },
        ));
    }
    let before = [
        resource(payer, ResourceKind::Electricity, agents, ctx)?,
        resource(payer, ResourceKind::Data, agents, ctx)?,
    ];
    let mut stock = ResourceStock::new();
    for (kind, amount) in [
        (ResourceKind::Electricity, before[0]),
        (ResourceKind::Data, before[1]),
    ] {
        if let Some(amount) = amount {
            stock.amounts.insert(kind, amount);
        }
    }
    let mut treasury = [None; 2];
    for (kind, amount) in [
        (charge.compute_fee_kind, charge.compute_fee_amount),
        (charge.electricity_fee_kind, charge.electricity_fee_amount),
    ] {
        if amount > 0 {
            treasury[ordinal(kind) as usize] = ctx.resource(kind)?;
        }
    }
    let changes =
        core::prepare_balances(charge, &mut stock, |kind| treasury[ordinal(kind) as usize])
            .map_err(RootModuleFeeErrorV1::World)?;
    let body_key = AgentBodyKeyV1::new(payer).map_err(ValueTreeError::Invalid)?;
    let mut body_context = BodyMutationContext {
        sink: &mut *write.sink,
        memory: write.memory,
        io: &mut *write.io,
        cancelled: &mut write.cancelled,
    };
    super::incremental_clock::check_clock(now, ctx.transaction, &mut body_context)?;
    let body_bytes = super::incremental_body_event::read_fixed::<AGENT_BODY_VALUE_BYTES, _, _, _, _>(
        ctx.transaction,
        StateNamespaceV1::Business,
        body_key.as_bytes(),
        ctx.values.source,
        &mut body_context,
    )?;
    let mut body =
        AgentBodyValueV1::from_canonical_bytes(&body_bytes).map_err(ValueTreeError::Invalid)?;
    for (kind, old) in [
        (ResourceKind::Electricity, before[0]),
        (ResourceKind::Data, before[1]),
    ] {
        let after = stock.amounts.get(&kind).copied();
        if old != after {
            let guard = ctx.transaction.guard();
            let live = ctx.transaction.parent_fence();
            change_agent_subtree_scalar_v1(
                AgentSubtreeCellChangeV1 {
                    query: AgentSubtreeQualifiedRequestV1 {
                        cell: AgentSubtreeScalarRequestV1 {
                            agent: payer,
                            field: AgentFieldV1::ResourceAmount,
                            ordinal: ordinal(kind),
                        },
                        qualification: agents,
                    },
                    expected: old.map(AgentScalarRefV1::I64),
                    after: after.map(AgentScalarRefV1::I64),
                    max_scalar_bytes: 128,
                },
                ctx,
                write,
                &guard,
                &live,
            )?;
        }
    }
    for (kind, after) in changes {
        let guard = ctx.transaction.guard();
        let live = ctx.transaction.parent_fence();
        ctx.change(
            InventoryChangeRefV1::Resource {
                key: kind,
                expected: treasury[ordinal(kind) as usize],
                after: Some(after),
            },
            &mut MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: write_keys,
                limits: member_limits,
                cancelled: &mut write.cancelled,
            },
            &guard,
            &live,
        )
        .map_err(RootModuleFeeErrorV1::Inventory)?;
    }
    if body.last_active != now {
        body.last_active = now;
        let id = write_body_value(
            &body,
            &mut BodyMutationContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                cancelled: &mut write.cancelled,
            },
        )?;
        ctx.transaction
            .write(StateNamespaceV1::Business, body_key.as_bytes(), Some(id))?;
    }
    finish(ctx)?;
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
fn ordinal(kind: ResourceKind) -> u64 {
    match kind {
        ResourceKind::Electricity => 0,
        ResourceKind::Data => 1,
    }
}
