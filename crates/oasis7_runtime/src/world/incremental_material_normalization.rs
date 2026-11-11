//! Original CorePolicy material normalization in the sole Current transaction.
//! This contributes only state. It publishes no event and grants no authority.
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    MaterialLedgerId,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory::{
        CurrentInventoryMaterialScopeV1, InventoryWorldGroupWorkV1,
        ensure_world_ledger_group_overlay_v1,
    },
    incremental_inventory_change::InventoryChangeRefV1,
    incremental_inventory_overlay_change::InventoryOverlayChangeError,
    incremental_inventory_read::LedgerQuantityV1,
};
#[derive(Debug)]
pub(in crate::world) enum MaterialNormalizationErrorV1 {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Inventory(InventoryOverlayChangeError),
}
impl From<ExecutionError> for MaterialNormalizationErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<ValueTreeError> for MaterialNormalizationErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<WorldFieldOverlayErrorV1> for MaterialNormalizationErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
impl From<InventoryOverlayChangeError> for MaterialNormalizationErrorV1 {
    fn from(e: InventoryOverlayChangeError) -> Self {
        Self::Inventory(e)
    }
}
/// Borrow the caller's continuous counters instead of copying/resetting a
/// per-event budget. Only the genuine Current scope supplies material entries.
pub(in crate::world) struct MaterialNormalizationWorkV1<'budget> {
    pub max_union_entries: usize,
    pub cells_remaining: &'budget mut u64,
    pub comparison_bytes_remaining: &'budget mut u64,
}
impl MaterialNormalizationWorkV1<'_> {
    fn cell(&mut self) -> Result<(), ValueTreeError> {
        *self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "material normalization cells",
                ))?;
        Ok(())
    }
    fn compare(&mut self, bytes: usize) -> Result<(), ValueTreeError> {
        *self.comparison_bytes_remaining = self
            .comparison_bytes_remaining
            .checked_sub(u64::try_from(bytes).map_err(|_| {
                ValueTreeError::ResourceLimited("material normalization comparison bound")
            })?)
            .ok_or(ValueTreeError::ResourceLimited(
                "material normalization comparisons",
            ))?;
        Ok(())
    }
}
struct MaterialDecision<'a> {
    key: &'a str,
    legacy: Option<i64>,
    world: Option<i64>,
    after: Option<i64>,
}
/// Mirror the original closed rule: a nonempty World ledger wins, otherwise
/// legacy materials win. Zero and negative quantities remain entries. Ensure
/// the World group even when both maps are empty, preserve every other ledger,
/// and make both selected maps exactly equal before the event owner publishes.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn normalize_current_materials_v1<M, R, E, C, W, CW>(
    materials: &ReservedValue<CurrentInventoryMaterialScopeV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    group_work: &mut InventoryWorldGroupWorkV1,
    work: &mut MaterialNormalizationWorkV1<'_>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), MaterialNormalizationErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || !materials.uses_budget(ctx.values.memory)
            || !ctx.values.memory.same_pool(write.memory)
        {
            return Err(
                ValueTreeError::Invalid("Current material normalization pool/class").into(),
            );
        }
        materials.validate_current(ctx, guard, live)?;
        if (ctx.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let capacity = materials
            .legacy_materials()
            .len()
            .checked_add(materials.world_materials().len())
            .ok_or(ValueTreeError::ResourceLimited("normalization union bound"))?;
        if capacity > work.max_union_entries {
            return Err(ValueTreeError::ResourceLimited("normalization union entries").into());
        }
        let bytes = capacity
            .checked_mul(std::mem::size_of::<MaterialDecision<'_>>())
            .and_then(|n| n.checked_add(512))
            .ok_or(ValueTreeError::ResourceLimited("normalization union bytes"))?;
        let _phase = ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut decisions = Vec::with_capacity(capacity);
        if decisions.capacity() > capacity {
            return Err(
                ValueTreeError::ResourceLimited("normalization union actual capacity").into(),
            );
        }
        let use_world = materials.world_materials().len() != 0;
        let mut legacy = materials.legacy_materials().peekable();
        let mut world = materials.world_materials().peekable();
        while legacy.peek().is_some() || world.peek().is_some() {
            work.cell()?;
            if (ctx.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let ordering = match (legacy.peek(), world.peek()) {
                (Some(a), Some(b)) => {
                    work.compare(a.material().len().saturating_add(b.material().len()))?;
                    a.material().cmp(b.material())
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => break,
            };
            let (key, left, right) = match ordering {
                std::cmp::Ordering::Less => {
                    let value = legacy.next().unwrap();
                    (value.material(), Some(value.quantity()), None)
                }
                std::cmp::Ordering::Greater => {
                    let value = world.next().unwrap();
                    (value.material(), None, Some(value.quantity()))
                }
                std::cmp::Ordering::Equal => {
                    let a = legacy.next().unwrap();
                    let b = world.next().unwrap();
                    (a.material(), Some(a.quantity()), Some(b.quantity()))
                }
            };
            decisions.push(MaterialDecision {
                key,
                legacy: left,
                world: right,
                after: if use_world { right } else { left },
            });
        }
        // This is the only source-dependent write boundary. Every decision above
        // borrows the genuine scope, whose fee remains held throughout publication.
        ensure_world_ledger_group_overlay_v1(materials, ctx, write, group_work, guard, live)?;
        for decision in decisions {
            work.cell()?;
            if (ctx.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if decision.legacy != decision.after {
                let guard = ctx.transaction.guard();
                ctx.change(
                    InventoryChangeRefV1::Material {
                        key: decision.key,
                        expected: decision.legacy,
                        after: decision.after,
                    },
                    write,
                    &guard,
                    live,
                )?;
            }
            if decision.world != decision.after {
                let guard = ctx.transaction.guard();
                ctx.change(
                    InventoryChangeRefV1::LedgerMaterial {
                        ledger: &MaterialLedgerId::World,
                        material: decision.key,
                        expected: decision
                            .world
                            .map_or(LedgerQuantityV1::MaterialAbsent, LedgerQuantityV1::Value),
                        after: decision.after,
                        create_ledger: false,
                    },
                    write,
                    &guard,
                    live,
                )?;
            }
        }
        if (ctx.cancelled)() || (write.cancelled)() {
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

#[path = "incremental_material_normalization_transfer.rs"]
pub(in crate::world) mod transfer;
