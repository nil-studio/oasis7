//! Root-only bootstrap proof with fixed workspace, including dense original slots.
use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateRootV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::RefCell;
#[derive(Debug)]
pub struct FactoryAuthorityQualificationV1 {
    kind: FactoryAuthorityRecordKindV1,
    owner: [u8; 32],
    root: StateRootV1,
}
pub(super) fn mint(
    kind: FactoryAuthorityRecordKindV1,
    owner: [u8; 32],
    root: StateRootV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
) -> Result<ReservedValue<FactoryAuthorityQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(FactoryAuthorityQualificationV1 { kind, owner, root }))
}
impl FactoryAuthorityQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn validate_binding(
        &self,
        kind: FactoryAuthorityRecordKindV1,
        owner: &[u8; 32],
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.kind != kind || self.owner != *owner || self.root != *root {
            return Err(read::invalid());
        }
        Ok(())
    }
}
fn spend(cells: &mut u64) -> Result<(), ValueTreeError> {
    *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
        "factory authority closure cells",
    ))?;
    Ok(())
}
fn check_cell<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: FactoryAuthorityRecordKindV1,
    cell: FactoryAuthorityCellV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max: u64,
    cells: &mut u64,
) -> Result<Option<FactoryAuthorityValueV1>, ValueTreeError> {
    spend(cells)?;
    let key = read::cell_key(kind, cell, &mut ctx.records())?;
    let root = ctx.lookup(&key.0)?;
    let Some(root) = root else {
        return Ok(None);
    };
    ctx.values
        .verify_closure(&root, ctx.record_bytes, &mut ctx.cancelled)?;
    read::value(
        cell,
        &root,
        &key,
        &mut ctx.records(),
        max,
        &mut std::io::sink(),
    )
    .map(Some)
}
/// This proves one exact child, not its membership in an accepted World family.
/// All required keys are authenticated independently. Exact real leaf count then
/// excludes unknown keys without collecting the variable-size allowlist.
pub fn qualify_factory_authority_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: FactoryAuthorityRecordKindV1,
    owner: [u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
) -> Result<ReservedValue<FactoryAuthorityQualificationV1>, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != kind.namespace() {
        return Err(read::invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _workspace = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let meta = WorldFieldKeyV1::new(kind.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    spend(cells)?;
    let meta_root = ctx.lookup(&meta)?.ok_or_else(read::invalid)?;
    ctx.values
        .verify_closure(&meta_root, ctx.record_bytes, &mut ctx.cancelled)?;
    ctx.records().presence(&meta_root, &meta, 0)?;
    let mut expected = 1u64;
    let mut members = 0u64;
    let mut module_present = None;
    let mut module_value = false;
    for cell in FactoryAuthorityCellV1::FIXED {
        if !cell.accepts(kind) {
            continue;
        }
        let value = check_cell(kind, cell, ctx, max_payload, cells)?;
        if cell == FactoryAuthorityCellV1::SourceModuleId {
            module_value = value.is_some();
        } else if value.is_none() {
            return Err(read::invalid());
        }
        if value.is_some() {
            expected = expected.checked_add(1).ok_or_else(read::invalid)?;
        }
        match (cell, value) {
            (
                FactoryAuthorityCellV1::AuthorizedCount,
                Some(FactoryAuthorityValueV1::Unsigned(n)),
            ) => members = n,
            (
                FactoryAuthorityCellV1::SourceModulePresent,
                Some(FactoryAuthorityValueV1::Bool(v)),
            ) => module_present = Some(v),
            _ => {}
        }
    }
    if kind == FactoryAuthorityRecordKindV1::Power && module_present != Some(module_value) {
        return Err(read::invalid());
    }
    expected = expected.checked_add(members).ok_or_else(read::invalid)?;
    if ctx.root.leaf_count != expected {
        return Err(read::invalid());
    }
    // Preflight the finite logical allowance before potentially large ordinal work.
    if members > *cells {
        return Err(ValueTreeError::ResourceLimited(
            "factory authority closure members",
        ));
    }
    for index in 0..members {
        if check_cell(
            kind,
            FactoryAuthorityCellV1::AuthorizedAgent(index),
            ctx,
            max_payload,
            cells,
        )?
        .is_none()
        {
            return Err(read::invalid());
        }
    }
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut actual = 0u64;
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
            actual = actual.checked_add(1).ok_or_else(read::invalid)?;
            Ok(())
        },
        || (cancel.borrow_mut())(),
    );
    let original = (ctx.take_state_error)();
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = original {
        return Err(error);
    }
    match result {
        Ok(count) if count == expected && actual == expected => {}
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited(
                "factory authority closure scan",
            ));
        }
        _ => return Err(read::invalid()),
    }
    Ok(retained.retain(FactoryAuthorityQualificationV1 {
        kind,
        owner,
        root: ctx.root.clone(),
    }))
}
