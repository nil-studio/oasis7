//! Bounded typed column recovery shared by closed owner-selected schemas.
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{
        MerkleNodeReader, StateRootV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryReservation;
use std::{cell::RefCell, collections::BTreeMap};
type ColumnReferenceSchemaV1 = fn(&ColumnKeyV1) -> Option<[u8; 32]>;
#[derive(Clone, Copy)]
pub(crate) struct ColumnReadProfileV1 {
    pub field: FieldDescriptor,
    pub schema: [u8; 32],
    pub groups: &'static [&'static str],
    pub reference_schema: Option<ColumnReferenceSchemaV1>,
}
pub struct RecoveredColumnsV1<T> {
    value: T,
    _extra: Vec<MemoryReservation>,
    _charge: MemoryReservation,
}
impl<T> RecoveredColumnsV1<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
}
pub(crate) struct ChargedColumnsV1 {
    cells: BTreeMap<ColumnKeyV1, ColumnCellV1>,
    charge: MemoryReservation,
    left: usize,
}
impl ChargedColumnsV1 {
    pub(crate) fn finish_parts(self) -> Result<MemoryReservation, ValueTreeError> {
        if !self.cells.is_empty() || self.left > self.charge.bytes() {
            return Err(invalid());
        }
        let Self {
            cells,
            mut charge,
            left,
        } = self;
        drop(cells);
        let used = charge.bytes() - left;
        let retained = charge.split(used).map_err(ValueTreeError::Invalid)?;
        drop(charge);
        Ok(retained)
    }
    pub(crate) fn spend_output(&mut self, n: usize) -> Result<(), ValueTreeError> {
        spend(&mut self.left, n)
    }

    pub(crate) fn cells_mut(&mut self) -> &mut BTreeMap<ColumnKeyV1, ColumnCellV1> {
        &mut self.cells
    }
    pub(crate) fn finish<T>(
        self,
        value: T,
        extra: Vec<MemoryReservation>,
    ) -> Result<RecoveredColumnsV1<T>, ValueTreeError> {
        if !self.cells.is_empty() || self.left > self.charge.bytes() {
            drop(value);
            drop(extra);
            return Err(invalid());
        }
        let Self {
            cells,
            mut charge,
            left,
        } = self;
        drop(cells);
        let used = charge.bytes() - left;
        let retained = match charge.split(used) {
            Ok(v) => v,
            Err(e) => {
                drop(value);
                drop(extra);
                return Err(ValueTreeError::Invalid(e));
            }
        };
        drop(charge);
        Ok(RecoveredColumnsV1 {
            value,
            _charge: retained,
            _extra: extra,
        })
    }
}
pub(crate) fn read_columns_v1<M, R, E, C>(
    profile: ColumnReadProfileV1,
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    max_heap: usize,
) -> Result<ChargedColumnsV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let charge = ctx.values.memory.reserve(max_heap, ctx.values.class)?;
    let mut left = max_heap;
    spend(&mut left, 1024)?;
    let cells = collect(profile, root, ctx, scan, &mut left)?;
    Ok(ChargedColumnsV1 {
        cells,
        charge,
        left,
    })
}
pub(crate) enum ColumnCellV1 {
    Unit,
    Bool(bool),
    U64(u64),
    I64(i64),
    Text(String),
    Root(StateRootV1),
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("closed typed column recovery")
}
fn spend(left: &mut usize, n: usize) -> Result<(), ValueTreeError> {
    *left = left.checked_sub(n).ok_or(ValueTreeError::ResourceLimited(
        "authorization selected DTO heap",
    ))?;
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ColumnKeyV1 {
    Text(String),
    Ordinal(u64),
    Pair(Box<Self>, Box<Self>),
}
impl ColumnKeyV1 {
    pub(crate) fn text(v: &str) -> Self {
        Self::Text(v.into())
    }
    pub(crate) fn pair(a: Self, b: Self) -> Self {
        Self::Pair(Box::new(a), Box::new(b))
    }
}
fn path(
    key: SemanticKeyRefV1<'_>,
    depth: usize,
    left: &mut usize,
) -> Result<ColumnKeyV1, ValueTreeError> {
    if depth > 8 {
        return Err(invalid());
    }
    // Each returned enum lives in its parent's Box (or the root map slot).
    // Charge its actual metadata before constructing either child Box.
    spend(left, std::mem::size_of::<ColumnKeyV1>())?;
    match key {
        SemanticKeyRefV1::Text(v) => {
            let allowance = v
                .len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(128))
                .ok_or_else(invalid)?;
            spend(left, allowance)?;
            let text = v.to_owned();
            if text.capacity() > allowance {
                return Err(invalid());
            }
            Ok(ColumnKeyV1::Text(text))
        }
        SemanticKeyRefV1::Unsigned(v) => Ok(ColumnKeyV1::Ordinal(v)),
        SemanticKeyRefV1::Pair { left: a, right: b } => {
            let a = path(
                a.decode().map_err(ValueTreeError::Invalid)?,
                depth + 1,
                left,
            )?;
            let b = path(
                b.decode().map_err(ValueTreeError::Invalid)?,
                depth + 1,
                left,
            )?;
            Ok(ColumnKeyV1::pair(a, b))
        }
        _ => Err(invalid()),
    }
}

fn collect<M, R, E, C>(
    profile: ColumnReadProfileV1,
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    left: &mut usize,
) -> Result<BTreeMap<ColumnKeyV1, ColumnCellV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if profile.groups.len() > 64
        || profile.field.destination != FieldDestination::State(root.namespace)
    {
        return Err(invalid());
    }
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut map = BTreeMap::new();
    let mut metadata = 0u8;
    let result = StateScanContext {
        source: ctx.state,
        memory: ctx.values.memory,
        class: ctx.values.class,
        io: scan,
    }
    .visit_entries(
        root,
        |entry| {
            let parsed =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            let key = WorldFieldKeyV1::new(profile.field, parsed.kind, parsed.semantic_key)
                .map_err(ValueTreeError::Invalid)?;
            if key.as_bytes() != entry.key || entry.namespace != root.namespace {
                return Err(invalid());
            }
            let mut stop = || (cancel.borrow_mut())();
            if parsed.kind == WorldFieldRecordKindV1::Metadata {
                ctx.values
                    .verify_closure(&entry.value, ctx.record_bytes, &mut stop)?;
                if parsed.semantic_key.is_some() || metadata != 0 {
                    return Err(invalid());
                }
                crate::incremental_inventory_value_read::InventoryValueReader {
                    values: &mut ctx.values,
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    cancelled: &mut stop,
                }
                .presence(&entry.value, &key, 0)?;
                metadata = 1;
                return Ok(());
            }
            if parsed.kind == WorldFieldRecordKindV1::Group {
                ctx.values
                    .verify_closure(&entry.value, ctx.record_bytes, &mut stop)?;
                let mut matched = None;
                for name in profile.groups {
                    let expected = crate::incremental_inventory_value_read::InventoryValueReader {
                        values: &mut ctx.values,
                        key_work: &mut *ctx.key_work,
                        semantic_bytes: ctx.semantic_bytes,
                        record_bytes: ctx.record_bytes,
                        cancelled: &mut stop,
                    }
                    .key(profile.field, WorldFieldRecordKindV1::Group, name)?;
                    if expected.0.as_bytes() == key.as_bytes() {
                        matched = Some((*name, expected.1));
                        break;
                    }
                }
                let (name, semantic_bytes) = matched.ok_or_else(invalid)?;
                crate::incremental_inventory_value_read::InventoryValueReader {
                    values: &mut ctx.values,
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    cancelled: &mut stop,
                }
                .presence(&entry.value, &key, semantic_bytes)?;
                spend(left, name.len().checked_add(512).ok_or_else(invalid)?)?;
                if map
                    .insert(
                        ColumnKeyV1::text(&format!("{name}.group")),
                        ColumnCellV1::Unit,
                    )
                    .is_some()
                {
                    return Err(invalid());
                }
                return Ok(());
            }
            if parsed.kind != WorldFieldRecordKindV1::Entry {
                return Err(invalid());
            }
            let row = row::read_row_into(
                profile,
                entry,
                &mut crate::incremental_inventory_value_read::InventoryValueReader {
                    values: &mut ctx.values,
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    cancelled: &mut stop,
                },
                left,
            )?;
            let ColumnRowV1 { key: name, cell } = row;
            if map.insert(name, cell).is_some() {
                return Err(invalid());
            }
            Ok(())
        },
        || (cancel.borrow_mut())(),
    );
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(e) = (ctx.take_state_error)() {
        return Err(e);
    }
    match result {
        Ok(_) => (),
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited("authorization DTO scan"));
        }
        Err(_) => return Err(invalid()),
    }
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if metadata != 1 {
        return Err(invalid());
    }
    Ok(map)
}
pub(crate) fn take(
    map: &mut BTreeMap<ColumnKeyV1, ColumnCellV1>,
    key: &str,
) -> Result<ColumnCellV1, ValueTreeError> {
    map.remove(&ColumnKeyV1::text(key)).ok_or_else(invalid)
}
pub(crate) fn text(
    map: &mut BTreeMap<ColumnKeyV1, ColumnCellV1>,
    key: &str,
) -> Result<String, ValueTreeError> {
    match take(map, key)? {
        ColumnCellV1::Text(v) => Ok(v),
        _ => Err(invalid()),
    }
}
pub(crate) fn unsigned(
    map: &mut BTreeMap<ColumnKeyV1, ColumnCellV1>,
    key: &str,
) -> Result<u64, ValueTreeError> {
    match take(map, key)? {
        ColumnCellV1::U64(v) => Ok(v),
        _ => Err(invalid()),
    }
}
pub(crate) fn signed(
    map: &mut BTreeMap<ColumnKeyV1, ColumnCellV1>,
    key: &str,
) -> Result<i64, ValueTreeError> {
    match take(map, key)? {
        ColumnCellV1::I64(v) => Ok(v),
        _ => Err(invalid()),
    }
}
pub(crate) fn present(
    map: &mut BTreeMap<ColumnKeyV1, ColumnCellV1>,
    key: &str,
) -> Result<bool, ValueTreeError> {
    match map
        .remove(&ColumnKeyV1::pair(
            ColumnKeyV1::text(key),
            ColumnKeyV1::text("present"),
        ))
        .ok_or_else(invalid)?
    {
        ColumnCellV1::Bool(v) => Ok(v),
        _ => Err(invalid()),
    }
}
pub(crate) fn optional(
    map: &mut BTreeMap<ColumnKeyV1, ColumnCellV1>,
    key: &str,
) -> Result<Option<ColumnCellV1>, ValueTreeError> {
    if present(map, key)? {
        Ok(Some(
            map.remove(&ColumnKeyV1::pair(
                ColumnKeyV1::text(key),
                ColumnKeyV1::text("value"),
            ))
            .ok_or_else(invalid)?,
        ))
    } else {
        Ok(None)
    }
}
pub(crate) fn opt_text(
    map: &mut BTreeMap<ColumnKeyV1, ColumnCellV1>,
    key: &str,
) -> Result<Option<String>, ValueTreeError> {
    match optional(map, key)? {
        Some(ColumnCellV1::Text(v)) => Ok(Some(v)),
        None => Ok(None),
        _ => Err(invalid()),
    }
}
pub(crate) fn opt_unsigned(
    map: &mut BTreeMap<ColumnKeyV1, ColumnCellV1>,
    key: &str,
) -> Result<Option<u64>, ValueTreeError> {
    match optional(map, key)? {
        Some(ColumnCellV1::U64(v)) => Ok(Some(v)),
        None => Ok(None),
        _ => Err(invalid()),
    }
}
pub(crate) fn opt_signed(
    map: &mut BTreeMap<ColumnKeyV1, ColumnCellV1>,
    key: &str,
) -> Result<Option<i64>, ValueTreeError> {
    match optional(map, key)? {
        Some(ColumnCellV1::I64(v)) => Ok(Some(v)),
        None => Ok(None),
        _ => Err(invalid()),
    }
}

#[cfg(test)]
#[path = "incremental_record_columns_read_tests.rs"]
mod tests;

#[path = "incremental_record_columns_row.rs"]
mod row;
pub(crate) use row::{ColumnRowV1, parse_column_key_v1, read_column_key_v1, read_column_row_v1};
