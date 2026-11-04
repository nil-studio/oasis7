//! One generic replay receipt shared by independent typed owner receivers.
//! This is integrity evidence, not a World-ready or accepted-head capability.
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, PreparedTransition, TransitionRecordV1, TransitionVerifier,
    },
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
const META_BYTES: usize = std::mem::size_of::<PreparedTransition>() + 512;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("verified World transition binding/pool/class")
}
#[derive(Debug)]
pub(crate) enum WorldTransitionVerificationErrorV1 {
    Execution(ExecutionError),
    Value(ValueTreeError),
}
impl From<ExecutionError> for WorldTransitionVerificationErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<ValueTreeError> for WorldTransitionVerificationErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<WorldTransitionVerificationErrorV1> for WorldFieldOverlayErrorV1 {
    fn from(e: WorldTransitionVerificationErrorV1) -> Self {
        match e {
            WorldTransitionVerificationErrorV1::Execution(e) => e.into(),
            WorldTransitionVerificationErrorV1::Value(e) => e.into(),
        }
    }
}
/// Adapter for existing recovery/embedded-state-source contexts. It owns no IO
/// allowance; all state/value IO remains in the original caller-owned readers.
pub(crate) struct WorldTransitionVerificationContextV1<'a, E, C> {
    pub after_root: &'a StateRootV1,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub take_state_error: E,
    pub cancelled: C,
}
impl<E: FnMut() -> Option<ValueTreeError>, C: FnMut() -> bool>
    WorldTransitionVerificationContextV1<'_, E, C>
{
    fn check(&mut self) -> Result<(), ValueTreeError> {
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(e) = (self.take_state_error)() {
            return Err(e);
        }
        Ok(())
    }
}
/// No Clone, mutable access, or unchecked PreparedTransition constructor exists.
/// The complete original reservation stays alive through every owner check.
pub(crate) struct VerifiedWorldTransitionV1 {
    checked: ReservedValue<PreparedTransition>,
    memory: MemoryBudget,
    class: MemoryClass,
    metadata: ReservedValue<()>,
    before: [u8; 32],
    after: [u8; 32],
}
impl VerifiedWorldTransitionV1 {
    pub(crate) fn record(&self) -> &TransitionRecordV1 {
        &self.checked.record
    }
    pub(crate) fn prepared(&self) -> &PreparedTransition {
        &self.checked
    }
    pub(crate) fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub(crate) fn before_digest(&self) -> &[u8; 32] {
        &self.before
    }
    pub(crate) fn after_digest(&self) -> &[u8; 32] {
        &self.after
    }
    pub(crate) fn check_pool_class(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if self.class != class
            || !self.memory.same_pool(memory)
            || !self.checked.uses_budget(memory)
            || !self.metadata.uses_budget(memory)
            || self.metadata.reserved_bytes() < META_BYTES
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(crate) fn check_context(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
        after_root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        self.check_pool_class(memory, class)?;
        if !self.record().after.state.iter().any(|r| r == after_root) {
            return Err(invalid());
        }
        Ok(())
    }
    /// Backward-compatible owner wrappers may return the same generic receipt
    /// after their own typed check. This still confers no whole-World authority.
    pub(crate) fn into_prepared(self) -> ReservedValue<PreparedTransition> {
        self.checked
    }
}
pub(crate) fn verify_world_transition_in_v1<
    P: MerkleNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldTransitionVerificationContextV1<'_, E, C>,
) -> Result<VerifiedWorldTransitionV1, WorldTransitionVerificationErrorV1> {
    ctx.check()?;
    if verifier.class != ctx.class || !verifier.memory.same_pool(ctx.memory) {
        return Err(invalid().into());
    }
    let _phase = ctx
        .memory
        .reserve(128 << 10, ctx.class)
        .map_err(ValueTreeError::Memory)?;
    record.after.validate().map_err(|_| invalid())?;
    if !record.after.state.iter().any(|r| r == ctx.after_root) {
        return Err(invalid().into());
    }
    let metadata = ctx
        .memory
        .reserve(META_BYTES, ctx.class)
        .map_err(ValueTreeError::Memory)?
        .retain(());
    let replay = verifier.verify(record);
    let original = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(original) = original {
        return Err(original.into());
    }
    let checked = replay?;
    if !checked.uses_budget(ctx.memory) {
        return Err(invalid().into());
    }
    let before = checked.record.before.digest().map_err(|_| invalid())?;
    let after = checked.record.after.digest().map_err(|_| invalid())?;
    ctx.check()?;
    Ok(VerifiedWorldTransitionV1 {
        checked,
        memory: ctx.memory.clone(),
        class: ctx.class,
        metadata,
        before,
        after,
    })
}
pub(crate) fn verify_world_transition_v1<M, P, R, E, C>(
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<VerifiedWorldTransitionV1, WorldTransitionVerificationErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    let result = verify_world_transition_in_v1(
        verifier,
        record,
        &mut WorldTransitionVerificationContextV1 {
            after_root: ctx.root,
            memory: ctx.values.memory,
            class: ctx.values.class,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        },
    );
    ctx.check()?;
    result
}

#[cfg(test)]
#[path = "incremental_verified_world_transition_tests.rs"]
mod tests;
