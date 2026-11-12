//! Actual pending-record, material, power, slot and activity effects. These
//! private routines run only inside the same-Core state-write lease.
use super::*;
use crate::world::incremental_action_factory_lifecycle::material_plan;
use crate::world::incremental_body::write_body_value;
use crate::world::incremental_body_event::read_fixed;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_world_state::{
    FactoryBuildJobState, MaterialLedgerId, RecipeJobState,
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_fields::{AgentFieldV1, AgentScalarRefV1},
    incremental_agent_subtree::*,
    incremental_factory_records::*,
    incremental_industry_jobs::*,
    incremental_value_mutation::ValueMutationContextV1,
    incremental_world_id_sets::{
        WorldIdSetFieldV1, WorldIdSetKeyRefV1, world_id_set_contains_overlay_v1,
    },
};
use std::fmt::Write as _;

fn spend(work: &mut FactoryScheduleWorkV1, n: usize) -> Result<(), BodyMutationError> {
    work.lifecycle.steps_remaining = work
        .lifecycle
        .steps_remaining
        .checked_sub(1)
        .ok_or_else(limited)?;
    work.lifecycle.bytes_remaining = work
        .lifecycle
        .bytes_remaining
        .checked_sub(u64::try_from(n).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    Ok(())
}
#[derive(Debug)]
struct StateError {
    cause: crate::WorldError,
    _fee: MemoryReservation,
}
impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.cause)
    }
}
impl std::error::Error for StateError {}
struct Reason {
    value: String,
    max: usize,
}
impl std::fmt::Write for Reason {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self
            .value
            .len()
            .checked_add(s.len())
            .is_none_or(|n| n > self.max)
        {
            return Err(std::fmt::Error);
        }
        self.value.push_str(s);
        Ok(())
    }
}
fn state_error(
    memory: &MemoryBudget,
    max: usize,
    args: std::fmt::Arguments<'_>,
) -> Result<BodyMutationError, BodyMutationError> {
    let fee = memory
        .reserve(
            max.checked_add(size_of::<StateError>())
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let value = String::with_capacity(max);
    if value.capacity() != max {
        return Err(limited());
    }
    let mut reason = Reason { value, max };
    reason.write_fmt(args).map_err(|_| invalid())?;
    Ok(BodyMutationError::Original(Box::new(StateError {
        cause: crate::WorldError::ResourceBalanceInvalid {
            reason: reason.value,
        },
        _fee: fee,
    })))
}

fn power<M, P, V, W, D, E, C>(
    payer: &str,
    amount: i64,
    recipe: bool,
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
    spend(work, payer.len())?;
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
    let query = || AgentSubtreeQualifiedRequestV1 {
        cell: AgentSubtreeScalarRequestV1 {
            agent: payer,
            field: AgentFieldV1::ResourceAmount,
            ordinal: 0,
        },
        qualification: agents,
    };
    let guard = ctx.transaction.guard();
    let before = match read_agent_subtree_scalar_overlay_v1(
        query(),
        &mut ctx,
        64,
        &mut std::io::sink(),
        &guard,
        live,
    )
    .map_err(overlay)?
    {
        None => None,
        Some(AgentSubtreeValueV1::I64(v)) => Some(v),
        _ => return Err(invalid()),
    };
    let after = match oasis7_agent_api::world_resource::checked_resource_remove_v1(
        ResourceKind::Electricity,
        before.unwrap_or(0),
        amount,
    ) {
        Ok(v) => v,
        Err(error) => {
            const STOCK: &str = "NegativeAmount { amount: }Overflow { kind: Electricity, current: , delta: }Insufficient { kind: Electricity, requested: , available: }";
            let prefix = if recipe {
                "recipe power consume failed for payer : "
            } else {
                "factory build consume failed: "
            };
            let cap = prefix
                .len()
                .checked_add(payer.len())
                .and_then(|n| n.checked_add(STOCK.len() + 3 * 20))
                .ok_or_else(limited)?;
            return Err(if recipe {
                state_error(
                    step.mutation.memory,
                    cap,
                    format_args!("recipe power consume failed for payer {payer}: {error:?}"),
                )?
            } else {
                state_error(
                    step.mutation.memory,
                    cap,
                    format_args!("factory build consume failed: {error:?}"),
                )?
            });
        }
    };
    let mut write = ValueMutationContextV1 {
        sink: &mut *step.mutation.sink,
        memory: step.mutation.memory,
        io: &mut *step.mutation.io,
        cancelled: &mut step.mutation.cancelled,
    };
    let guard = ctx.transaction.guard();
    change_agent_subtree_scalar_v1(
        AgentSubtreeCellChangeV1 {
            query: query(),
            expected: before.map(AgentScalarRefV1::I64),
            after: (after != 0).then_some(AgentScalarRefV1::I64(after)),
            max_scalar_bytes: 64,
        },
        &mut ctx,
        &mut write,
        &guard,
        live,
    )
    .map_err(overlay)?;
    Ok(())
}
fn touch<M, P, V, W, D, E, C>(
    agent: &str,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    _read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryScheduleWorkV1,
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
    spend(work, agent.len())?;
    let key = AgentBodyKeyV1::new(agent).map_err(BodyMutationError::Invalid)?;
    let current =
        AgentBodyValueV1::from_canonical_bytes(&read_fixed::<AGENT_BODY_VALUE_BYTES, _, _, _, _>(
            step.transaction,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
            key.as_bytes(),
            step.values,
            step.mutation,
        )?)
        .map_err(BodyMutationError::Invalid)?;
    let after = write_body_value(
        &AgentBodyValueV1 {
            body_view: current.body_view,
            last_active: step.time(),
        },
        step.mutation,
    )?;
    step.transaction.write(
        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
        key.as_bytes(),
        Some(after),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_build_v1<M, P, V, W, D, E, C, CW, ME>(
    job: &FactoryBuildJobState,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    spend(work, 0)?;
    {
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
        let pending = read_factory_cell_overlay_v1(
            FactoryRecordKeyV1::Build(job.job_id),
            FactoryCellV1::Record,
            &mut ctx,
            0,
            &mut std::io::sink(),
        )
        .map_err(overlay)?;
        if !matches!(pending, FactoryCellReadV1::RecordAbsent)
            || world_id_set_contains_overlay_v1(
                WorldIdSetFieldV1::SettledFactoryBuilds,
                WorldIdSetKeyRefV1::Action(job.job_id),
                &mut ctx,
            )
            .map_err(overlay)?
        {
            return Err(state_error(
                step.mutation.memory,
                "factory build job already settled or pending: job_id=".len() + 20,
                format_args!(
                    "factory build job already settled or pending: job_id={}",
                    job.job_id
                ),
            )?);
        }
    }
    if job.builder_agent_id.trim().is_empty() {
        return Err(state_error(
            step.mutation.memory,
            "factory build builder agent is missing or empty: builder_agent_id=".len()
                + job.builder_agent_id.len(),
            format_args!(
                "factory build builder agent is missing or empty: builder_agent_id={}",
                job.builder_agent_id
            ),
        )?);
    }
    if job.site_id.trim().is_empty() {
        return Err(state_error(
            step.mutation.memory,
            "factory build site_id cannot be empty".len(),
            format_args!("factory build site_id cannot be empty"),
        )?);
    }
    if job.ready_at <= step.time() {
        return Err(state_error(
            step.mutation.memory,
            "factory build ready_at must be in the future: now= ready_at=".len() + 2 * 20,
            format_args!(
                "factory build ready_at must be in the future: now={} ready_at={}",
                step.time(),
                job.ready_at
            ),
        )?);
    }
    // The sole modern admission kernel has just checked the actual site,
    // active anchor, profile/spec, payer and canonical construction source.
    // No write occurs between it and these private replays.
    let obligation = job
        .construction_power_obligation
        .as_ref()
        .ok_or_else(invalid)?;
    if job.contract_version != oasis7_world_state::FACTORY_BUILD_STARTED_MODERN_VERSION
        || job.site_authority_revision.is_none()
        || job.site_location_id.is_none()
        || job.location_anchor_revision.is_none()
    {
        return Err(invalid());
    }
    let plan = material_plan::prepare_with_prefixes_v1(
        step,
        materials,
        &job.consume_ledger,
        &job.spec.build_cost,
        true,
        read,
        &mut work.lifecycle,
        live,
        "factory build consume failed: ",
        "factory build consume failed: ",
        "factory build consume failed: ",
    )?;
    material_plan::apply(
        &plan,
        &job.consume_ledger,
        step,
        materials,
        read,
        &mut work.lifecycle,
        live,
    )?;
    power(
        &job.builder_agent_id,
        obligation.electricity_amount,
        false,
        sources.lifecycle.agents,
        step,
        read,
        work,
        live,
    )?;
    {
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
        let guard = ctx.transaction.guard();
        create_factory_record_overlay_v1(
            FactoryRecordCreateV1 {
                key: FactoryRecordKeyV1::Build(job.job_id),
                after: FactoryRecordRefV1::Build(job),
                qualification: sources.lifecycle.factories,
            },
            &mut ctx,
            write,
            &mut work.lifecycle.factory,
            &guard,
            live,
        )
        .map_err(overlay)?;
    }
    touch(&job.builder_agent_id, step, read, work)?;
    Ok(())
}

#[path = "incremental_normal_action_factory_schedule_activity.rs"]
mod activity;
#[path = "incremental_normal_action_factory_schedule_recipe_commit.rs"]
mod recipe;
pub(super) use recipe::apply_recipe_v1;
