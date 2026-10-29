use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_genesis_state as state,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1},
    incremental_value::{
        ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueStreamWriter, ValueTreeError,
    },
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::incremental_module_instance_fields::{
    ModuleInstanceFieldV1, ModuleInstanceKeyV1, ModuleInstanceScalarRefV1,
    visit_module_instance_fields,
};
use oasis7_world_state::incremental_module_instance_index::*;
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
    /// Build the complete field/count family from a base containing no module
    /// instance fields. Earlier pages are provisional until the final root.
    pub fn write_iws_module_instance_genesis<
        W: StatePageWriter + ValueNodeReader,
        C: FnMut() -> bool,
    >(
        &self,
        mut root: StateRootV1,
        context: &mut BodyMutationContext<'_, W, C>,
        remaining_instances: &mut u64,
        max_scalar_bytes: u64,
    ) -> Result<ReservedValue<StateRootV1>, BodyMutationError> {
        let mut metadata = context
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if root.namespace != StateNamespaceV1::Business {
            return Err(BodyMutationError::Invalid(
                "module instance genesis namespace",
            ));
        }
        if state::lookup(&root, MODULE_INSTANCE_INDEX_KEY_V1, context)?.is_some() {
            return Err(BodyMutationError::Invalid(
                "module instance index already exists",
            ));
        }
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
            let mut writes = Vec::with_capacity(9);
            visit_module_instance_fields(instance, |field, value| {
                let key =
                    ModuleInstanceKeyV1::new(map_key, field).map_err(BodyMutationError::Invalid)?;
                let id = write_module_instance_scalar(field, value, context, max_scalar_bytes)?;
                writes.push(StateWriteV1 {
                    key: key.as_bytes().to_vec(),
                    before: None,
                    after: Some(id),
                });
                Ok::<_, BodyMutationError>(())
            })?;
            let key = module_instance_count_key(&instance.module_id);
            let before = state::lookup(&root, &key, context)?;
            let previous = match before {
                None => 0,
                Some(id) => {
                    let mut reader = ValueReadContext {
                        source: &*context.sink,
                        memory: context.memory,
                        class: MemoryClass::Ordinary,
                        io: context.io,
                    };
                    let value = reader.load_root(&id)?;
                    if value.0.logical_len != MODULE_INSTANCE_COUNT_BYTES as u64 {
                        return Err(BodyMutationError::Invalid("module count value length"));
                    }
                    let mut bytes = [0; MODULE_INSTANCE_COUNT_BYTES];
                    reader.read_range(
                        &value,
                        0,
                        value.0.logical_len,
                        &mut bytes.as_mut_slice(),
                        &mut context.cancelled,
                    )?;
                    module_instance_count_from_bytes(&bytes).map_err(BodyMutationError::Invalid)?
                }
            };
            let next = previous
                .checked_add(1)
                .ok_or(BodyMutationError::Invalid("module count overflow"))?;
            let value = write_field_bytes(
                &module_instance_count_bytes(next).map_err(BodyMutationError::Invalid)?,
                context,
            )?;
            writes.push(StateWriteV1 {
                key: key.to_vec(),
                before,
                after: Some(value),
            });
            writes.sort_by(|a, b| a.key.cmp(&b.key));
            state::apply(&mut root, writes, context)?;
            count = count
                .checked_add(1)
                .ok_or(BodyMutationError::Invalid("module instance genesis count"))?;
        }
        let value = write_field_bytes(&module_instance_index_bytes(count), context)?;
        state::apply(
            &mut root,
            vec![StateWriteV1 {
                key: MODULE_INSTANCE_INDEX_KEY_V1.to_vec(),
                before: None,
                after: Some(value),
            }],
            context,
        )?;
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let charge = metadata.split(256).map_err(BodyMutationError::Invalid)?;
        Ok(charge.retain(root))
    }
}
