//! Charged provisional field writes; typed producers supply registered keys.
use crate::{
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1, incremental_state::StateWriteV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub(crate) fn emit<
    W,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    key: WorldFieldKeyV1,
    root: [u8; 32],
    context: &mut MemberRecordWriteContext<'_, W, C>,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let charge = context
        .memory
        .reserve(1024 + key.as_bytes().len(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    sink(
        charge.retain(ExecutionWriteV1 {
            namespace: WorldFieldKeyV1::from_bytes(key.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .namespace,
            mutation: StateWriteV1 {
                key: key.as_bytes().to_vec(),
                before: None,
                after: Some(root),
            },
        }),
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
