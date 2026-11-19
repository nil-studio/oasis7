//! Actual original retention JSON projection. Full materialization is only a
//! GEN/qualified Cold operation; ordinary capture emits dirty JSON paths only.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_json_records::{
        CognitionAuxJsonPatchV1, JsonAuxBuildV1, project_json_aux_paths_v1,
    },
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_world_rule_projection_delta::{Processing, spend},
};
use std::{cell::RefCell, sync::Arc};
pub fn materialize_qualified_cognition_retention_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source: &ReservedValue<AcceptedCognitionRetentionLedgerV1>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    json: &mut JsonReadWorkV1,
) -> Result<Option<ReservedValue<serde_json::Value>>, ValueTreeError> {
    read.check()?;
    source.check(read.values.memory, read.values.class)?;
    if !source.uses_budget(read.values.memory) {
        return Err(invalid());
    }
    let Some(q) = &source.payload else {
        return Ok(None);
    };
    q.validate_binding(
        profile(),
        source.manifest.payload.as_ref().ok_or_else(invalid)?,
    )?;
    let limit = usize::try_from(json.max_payload_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("retention qualified heap bound"))?;
    let (value, fee) = crate::incremental_json_records::materialize_records(
        profile(),
        q,
        &mut read.records(),
        json,
        limit,
    )?;
    read.check()?;
    Ok(Some(fee.retain(value)))
}
pub struct CognitionRetentionProjectionAuxV1 {
    present: bool,
    patches: JsonAuxBuildV1,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    fee: MemoryReservation,
}
impl CognitionRetentionProjectionAuxV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        tx.validate_read_source_guard(&self.guard, live)
            .map_err(|_| invalid())?;
        if self.parent != tx.parent_fence().parent_roots
            || !self.memory.same_pool(memory)
            || !self.fee.uses_budget(memory)
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn present(&self) -> bool {
        self.present
    }
    pub fn visit_paths_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&CognitionAuxJsonPatchV1) -> Result<(), E>,
    ) -> Result<(), E> {
        for patch in &self.patches.patches {
            work(1, 0)?;
            emit(patch)?
        }
        Ok(())
    }
}
struct PaidValues<'a, 'b, R> {
    source: &'a R,
    p: &'a Processing<'b>,
}
impl<R: ValueNodeReader> ValueNodeReader for PaidValues<'_, '_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        spend(self.p, 0, &mut || false)?;
        let raw = self.source.read_node(id, max, memory, class)?;
        let bytes = raw
            .len()
            .checked_mul(16)
            .and_then(|n| n.checked_add(128))
            .ok_or_else(invalid)?;
        spend(self.p, bytes, &mut || false)?;
        Ok(raw)
    }
}
pub fn capture_current_cognition_retention_projection_aux_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: &ReservedValue<AcceptedCognitionRetentionLedgerV1>,
    after: &FrozenCognitionRetentionLedgerV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    json: &mut JsonChangeWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    live: &ExecutionFence,
) -> Result<CognitionRetentionProjectionAuxV1, WorldFieldOverlayErrorV1> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        after.validate_read_context_v1(ctx.transaction, ctx.values.memory, live)?;
        before.check(ctx.values.memory, MemoryClass::Ordinary)?;
        if !before.uses_budget(ctx.values.memory)
            || before.record != after.parent_record_data_v1()
            || before.body != ctx.transaction.parent_fence().parent_commit
            || before.manifest.roots != ctx.transaction.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        let memory = ctx.values.memory.clone();
        let p = Processing {
            bank: RefCell::new(processing),
        };
        spend(
            &p,
            size_of::<CognitionRetentionProjectionAuxV1>(),
            &mut ctx.cancelled,
        )?;
        let fee = memory
            .reserve(
                size_of::<CognitionRetentionProjectionAuxV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let source = PaidValues {
            source: ctx.values.source,
            p: &p,
        };
        let mut values = oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut *ctx.values.io,
        };
        let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
            values: &mut values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let mut patches = JsonAuxBuildV1::new(&memory)?;
        project_json_aux_paths_v1(
            profile(),
            before.manifest.payload.as_ref(),
            after.payload_root_data_v1(),
            &mut records,
            &mut json.json,
            &mut json.paths_remaining,
            &p,
            &mut patches,
        )?;
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        after.validate_read_context_v1(ctx.transaction, &memory, live)?;
        Ok(CognitionRetentionProjectionAuxV1 {
            present: after.payload_root_data_v1().is_some(),
            patches,
            guard: ctx.transaction.guard(),
            parent: ctx.transaction.parent_fence().parent_roots,
            memory,
            fee,
        })
    }));
    match outcome {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
