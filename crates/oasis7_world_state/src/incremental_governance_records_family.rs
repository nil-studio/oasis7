//! Complete governance-owner closure over authenticated top-level roots.
//! This token covers these three families only, never the whole World live set.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
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
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("governance family closure")
}
#[derive(Debug)]
pub struct GovernanceFamilyQualificationV1 {
    roots: [u8; 32],
    counts: [u64; 3],
}
impl GovernanceFamilyQualificationV1 {
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if &self.roots == roots {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    pub fn record_counts(&self) -> [u64; 3] {
        self.counts
    }
}
fn index(kind: GovernanceRecordKindV1) -> usize {
    match kind {
        GovernanceRecordKindV1::Identity => 0,
        GovernanceRecordKindV1::Proposal => 1,
        GovernanceRecordKindV1::Vote => 2,
    }
}
fn marker<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        kind.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &(RECORD_FORMAT_KEY, ()),
    )
}
pub(super) fn outer<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<(GovernanceRecordKindV1, [u8; 32], StateRootV1)>, ValueTreeError> {
    if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let Some(kind) = GovernanceRecordKindV1::ALL
        .into_iter()
        .find(|k| k.field_name() == binding.name)
    else {
        return Ok(None);
    };
    let key = WorldFieldKeyV1::new(kind.descriptor()?, binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key || entry.namespace != kind.namespace() {
        return Err(invalid());
    }
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        records.presence(&entry.value, &key, 0)?;
        return Ok(None);
    }
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Err(invalid());
    }
    let format = marker(kind, &mut records)?;
    if key == format.0 {
        read::value(
            GovernanceCellV1::Record,
            &entry.value,
            &format,
            &mut records,
            0,
            &mut std::io::sink(),
        )?;
        return Ok(None);
    }
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES
                as u64,
            record_bytes: records.record_bytes,
        },
        &mut *records.cancelled,
    )?;
    let child = read::outer(
        kind,
        &(key, info.semantic_bytes),
        &entry.value,
        &mut records,
    )?;
    let owner = binding.semantic_key.ok_or_else(invalid)?;
    ctx.check()?;
    Ok(Some((kind, owner, child)))
}
/// Raw bootstrap/recovery checks every original key, closed child and required
/// metadata/format marker. All scans share caller-owned budgets continuously.
pub fn qualify_governance_family_v1<
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
    max_payload: u64,
) -> Result<ReservedValue<GovernanceFamilyQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut counts = [0u64; 3];
    for state_root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root: state_root,
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
        for kind in GovernanceRecordKindV1::ALL
            .into_iter()
            .filter(|k| k.namespace() == state_root.namespace)
        {
            let key =
                WorldFieldKeyV1::new(kind.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            let value = local.lookup(&key)?.ok_or_else(invalid)?;
            let format = marker(kind, &mut local.records())?;
            let format_value = local.lookup(&format.0)?.ok_or_else(invalid)?;
            let mut records = local.records();
            records.presence(&value, &key, 0)?;
            read::value(
                GovernanceCellV1::Record,
                &format_value,
                &format,
                &mut records,
                0,
                &mut std::io::sink(),
            )?;
        }
        let result = StateScanContext {
            source: ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *outer_scan,
        }
        .visit_entries(
            state_root,
            |entry| {
                *cells = cells
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("governance family cells"))?;
                if let Some((kind, owner, child)) = outer(entry, &mut local)? {
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
                    qualify_governance_body_v1(
                        kind,
                        owner,
                        &mut body,
                        inner_scan,
                        cells,
                        max_payload,
                    )?;
                    let slot = index(kind);
                    counts[slot] = counts[slot].checked_add(1).ok_or_else(invalid)?;
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
                return Err(ValueTreeError::ResourceLimited("governance family scan"));
            }
            Err(_) => return Err(invalid()),
        }
    }
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(GovernanceFamilyQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        counts,
    }))
}

/// Open one record under an already-qualified exact family RootSet.
pub fn open_qualified_governance_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: GovernanceRecordKeyV1<'_>,
    roots: &RootSetV1,
    family: &ReservedValue<GovernanceFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<GovernanceRecordQualificationV1>>, ValueTreeError> {
    ctx.check()?;
    if !family.uses_budget(ctx.values.memory) {
        return Err(invalid());
    }
    family.validate_binding(&roots.digest().map_err(|_| invalid())?)?;
    let top = roots
        .state
        .iter()
        .find(|r| r.namespace == key.kind.namespace())
        .ok_or_else(invalid)?;
    if top != ctx.root {
        return Err(invalid());
    }
    let Some(child) = inspect_governance_record_root_v1(key, ctx)? else {
        return Ok(None);
    };
    let outer = read::outer_key(key, &mut ctx.records())?;
    let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let proof = closure::mint(
        key.kind,
        owner,
        (*child).clone(),
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(Some(proof))
}

pub(super) fn mint(
    roots: [u8; 32],
    counts: [u64; 3],
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
) -> Result<ReservedValue<GovernanceFamilyQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(GovernanceFamilyQualificationV1 { roots, counts }))
}
