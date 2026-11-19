//! A single actual pending output's economy module call. Only the fixed due
//! dispatcher can construct it after persisting its real attempt; it is not a
//! public module-id/bytes/purpose interface or an Action authorization.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_proto::memory_budget::MemoryClass;

pub(in crate::world) struct PreparedDueProductCallV1<'a> {
    pending: &'a ReservedValue<crate::RecipeJobState>,
    module_id: &'a ReservedValue<String>,
    output_index: usize,
    request: ReservedValue<oasis7_wasm_abi::ProductValidationRequest>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    _phase: MemoryReservation,
}
enum CallValidationV1 {
    Execution,
    ExistingEvidence,
}
impl PreparedDueProductCallV1<'_> {
    pub(in crate::world) fn module_id(&self) -> &str {
        self.module_id
    }
    pub(in crate::world) fn job(&self) -> &crate::RecipeJobState {
        self.pending
    }
    pub(in crate::world) fn validation_index(&self) -> u32 {
        self.output_index as u32
    }
    pub(in crate::world) fn output_index(&self) -> usize {
        self.output_index
    }
    pub(in crate::world) fn request(&self) -> &oasis7_wasm_abi::ProductValidationRequest {
        &self.request
    }
    pub(in crate::world) fn now(&self) -> u64 {
        self.now
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.validate_inner(tx, common, now, live, CallValidationV1::Execution)
    }
    pub(in crate::world) fn validate_evidence_identity<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.validate_inner(tx, common, now, live, CallValidationV1::ExistingEvidence)
    }
    fn validate_inner<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
        mode: CallValidationV1,
    ) -> Result<(), BodyMutationError> {
        let job = &**self.pending;
        let stack = if self.output_index < job.produce.len() {
            job.produce.get(self.output_index)
        } else {
            job.byproducts.get(
                self.output_index
                    .checked_sub(job.produce.len())
                    .ok_or(BodyMutationError::Invalid("due product output index"))?,
            )
        }
        .ok_or(BodyMutationError::Invalid("due product output absent"))?;
        let bytes = stack
            .kind
            .len()
            .checked_add(self.request.product_id.len())
            .and_then(|n| n.checked_add(self.request.stack.kind.len()))
            .and_then(|n| n.checked_add(self.module_id.len()))
            .ok_or(BodyMutationError::Invalid("due product source length"))?;
        common
            .charge(
                1,
                u64::try_from(bytes)
                    .map_err(|_| BodyMutationError::Invalid("due product source length"))?,
            )
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        match mode {
            CallValidationV1::Execution => tx.validate_guard(&self.guard, live)?,
            CallValidationV1::ExistingEvidence => {
                tx.validate_transaction_identity(&self.guard)?;
                tx.validate_guard(&tx.guard(), live)?;
            }
        }
        if self.identity != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !tx.uses_budget(common.memory())
            || !self.pending.uses_budget(common.memory())
            || !self.module_id.uses_budget(common.memory())
            || !self.request.uses_budget(common.memory())
            || self.now != now
            || self.request.product_id != stack.kind
            || self.request.stack != *stack
            || self.request.deterministic_seed
                != job
                    .job_id
                    .wrapping_mul(1_000_003)
                    .wrapping_add(self.output_index as u64)
                    .wrapping_add(now)
        {
            return Err(BodyMutationError::Invalid(
                "due product original source binding",
            ));
        }
        Ok(())
    }
}
/// Kept private to this due implementation. Invocation choice has already come
/// from the original active registry or the actual matching persisted attempt.
pub(super) fn prepare_product_call_v1<'a, M: MerkleNodeReader>(
    pending: &'a ReservedValue<crate::RecipeJobState>,
    module_id: &'a ReservedValue<String>,
    output_index: usize,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedDueProductCallV1<'a>, BodyMutationError> {
    tx.validate_guard(&tx.guard(), live)?;
    let stack = if output_index < pending.produce.len() {
        pending.produce.get(output_index)
    } else {
        pending.byproducts.get(
            output_index
                .checked_sub(pending.produce.len())
                .ok_or(BodyMutationError::Invalid("due product output index"))?,
        )
    }
    .ok_or(BodyMutationError::Invalid("due product output absent"))?;
    let bytes = stack
        .kind
        .len()
        .checked_mul(2)
        .and_then(|n| {
            n.checked_add(std::mem::size_of::<oasis7_wasm_abi::ProductValidationRequest>())
        })
        .ok_or(BodyMutationError::Invalid("due product request layout"))?;
    common
        .charge(
            1,
            u64::try_from(bytes)
                .map_err(|_| BodyMutationError::Invalid("due product request layout"))?,
        )
        .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    let phase = common
        .memory()
        .reserve(
            std::mem::size_of::<PreparedDueProductCallV1<'a>>(),
            MemoryClass::Ordinary,
        )
        .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
    let call = PreparedDueProductCallV1 {
        pending,
        module_id,
        output_index,
        request: fee.retain(oasis7_wasm_abi::ProductValidationRequest {
            product_id: stack.kind.clone(),
            stack: stack.clone(),
            deterministic_seed: pending
                .job_id
                .wrapping_mul(1_000_003)
                .wrapping_add(output_index as u64)
                .wrapping_add(now),
        }),
        guard: tx.guard(),
        identity: common.work().identity(),
        memory: common.memory().clone(),
        now,
        _phase: phase,
    };
    call.validate(tx, common, now, live)?;
    Ok(call)
}
