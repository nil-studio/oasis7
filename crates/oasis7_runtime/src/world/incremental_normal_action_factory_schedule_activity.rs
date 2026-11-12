//! Exact selected old Activity group, then original optional-group CAS.
use super::*;
use oasis7_world_state::agent_cell::{AgentActivityStatus, AgentActivityV1};
fn value<M, R, E, C>(
    agent: &str,
    field: AgentFieldV1,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleWorkV1,
    out: &mut impl std::io::Write,
    live: &ExecutionFence,
) -> Result<AgentSubtreeValueV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    spend(work, agent.len())?;
    let guard = ctx.transaction.guard();
    read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent,
                field,
                ordinal: 0,
            },
            qualification: agents,
        },
        ctx,
        work.lifecycle
            .max_scalar_bytes
            .checked_add(64)
            .ok_or_else(limited)? as u64,
        out,
        &guard,
        live,
    )
    .map_err(overlay)?
    .ok_or_else(invalid)
}
fn number<M, R, E, C>(
    agent: &str,
    field: AgentFieldV1,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<Option<u64>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    match value(agent, field, agents, ctx, work, &mut std::io::sink(), live)? {
        AgentSubtreeValueV1::OptionalU64(v) => Ok(v),
        _ => Err(invalid()),
    }
}
fn text<M, R, E, C>(
    agent: &str,
    field: AgentFieldV1,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<Option<String>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let cap = work.lifecycle.max_scalar_bytes;
    let mut bytes = Vec::with_capacity(cap);
    if bytes.capacity() != cap {
        return Err(limited());
    }
    match value(agent, field, agents, ctx, work, &mut bytes, live)? {
        AgentSubtreeValueV1::OptionalText(None) if bytes.is_empty() => Ok(None),
        AgentSubtreeValueV1::OptionalText(Some(n))
            if usize::try_from(n).ok() == Some(bytes.len()) =>
        {
            spend(work, bytes.len())?;
            Ok(Some(String::from_utf8(bytes).map_err(|_| invalid())?))
        }
        _ => Err(invalid()),
    }
}
pub(super) fn set_recipe_v1<M, P, V, W, D, E, C>(
    agent: &str,
    job_id: u64,
    factory: &str,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let now = step.time();
    let max = work.lifecycle.max_scalar_bytes;
    let fee = step
        .mutation
        .memory
        .reserve(
            size_of::<Option<AgentActivityV1>>()
                .checked_add(max.checked_mul(4).ok_or_else(limited)?)
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut *step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    let present = match value(
        agent,
        AgentFieldV1::ActivityPresent,
        agents,
        &mut ctx,
        work,
        &mut std::io::sink(),
        live,
    )? {
        AgentSubtreeValueV1::Bool(v) => v,
        _ => return Err(invalid()),
    };
    let old = if present {
        let status = match value(
            agent,
            AgentFieldV1::ActivityStatus,
            agents,
            &mut ctx,
            work,
            &mut std::io::sink(),
            live,
        )? {
            AgentSubtreeValueV1::U8(0) => AgentActivityStatus::Idle,
            AgentSubtreeValueV1::U8(1) => AgentActivityStatus::Executing,
            AgentSubtreeValueV1::U8(2) => AgentActivityStatus::Blocked,
            AgentSubtreeValueV1::U8(3) => AgentActivityStatus::Waiting,
            AgentSubtreeValueV1::U8(4) => AgentActivityStatus::Unavailable,
            _ => return Err(invalid()),
        };
        let updated_at = match value(
            agent,
            AgentFieldV1::ActivityUpdated,
            agents,
            &mut ctx,
            work,
            &mut std::io::sink(),
            live,
        )? {
            AgentSubtreeValueV1::U64(n) => n,
            _ => return Err(invalid()),
        };
        Some(AgentActivityV1 {
            status,
            updated_at,
            operation_kind: text(
                agent,
                AgentFieldV1::ActivityOperationKind,
                agents,
                &mut ctx,
                work,
                live,
            )?,
            operation_id: number(
                agent,
                AgentFieldV1::ActivityOperationId,
                agents,
                &mut ctx,
                work,
                live,
            )?,
            target_id: text(
                agent,
                AgentFieldV1::ActivityTarget,
                agents,
                &mut ctx,
                work,
                live,
            )?,
            reason_code: text(
                agent,
                AgentFieldV1::ActivityReasonCode,
                agents,
                &mut ctx,
                work,
                live,
            )?,
            reason_summary: text(
                agent,
                AgentFieldV1::ActivityReasonSummary,
                agents,
                &mut ctx,
                work,
                live,
            )?,
        })
    } else {
        None
    };
    let old = fee.retain(old);
    let after_fee = step
        .mutation
        .memory
        .reserve(
            size_of::<AgentActivityV1>()
                .checked_add("recipe".len())
                .and_then(|n| n.checked_add(factory.len()))
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    spend(work, factory.len())?;
    let after = after_fee.retain(AgentActivityV1::executing("recipe", job_id, factory, now));
    let guard = ctx.transaction.guard();
    let mut output = ValueMutationContextV1 {
        sink: &mut *step.mutation.sink,
        memory: step.mutation.memory,
        io: &mut *step.mutation.io,
        cancelled: &mut step.mutation.cancelled,
    };
    change_agent_subtree_optional_v1(
        AgentSubtreeOptionalChangeV1 {
            agent,
            qualification: agents,
            expected: AgentSubtreeOptionalRefV1::Activity(old.as_ref()),
            after: AgentSubtreeOptionalRefV1::Activity(Some(&after)),
            max_scalar_bytes: work
                .lifecycle
                .max_scalar_bytes
                .checked_add(64)
                .ok_or_else(limited)? as u64,
        },
        &mut ctx,
        &mut output,
        &guard,
        live,
    )
    .map_err(overlay)?;
    Ok(())
}
