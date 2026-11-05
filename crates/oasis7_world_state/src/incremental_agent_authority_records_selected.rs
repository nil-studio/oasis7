//! Exact current agent-authority body selection after earlier writes in the
//! sole transaction. Parent-only body qualifications are never substituted.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

/// Authenticate the actual outer edge and qualify precisely its current body.
/// The bounded body scan is over this selected record, never the family or H.
#[expect(
    clippy::too_many_arguments,
    reason = "The genuine parent family, current transaction and separate finite body limits remain explicit."
)]
pub fn open_agent_authority_body_overlay_v1<M, R, E, C>(
    key: AgentAuthorityRecordKeyV1<'_>,
    family: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
) -> Result<Option<ReservedValue<AgentAuthorityRecordQualificationV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let values = &mut ctx.values;
    let keys = &mut *ctx.key_work;
    let cancelled = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    let semantic_bytes = ctx.semantic_bytes;
    let record_bytes = ctx.record_bytes;
    ctx.transaction.contribute(|tx| {
        if (cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if values.class != MemoryClass::Ordinary
            || !tx.uses_budget(values.memory)
            || !family.uses_budget(values.memory)
            || family.allocation_class() != MemoryClass::Ordinary
        {
            return Err(
                ValueTreeError::Invalid("current agent authority family pool/class").into(),
            );
        }
        family.validate_binding(&tx.parent_fence().parent_roots)?;
        let _phase = values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(oasis7_formal_protocol::incremental_execution::ExecutionError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut *values,
            key_work: &mut *keys,
            semantic_bytes,
            record_bytes,
            cancelled: &mut *cancelled,
        };
        point::metadata(key, tx, &mut records, error)?;
        let outer = read::outer_key(key, &mut records)?;
        let value = lookup(tx, &outer.0, &mut records, error)?;
        let Some(value) = value else {
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(None);
        };
        // The existing owner/schema decoder validates this actual current edge.
        let child = read::outer(key.kind, &outer, &value, &mut records)?;
        let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or(ValueTreeError::Invalid("agent authority current owner"))?;
        drop(records);
        let mut body = WorldFieldReadContextV1 {
            root: &child,
            state,
            values: ValueReadContext {
                source: values.source,
                memory: values.memory,
                class: values.class,
                io: &mut *values.io,
            },
            key_work: &mut *keys,
            semantic_bytes,
            record_bytes,
            take_state_error: &mut *error,
            cancelled: &mut *cancelled,
        };
        // Qualify all actual cells with the existing closed schema. An arbitrary
        // outer ref cannot mint a typed body from metadata alone.
        let qualified =
            qualify_agent_authority_body_v1(key.kind, owner, &mut body, scan, cells, max_payload)?;
        if let Some(error) = (body.take_state_error)() {
            return Err(error.into());
        }
        if (body.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(qualified))
    })
}
