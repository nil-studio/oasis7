//! NEW unhooked typed resource debit for the private DeclareWar committer.
//! Storage CAS only; this helper cannot mint an event or execution authority.
use super::*;
use crate::world::incremental_step::IwsStepContext;
use oasis7_formal_protocol::incremental_value::{ValueNodeWriter, ValueReadContext};
use oasis7_world_state::{
    incremental_agent_fields::{AgentFieldV1, AgentScalarRefV1},
    incremental_value_mutation::ValueMutationContextV1,
};
use oasis7_world_state::{
    incremental_agent_subtree::{
        AgentSubtreeCellChangeV1, AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1,
        AgentSubtreeValueV1, change_agent_subtree_scalar_v1, read_agent_subtree_scalar_overlay_v1,
    },
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
#[allow(clippy::too_many_arguments)]
pub(super) fn debit_resource_v1<M, P, V, R, W, D, E, C>(
    agent: &str,
    kind: oasis7_agent_api::world_action::ResourceKind,
    amount: i64,
    agents: &ReservedValue<
        oasis7_world_state::incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    >,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    max: u64,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    charge(common, agent.len())?;
    if amount < 0 {
        return Err(bad());
    }
    if amount == 0 {
        return Ok(());
    }
    let ordinal = match kind {
        oasis7_agent_api::world_action::ResourceKind::Electricity => 0,
        oasis7_agent_api::world_action::ResourceKind::Data => 1,
    };
    let mut overlay = WorldFieldOverlayContextV1 {
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
    let guard = overlay.transaction.guard();
    let before = match read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent,
                field: AgentFieldV1::ResourceAmount,
                ordinal,
            },
            qualification: agents,
        },
        &mut overlay,
        max,
        &mut std::io::sink(),
        &guard,
        live,
    )
    .map_err(super::overlay)?
    {
        None => None,
        Some(AgentSubtreeValueV1::I64(n)) => Some(n),
        _ => return Err(bad()),
    };
    let before_value = before.unwrap_or(0);
    let after = before_value
        .checked_sub(amount)
        .filter(|n| *n >= 0)
        .ok_or_else(bad)?;
    charge(common, 2 * size_of::<i64>())?;
    let mut output = ValueMutationContextV1 {
        sink: &mut *step.mutation.sink,
        memory: step.mutation.memory,
        io: &mut *step.mutation.io,
        cancelled: &mut step.mutation.cancelled,
    };
    let guard = overlay.transaction.guard();
    let changed = change_agent_subtree_scalar_v1(
        AgentSubtreeCellChangeV1 {
            query: AgentSubtreeQualifiedRequestV1 {
                cell: AgentSubtreeScalarRequestV1 {
                    agent,
                    field: AgentFieldV1::ResourceAmount,
                    ordinal,
                },
                qualification: agents,
            },
            expected: before.map(AgentScalarRefV1::I64),
            after: (after != 0).then_some(AgentScalarRefV1::I64(after)),
            max_scalar_bytes: max,
        },
        &mut overlay,
        &mut output,
        &guard,
        live,
    )
    .map_err(super::overlay)?;
    if !changed {
        return Err(bad());
    }
    Ok(())
}
