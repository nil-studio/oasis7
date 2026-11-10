//! Complete legacy materials and world-ledger material source, never authority.
//! Parent discovery is GEN/cold only; Current reads only real pending entries.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_semantic_key::LedgerKeyRefV1,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, PreparedGuard,
};
#[derive(Debug)]
pub struct MaterialQuantityV1 {
    material: String,
    quantity: i64,
}
impl MaterialQuantityV1 {
    pub fn material(&self) -> &str {
        &self.material
    }
    pub fn quantity(&self) -> i64 {
        self.quantity
    }
}
pub struct InventoryMaterialScopeWorkV1 {
    pub max_entries_per_map: usize,
    pub max_material_bytes: usize,
    pub pending_entries_remaining: usize,
    pub comparison_bytes_remaining: u64,
}
pub struct ParentInventoryMaterialScopeV1 {
    roots: [u8; 32],
    legacy: Vec<MaterialQuantityV1>,
    world: Vec<MaterialQuantityV1>,
    world_present: bool,
}
pub struct CurrentInventoryMaterialScopeV1 {
    identity: PreparedGuard,
    memory: MemoryBudget,
    legacy: Vec<MaterialQuantityV1>,
    world: Vec<MaterialQuantityV1>,
    world_present: bool,
}
impl CurrentInventoryMaterialScopeV1 {
    pub fn legacy_materials(&self) -> impl ExactSizeIterator<Item = &MaterialQuantityV1> {
        self.legacy.iter()
    }
    pub fn world_materials(&self) -> impl ExactSizeIterator<Item = &MaterialQuantityV1> {
        self.world.iter()
    }
    pub fn world_ledger_present(&self) -> bool {
        self.world_present
    }
    /// Exact preparation snapshot. Validate once before its owner publishes the
    /// corresponding complete normalization; subsequent owner writes change guard.
    pub fn validate_current<M, R, E, C>(
        &self,
        ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        let result = (|| {
            ctx.transaction.validate_guard(guard, live)?;
            ctx.transaction.validate_guard(&self.identity, live)?;
            if !self.memory.same_pool(ctx.values.memory)
                || !ctx.transaction.uses_budget(ctx.values.memory)
                || ctx.values.class != MemoryClass::Ordinary
            {
                return Err(invalid().into());
            }
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
fn compare(work: &mut InventoryMaterialScopeWorkV1, n: usize) -> Result<(), ValueTreeError> {
    work.comparison_bytes_remaining = work
        .comparison_bytes_remaining
        .checked_sub(u64::try_from(n).map_err(|_| invalid())?)
        .ok_or(ValueTreeError::ResourceLimited(
            "material scope comparison work",
        ))?;
    Ok(())
}
fn admission(work: &InventoryMaterialScopeWorkV1, current: bool) -> Result<usize, ValueTreeError> {
    let count = work
        .max_entries_per_map
        .checked_mul(if current { 4 } else { 2 })
        .ok_or_else(invalid)?;
    count
        .checked_mul(
            std::mem::size_of::<MaterialQuantityV1>()
                .checked_add(work.max_material_bytes)
                .ok_or_else(invalid)?,
        )
        // Pending projection holds both before/after names before folding;
        // parent discovery may decode one extra row before rejecting its cap.
        .and_then(|v| {
            work.max_material_bytes
                .checked_mul(2)
                .and_then(|extra| v.checked_add(extra))
        })
        .and_then(|v| v.checked_add(8192))
        .ok_or_else(invalid)
}
enum ScopeRow {
    Metadata,
    WorldGroup,
    Legacy(MaterialQuantityV1),
    World(MaterialQuantityV1),
    Other,
}
/// Original record readers authenticate the structural key, canonical semantic
/// framing and scalar quantity. No mirrored wire parser is introduced.
fn decode<M, R, E, C>(
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    family: &mut InventoryFamilyWorkV1,
    work: &InventoryMaterialScopeWorkV1,
) -> Result<ScopeRow, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    if parsed.name == "resources" {
        return Ok(ScopeRow::Other);
    }
    // Reuse the complete family's original validation, including semantic hash
    // binding and full value closure, before projecting selected plain data.
    let _ = row(key, value, ctx, family)?;
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(ScopeRow::Metadata);
    }
    let max = family
        .max_semantic_bytes
        .min(usize::try_from(ctx.semantic_bytes).unwrap_or(usize::MAX));
    let _phase = ctx.values.memory.reserve(
        max.checked_add(work.max_material_bytes)
            .and_then(|v| v.checked_add(256))
            .ok_or_else(invalid)?,
        ctx.values.class,
    )?;
    let mut semantic = Vec::with_capacity(max);
    let mut quantity = [0u8; 8];
    if parsed.kind == WorldFieldRecordKindV1::Group {
        read_field_presence_v1(
            value,
            key,
            &mut ctx.values,
            PresenceRecordLimitsV1 {
                semantic_bytes: max as u64,
                record_bytes: ctx.record_bytes,
            },
            &mut semantic,
            &mut ctx.cancelled,
        )?;
    } else {
        let info = read_member_record_v1(
            value,
            key,
            &mut ctx.values,
            MemberRecordLimitsV1 {
                semantic_bytes: max as u64,
                payload_bytes: 8,
                record_bytes: ctx.record_bytes,
            },
            &mut semantic,
            &mut &mut quantity[..],
            &mut ctx.cancelled,
        )?;
        if info.codec != MemberPayloadCodecV1::I64 || info.payload_bytes != 8 {
            return Err(invalid());
        }
    }
    if semantic.capacity() > max {
        return Err(ValueTreeError::ResourceLimited(
            "material scope semantic capacity",
        ));
    }
    let decoded = read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?;
    let (material, is_world) = match (parsed.name, parsed.kind, decoded) {
        ("materials", WorldFieldRecordKindV1::Entry, SemanticKeyRefV1::Text(text)) => (text, false),
        (
            "material_ledgers",
            WorldFieldRecordKindV1::Group,
            SemanticKeyRefV1::Ledger(LedgerKeyRefV1::World),
        ) => return Ok(ScopeRow::WorldGroup),
        ("material_ledgers", WorldFieldRecordKindV1::Group, SemanticKeyRefV1::Ledger(_)) => {
            return Ok(ScopeRow::Other);
        }
        (
            "material_ledgers",
            WorldFieldRecordKindV1::Entry,
            SemanticKeyRefV1::Pair { left, right },
        ) => {
            let ledger = left.decode().map_err(ValueTreeError::Invalid)?;
            let SemanticKeyRefV1::Text(text) = right.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(invalid());
            };
            if !matches!(ledger, SemanticKeyRefV1::Ledger(LedgerKeyRefV1::World)) {
                return Ok(ScopeRow::Other);
            }
            (text, true)
        }
        _ => return Err(invalid()),
    };
    if material.len() > work.max_material_bytes {
        return Err(ValueTreeError::ResourceLimited(
            "material scope material bytes",
        ));
    }
    let material = material.to_owned();
    if material.capacity() > work.max_material_bytes {
        return Err(ValueTreeError::ResourceLimited(
            "material scope material capacity",
        ));
    }
    let entry = MaterialQuantityV1 {
        material,
        quantity: i64::from_be_bytes(quantity),
    };
    if is_world {
        Ok(ScopeRow::World(entry))
    } else {
        Ok(ScopeRow::Legacy(entry))
    }
}
fn copy_rows(
    source: &[MaterialQuantityV1],
    output: &mut Vec<MaterialQuantityV1>,
    work: &mut InventoryMaterialScopeWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    for entry in source {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        compare(work, entry.material.len())?;
        if entry.material.len() > work.max_material_bytes {
            return Err(invalid());
        }
        let material = entry.material.clone();
        if material.capacity() > work.max_material_bytes {
            return Err(invalid());
        }
        output.push(MaterialQuantityV1 {
            material,
            quantity: entry.quantity,
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn build_parent_inventory_material_scope_v1<M, R, E, C>(
    parent: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    family: &mut InventoryFamilyWorkV1,
    work: &mut InventoryMaterialScopeWorkV1,
) -> Result<ReservedValue<ParentInventoryMaterialScopeV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    let memory = ctx.values.memory;
    if ctx.values.class != MemoryClass::Ordinary || !parent.uses_budget(memory) {
        return Err(invalid());
    }
    parent.check(memory, MemoryClass::Ordinary)?;
    let fee = memory.reserve(admission(work, false)?, MemoryClass::Ordinary)?;
    parent.validate_binding(roots)?;
    if ctx.state.allocation_class() != Some(MemoryClass::Ordinary)
        || ctx
            .state
            .allocation_budget()
            .is_none_or(|m| !m.same_pool(memory))
    {
        return Err(invalid());
    }
    let mut legacy = Vec::with_capacity(work.max_entries_per_map);
    let mut world = Vec::with_capacity(work.max_entries_per_map);
    let mut world_present = false;
    let mut metadata = [false; 2];
    if legacy.capacity() > work.max_entries_per_map || world.capacity() > work.max_entries_per_map {
        return Err(invalid());
    }
    let stop = std::cell::RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Business),
        state: ctx.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: ctx.values.source,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: || stop.borrow_mut()(),
    };
    let result = StateScanContext {
        source: local.state,
        memory,
        class: MemoryClass::Ordinary,
        io: scan,
    }
    .visit_entries(
        roots.state_root(StateNamespaceV1::Business),
        |entry| {
            local.check()?;
            let Some(key) = key(&entry.key)? else {
                return Ok(());
            };
            let parsed =
                WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
            if parsed.name == "resources" {
                return Ok(());
            }
            match decode(&key, &entry.value, &mut local, family, work)? {
                ScopeRow::Metadata => {
                    let slot = if parsed.name == "materials" { 0 } else { 1 };
                    if metadata[slot] {
                        return Err(invalid());
                    }
                    metadata[slot] = true;
                }
                ScopeRow::WorldGroup => {
                    if world_present {
                        return Err(invalid());
                    }
                    world_present = true;
                }
                ScopeRow::Legacy(entry) => {
                    if legacy.len() == work.max_entries_per_map {
                        return Err(ValueTreeError::ResourceLimited(
                            "material scope parent entries",
                        ));
                    }
                    legacy.push(entry);
                }
                ScopeRow::World(entry) => {
                    if world.len() == work.max_entries_per_map {
                        return Err(ValueTreeError::ResourceLimited(
                            "material scope parent entries",
                        ));
                    }
                    world.push(entry);
                }
                ScopeRow::Other => {}
            }
            local.check()
        },
        || stop.borrow_mut()(),
    );
    match result {
        Ok(_) => {}
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited(
                "material scope parent scan",
            ));
        }
        Err(_) => return Err(invalid()),
    }
    if metadata != [true; 2]
        || (!world_present && !world.is_empty())
        || legacy.len() as u64 != parent.counts[1]
    {
        return Err(invalid());
    }
    let cost = work
        .max_entries_per_map
        .checked_mul(work.max_entries_per_map)
        .and_then(|v| v.checked_mul(work.max_material_bytes.max(1)))
        .and_then(|v| v.checked_mul(16))
        .ok_or_else(invalid)?;
    compare(work, cost)?;
    let mut cancelled = false;
    legacy.sort_unstable_by(|a, b| {
        cancelled |= stop.borrow_mut()();
        a.material.cmp(&b.material)
    });
    world.sort_unstable_by(|a, b| {
        cancelled |= stop.borrow_mut()();
        a.material.cmp(&b.material)
    });
    if cancelled {
        return Err(ValueTreeError::Cancelled);
    }
    if legacy
        .windows(2)
        .chain(world.windows(2))
        .any(|v| v[0].material == v[1].material)
    {
        return Err(invalid());
    }
    local.check()?;
    Ok(fee.retain(ParentInventoryMaterialScopeV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        legacy,
        world,
        world_present,
    }))
}

fn apply_row(
    rows: &mut Vec<MaterialQuantityV1>,
    before: Option<MaterialQuantityV1>,
    after: Option<MaterialQuantityV1>,
    capacity: usize,
    work: &mut InventoryMaterialScopeWorkV1,
) -> Result<(), ValueTreeError> {
    let material = before
        .as_ref()
        .or(after.as_ref())
        .ok_or_else(invalid)?
        .material
        .as_str();
    if before
        .as_ref()
        .zip(after.as_ref())
        .is_some_and(|(a, b)| a.material != b.material)
    {
        return Err(invalid());
    }
    compare(
        work,
        rows.len()
            .checked_add(1)
            .and_then(|v| v.checked_mul(work.max_material_bytes.max(1)))
            .ok_or_else(invalid)?,
    )?;
    let index = rows.iter().position(|entry| entry.material == material);
    match (index, before.as_ref()) {
        (None, None) => {}
        (Some(i), Some(value)) if rows[i].quantity == value.quantity => {}
        _ => return Err(invalid()),
    }
    if let Some(value) = after {
        if let Some(i) = index {
            rows[i] = value
        } else {
            if rows.len() == capacity {
                return Err(ValueTreeError::ResourceLimited(
                    "material scope Current entries",
                ));
            }
            rows.push(value);
        }
    } else if let Some(i) = index {
        rows.remove(i);
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub fn open_current_inventory_material_scope_v1<M, P, R, E, C>(
    parent: &ReservedValue<ParentInventoryMaterialScopeV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    family: &mut InventoryFamilyWorkV1,
    work: &mut InventoryMaterialScopeWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentInventoryMaterialScopeV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let memory = ctx.values.memory;
        if ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(memory)
            || !ctx.transaction.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        let fee = memory
            .reserve(admission(work, true)?, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if parent.roots != ctx.transaction.parent_fence().parent_roots
            || roots.digest().map_err(|_| invalid())? != parent.roots
        {
            return Err(invalid().into());
        }
        let capacity = work
            .max_entries_per_map
            .checked_mul(2)
            .ok_or_else(invalid)?;
        if parent.legacy.len() > work.max_entries_per_map
            || parent.world.len() > work.max_entries_per_map
        {
            return Err(invalid().into());
        }
        let mut legacy = Vec::with_capacity(capacity);
        let mut world = Vec::with_capacity(capacity);
        let mut world_present = parent.world_present;
        if legacy.capacity() > capacity || world.capacity() > capacity {
            return Err(invalid().into());
        }
        copy_rows(&parent.legacy, &mut legacy, work, &mut ctx.cancelled)?;
        copy_rows(&parent.world, &mut world, work, &mut ctx.cancelled)?;
        let transaction = &mut *ctx.transaction;
        let stop = std::cell::RefCell::new(&mut ctx.cancelled);
        let mut local = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: ctx.values.source,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: || stop.borrow_mut()(),
        };
        // visit_pending_writes borrows the actual transaction and authenticates its
        // before roots. It never traverses a parent namespace on the ordinary path.
        let mut pending = work.pending_entries_remaining;
        let visited = transaction.visit_pending_writes::<WorldFieldOverlayErrorV1>(
            StateNamespaceV1::Business,
            &mut pending,
            || {
                if stop.borrow_mut()() {
                    Err(ValueTreeError::Cancelled.into())
                } else {
                    Ok(())
                }
            },
            |raw, before, after| {
                local.check()?;
                let Some(key) = key(raw)? else { return Ok(()) };
                let parsed =
                    WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
                if parsed.name == "resources" {
                    return Ok(());
                }
                let before = before
                    .as_ref()
                    .map(|root| decode(&key, root, &mut local, family, work))
                    .transpose()?;
                let after = after
                    .as_ref()
                    .map(|root| decode(&key, root, &mut local, family, work))
                    .transpose()?;
                match (before, after) {
                    (Some(ScopeRow::Metadata), Some(ScopeRow::Metadata)) => {}
                    (Some(ScopeRow::Metadata), _) | (_, Some(ScopeRow::Metadata)) => {
                        return Err(invalid().into());
                    }
                    (before, after)
                        if matches!(before, Some(ScopeRow::WorldGroup))
                            || matches!(after, Some(ScopeRow::WorldGroup)) =>
                    {
                        if !matches!(before, None | Some(ScopeRow::WorldGroup))
                            || !matches!(after, None | Some(ScopeRow::WorldGroup))
                            || world_present != before.is_some()
                        {
                            return Err(invalid().into());
                        }
                        world_present = after.is_some();
                    }
                    (Some(ScopeRow::Legacy(a)), Some(ScopeRow::Legacy(b))) => {
                        apply_row(&mut legacy, Some(a), Some(b), capacity, work)?
                    }
                    (Some(ScopeRow::Legacy(a)), None) => {
                        apply_row(&mut legacy, Some(a), None, capacity, work)?
                    }
                    (None, Some(ScopeRow::Legacy(b))) => {
                        apply_row(&mut legacy, None, Some(b), capacity, work)?
                    }
                    (Some(ScopeRow::World(a)), Some(ScopeRow::World(b))) => {
                        apply_row(&mut world, Some(a), Some(b), capacity, work)?
                    }
                    (Some(ScopeRow::World(a)), None) => {
                        apply_row(&mut world, Some(a), None, capacity, work)?
                    }
                    (None, Some(ScopeRow::World(b))) => {
                        apply_row(&mut world, None, Some(b), capacity, work)?
                    }
                    (None | Some(ScopeRow::Other), None | Some(ScopeRow::Other)) => {}
                    _ => return Err(invalid().into()),
                }
                local.check()?;
                Ok(())
            },
        );
        work.pending_entries_remaining = pending;
        visited?;
        if legacy.len() > work.max_entries_per_map
            || world.len() > work.max_entries_per_map
            || (!world_present && !world.is_empty())
        {
            return Err(invalid().into());
        }
        let cost = capacity
            .checked_mul(capacity)
            .and_then(|v| v.checked_mul(work.max_material_bytes.max(1)))
            .and_then(|v| v.checked_mul(16))
            .ok_or_else(invalid)?;
        compare(work, cost)?;
        let mut cancelled = false;
        legacy.sort_unstable_by(|a, b| {
            cancelled |= stop.borrow_mut()();
            a.material.cmp(&b.material)
        });
        world.sort_unstable_by(|a, b| {
            cancelled |= stop.borrow_mut()();
            a.material.cmp(&b.material)
        });
        if cancelled {
            return Err(ValueTreeError::Cancelled.into());
        }
        local.check()?;
        Ok(fee.retain(CurrentInventoryMaterialScopeV1 {
            identity: transaction.guard(),
            memory: memory.clone(),
            legacy,
            world,
            world_present,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
#[cfg(test)]
#[path = "incremental_inventory_material_scope_tests.rs"]
mod tests;

#[path = "incremental_inventory_material_scope_freeze.rs"]
mod freeze;
pub use freeze::{FrozenInventoryMaterialScopeV1, freeze_current_inventory_material_scope_v1};
