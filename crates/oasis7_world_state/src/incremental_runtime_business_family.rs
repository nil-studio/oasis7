//! Complete family verification; no business authorization or World admission.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_owner_closure::key_probe::{OwnerKeyKind, OwnerSemanticKeyProbe},
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{StateEntryV1, StateScanBudget, StateScanContext, StateScanError},
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::cell::RefCell;

pub struct RuntimeBusinessQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    heatmap_entries: u64,
}
impl RuntimeBusinessQualificationV1 {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn heatmap_entries(&self) -> u64 {
        self.heatmap_entries
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
fn identify(
    entry: &StateEntryV1,
) -> Result<
    Option<(
        RuntimeBusinessFieldV1,
        WorldFieldKeyV1,
        WorldFieldRecordKindV1,
    )>,
    ValueTreeError,
> {
    if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if parsed.owner != FieldOwner::Runtime {
        return Ok(None);
    }
    let Some(field) = RuntimeBusinessFieldV1::ALL
        .into_iter()
        .find(|f| f.descriptor().name == parsed.name)
    else {
        return Ok(None);
    };
    let key = WorldFieldKeyV1::new(field.descriptor(), parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if parsed.namespace != entry.namespace
        || entry.namespace != StateNamespaceV1::Business
        || key.as_bytes() != entry.key
        || !matches!(
            parsed.kind,
            WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Entry
        )
    {
        return Err(invalid());
    }
    Ok(Some((field, key, parsed.kind)))
}
fn check_entry<R: ValueNodeReader, C: FnMut() -> bool>(
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<Option<(RuntimeBusinessFieldV1, Option<usize>)>, ValueTreeError> {
    let Some((field, key, kind)) = identify(entry)? else {
        return Ok(None);
    };
    records
        .values
        .verify_closure(&entry.value, records.record_bytes, &mut *records.cancelled)?;
    if kind == WorldFieldRecordKindV1::Metadata {
        records.presence(&entry.value, &key, 0)?;
        return Ok(Some((field, None)));
    }
    if field == RuntimeBusinessFieldV1::LogisticsSlaMetrics {
        for (index, column) in LOGISTICS_SLA_COLUMNS_V1.into_iter().enumerate() {
            let (expected, len) = records.key(field.descriptor(), kind, column)?;
            if expected == key {
                records.unsigned(&entry.value, &key, len)?;
                return Ok(Some((field, Some(index))));
            }
        }
        return Err(invalid());
    }
    let mut probe = OwnerSemanticKeyProbe::new(OwnerKeyKind::Text, records.semantic_bytes);
    let mut payload = [0u8; 8];
    let info = read_member_record_v1(
        &entry.value,
        &key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: 8,
            record_bytes: records.record_bytes,
        },
        &mut probe,
        &mut &mut payload[..],
        &mut *records.cancelled,
    )?;
    if !probe.finish() || info.codec != MemberPayloadCodecV1::I64 || info.payload_bytes != 8 {
        return Err(invalid());
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(Some((field, Some(0))))
}
/// Full bootstrap/recovery scan shares the caller's scan, value and key budgets.
pub fn qualify_runtime_business_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
) -> Result<ReservedValue<RuntimeBusinessQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _workspace = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut metadata = 0u8;
    let mut columns = 0u8;
    let mut heatmap_entries = 0u64;
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
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |entry| {
                if let Some((field, column)) = check_entry(entry, &mut local.records())? {
                    match (field, column) {
                        (RuntimeBusinessFieldV1::LogisticsSlaMetrics, Some(index)) => {
                            let bit = 1u8 << index;
                            if columns & bit != 0 {
                                return Err(invalid());
                            }
                            columns |= bit;
                        }
                        (RuntimeBusinessFieldV1::ThreatHeatmap, Some(_)) => {
                            heatmap_entries = heatmap_entries.checked_add(1).ok_or_else(invalid)?;
                        }
                        (field, None) => {
                            let bit = if field == RuntimeBusinessFieldV1::LogisticsSlaMetrics {
                                1
                            } else {
                                2
                            };
                            if metadata & bit != 0 {
                                return Err(invalid());
                            }
                            metadata |= bit;
                        }
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
            Ok(_) => (),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("Runtime business scan"));
            }
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(_) => return Err(invalid()),
        }
    }
    if metadata != 3 || columns != u8::MAX {
        return Err(invalid());
    }
    let proof = RuntimeBusinessQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        class: ctx.values.class,
        heatmap_entries,
    };
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(proof))
}
pub fn readmit_runtime_business_ordinary_v1(
    source: &ReservedValue<RuntimeBusinessQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeBusinessQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let mut phase = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(roots)?;
    let charge = phase.split(512).map_err(ValueTreeError::Invalid)?;
    let result = charge.retain(RuntimeBusinessQualificationV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
        heatmap_entries: source.heatmap_entries,
    });
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(result)
}

/// Independent replay and changed-member verification inherit only this exact parent.
pub fn accept_runtime_business_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeBusinessQualificationV1>,
    verifier: &oasis7_formal_protocol::incremental_execution::TransitionVerifier<'_, P>,
    record: &oasis7_formal_protocol::incremental_execution::TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<
    ReservedValue<RuntimeBusinessQualificationV1>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
> {
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < 512
        || parent.class != ctx.values.class
        || verifier.class != ctx.values.class
        || !verifier.memory.same_pool(ctx.values.memory)
        || ctx.root != record.after.state_root(StateNamespaceV1::Business)
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before)?;
    let charge = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let result = verifier.verify(record);
    let error = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(e) = error {
        return Err(e.into());
    }
    let checked = result?;
    let _scratch = ctx
        .values
        .memory
        .reserve(32 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut count = i128::from(parent.heatmap_entries);
    for write in &checked.record.writes {
        ctx.check()?;
        // Identify deletions using the original structural key before reading a value.
        let entry = StateEntryV1 {
            namespace: write.namespace,
            key: write.mutation.key.clone(),
            value: write.mutation.after.unwrap_or([0; 32]),
        };
        let Some((field, _, kind)) = identify(&entry)? else {
            continue;
        };
        if (kind == WorldFieldRecordKindV1::Metadata
            || field == RuntimeBusinessFieldV1::LogisticsSlaMetrics)
            && (write.mutation.before.is_none() || write.mutation.after.is_none())
        {
            return Err(invalid().into());
        }
        if kind == WorldFieldRecordKindV1::Metadata && write.mutation.before != write.mutation.after
        {
            return Err(invalid().into());
        }
        if field == RuntimeBusinessFieldV1::ThreatHeatmap && kind == WorldFieldRecordKindV1::Entry {
            count += i128::from(write.mutation.after.is_some())
                - i128::from(write.mutation.before.is_some());
        }
        if write.mutation.after.is_some() {
            check_entry(&entry, &mut ctx.records())?;
        }
    }
    let heatmap_entries = u64::try_from(count).map_err(|_| invalid())?;
    let result = charge.retain(RuntimeBusinessQualificationV1 {
        roots: checked.record.after.digest().map_err(|_| invalid())?,
        class: ctx.values.class,
        heatmap_entries,
    });
    ctx.check()?;
    Ok(result)
}
