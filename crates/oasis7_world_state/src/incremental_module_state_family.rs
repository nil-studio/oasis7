//! Exact six-root structural qualification, without invocation authority.
use super::{
    incremental_agent_mutation::BodyMutationError,
    incremental_module_state_recovery::ModuleStateRecoveryContextV1,
};
use crate::incremental_module_fields::ModuleStateKeyV1;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionTransaction, RootSetV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanContext},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
const FEE: usize = 1024;
pub struct ModuleStateFamilyQualificationV1 {
    roots: [u8; 32],
    states: u64,
    logical_bytes: u64,
    class: MemoryClass,
}
impl ModuleStateFamilyQualificationV1 {
    pub fn states(&self) -> u64 {
        self.states
    }
    pub fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), BodyMutationError> {
        if self.roots != roots.digest()? {
            return Err(invalid());
        }
        Ok(())
    }
}
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("module state family binding")
}
pub fn validate_module_state_transaction_v1<R: MerkleNodeReader>(
    proof: &ReservedValue<ModuleStateFamilyQualificationV1>,
    txn: &ExecutionTransaction<'_, R>,
    memory: &MemoryBudget,
) -> Result<(), BodyMutationError> {
    if !proof.uses_budget(memory)
        || !txn.uses_budget(memory)
        || proof.reserved_bytes() < FEE
        || proof.class != MemoryClass::Ordinary
        || proof.roots != txn.parent_fence().parent_roots
    {
        return Err(invalid());
    }
    Ok(())
}
impl<R: MerkleNodeReader, V: ValueNodeReader> ModuleStateRecoveryContextV1<'_, R, V> {
    pub fn qualify_family(
        &mut self,
        roots: &RootSetV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<ModuleStateFamilyQualificationV1>, BodyMutationError> {
        roots.validate()?;
        if self.root != roots.state_root(StateNamespaceV1::Business) {
            return Err(invalid());
        }
        let mut charge = self
            .memory
            .reserve(128 << 10, self.class)
            .map_err(ValueTreeError::Memory)?;
        for root in &roots.state {
            if root.namespace == StateNamespaceV1::Business {
                continue;
            }
            StateScanContext {
                source: self.state,
                memory: self.memory,
                class: self.class,
                io: self.state_io,
            }
            .visit_entries(
                root,
                |entry| {
                    if ModuleStateKeyV1::is_state_key(&entry.key) {
                        Err(invalid())
                    } else {
                        Ok(())
                    }
                },
                &mut cancelled,
            )
            .map_err(super::incremental_module_state_recovery::scan_error)?;
        }
        let counts = self.verify(&mut cancelled)?;
        let result = charge
            .split(FEE)
            .map_err(BodyMutationError::Invalid)?
            .retain(ModuleStateFamilyQualificationV1 {
                roots: roots.digest()?,
                states: counts.states,
                logical_bytes: counts.logical_bytes,
                class: self.class,
            });
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(result)
    }
}
pub fn readmit_module_state_family_ordinary_v1(
    source: &ReservedValue<ModuleStateFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<ModuleStateFamilyQualificationV1>, BodyMutationError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < FEE
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let mut charge = memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    source.validate_binding(roots)?;
    let result = charge
        .split(FEE)
        .map_err(BodyMutationError::Invalid)?
        .retain(ModuleStateFamilyQualificationV1 {
            roots: source.roots,
            states: source.states,
            logical_bytes: source.logical_bytes,
            class: MemoryClass::Ordinary,
        });
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(result)
}

/// Replays generic mutations, then validates only actual dedicated writes.
pub fn accept_module_state_transition_v1<R: MerkleNodeReader, V: ValueNodeReader>(
    parent: &ReservedValue<ModuleStateFamilyQualificationV1>,
    record: &oasis7_formal_protocol::incremental_execution::TransitionRecordV1,
    verifier: &oasis7_formal_protocol::incremental_execution::TransitionVerifier<'_, R>,
    ctx: &mut ModuleStateRecoveryContextV1<'_, R, V>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<ModuleStateFamilyQualificationV1>, BodyMutationError> {
    if ctx.class != MemoryClass::Ordinary {
        return Err(invalid());
    }
    {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !parent.uses_budget(ctx.memory)
            || parent.reserved_bytes() < FEE
            || parent.class != ctx.class
            || verifier.class != ctx.class
            || !verifier.memory.same_pool(ctx.memory)
            || ctx.root != record.after.state_root(StateNamespaceV1::Business)
        {
            return Err(invalid());
        }
        parent.validate_binding(&record.before)?;
        let _charge = ctx
            .memory
            .reserve(128 << 10, ctx.class)
            .map_err(ValueTreeError::Memory)?;
    }
    let witness = {
        let mut verify =
            crate::incremental_verified_world_transition::WorldTransitionVerificationContextV1 {
                after_root: ctx.root,
                memory: ctx.memory,
                class: ctx.class,
                take_state_error: || None,
                cancelled: &mut cancelled,
            };
        crate::incremental_verified_world_transition::verify_world_transition_in_v1(verifier,record,&mut verify).map_err(|e|match e {crate::incremental_verified_world_transition::WorldTransitionVerificationErrorV1::Execution(e)=>BodyMutationError::from(e),crate::incremental_verified_world_transition::WorldTransitionVerificationErrorV1::Value(e)=>BodyMutationError::from(e)})?
    };
    accept_module_state_verified_v1(parent, &witness, ctx, cancelled)
}

pub(crate) fn accept_module_state_verified_v1<R: MerkleNodeReader, V: ValueNodeReader>(
    parent: &ReservedValue<ModuleStateFamilyQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut ModuleStateRecoveryContextV1<'_, R, V>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<ModuleStateFamilyQualificationV1>, BodyMutationError> {
    if ctx.class != MemoryClass::Ordinary {
        return Err(invalid());
    }
    witness.check_context(ctx.memory, ctx.class, ctx.root)?;
    let record = witness.record();

    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !parent.uses_budget(ctx.memory)
        || parent.reserved_bytes() < FEE
        || parent.class != ctx.class
        || ctx.root != record.after.state_root(StateNamespaceV1::Business)
    {
        return Err(invalid());
    }
    parent.validate_binding(&record.before)?;
    let mut charge = ctx
        .memory
        .reserve(128 << 10, ctx.class)
        .map_err(ValueTreeError::Memory)?;

    let mut states = i128::from(parent.states);
    let mut bytes = i128::from(parent.logical_bytes);
    for write in &record.writes {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !ModuleStateKeyV1::is_state_key(&write.mutation.key) {
            continue;
        }
        if write.namespace != StateNamespaceV1::Business {
            return Err(invalid());
        }
        ctx.content.states_remaining =
            ctx.content
                .states_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "module state changed count",
                ))?;
        ModuleStateKeyV1::state_key_from_bytes(&write.mutation.key)
            .map_err(BodyMutationError::Invalid)?;
        for (id, sign) in [(write.mutation.before, -1i128), (write.mutation.after, 1)] {
            if let Some(id) = id {
                let mut read = oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: ctx.values,
                    memory: ctx.memory,
                    class: ctx.class,
                    io: ctx.value_io,
                };
                let root = read.load_root(&id)?;
                let length = root.0.logical_len;
                if length > ctx.content.max_state_bytes
                    || length > ctx.content.logical_bytes_remaining
                {
                    return Err(
                        ValueTreeError::ResourceLimited("module state changed bytes").into(),
                    );
                }
                ctx.content.logical_bytes_remaining -= length;
                // Parent content is inherited; new opaque bytes need full closure.
                if sign > 0 {
                    read.verify_closure(&id, ctx.content.max_state_bytes, &mut cancelled)?;
                }
                states += sign;
                bytes += sign * i128::from(length);
            }
        }
    }
    let output = charge
        .split(FEE)
        .map_err(BodyMutationError::Invalid)?
        .retain(ModuleStateFamilyQualificationV1 {
            roots: record.after.digest()?,
            states: u64::try_from(states).map_err(|_| invalid())?,
            logical_bytes: u64::try_from(bytes).map_err(|_| invalid())?,
            class: ctx.class,
        });
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(output)
}
