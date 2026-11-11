//! Global rule costs share the original arithmetic and genuine Inventory codec.
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_inventory_change::InventoryChangeRefV1,
    incremental_inventory_overlay_change::InventoryOverlayChangeError,
    incremental_inventory_overlay_read::InventoryOverlayReadContext,
};
#[derive(Debug)]
pub(super) enum RuleResourceOutcome {
    Applied,
    Insufficient(Vec<(ResourceKind, i64)>),
    Overflow(crate::WorldError),
}
#[derive(Debug)]
pub(super) enum RuleResourceError {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Read(WorldFieldOverlayErrorV1),
    Change(InventoryOverlayChangeError),
}
impl From<ExecutionError> for RuleResourceError {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<ValueTreeError> for RuleResourceError {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
pub(super) fn settle_current_resources<M, R, E, C, W, CW>(
    delta: &crate::ResourceDelta,
    read: &mut InventoryOverlayReadContext<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    live: &ExecutionFence,
) -> Result<ReservedValue<RuleResourceOutcome>, RuleResourceError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
{
    let result = (|| {
        read.transaction
            .validate_guard(&read.transaction.guard(), live)?;
        if read.values.class != MemoryClass::Ordinary
            || !read.transaction.uses_budget(read.values.memory)
            || !read.values.memory.same_pool(write.memory)
        {
            return Err(ValueTreeError::Invalid("rule resources pool/class").into());
        }
        let n = delta.entries.len();
        // Three exact-capacity vectors coexist. The fixed output size plus 256 bytes cover the
        // output enum plus the original overflow text: two i64 decimal values,
        // the fixed prefix and either of the two ResourceKind Debug labels.
        let per = std::mem::size_of::<(ResourceKind, i64, Option<i64>)>()
            + std::mem::size_of::<(ResourceKind, Option<i64>, i64)>()
            + std::mem::size_of::<(ResourceKind, i64)>();
        let fee = n
            .checked_mul(per)
            .and_then(|v| v.checked_add(std::mem::size_of::<RuleResourceOutcome>() + 256))
            .ok_or(ValueTreeError::ResourceLimited("rule resource workspace"))?;
        let phase = read
            .values
            .memory
            .reserve(fee, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let mut points = Vec::new();
        let mut planned = Vec::new();
        let mut deficits = Vec::new();
        points
            .try_reserve_exact(n)
            .map_err(|_| ValueTreeError::ResourceLimited("rule point allocation"))?;
        planned
            .try_reserve_exact(n)
            .map_err(|_| ValueTreeError::ResourceLimited("rule plan allocation"))?;
        deficits
            .try_reserve_exact(n)
            .map_err(|_| ValueTreeError::ResourceLimited("rule deficit allocation"))?;
        if points.capacity() > n || planned.capacity() > n || deficits.capacity() > n {
            return Err(ValueTreeError::ResourceLimited("rule resource capacity").into());
        }
        for (&kind, &amount) in &delta.entries {
            if (read.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let before = read.resource(kind).map_err(RuleResourceError::Read)?;
            points.push((kind, amount, before));
        }
        delta.visit_deficits(
            |kind| {
                points
                    .iter()
                    .find(|p| p.0 == kind)
                    .map_or(0, |p| p.2.unwrap_or(0))
            },
            |kind, shortfall| deficits.push((kind, shortfall)),
        );
        if (read.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (read.take_state_error)() {
            return Err(error.into());
        }
        if !deficits.is_empty() {
            return Ok(phase.retain(RuleResourceOutcome::Insufficient(deficits)));
        }
        for (kind, amount, before) in points {
            let after = match super::resource_balance_kernel::checked_next(
                kind,
                before.unwrap_or(0),
                amount,
            ) {
                Ok(v) => v,
                Err(error) => {
                    if (read.cancelled)() || (write.cancelled)() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    if let Some(error) = (read.take_state_error)() {
                        return Err(error.into());
                    }
                    return Ok(phase.retain(RuleResourceOutcome::Overflow(error)));
                }
            };
            planned.push((kind, before, after));
        }
        for (key, expected, after) in planned {
            let guard = read.transaction.guard();
            read.change(
                InventoryChangeRefV1::Resource {
                    key,
                    expected,
                    after: Some(after),
                },
                write,
                &guard,
                live,
            )
            .map_err(RuleResourceError::Change)?;
        }
        if (read.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (read.take_state_error)() {
            return Err(error.into());
        }
        Ok(phase.retain(RuleResourceOutcome::Applied))
    })();
    match result {
        // Existing point adapters already poison while preserving their exact error.
        Err(error @ (RuleResourceError::Read(_) | RuleResourceError::Change(_))) => Err(error),
        Err(error) => read.transaction.contribute(|_| Err(error)),
        Ok(value) => Ok(value),
    }
}
