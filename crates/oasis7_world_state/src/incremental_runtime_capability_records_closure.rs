//! Complete raw child validation with no original record oracle.
use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateRootV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
#[derive(Debug)]
pub struct RuntimeCapabilityRecordQualificationV1 {
    kind: RuntimeCapabilityRecordKindV1,
    owner: [u8; 32],
    root: StateRootV1,
    parent: [u8; 32],
}
impl RuntimeCapabilityRecordQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub(super) fn validate_binding(
        &self,
        kind: RuntimeCapabilityRecordKindV1,
        owner: &[u8; 32],
        root: &StateRootV1,
        parent: &[u8; 32],
    ) -> Result<(), ValueTreeError> {
        if self.kind != kind || self.owner != *owner || &self.root != root || &self.parent != parent
        {
            return Err(read::invalid());
        }
        Ok(())
    }
    pub(super) fn parent(&self) -> &[u8; 32] {
        &self.parent
    }
}
pub(super) fn mint(
    kind: RuntimeCapabilityRecordKindV1,
    owner: [u8; 32],
    root: StateRootV1,
    parent: [u8; 32],
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<RuntimeCapabilityRecordQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(RuntimeCapabilityRecordQualificationV1 {
            kind,
            owner,
            root,
            parent,
        }))
}
pub(super) fn spend(cells: &mut u64) -> Result<(), ValueTreeError> {
    *cells = cells
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("Runtime capability cells"))?;
    Ok(())
}
pub(super) fn subject_kind(bytes: &[u8]) -> Result<u8, ValueTreeError> {
    match bytes {
        b"agent" => Ok(1),
        b"module" => Ok(2),
        b"system" => Ok(4),
        _ => Err(read::invalid()),
    }
}
pub(super) fn body<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: RuntimeCapabilityRecordKindV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != kind.namespace() || ctx.root.leaf_count != kind.leaf_count() {
        return Err(read::invalid());
    }
    let _workspace = ctx.values.memory.reserve(16 << 10, ctx.values.class)?;
    let mut keys = Vec::with_capacity(kind.leaf_count() as usize);
    let meta = WorldFieldKeyV1::new(kind.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    keys.push((meta, None));
    keys.push((read::cell_key(kind, None, &mut ctx.records())?.0, None));
    for column in kind.columns() {
        keys.push((
            read::cell_key(kind, Some(*column), &mut ctx.records())?.0,
            Some(*column),
        ));
    }
    let mut seen = 0u32;
    let mut nonunit = 0u32;
    let mut branch = 0u8;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let result = StateScanContext {
        source: ctx.state,
        memory: ctx.values.memory,
        class: ctx.values.class,
        io: scan,
    }
    .visit_entries(
        ctx.root,
        |entry| -> Result<(), ValueTreeError> {
            spend(cells)?;
            if entry.namespace != kind.namespace() {
                return Err(read::invalid());
            }
            let index = keys
                .iter()
                .position(|(k, _)| k.as_bytes() == entry.key)
                .ok_or_else(read::invalid)?;
            let bit = 1u32 << index;
            if seen & bit != 0 {
                return Err(read::invalid());
            }
            seen |= bit;
            let key = &keys[index].0;
            let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut || cancel.borrow_mut()(),
            };
            records.values.verify_closure(
                &entry.value,
                records.record_bytes,
                &mut *records.cancelled,
            )?;
            if index == 0 {
                return records.presence(&entry.value, key, 0);
            }
            let column = keys[index].1;
            let expected = read::cell_key(kind, column, &mut records)?;
            let mut text = [0u8; 6];
            let decoded = if column == Some(RuntimeCapabilityColumnV1::SubjectKind) {
                let info = read_member_record_header_v1(
                    &entry.value,
                    &expected.0,
                    records.values,
                    MemberRecordLimitsV1 {
                        semantic_bytes: records.semantic_bytes,
                        payload_bytes: records.record_bytes,
                        record_bytes: records.record_bytes,
                    },
                    &mut *records.cancelled,
                )?;
                if info.codec != MemberPayloadCodecV1::Text || info.payload_bytes > 6 {
                    return Err(read::invalid());
                }
                let decoded = read::value(
                    column,
                    &entry.value,
                    &expected,
                    &mut records,
                    6,
                    &mut std::io::Cursor::new(text.as_mut_slice()),
                )?;
                let RuntimeCapabilityValueV1::Text { utf8_bytes } = decoded else {
                    return Err(read::invalid());
                };
                branch = subject_kind(
                    &text[..usize::try_from(utf8_bytes).map_err(|_| read::invalid())?],
                )?;
                decoded
            } else {
                read::value(
                    column,
                    &entry.value,
                    &expected,
                    &mut records,
                    max,
                    &mut std::io::sink(),
                )?
            };
            if decoded != RuntimeCapabilityValueV1::Unit {
                nonunit |= bit;
            }
            Ok(())
        },
        || cancel.borrow_mut()(),
    );
    let error = (ctx.take_state_error)();
    if cancel.borrow_mut()() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(e) = error {
        return Err(e);
    }
    match result {
        Ok(_) => {}
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited(
                "Runtime capability child scan",
            ));
        }
        _ => return Err(read::invalid()),
    }
    if seen != (1u32 << keys.len()) - 1 {
        return Err(read::invalid());
    }
    if kind == RuntimeCapabilityRecordKindV1::Budget {
        if branch == 0 {
            return Err(read::invalid());
        }
        for (index, (_, column)) in keys.iter().enumerate() {
            if let Some(column) = column {
                let mask = column.subject_mask();
                if mask != 0 && ((nonunit & (1 << index) != 0) != (mask == branch)) {
                    return Err(read::invalid());
                }
            }
        }
    }
    Ok(())
}
