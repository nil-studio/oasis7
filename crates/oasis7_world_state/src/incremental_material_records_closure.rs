//! Constant-workspace qualification of the two closed fixed-column record trees.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateRootV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::RefCell;
#[derive(Debug)]
pub struct MaterialRecordQualificationV1 {
    schema: [u8; 32],
    owner: [u8; 32],
    root: StateRootV1,
}
impl MaterialRecordQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn validate_binding(
        &self,
        kind: MaterialRecordKindV1,
        owner: &[u8; 32],
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.schema != kind.schema() || self.owner != *owner || self.root != *root {
            return Err(invalid());
        }
        Ok(())
    }
}
pub(super) fn mint(
    kind: MaterialRecordKindV1,
    owner: [u8; 32],
    root: StateRootV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
) -> Result<ReservedValue<MaterialRecordQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(MaterialRecordQualificationV1 {
            schema: kind.schema(),
            owner,
            root,
        }))
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("material record closure")
}
fn slot(cell: MaterialCellV1) -> usize {
    MaterialCellV1::ALL
        .iter()
        .position(|v| *v == cell)
        .expect("closed material cell")
}
/// Root-only bootstrap/recovery. This qualifies one exact child root only;
/// a complete family/World receiver must independently bind its outer edge.
pub fn qualify_material_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: MaterialRecordKindV1,
    owner: [u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
) -> Result<ReservedValue<MaterialRecordQualificationV1>, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != kind.namespace() {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _workspace = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let meta = WorldFieldKeyV1::new(kind.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let meta_root = ctx.lookup(&meta)?.ok_or_else(invalid)?;
    let mut keys: [Option<(WorldFieldKeyV1, u64)>; 17] = std::array::from_fn(|_| None);
    let mut roots = [None; 17];
    for (index, cell) in MaterialCellV1::ALL.into_iter().enumerate() {
        if !cell.accepts(kind) {
            continue;
        }
        let key = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            read::cell_key(kind, cell, &mut records)?
        };
        roots[index] = ctx.lookup(&key.0)?;
        if cell != MaterialCellV1::RouteId && roots[index].is_none() {
            return Err(invalid());
        }
        keys[index] = Some(key);
    }
    if kind == MaterialRecordKindV1::Transfer {
        let index = slot(MaterialCellV1::RoutePresent);
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let read::MaterialValueV1::Bool(present) = read::value(
            MaterialCellV1::RoutePresent,
            &roots[index].ok_or_else(invalid)?,
            keys[index].as_ref().unwrap(),
            &mut records,
            1,
            &mut std::io::sink(),
        )?
        else {
            return Err(invalid());
        };
        if present != roots[slot(MaterialCellV1::RouteId)].is_some() {
            return Err(invalid());
        }
    }
    let expected = 1 + roots.iter().filter(|root| root.is_some()).count() as u64;
    if ctx.root.leaf_count != expected {
        return Err(invalid());
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
            *cells = cells
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("material closure cells"))?;
            if entry.namespace != kind.namespace() {
                return Err(invalid());
            }
            let mut records = InventoryValueReader {
                values: &mut local.values,
                key_work: local.key_work,
                semantic_bytes: local.semantic_bytes,
                record_bytes: local.record_bytes,
                cancelled: &mut local.cancelled,
            };
            if entry.key == meta.as_bytes() {
                if entry.value != meta_root {
                    return Err(invalid());
                }
                return records.presence(&entry.value, &meta, 0);
            }
            let (index, key) = keys
                .iter()
                .enumerate()
                .find_map(|(i, key)| {
                    key.as_ref()
                        .filter(|(k, _)| k.as_bytes() == entry.key)
                        .map(|key| (i, key))
                })
                .ok_or_else(invalid)?;
            if roots[index] != Some(entry.value) {
                return Err(invalid());
            }
            read::value(
                MaterialCellV1::ALL[index],
                &entry.value,
                key,
                &mut records,
                max_payload,
                &mut std::io::sink(),
            )?;
            Ok(())
        },
        || (cancel.borrow_mut())(),
    );
    let original = (local.take_state_error)();
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = original {
        return Err(error);
    }
    match result {
        Ok(_) => {}
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited("material closure scan"));
        }
        Err(_) => return Err(invalid()),
    }
    Ok(retained.retain(MaterialRecordQualificationV1 {
        schema: kind.schema(),
        owner,
        root: ctx.root.clone(),
    }))
}
