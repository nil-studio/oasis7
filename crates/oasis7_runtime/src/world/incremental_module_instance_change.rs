//! Storage contribution for an already owner-validated instance replacement.
//! Original lifecycle facts, ownership, wakes and fees remain outer obligations.
use super::{
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_module_instance_genesis::write_module_instance_scalar,
    incremental_module_read::ModuleReadContext,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_world_state::{
    ModuleInstanceState,
    incremental_module_instance_fields::{
        ModuleInstanceFieldV1 as Field, ModuleInstanceKeyV1, ModuleInstanceScalarRefV1 as Scalar,
        visit_module_instance_fields,
    },
    incremental_module_instance_index::*,
};

pub fn contribute_module_instance_change<R, V, W, C>(
    map_key: &str,
    after: Option<&ModuleInstanceState>,
    txn: &mut ExecutionTransaction<'_, R>,
    values: &V,
    context: &mut BodyMutationContext<'_, W, C>,
    max_scalar_bytes: u64,
) -> Result<(), BodyMutationError>
where
    R: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
{
    txn.contribute(|txn| {
        let mut read = ModuleReadContext {
            values,
            memory: context.memory,
            io: context.io,
            max_value_bytes: max_scalar_bytes,
            cancelled: &mut context.cancelled,
        };
        let total = read.instance_index_total(txn)?;
        let before = read.read_instance(map_key, txn)?;
        let old_module = before.as_ref().map(|row| match row.field(Field::ModuleId) {
            Scalar::Text(id) => id,
            _ => unreachable!("validated module ID text"),
        });
        let old_count = match old_module {
            Some(id) => {
                let count = read.module_instance_count(id, txn)?;
                if count == 0 {
                    return Err(BodyMutationError::Invalid(
                        "module count missing for stored instance",
                    ));
                }
                count
            }
            None => 0,
        };
        let new_module = after.map(|instance| instance.module_id.as_str());
        let new_count = match new_module {
            Some(id) if Some(id) != old_module => read.module_instance_count(id, txn)?,
            _ => old_count,
        };
        let next_total = match (before.is_some(), after.is_some()) {
            (false, true) => total.checked_add(1),
            (true, false) => total.checked_sub(1),
            _ => Some(total),
        }
        .ok_or(BodyMutationError::Invalid(
            "module instance total overflow/underflow",
        ))?;
        // End read-context borrows before writing; all later reads see this
        // transaction's overlay through the shared underlying value CAS.
        if let Some(instance) = after {
            visit_module_instance_fields(instance, |field, value| {
                let key =
                    ModuleInstanceKeyV1::new(map_key, field).map_err(BodyMutationError::Invalid)?;
                if let Some(old) = &before {
                    if old.field(field) == value {
                        return Ok(());
                    }
                } else if txn.read(Ns::Business, key.as_bytes())?.is_some() {
                    return Err(BodyMutationError::Invalid("orphan module instance field"));
                }
                let id = write_module_instance_scalar(field, value, context, max_scalar_bytes)?;
                txn.write(Ns::Business, key.as_bytes(), Some(id))?;
                Ok::<_, BodyMutationError>(())
            })?;
        } else if before.is_some() {
            for field in Field::ALL {
                let key =
                    ModuleInstanceKeyV1::new(map_key, field).map_err(BodyMutationError::Invalid)?;
                txn.write(Ns::Business, key.as_bytes(), None)?;
            }
        }
        if old_module != new_module {
            if let Some(id) = old_module {
                put_count(
                    id,
                    old_count
                        .checked_sub(1)
                        .ok_or(BodyMutationError::Invalid("module count underflow"))?,
                    txn,
                    context,
                )?;
            }
            if let Some(id) = new_module {
                put_count(
                    id,
                    new_count
                        .checked_add(1)
                        .ok_or(BodyMutationError::Invalid("module count overflow"))?,
                    txn,
                    context,
                )?;
            }
        }
        if next_total != total {
            let id = write_field_bytes(&module_instance_index_bytes(next_total), context)?;
            txn.write(Ns::Business, MODULE_INSTANCE_INDEX_KEY_V1, Some(id))?;
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
fn put_count<R: MerkleNodeReader, W: ValueNodeWriter, C: FnMut() -> bool>(
    module_id: &str,
    count: u64,
    txn: &mut ExecutionTransaction<'_, R>,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    let value = if count == 0 {
        None
    } else {
        Some(write_field_bytes(
            &module_instance_count_bytes(count).map_err(BodyMutationError::Invalid)?,
            context,
        )?)
    };
    txn.write(Ns::Business, &module_instance_count_key(module_id), value)?;
    Ok(())
}
