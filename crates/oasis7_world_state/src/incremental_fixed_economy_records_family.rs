use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::RefCell;
#[derive(Debug)]
pub struct FixedEconomyFamilyQualificationV1 {
    roots: [u8; 32],
}
impl FixedEconomyFamilyQualificationV1 {
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if self.roots != *roots {
            Err(read::invalid())
        } else {
            Ok(())
        }
    }
}
pub(super) fn mint(
    roots: [u8; 32],
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
) -> Result<ReservedValue<FixedEconomyFamilyQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(FixedEconomyFamilyQualificationV1 { roots }))
}
type Outer = (FixedEconomyFieldV1, Option<ReservedValue<StateRootV1>>);
pub(super) fn outer<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<Outer>, ValueTreeError> {
    if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let Some(field) = FixedEconomyFieldV1::ALL
        .into_iter()
        .find(|field| field.name() == binding.name)
    else {
        return Ok(None);
    };
    if entry.namespace != field.namespace() {
        return Err(read::invalid());
    }
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        let key = WorldFieldKeyV1::new(field.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        if key.as_bytes() != entry.key {
            return Err(read::invalid());
        }
        ctx.records().presence(&entry.value, &key, 0)?;
        return Ok(Some((field, None)));
    }
    let key = read::cell_key(field, None, &mut ctx.records())?;
    if key.0.as_bytes() != entry.key {
        return Err(read::invalid());
    }
    let child = read::outer(field, &key, &entry.value, &mut ctx.records())?;
    ctx.check()?;
    Ok(Some((field, Some(child))))
}
/// Qualifies exactly the four required globals; never a whole-World proof.
pub fn qualify_fixed_economy_family_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
) -> Result<ReservedValue<FixedEconomyFamilyQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| read::invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|root| root == ctx.root) {
        return Err(read::invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let mut metadata = [false; 4];
    let mut records = [false; 4];
    let cancel = RefCell::new(&mut ctx.cancelled);
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
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
            io: &mut *outer_scan,
        }
        .visit_entries(
            root,
            |entry| {
                *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
                    "fixed economy family cells",
                ))?;
                if let Some((field, child)) = outer(entry, &mut local)? {
                    let index = FixedEconomyFieldV1::ALL
                        .iter()
                        .position(|v| *v == field)
                        .ok_or_else(read::invalid)?;
                    if let Some(child) = child {
                        if records[index] {
                            return Err(read::invalid());
                        }
                        records[index] = true;
                        let mut body = WorldFieldReadContextV1 {
                            root: &child,
                            state: local.state,
                            values: ValueReadContext {
                                source: local.values.source,
                                memory: local.values.memory,
                                class: local.values.class,
                                io: &mut *local.values.io,
                            },
                            key_work: &mut *local.key_work,
                            semantic_bytes: local.semantic_bytes,
                            record_bytes: local.record_bytes,
                            take_state_error: &mut local.take_state_error,
                            cancelled: &mut local.cancelled,
                        };
                        qualify_fixed_economy_body_v1(field, &mut body, inner_scan, cells)?;
                    } else {
                        if metadata[index] {
                            return Err(read::invalid());
                        }
                        metadata[index] = true;
                    }
                }
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
                return Err(ValueTreeError::ResourceLimited("fixed economy family scan"));
            }
            Err(_) => return Err(read::invalid()),
        };
    }
    if metadata != [true; 4] || records != [true; 4] {
        return Err(read::invalid());
    }
    let digest = roots.digest().map_err(|_| read::invalid())?;
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(FixedEconomyFamilyQualificationV1 { roots: digest }))
}
pub fn open_qualified_fixed_economy_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: FixedEconomyFieldV1,
    roots: &RootSetV1,
    family: &ReservedValue<FixedEconomyFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<ReservedValue<FixedEconomyRecordQualificationV1>, ValueTreeError> {
    ctx.check()?;
    if !family.uses_budget(ctx.values.memory) {
        return Err(read::invalid());
    }
    family.validate_binding(&roots.digest().map_err(|_| read::invalid())?)?;
    let top = roots
        .state
        .iter()
        .find(|root| root.namespace == field.namespace())
        .ok_or_else(read::invalid)?;
    if ctx.root != top {
        return Err(read::invalid());
    }
    let child = inspect_fixed_economy_root_v1(field, ctx)?;
    let result = closure::mint(field, child.clone(), ctx.values.memory, ctx.values.class)?;
    ctx.check()?;
    Ok(result)
}
