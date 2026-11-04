//! Streaming instance GEN for the shared six-namespace index builder.
use super::incremental_module_instance_genesis::write_module_instance_scalar;
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError,
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_module_instance_fields::{ModuleInstanceKeyV1, visit_module_instance_fields},
    incremental_module_instance_index::*,
};
use std::collections::BTreeMap;

fn count_key_cancelled(
    module: &str,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<[u8; MODULE_INSTANCE_COUNT_KEY_BYTES], BodyMutationError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let length = u64::try_from(module.len())
        .map_err(|_| BodyMutationError::Invalid("instance GEN module ID length"))?;
    let mut hash = ModuleInstanceCountKeyHashV1::new(length);
    for chunk in module.as_bytes().chunks(4096) {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        hash.update(chunk).map_err(BodyMutationError::Invalid)?;
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    hash.finish().map_err(BodyMutationError::Invalid)
}

impl World {
    /// Each instance field, module count and global count is emitted once.
    /// The bounded derived count index borrows original module IDs and never
    /// collects the World write-set or clones an instance.
    pub fn write_iws_module_instance_genesis_rows<W: ValueNodeWriter, C: FnMut() -> bool>(
        &self,
        context: &mut BodyMutationContext<'_, W, C>,
        remaining_instances: &mut u64,
        remaining_count_rows: &mut u64,
        max_scalar_bytes: u64,
        emit: &mut impl FnMut(StateNamespaceV1, &[u8], [u8; 32]) -> Result<(), BodyMutationError>,
    ) -> Result<u64, BodyMutationError> {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let count = u64::try_from(self.state.module_instances.len())
            .map_err(|_| BodyMutationError::Invalid("instance GEN length"))?;
        if count > *remaining_instances {
            return Err(ExecutionError::ResourceLimited("instance GEN count").into());
        }
        let bytes = self
            .state
            .module_instances
            .len()
            .checked_mul(256)
            .and_then(|n| n.checked_add(4096))
            .ok_or(BodyMutationError::Invalid("instance GEN metadata overflow"))?;
        let _metadata = context
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let mut counts = BTreeMap::<&str, u64>::new();
        for (key, instance) in &self.state.module_instances {
            if (context.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            *remaining_instances = remaining_instances
                .checked_sub(1)
                .ok_or(ExecutionError::ResourceLimited("instance GEN count"))?;
            visit_module_instance_fields(instance, |field, value| {
                let key =
                    ModuleInstanceKeyV1::new(key, field).map_err(BodyMutationError::Invalid)?;
                let value = write_module_instance_scalar(field, value, context, max_scalar_bytes)?;
                emit(StateNamespaceV1::Business, key.as_bytes(), value)?;
                if (context.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                Ok::<_, BodyMutationError>(())
            })?;
            if let Some(previous) = counts.get_mut(instance.module_id.as_str()) {
                *previous = previous
                    .checked_add(1)
                    .ok_or(BodyMutationError::Invalid("instance module count overflow"))?;
            } else {
                *remaining_count_rows =
                    remaining_count_rows
                        .checked_sub(1)
                        .ok_or(ExecutionError::ResourceLimited(
                            "instance GEN module count rows",
                        ))?;
                counts.insert(instance.module_id.as_str(), 1);
            }
        }
        for (module, count) in counts {
            if (context.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let key = count_key_cancelled(module, &mut context.cancelled)?;
            let value = write_field_bytes(
                &module_instance_count_bytes(count).map_err(BodyMutationError::Invalid)?,
                context,
            )?;
            emit(StateNamespaceV1::Business, &key, value)?;
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let value = write_field_bytes(&module_instance_index_bytes(count), context)?;
        emit(
            StateNamespaceV1::Business,
            MODULE_INSTANCE_INDEX_KEY_V1,
            value,
        )?;
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(count)
    }
}

#[cfg(test)]
#[path = "incremental_genesis_instances_tests.rs"]
mod tests;
