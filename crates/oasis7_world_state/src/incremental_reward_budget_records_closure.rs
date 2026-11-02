use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
#[derive(Debug)]
pub struct RewardRecordQualificationV1 {
    key: RewardRecordKeyV1,
    root: StateRootV1,
}
impl RewardRecordQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn validate_binding(
        &self,
        key: RewardRecordKeyV1,
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.key != key || self.root != *root || root.namespace != key.kind.namespace() {
            Err(read::invalid())
        } else {
            Ok(())
        }
    }
}
pub(super) fn mint(
    key: RewardRecordKeyV1,
    root: StateRootV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<RewardRecordQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(RewardRecordQualificationV1 { key, root }))
}
pub(super) fn cell<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RewardRecordKindV1,
    entry: &StateEntryV1,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    max: u64,
) -> Result<key_read::Parsed, ValueTreeError> {
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(kind.descriptor()?, binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if entry.namespace != kind.namespace()
        || key.as_bytes() != entry.key
        || binding.kind != WorldFieldRecordKindV1::Entry
    {
        return Err(read::invalid());
    }
    let mut probe = key_read::CellProbe::new();
    let result = read_member_record_v1(
        &entry.value,
        &key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: max,
            record_bytes: records.record_bytes,
        },
        &mut probe,
        &mut std::io::sink(),
        &mut *records.cancelled,
    );
    if probe.is_invalid() {
        return Err(read::invalid());
    }
    let info = result?;
    let parsed = probe.finish()?;
    if !parsed.accepts(kind) {
        return Err(read::invalid());
    }
    read::primitive(
        parsed.codec(),
        &entry.value,
        &(key, info.semantic_bytes),
        records,
        max,
        &mut std::io::sink(),
    )?;
    Ok(parsed)
}
/// Explicit complete child only; family qualification independently binds its outer edge.
pub fn qualify_reward_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: RewardRecordKeyV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<RewardRecordQualificationV1>, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != key.kind.namespace() {
        return Err(read::invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _workspace = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let meta = WorldFieldKeyV1::new(
        key.kind.descriptor()?,
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let mut metadata = false;
    let mut marker = false;
    let mut columns = [false; 9];
    let mut collections = [false; 2];
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
                .ok_or(ValueTreeError::ResourceLimited("reward closure cells"))?;
            if entry.namespace != key.kind.namespace() {
                return Err(read::invalid());
            }
            let mut records = local.records();
            records.values.verify_closure(
                &entry.value,
                records.record_bytes,
                &mut *records.cancelled,
            )?;
            if entry.key == meta.as_bytes() {
                if metadata {
                    return Err(read::invalid());
                }
                metadata = true;
                return records.presence(&entry.value, &meta, 0);
            }
            match cell(key.kind, entry, &mut records, max)? {
                key_read::Parsed::Record => {
                    if marker {
                        return Err(read::invalid());
                    }
                    marker = true;
                }
                key_read::Parsed::Column(c) => {
                    let index = RewardColumnV1::ALL
                        .iter()
                        .position(|v| *v == c)
                        .ok_or_else(read::invalid)?;
                    if columns[index] {
                        return Err(read::invalid());
                    }
                    columns[index] = true;
                }
                key_read::Parsed::Collection(c) => {
                    let index = RewardCollectionV1::ALL
                        .iter()
                        .position(|v| *v == c)
                        .ok_or_else(read::invalid)?;
                    if collections[index] {
                        return Err(read::invalid());
                    }
                    collections[index] = true;
                }
                key_read::Parsed::Member(_) => {}
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
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited("reward closure scan"));
        }
        Err(_) => return Err(read::invalid()),
    }
    if !metadata || !marker {
        return Err(read::invalid());
    }
    for (index, c) in RewardColumnV1::ALL.into_iter().enumerate() {
        if columns[index] != c.accepts(key.kind) {
            return Err(read::invalid());
        }
    }
    if collections != [key.kind == RewardRecordKindV1::Pool; 2] {
        return Err(read::invalid());
    }
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(RewardRecordQualificationV1 {
        key,
        root: ctx.root.clone(),
    }))
}
