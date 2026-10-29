use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError,
    incremental_value::{ValueNodeWriter, ValueStreamWriter, ValueTreeError},
};
use oasis7_world_state::incremental_module_instance_fields::{
    ModuleInstanceFieldV1, ModuleInstanceKeyV1, ModuleInstanceScalarRefV1,
    visit_module_instance_fields,
};
pub fn write_module_instance_scalar<W: ValueNodeWriter, C: FnMut() -> bool>(
    field: ModuleInstanceFieldV1,
    value: ModuleInstanceScalarRefV1<'_>,
    context: &mut BodyMutationContext<'_, W, C>,
    max_bytes: u64,
) -> Result<[u8; 32], BodyMutationError> {
    let length = value
        .encoded_len(field)
        .map_err(BodyMutationError::Invalid)?;
    if length > max_bytes {
        return Err(ValueTreeError::ResourceLimited("module instance scalar bytes").into());
    }
    let mut writer = ValueStreamWriter::new(
        context.sink,
        context.memory,
        context.io,
        length,
        &mut context.cancelled,
    )?;
    if value.write_to(field, &mut writer).is_err() {
        return Err(writer
            .finish()
            .err()
            .unwrap_or(ValueTreeError::Invalid("module instance scalar write"))
            .into());
    }
    Ok(writer.finish()?.digest()?)
}
impl World {
    /// Borrowed per-field preparation; receiver output remains provisional.
    pub fn write_iws_module_instance_genesis<W: ValueNodeWriter, C: FnMut() -> bool>(
        &self,
        context: &mut BodyMutationContext<'_, W, C>,
        remaining_instances: &mut u64,
        max_scalar_bytes: u64,
        mut emit: impl FnMut(&[u8], [u8; 32]) -> Result<(), BodyMutationError>,
    ) -> Result<u64, BodyMutationError> {
        let mut count = 0u64;
        for (map_key, instance) in &self.state.module_instances {
            if (context.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            *remaining_instances =
                remaining_instances
                    .checked_sub(1)
                    .ok_or(ExecutionError::ResourceLimited(
                        "module instance genesis count",
                    ))?;
            visit_module_instance_fields(instance, |field, value| {
                let key =
                    ModuleInstanceKeyV1::new(map_key, field).map_err(BodyMutationError::Invalid)?;
                let id = write_module_instance_scalar(field, value, context, max_scalar_bytes)?;
                emit(key.as_bytes(), id)
            })?;
            count = count
                .checked_add(1)
                .ok_or(BodyMutationError::Invalid("module instance genesis count"))?;
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(count)
    }
}
