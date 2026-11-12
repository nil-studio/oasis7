//! A process-local witness of ONE successful real transaction seal.
//! It is neither execution acceptance nor durable/selected-head authority.
use super::*;
/// No public constructor or Clone: arbitrary public PreparedTransition values
/// cannot manufacture the opaque guard owner captured by the actual seal.
#[derive(Debug)]
pub struct SealedExecutionBindingV1 {
    guard: PreparedGuard,
    after_roots: [u8; 32],
    memory: MemoryBudget,
}
impl SealedExecutionBindingV1 {
    pub fn after_roots_digest(&self) -> [u8; 32] {
        self.after_roots
    }
    pub fn validate_source_guard(
        &self,
        guard: &PreparedGuard,
        memory: &MemoryBudget,
    ) -> Result<(), ExecutionError> {
        if !self.memory.same_pool(memory)
            || !self.guard.owner.uses_budget(memory)
            || !guard.owner.uses_budget(memory)
        {
            return Err(ExecutionError::Invalid("sealed binding memory pool"));
        }
        if !Arc::ptr_eq(&self.guard.owner, &guard.owner)
            || self.guard.revision != guard.revision
            || self.guard.fence != guard.fence
        {
            return Err(ExecutionError::StaleGuard);
        }
        Ok(())
    }
    pub fn validate_after_roots(
        &self,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), ExecutionError> {
        if !self.memory.same_pool(memory) {
            return Err(ExecutionError::Invalid("sealed binding memory pool"));
        }
        // RootSet's original format bounds world_id to 256 and the canonical image
        // to 1024 bytes. This phase pays for its actual canonical digest buffer.
        let _phase = memory.reserve(4096, MemoryClass::Ordinary)?;
        if roots.digest()? != self.after_roots {
            return Err(ExecutionError::Invalid("sealed binding after roots"));
        }
        Ok(())
    }
}
/// The transition remains ordinary provisional material. The private binding
/// only authenticates its actual same-process seal/source-guard relationship.
pub struct SealedExecutionTransitionV1 {
    transition: ReservedValue<PreparedTransition>,
    binding: ReservedValue<SealedExecutionBindingV1>,
}
impl SealedExecutionTransitionV1 {
    pub fn transition(&self) -> &ReservedValue<PreparedTransition> {
        &self.transition
    }
    pub fn binding(&self) -> &ReservedValue<SealedExecutionBindingV1> {
        &self.binding
    }
    pub fn into_parts(
        self,
    ) -> (
        ReservedValue<PreparedTransition>,
        ReservedValue<SealedExecutionBindingV1>,
    ) {
        (self.transition, self.binding)
    }
}
impl<R: MerkleNodeReader> ExecutionTransaction<'_, R> {
    /// Preserve legacy seal behavior and mint a witness only AFTER its real
    /// success. Failure never exposes a binding for a partial/forged transition.
    pub fn seal_with_binding(
        self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        next_tick: u64,
    ) -> Result<SealedExecutionTransitionV1, ExecutionError> {
        self.validate_guard(guard, live)?;
        let memory = self.memory.clone();
        let charge = memory.reserve(
            std::mem::size_of::<SealedExecutionBindingV1>()
                + std::mem::size_of::<SealedExecutionTransitionV1>()
                + 128,
            MemoryClass::Ordinary,
        )?;
        let _phase = memory.reserve(4096, MemoryClass::Ordinary)?;
        let captured = self.guard();
        let transition = self.seal(guard, live, next_tick)?;
        let after_roots = transition.record.after.digest()?;
        Ok(SealedExecutionTransitionV1 {
            transition,
            binding: charge.retain(SealedExecutionBindingV1 {
                guard: captured,
                after_roots,
                memory,
            }),
        })
    }
    /// The same witness for the complete profile: final Result originals still
    /// come only from the existing frozen-after-root producer and real seal.
    #[allow(clippy::too_many_arguments)]
    pub fn seal_with_final_results_and_binding<E>(
        self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        next_tick: u64,
        mut cancelled: impl FnMut() -> bool,
        produce: impl FnOnce(&RootSetV1) -> Result<[ExecutionFactRefV1; 2], E>,
    ) -> Result<SealedExecutionTransitionV1, SealWithFinalFactError<E>> {
        self.validate_guard(guard, live)?;
        let memory = self.memory.clone();
        let charge = memory
            .reserve(
                std::mem::size_of::<SealedExecutionBindingV1>()
                    + std::mem::size_of::<SealedExecutionTransitionV1>()
                    + 128,
                MemoryClass::Ordinary,
            )
            .map_err(ExecutionError::Memory)?;
        let _phase = memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let captured = self.guard();
        let transition =
            self.seal_with_final_results(guard, live, next_tick, &mut cancelled, produce)?;
        let after_roots = transition.record.after.digest()?;
        if cancelled() {
            return Err(SealWithFinalFactError::Cancelled);
        }
        Ok(SealedExecutionTransitionV1 {
            transition,
            binding: charge.retain(SealedExecutionBindingV1 {
                guard: captured,
                after_roots,
                memory,
            }),
        })
    }
    /// Opaque same-transaction seal for the independent three-Result profile.
    #[allow(clippy::too_many_arguments)]
    pub fn seal_with_proof_ledger_results_and_binding<E>(
        self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        next_tick: u64,
        mut cancelled: impl FnMut() -> bool,
        produce: impl FnOnce(&RootSetV1) -> Result<[ExecutionFactRefV1; 3], E>,
    ) -> Result<SealedExecutionTransitionV1, SealWithFinalFactError<E>> {
        self.validate_guard(guard, live)?;
        let memory = self.memory.clone();
        let charge = memory
            .reserve(
                std::mem::size_of::<SealedExecutionBindingV1>()
                    + std::mem::size_of::<SealedExecutionTransitionV1>()
                    + 128,
                MemoryClass::Ordinary,
            )
            .map_err(ExecutionError::Memory)?;
        let _phase = memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let captured = self.guard();
        let transition =
            self.seal_with_proof_ledger_results(guard, live, next_tick, &mut cancelled, produce)?;
        let after_roots = transition.record.after.digest()?;
        if cancelled() {
            return Err(SealWithFinalFactError::Cancelled);
        }
        Ok(SealedExecutionTransitionV1 {
            transition,
            binding: charge.retain(SealedExecutionBindingV1 {
                guard: captured,
                after_roots,
                memory,
            }),
        })
    }
}
#[cfg(test)]
#[path = "incremental_seal_binding_tests.rs"]
mod tests;
