//! Changed-write instance closure; unchanged scalar payloads inherit the parent proof.
use super::{
    incremental_body::BodyMutationError,
    incremental_module_instance_family::{self as family, ModuleInstanceFamilyQualificationV1},
    incremental_module_instance_recovery::{self as recovery, ModuleInstanceRecoveryContext},
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_scratch_counter::CounterScratchStore,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_module_instance_fields::{
        ModuleInstanceFieldV1 as Field, ModuleInstanceKeyV1 as Key,
    },
    incremental_module_instance_index::*,
};
use std::cell::RefCell;

fn step(work: &mut u64) -> Result<(), BodyMutationError> {
    *work = work.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
        "instance changed relations",
    ))?;
    Ok(())
}
struct Check<'a, 'b, R, V, S, C> {
    ctx: &'a mut ModuleInstanceRecoveryContext<'b, R, V, S>,
    cancelled: RefCell<C>,
    work: &'a mut u64,
}
impl<R: MerkleNodeReader, V: ValueNodeReader, S: CounterScratchStore, C: FnMut() -> bool>
    Check<'_, '_, R, V, S, C>
{
    fn point(
        &mut self,
        roots: &RootSetV1,
        key: &[u8],
    ) -> Result<Option<[u8; 32]>, BodyMutationError> {
        step(self.work)?;
        recovery::point(
            roots.state_root(StateNamespaceV1::Business),
            key,
            self.ctx.state,
            &RefCell::new(&mut *self.ctx.scratch_io),
            &self.cancelled,
        )
    }
    fn load(&mut self, id: &[u8; 32]) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        step(self.work)?;
        recovery::load(
            id,
            self.ctx.values,
            self.ctx.memory,
            self.ctx.class,
            self.ctx.value_io,
            self.ctx.max_scalar_bytes,
            &self.cancelled,
        )
    }
    fn count(&mut self, roots: &RootSetV1, key: &[u8]) -> Result<u64, BodyMutationError> {
        let Some(id) = self.point(roots, key)? else {
            return Ok(0);
        };
        let bytes = self.load(&id)?;
        module_instance_count_from_bytes(&bytes).map_err(BodyMutationError::Invalid)
    }
    fn scalar(
        &mut self,
        id: &[u8; 32],
        field: Field,
    ) -> Result<Option<[u8; MODULE_INSTANCE_COUNT_KEY_BYTES]>, BodyMutationError> {
        step(self.work)?;
        recovery::validate_scalar(
            id,
            self.ctx.values,
            self.ctx.memory,
            self.ctx.class,
            self.ctx.value_io,
            self.ctx.max_scalar_bytes,
            &self.cancelled,
            field,
        )
    }
    // Bounded by changed writes, never by the unchanged family size. This
    // no-growing-map implementation explicitly charges the O(delta^2) joins.
    fn net(&mut self, record: &TransitionRecordV1, key: &[u8]) -> Result<i128, BodyMutationError> {
        let mut net = 0i128;
        for write in &record.writes {
            step(self.work)?;
            if !Key::is_instance_key(&write.mutation.key) {
                continue;
            }
            let (_, field) =
                Key::from_bytes(&write.mutation.key).map_err(BodyMutationError::Invalid)?;
            if field != Field::ModuleId {
                continue;
            }
            for (id, sign) in [(write.mutation.before, -1i128), (write.mutation.after, 1)] {
                if let Some(id) = id {
                    let module_key = self.scalar(&id, field)?.ok_or_else(family::invalid)?;
                    if module_key.as_slice() == key {
                        net += sign;
                    }
                }
            }
        }
        Ok(net)
    }
    fn count_relation(
        &mut self,
        record: &TransitionRecordV1,
        key: &[u8],
    ) -> Result<(), BodyMutationError> {
        validate_module_instance_count_key(key).map_err(BodyMutationError::Invalid)?;
        let old = self.count(&record.before, key)?;
        let new = self.count(&record.after, key)?;
        if i128::from(new) - i128::from(old) != self.net(record, key)? {
            return Err(family::invalid());
        }
        Ok(())
    }
}

/// Independent replay followed by owner closure of actual writes. Unchanged
/// fields/rows are inherited only from the exact same-pool/class parent proof.
pub fn accept_module_instance_transition_v1<R, V, S, C>(
    parent: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    record: &TransitionRecordV1,
    verifier: &TransitionVerifier<'_, R>,
    ctx: &mut ModuleInstanceRecoveryContext<'_, R, V, S>,
    relation_work: &mut u64,
    mut cancelled: C,
) -> Result<ReservedValue<ModuleInstanceFamilyQualificationV1>, BodyMutationError>
where
    R: MerkleNodeReader,
    V: ValueNodeReader,
    S: CounterScratchStore,
    C: FnMut() -> bool,
{
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !parent.uses_budget(ctx.memory)
        || parent.reserved_bytes() < 1024
        || parent.allocation_class() != ctx.class
        || verifier.class != ctx.class
        || !verifier.memory.same_pool(ctx.memory)
        || ctx.root != record.after.state_root(StateNamespaceV1::Business)
    {
        return Err(family::invalid());
    }
    parent.validate_binding(&record.before)?;
    let charge = ctx
        .memory
        .reserve(128 << 10, ctx.class)
        .map_err(ValueTreeError::Memory)?;
    let generic = verifier.verify(record)?;
    drop(generic);
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let mut check = Check {
        ctx,
        cancelled: RefCell::new(cancelled),
        work: relation_work,
    };
    let mut rows = i128::from(parent.instance_count());
    let mut modules = i128::from(parent.module_count());
    for write in &record.writes {
        step(check.work)?;
        let mutation = &write.mutation;
        if !family::owns(&mutation.key) {
            continue;
        }
        if write.namespace != StateNamespaceV1::Business {
            return Err(family::invalid());
        }
        if mutation.key == MODULE_INSTANCE_INDEX_KEY_V1 {
            if mutation.after.is_none() {
                return Err(family::invalid());
            }
        } else if Key::is_instance_key(&mutation.key) {
            let (map_key, field) =
                Key::from_bytes(&mutation.key).map_err(BodyMutationError::Invalid)?;
            if let Some(id) = mutation.after {
                check.scalar(&id, field)?;
            }
            let marker =
                Key::new(map_key, Field::InstanceId).map_err(BodyMutationError::Invalid)?;
            let exists = check.point(&record.after, marker.as_bytes())?.is_some();
            for field in Field::ALL {
                let key = Key::new(map_key, field).map_err(BodyMutationError::Invalid)?;
                if check.point(&record.after, key.as_bytes())?.is_some() != exists {
                    return Err(family::invalid());
                }
            }
            if field == Field::InstanceId {
                rows +=
                    i128::from(mutation.before.is_none()) - i128::from(mutation.after.is_none());
            }
            if field == Field::ModuleId {
                for id in [mutation.before, mutation.after].into_iter().flatten() {
                    let module_key = check.scalar(&id, field)?.ok_or_else(family::invalid)?;
                    check.count_relation(record, &module_key)?;
                }
            }
        } else {
            check.count_relation(record, &mutation.key)?;
            modules += i128::from(mutation.before.is_none()) - i128::from(mutation.after.is_none());
        }
    }
    let index = check
        .point(&record.after, MODULE_INSTANCE_INDEX_KEY_V1)?
        .ok_or_else(family::invalid)?;
    let bytes = check.load(&index)?;
    let actual = module_instance_index_from_bytes(&bytes).map_err(BodyMutationError::Invalid)?;
    if rows != i128::from(actual) {
        return Err(family::invalid());
    }
    let modules = u64::try_from(modules).map_err(|_| family::invalid())?;
    if (check.cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let output = family::mint(
        &record.after,
        actual,
        modules,
        check.ctx.memory,
        check.ctx.class,
        charge,
    )?;
    if (check.cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(output)
}
