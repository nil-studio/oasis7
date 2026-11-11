//! Frozen complete Current original invocation keys, not caller invocation IDs.
use super::*;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionTransaction, RootSetV1, SealedExecutionBindingV1,
};
use oasis7_proto::memory_budget::MemoryBudget;
pub(in crate::world) struct FrozenInvocationDirectoryV1 {
    guard: PreparedGuard,
    memory: MemoryBudget,
    instances: ReservedValue<Vec<Vec<u8>>>,
    registry: ReservedValue<Vec<Vec<u8>>>,
}
impl FrozenInvocationDirectoryV1 {
    pub(in crate::world) fn validate_guard<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), CurrentInvocationEnumerationErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("frozen invocation pool").into());
        }
        Ok(())
    }
    pub(in crate::world) fn validate_sealed(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), CurrentInvocationEnumerationErrorV1> {
        if !self.memory.same_pool(memory) {
            return Err(ValueTreeError::Invalid("frozen invocation pool").into());
        }
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn freeze_current_invocation_directory_v1<M, R, E, C>(
    parent: &ReservedValue<AcceptedInvocationDirectoryV1>,
    instances: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    config: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut CurrentInvocationEnumerationWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FrozenInvocationDirectoryV1>, CurrentInvocationEnumerationErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        // Real Current selection validates Q binding and its actual child readers.
        // Its opaque result is not replaced by public counts or caller key lists.
        let _selected = collect_current_module_invocation_ids_v1(
            parent, instances, config, state, ctx, work, guard, live,
        )?;
        let extra_comparisons = work
            .max_keys
            .checked_mul(work.max_keys)
            .and_then(|n| n.checked_mul(work.max_key_bytes.max(1)))
            .and_then(|n| n.checked_mul(16))
            .ok_or(ValueTreeError::ResourceLimited(
                "freeze invocation comparison bound",
            ))?;
        work.comparison_bytes_remaining = work
            .comparison_bytes_remaining
            .checked_sub(u64::try_from(extra_comparisons).map_err(|_| {
                ValueTreeError::ResourceLimited("freeze invocation comparison bound")
            })?)
            .ok_or(ValueTreeError::ResourceLimited(
                "freeze invocation comparison work",
            ))?;
        let fee = ctx
            .values
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let instances = keys::current_keys_v1(
            ctx.transaction,
            state,
            Some(&parent.instances),
            StateNamespaceV1::Business,
            CurrentKeyEnumerationBudgetV1 {
                memory: ctx.values.memory,
                scan: &mut work.instance_scan,
                max_entries: work.max_keys,
                max_key_bytes: work.max_key_bytes,
                pending_entries: &mut work.pending_entries,
            },
            &mut ctx.cancelled,
            ModuleInstanceKeyV1::is_instance_key,
        )?;
        let registry = keys::current_keys_v1(
            ctx.transaction,
            state,
            Some(&parent.registry),
            StateNamespaceV1::Artifacts,
            CurrentKeyEnumerationBudgetV1 {
                memory: ctx.values.memory,
                scan: &mut work.registry_scan,
                max_entries: work.max_keys,
                max_key_bytes: work.max_key_bytes,
                pending_entries: &mut work.pending_entries,
            },
            &mut ctx.cancelled,
            |raw| {
                WorldFieldKeyV1::from_bytes(raw).is_ok_and(|key| {
                    key.kind == WorldFieldRecordKindV1::Entry
                        && WorldFieldKeyV1::new(
                            ArtifactConfigFieldV1::RegistryActive.descriptor(),
                            key.kind,
                            key.semantic_key,
                        )
                        .is_ok_and(|expected| expected.as_bytes() == raw)
                })
            },
        )?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(fee.retain(FrozenInvocationDirectoryV1 {
            guard: ctx.transaction.guard(),
            memory: ctx.values.memory.clone(),
            instances,
            registry,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
