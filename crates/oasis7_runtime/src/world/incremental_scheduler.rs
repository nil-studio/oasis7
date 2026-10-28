//! Point updates of module wakes in the shared IWS execution transaction.
//! Due enumeration, invocation/artifact qualification and full tick routing
//! remain driver obligations; these helpers do not publish or install state.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_body_event::read_fixed,
    module_tick_runtime::wake_after_directive,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_wasm_abi::ModuleTickLifecycleDirective;
use oasis7_world_state::{
    incremental_agent_body::{WORLD_TIME_KEY_V1, world_time_bytes_v1},
    incremental_scheduler_fields::{MODULE_WAKE_BYTES, ModuleWakeKeyV1, ModuleWakeV1},
};

fn cancelled<W, C: FnMut() -> bool>(
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    if (context.cancelled)() {
        Err(ValueTreeError::Cancelled.into())
    } else {
        Ok(())
    }
}
fn check_clock<R: MerkleNodeReader, V: ValueNodeReader, W, C: FnMut() -> bool>(
    now: u64,
    txn: &mut ExecutionTransaction<'_, R>,
    values: &V,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    if !txn.uses_budget(context.memory) {
        return Err(BodyMutationError::Invalid("scheduler transaction pool"));
    }
    cancelled(context)?;
    let bytes = read_fixed::<{ b"oasis7.iws.world-time-value.v1\0".len() + 8 }, _, _, _, _>(
        txn,
        Ns::Allocator,
        WORLD_TIME_KEY_V1,
        values,
        context,
    )?;
    if bytes != world_time_bytes_v1(now) {
        return Err(BodyMutationError::Invalid("scheduler clock precondition"));
    }
    Ok(())
}
fn read_wake<R: MerkleNodeReader, V: ValueNodeReader, W, C: FnMut() -> bool>(
    key: &[u8],
    txn: &mut ExecutionTransaction<'_, R>,
    values: &V,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<Option<u64>, BodyMutationError> {
    if txn.read(Ns::Scheduler, key)?.is_none() {
        return Ok(None);
    }
    let bytes =
        read_fixed::<MODULE_WAKE_BYTES, _, _, _, _>(txn, Ns::Scheduler, key, values, context)?;
    Ok(Some(
        ModuleWakeV1::from_canonical_bytes(&bytes)
            .map_err(BodyMutationError::Invalid)?
            .0,
    ))
}
fn put_wake<R: MerkleNodeReader, W: ValueNodeWriter, C: FnMut() -> bool>(
    key: &[u8],
    wake: u64,
    txn: &mut ExecutionTransaction<'_, R>,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    let id = write_field_bytes(&ModuleWakeV1(wake).canonical_bytes(), context)?;
    txn.write(Ns::Scheduler, key, Some(id))?;
    Ok(())
}
impl World {
    /// Stage an owner-validated activation/update/clear using exact overlay
    /// state. This method does not itself authorize the lifecycle operation.
    pub fn contribute_iws_module_wake<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
    >(
        &self,
        instance: &str,
        wake: Option<u64>,
        txn: &mut ExecutionTransaction<'_, R>,
        values: &V,
        context: &mut BodyMutationContext<'_, W, C>,
    ) -> Result<(), BodyMutationError> {
        txn.contribute(|txn| {
            let key = ModuleWakeKeyV1::new(instance).map_err(BodyMutationError::Invalid)?;
            check_clock(self.state.time, txn, values, context)?;
            let before = read_wake(key.as_bytes(), txn, values, context)?;
            if before != wake {
                match wake {
                    Some(wake) => put_wake(key.as_bytes(), wake, txn, context)?,
                    None => txn.write(Ns::Scheduler, key.as_bytes(), None)?,
                }
            }
            cancelled(context)
        })
    }

    /// Consume one supplied due instance, stage its trusted execution in the
    /// same transaction, then apply its lifecycle directive. The caller must
    /// supply the authenticated due set and real module execution/charge facts.
    pub fn contribute_iws_due_module_tick<'a, 'b, R, V, W, C, F>(
        &self,
        instance: &str,
        txn: &mut ExecutionTransaction<'a, R>,
        values: &V,
        context: &mut BodyMutationContext<'b, W, C>,
        execute: F,
    ) -> Result<bool, BodyMutationError>
    where
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnOnce(
            &str,
            u64,
            &mut ExecutionTransaction<'a, R>,
            &mut BodyMutationContext<'b, W, C>,
        ) -> Result<Option<ModuleTickLifecycleDirective>, BodyMutationError>,
    {
        txn.contribute(|txn| {
            let key = ModuleWakeKeyV1::new(instance).map_err(BodyMutationError::Invalid)?;
            let now = self.state.time;
            check_clock(now, txn, values, context)?;
            let wake = read_wake(key.as_bytes(), txn, values, context)?;
            if wake.is_none_or(|wake| wake > now) {
                cancelled(context)?;
                return Ok(false);
            }
            txn.write(Ns::Scheduler, key.as_bytes(), None)?;
            let directive = execute(instance, now, txn, context)?;
            if !txn.uses_budget(context.memory) {
                return Err(BodyMutationError::Invalid("scheduler transaction pool"));
            }
            cancelled(context)?;
            if let Some(next) = wake_after_directive(now, directive.as_ref()) {
                put_wake(key.as_bytes(), next, txn, context)?;
            }
            cancelled(context)?;
            Ok(true)
        })
    }

    /// Borrowed full field preparation for genesis only. Emitted pages/writes
    /// remain provisional and the receiver admits its own output metadata.
    pub fn write_iws_module_schedule_genesis<
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&[u8], [u8; 32]) -> Result<(), BodyMutationError>,
    >(
        &self,
        context: &mut BodyMutationContext<'_, W, C>,
        remaining_entries: &mut u64,
        mut emit: F,
    ) -> Result<u64, BodyMutationError> {
        cancelled(context)?;
        let _metadata = context
            .memory
            .reserve(16 * 1024, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let mut count = 0u64;
        for (instance, wake) in &self.module_tick_schedule {
            cancelled(context)?;
            *remaining_entries =
                remaining_entries
                    .checked_sub(1)
                    .ok_or(ExecutionError::ResourceLimited(
                        "module schedule genesis entries",
                    ))?;
            let key = ModuleWakeKeyV1::new(instance).map_err(BodyMutationError::Invalid)?;
            let id = write_field_bytes(&ModuleWakeV1(*wake).canonical_bytes(), context)?;
            emit(key.as_bytes(), id)?;
            count = count
                .checked_add(1)
                .ok_or(BodyMutationError::Invalid("module schedule count"))?;
        }
        cancelled(context)?;
        Ok(count)
    }
}
