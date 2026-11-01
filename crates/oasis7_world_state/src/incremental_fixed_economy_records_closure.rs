use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateRootV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
#[derive(Debug)]
pub struct FixedEconomyRecordQualificationV1 {
    field: FixedEconomyFieldV1,
    root: StateRootV1,
}
impl FixedEconomyRecordQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn field(&self) -> FixedEconomyFieldV1 {
        self.field
    }
    pub fn validate_binding(
        &self,
        field: FixedEconomyFieldV1,
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.field != field || self.root != *root || root.namespace != field.namespace() {
            Err(read::invalid())
        } else {
            Ok(())
        }
    }
}
pub(super) fn mint(
    field: FixedEconomyFieldV1,
    root: StateRootV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<FixedEconomyRecordQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(FixedEconomyRecordQualificationV1 { field, root }))
}
/// One exact child only. This does not authenticate the owning RootSet/family.
pub fn qualify_fixed_economy_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: FixedEconomyFieldV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
) -> Result<ReservedValue<FixedEconomyRecordQualificationV1>, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != field.namespace() {
        return Err(read::invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _scratch = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let meta = WorldFieldKeyV1::new(field.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let meta_root = ctx.lookup(&meta)?.ok_or_else(read::invalid)?;
    let mut keys = std::array::from_fn::<_, 16, _>(|_| None);
    let mut roots = [None; 16];
    for (i, column) in std::iter::once(None)
        .chain(
            FixedEconomyColumnV1::ALL
                .into_iter()
                .filter(|column| column.field() == field)
                .map(Some),
        )
        .enumerate()
    {
        let key = read::cell_key(field, column, &mut ctx.records())?;
        roots[i] = Some(ctx.lookup(&key.0)?.ok_or_else(read::invalid)?);
        keys[i] = Some((column, key));
    }
    if ctx.root.leaf_count != 1 + keys.iter().filter(|key| key.is_some()).count() as u64 {
        return Err(read::invalid());
    }
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: ctx.root,
        state: ctx.state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: || (cancel.borrow_mut())(),
    };
    let result = StateScanContext {
        source: ctx.state,
        memory: local.values.memory,
        class: local.values.class,
        io: scan,
    }
    .visit_entries(
        ctx.root,
        |entry| {
            *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
                "fixed economy closure cells",
            ))?;
            if entry.namespace != field.namespace() {
                return Err(read::invalid());
            }
            if entry.key == meta.as_bytes() {
                if entry.value != meta_root {
                    return Err(read::invalid());
                }
                return local.records().presence(&entry.value, &meta, 0);
            }
            let (i, (column, key)) = keys
                .iter()
                .enumerate()
                .find_map(|(i, key)| {
                    key.as_ref()
                        .filter(|(_, key)| key.0.as_bytes() == entry.key)
                        .map(|key| (i, key))
                })
                .ok_or_else(read::invalid)?;
            if roots[i] != Some(entry.value) {
                return Err(read::invalid());
            }
            read::value(*column, &entry.value, key, &mut local.records())?;
            Ok(())
        },
        || (cancel.borrow_mut())(),
    );
    let error = (local.take_state_error)();
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = error {
        return Err(error);
    }
    match result {
        Ok(_) => {}
        Err(StateScanError::Visitor(error)) => return Err(error),
        Err(StateScanError::Memory(error)) => return Err(error.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited(
                "fixed economy closure scan",
            ));
        }
        Err(_) => return Err(read::invalid()),
    };
    Ok(retained.retain(FixedEconomyRecordQualificationV1 {
        field,
        root: ctx.root.clone(),
    }))
}
