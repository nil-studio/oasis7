//! Independent pending structural/original evidence; no execution authority.
use crate::{
    incremental_pending_action_index::*,
    incremental_pending_action_original_binding::{
        PendingActionOriginalErrorV1, PendingActionOriginalWorkV1,
        verify_pending_action_original_v1,
    },
    incremental_runtime_pending_action_record::{
        PendingActionPositionV1 as Position, recover_pending_action_record_v1,
    },
    incremental_runtime_pending_actions::PENDING_ACTIONS_FIELD_V1,
    incremental_typed_record_codec::*,
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{StateScanContext, StateScanError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::cell::RefCell;
#[path = "incremental_pending_action_family_key.rs"]
mod key;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("pending action family")
}
pub struct PendingActionFamilyWorkV1 {
    pub index: PendingActionIndexWorkV1,
    pub typed: TypedRecordReadWorkV1,
    pub original: PendingActionOriginalWorkV1,
    pub selected_scan: StateScanBudget,
}
pub struct PendingActionQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    index: PendingActionIndexV1,
}
impl PendingActionQualificationV1 {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
    pub(crate) fn index(&self) -> &PendingActionIndexV1 {
        &self.index
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
#[derive(Debug)]
pub enum PendingActionFamilyErrorV1<E> {
    Value(ValueTreeError),
    Original(PendingActionOriginalErrorV1<E>),
    Overlay(crate::incremental_field_overlay::WorldFieldOverlayErrorV1),
}
impl<E> From<ValueTreeError> for PendingActionFamilyErrorV1<E> {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(e) = (ctx.take_state_error)() {
        return Err(e);
    }
    Ok(())
}
fn mint<E>(
    roots: &RootSetV1,
    index: PendingActionIndexV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> Result<ReservedValue<PendingActionQualificationV1>, PendingActionFamilyErrorV1<E>> {
    let retained = memory
        .reserve(512, index.class())
        .map_err(ValueTreeError::Memory)?;
    let digest = roots.digest().map_err(|_| invalid())?;
    Ok(retained.retain(PendingActionQualificationV1 {
        roots: digest,
        class: index.class(),
        index,
    }))
}
fn load<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
>(
    p: Position,
    value: [u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut PendingActionFamilyWorkV1,
    objects: &mut O,
) -> Result<PendingActionMetaV1, PendingActionFamilyErrorV1<O::Error>> {
    work.index.step(&mut ctx.cancelled)?;
    let record =
        recover_pending_action_record_v1(p, ctx, &mut work.selected_scan, &mut work.typed)?
            .ok_or_else(invalid)?;
    let checked = verify_pending_action_original_v1(
        &record,
        objects,
        ctx.values.memory,
        &mut work.original,
        &mut ctx.cancelled,
    )
    .map_err(PendingActionFamilyErrorV1::Original)?;
    Ok(PendingActionMetaV1 {
        position: p,
        action_id: record.value().envelope().id,
        original: checked.original(),
        value_root: value,
    })
}
pub fn qualify_pending_actions_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut PendingActionFamilyWorkV1,
    objects: &mut O,
) -> Result<ReservedValue<PendingActionQualificationV1>, PendingActionFamilyErrorV1<O::Error>> {
    check(ctx)?;
    if !objects.allocation_budget().same_pool(ctx.values.memory)
        || objects.allocation_class() != ctx.values.class
    {
        return Err(invalid().into());
    }
    let _phase = ctx
        .values
        .memory
        .reserve(64 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    roots.validate().map_err(|_| invalid())?;
    if ctx.root != roots.state_root(StateNamespaceV1::Scheduler) {
        return Err(invalid().into());
    }
    let mut index = PendingActionIndexV1::new(ctx.values.memory, ctx.values.class);
    let mut seen = false;
    let stop = RefCell::new(&mut ctx.cancelled);
    let errors = RefCell::new(&mut ctx.take_state_error);
    for root in &roots.state {
        let mut child = WorldFieldReadContextV1 {
            root: ctx.root,
            state: ctx.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: || (errors.borrow_mut())(),
            cancelled: || (stop.borrow_mut())(),
        };
        // Scanner IO and selected-record IO share one continuous allowance through a cell.

        let result = StateScanContext {
            source: child.state,
            memory: child.values.memory,
            class: child.values.class,
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |row| -> Result<(), PendingActionFamilyErrorV1<O::Error>> {
                let Some(kind) = key::identify(row.namespace, &row.key)? else {
                    return Ok(());
                };
                work.index.step(&mut child.cancelled)?;
                if kind == WorldFieldRecordKindV1::Metadata {
                    if seen {
                        return Err(invalid().into());
                    }
                    seen = true;
                    let k = WorldFieldKeyV1::new(PENDING_ACTIONS_FIELD_V1, kind, None)
                        .map_err(ValueTreeError::Invalid)?;
                    if row.key != k.as_bytes() {
                        return Err(invalid().into());
                    }
                    child.records().presence(&row.value, &k, 0)?;
                    return Ok(());
                }
                let p = key::position(&row.key, &row.value, &mut child)?;
                if index
                    .get(p, &mut work.index, &mut child.cancelled)?
                    .is_some()
                {
                    return Err(invalid().into());
                }
                let meta = load(p, row.value, &mut child, work, objects)?;
                index = index.insert(meta, &mut work.index, &mut child.cancelled)?;
                Ok(())
            },
            || (stop.borrow_mut())(),
        );
        match result {
            Ok(_) => (),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e).into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
            Err(_) => return Err(invalid().into()),
        }
    }
    check(ctx)?;
    if !seen {
        return Err(invalid().into());
    }
    if let Some(max) = index.last(&mut work.index, &mut ctx.cancelled)? {
        if max.position.era > roots.height {
            return Err(invalid().into());
        }
        if roots.height == 0 {
            let first = index
                .first(&mut work.index, &mut ctx.cancelled)?
                .ok_or_else(invalid)?;
            if first.position.era != 0
                || first.position.ordinal != 0
                || max.position.era != 0
                || max.position.ordinal != index.len() as u64 - 1
            {
                return Err(invalid().into());
            }
        }
    }
    let output = mint(roots, index, ctx.values.memory);
    check(ctx)?;
    output
}

pub fn accept_pending_actions_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
>(
    parent: &ReservedValue<PendingActionQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut PendingActionFamilyWorkV1,
    objects: &mut O,
) -> Result<ReservedValue<PendingActionQualificationV1>, PendingActionFamilyErrorV1<O::Error>> {
    check(ctx)?;
    if !objects.allocation_budget().same_pool(ctx.values.memory)
        || objects.allocation_class() != ctx.values.class
    {
        return Err(invalid().into());
    }
    if !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < 512
        || parent.class != ctx.values.class
        || ctx.values.class != MemoryClass::Ordinary
        || !verifier.memory.same_pool(ctx.values.memory)
        || verifier.class != ctx.values.class
    {
        return Err(invalid().into());
    }
    let _phase = ctx
        .values
        .memory
        .reserve(64 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    parent.validate_binding(&record.before)?;
    let replay = verifier.verify(record);
    check(ctx)?;
    let verified = replay.map_err(|e| PendingActionFamilyErrorV1::Overlay(e.into()))?;
    let record = &verified.record;
    if ctx.root != record.after.state_root(StateNamespaceV1::Scheduler) {
        return Err(invalid().into());
    }
    let mut index = parent.index.clone();
    for write in &record.writes {
        check(ctx)?;
        work.index.step(&mut ctx.cancelled)?;
        let Some(kind) = key::identify(write.namespace, &write.mutation.key)? else {
            continue;
        };
        let mutation = &write.mutation;
        if kind == WorldFieldRecordKindV1::Metadata {
            let expected = WorldFieldKeyV1::new(PENDING_ACTIONS_FIELD_V1, kind, None)
                .map_err(ValueTreeError::Invalid)?;
            if mutation.key != expected.as_bytes()
                || mutation.before.is_none()
                || mutation.before != mutation.after
            {
                return Err(invalid().into());
            }
            continue;
        }
        let value = mutation.after.or(mutation.before).ok_or_else(invalid)?;
        let p = key::position(&mutation.key, &value, ctx)?;
        if let Some(before) = mutation.before {
            let old = parent
                .index
                .get(p, &mut work.index, &mut ctx.cancelled)?
                .ok_or_else(invalid)?;
            if old.value_root != before {
                return Err(invalid().into());
            }
            if let Some(after) = mutation.after {
                if after != before {
                    return Err(invalid().into());
                }
            } else {
                index = index.remove(p, &mut work.index, &mut ctx.cancelled)?;
            }
        } else {
            let after = mutation.after.ok_or_else(invalid)?;
            if index.get(p, &mut work.index, &mut ctx.cancelled)?.is_some() {
                return Err(invalid().into());
            }
            if p.era != record.after.height {
                return Err(invalid().into());
            }
            let meta = load(p, after, ctx, work, objects)?;
            index = index.insert(meta, &mut work.index, &mut ctx.cancelled)?;
        }
    }
    check(ctx)?;
    let output = mint(&record.after, index, ctx.values.memory);
    check(ctx)?;
    output
}
pub fn readmit_pending_actions_v1(
    parent: &ReservedValue<PendingActionQualificationV1>,
    roots: &RootSetV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    work: &mut PendingActionIndexWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<PendingActionQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let result = (|| {
        if !parent.uses_budget(memory)
            || parent.reserved_bytes() < 512
            || parent.class != MemoryClass::Recovery
            || parent.index.class() != MemoryClass::Recovery
        {
            return Err(invalid());
        }
        let _phase = memory.reserve(512, MemoryClass::Ordinary)?;
        parent.validate_binding(roots)?;
        let index = parent
            .index
            .readmit(memory, MemoryClass::Ordinary, work, cancelled)?;
        let retained = memory.reserve(512, MemoryClass::Ordinary)?;
        let digest = roots.digest().map_err(|_| invalid())?;
        Ok(retained.retain(PendingActionQualificationV1 {
            roots: digest,
            class: MemoryClass::Ordinary,
            index,
        }))
    })();
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    result
}

#[cfg(test)]
#[path = "incremental_pending_action_family_tests.rs"]
mod tests;

pub use crate::incremental_pending_action_index::{PendingActionIndexWorkV1, PendingActionMetaV1};
impl PendingActionQualificationV1 {
    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn first(
        &self,
        w: &mut PendingActionIndexWorkV1,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Option<PendingActionMetaV1>, ValueTreeError> {
        self.index.first(w, c)
    }
    pub fn last(
        &self,
        w: &mut PendingActionIndexWorkV1,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Option<PendingActionMetaV1>, ValueTreeError> {
        self.index.last(w, c)
    }
    pub fn get(
        &self,
        p: Position,
        w: &mut PendingActionIndexWorkV1,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Option<PendingActionMetaV1>, ValueTreeError> {
        self.index.get(p, w, c)
    }
    pub fn visit_page(
        &self,
        after: Option<Position>,
        limit: usize,
        w: &mut PendingActionIndexWorkV1,
        c: &mut impl FnMut() -> bool,
        emit: impl FnMut(PendingActionMetaV1) -> Result<(), ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        self.index.visit_page(after, limit, w, c, emit)
    }
}

pub use crate::incremental_runtime_pending_action_record::PendingActionPositionV1;
