//! Actual selected receipt/attempt history, shared by new and replay flows.
use super::*;

pub(in crate::world::incremental_normal_industry_due) struct CurrentProductHistoryV1 {
    receipts: ReservedValue<CurrentFactoryNativeRecordV1<Vec<crate::ProductValidationReceiptV1>>>,
    attempts: ReservedValue<CurrentFactoryNativeRecordV1<Vec<crate::ProductValidationAttemptV1>>>,
    receipt: Option<usize>,
    attempt: Option<usize>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    job_id: u64,
    output_index: usize,
}
impl CurrentProductHistoryV1 {
    pub(in crate::world::incremental_normal_industry_due) fn validate_evidence_identity<
        M: MerkleNodeReader,
    >(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        job: &crate::RecipeJobState,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.identity != identity
            || !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || !self.receipts.uses_budget(memory)
            || !self.attempts.uses_budget(memory)
            || self.now != now
            || self.job_id != job.job_id
        {
            return Err(invalid());
        }
        let stack = if self.output_index < job.produce.len() {
            job.produce.get(self.output_index)
        } else {
            job.byproducts.get(
                self.output_index
                    .checked_sub(job.produce.len())
                    .ok_or_else(invalid)?,
            )
        }
        .ok_or_else(invalid)?;
        let receipt = self.receipt().ok_or_else(invalid)?;
        if receipt.requester_agent_id != job.requester_agent_id
            || receipt.stack != *stack
            || receipt.validation_index != Some(self.output_index as u32)
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub(in crate::world::incremental_normal_industry_due) fn receipt(
        &self,
    ) -> Option<&crate::ProductValidationReceiptV1> {
        self.receipt
            .and_then(|i| self.receipts.value().and_then(|rows| rows.get(i)))
    }
    pub(in crate::world::incremental_normal_industry_due) fn attempt(
        &self,
    ) -> Option<&crate::ProductValidationAttemptV1> {
        self.attempt
            .and_then(|i| self.attempts.value().and_then(|rows| rows.get(i)))
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_industry_due) fn read_product_history_v1<
    M,
    P,
    V,
    W,
    D,
    E,
    C,
    PE,
    PC,
    CW,
    ME,
>(
    pending: &crate::RecipeJobState,
    output_index: usize,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentProductHistoryV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    crate::world::incremental_step::factory_schedule_write_io::paid_with_tail(
        step,
        read,
        parent,
        write,
        materials,
        work,
        common,
        live,
        |step, read, _parent, _write, _materials, work, tail| {
            let memory = step.mutation.memory;
            let stack = if output_index < pending.produce.len() {
                pending.produce.get(output_index)
            } else {
                pending.byproducts.get(
                    output_index
                        .checked_sub(pending.produce.len())
                        .ok_or_else(invalid)?,
                )
            }
            .ok_or_else(invalid)?;
            let index = Some(output_index as u32);
            let mut local = WorldFieldReadContextV1 {
                root: sources.roots.state_root(StateNamespaceV1::Authorization),
                state: read.state,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            let guard = step.transaction.guard();
            let receipts = recover_current_product_validation_receipts_v1(
                pending.job_id,
                sources.lifecycle.native,
                sources.roots,
                step.transaction,
                &mut local,
                &mut work.lifecycle.native_scan,
                &mut work.lifecycle.native_read,
                &guard,
                live,
            )
            .map_err(overlay)?;
            let rows = receipts.value().map(Vec::as_slice);
            let bytes = pending
                .requester_agent_id
                .len()
                .checked_add(stack.kind.len())
                .ok_or_else(limited)?;
            for row in rows.into_iter().flatten() {
                tail.charge(bytes.checked_add(receipt_heap(row)?).ok_or_else(limited)?)?;
            }
            tail.charge(1024 + size_of::<StateCause>())?;
            let error_fee = memory
                .reserve(1024 + size_of::<StateCause>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let receipt=crate::world::economy_product_validation::receipt_match::receipt_for_output_from_rows_v1(rows,pending.job_id,index,&pending.requester_agent_id,stack).map_err(|e|phase_error(e,error_fee))?;
            let receipt_index =
                rows.and_then(|rows| rows.iter().position(|row| row.validation_index == index));
            // Receipt conflict is deliberately observed before the attempt row is read.
            let module = receipt.map(|receipt| receipt.module_id.as_str());
            drop(local);
            let mut local = WorldFieldReadContextV1 {
                root: sources.roots.state_root(StateNamespaceV1::Effects),
                state: read.state,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            let attempts = recover_current_product_validation_attempts_v1(
                pending.job_id,
                sources.lifecycle.native,
                sources.roots,
                step.transaction,
                &mut local,
                &mut work.lifecycle.native_scan,
                &mut work.lifecycle.native_read,
                &guard,
                live,
            )
            .map_err(overlay)?;
            let rows = attempts.value().map(Vec::as_slice);
            for row in rows.into_iter().flatten() {
                tail.charge(
                    bytes
                        .checked_add(attempt_heap(row)?)
                        .and_then(|n| n.checked_add(module.map_or(0, str::len)))
                        .ok_or_else(limited)?,
                )?;
            }
            tail.charge(1024 + size_of::<StateCause>())?;
            let error_fee = memory
                .reserve(1024 + size_of::<StateCause>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            crate::world::economy_product_validation::receipt_match::attempt_for_output_from_rows_v1(rows,pending.job_id,index,&pending.requester_agent_id,stack,module).map_err(|e|phase_error(e,error_fee))?;
            let attempt =
                rows.and_then(|rows| rows.iter().position(|row| row.validation_index == index));
            tail.charge(size_of::<CurrentProductHistoryV1>())?;
            let fee = memory
                .reserve(size_of::<CurrentProductHistoryV1>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Ok(fee.retain(CurrentProductHistoryV1 {
                receipts,
                attempts,
                receipt: receipt_index,
                attempt,
                guard: step.transaction.guard(),
                identity: tail.identity(),
                memory: memory.clone(),
                now: step.time(),
                job_id: pending.job_id,
                output_index,
            }))
        },
    )
}
