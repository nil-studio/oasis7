//! Private typed Current observations for the industrial eligibility consumer.
//! These values convey no independent qualification or business authority.
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_wasm_abi::FactoryModuleSpec;
use oasis7_world_state::{
    incremental_factory_records::{
        FactoryCellReadV1, FactoryCellV1, FactoryCollectionV1, FactoryColumnV1, FactoryRecordKeyV1,
        FactoryStackColumnV1, FactoryValueV1, read_factory_cell_overlay_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
};
use std::io::{self, Write};

/// Shared by the complete single-order consumer, never reset at an individual cell.
pub(crate) struct IndustrialEligibilityReadWorkV1 {
    pub cells_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub max_text_bytes: u64,
}
impl IndustrialEligibilityReadWorkV1 {
    pub(super) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::Invalid("industrial eligibility cell work"))?;
        Ok(())
    }
}
struct Compare<'a> {
    expected: &'a [u8],
    offset: usize,
    equal: bool,
    work: &'a mut u64,
}
impl Write for Compare<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        *self.work = self
            .work
            .checked_sub(bytes.len() as u64)
            .ok_or_else(|| io::Error::other("industrial comparison work"))?;
        let end = self
            .offset
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("industrial comparison offset"))?;
        self.equal &= self.expected.get(self.offset..end) == Some(bytes);
        self.offset = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Compare every field of the actual catalog value with original typed Current
/// cells. The caller holds the catalog allocation's reservation throughout.
/// Exhaustive destructuring intentionally makes a new ABI field a compile error.
#[allow(clippy::too_many_arguments)]
pub(super) fn current_factory_spec_matches_v1<M, R, E, C>(
    factory_id: &str,
    expected: &FactoryModuleSpec,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        let FactoryModuleSpec {
            factory_id: spec_id,
            display_name,
            tier,
            tags,
            build_cost,
            build_time_ticks,
            base_power_draw,
            recipe_slots,
            throughput_bps,
            maintenance_per_tick,
        } = expected;
        let mut equal = true;
        for (column, text) in [
            (FactoryColumnV1::SpecFactoryId, spec_id.as_str()),
            (FactoryColumnV1::SpecDisplayName, display_name.as_str()),
        ] {
            equal &= text_matches(
                factory_id,
                FactoryCellV1::Column(column),
                text,
                context,
                work,
            )?;
        }
        for (column, value) in [
            (FactoryColumnV1::SpecTier, u64::from(*tier)),
            (
                FactoryColumnV1::SpecBuildTimeTicks,
                u64::from(*build_time_ticks),
            ),
            (FactoryColumnV1::SpecRecipeSlots, u64::from(*recipe_slots)),
            (
                FactoryColumnV1::SpecThroughputBps,
                u64::from(*throughput_bps),
            ),
        ] {
            equal &= read_value(factory_id, FactoryCellV1::Column(column), context, work)?
                == FactoryCellReadV1::Value(FactoryValueV1::Unsigned(value));
        }
        for (column, value) in [
            (FactoryColumnV1::SpecBasePowerDraw, *base_power_draw),
            (
                FactoryColumnV1::SpecMaintenancePerTick,
                *maintenance_per_tick,
            ),
        ] {
            equal &= read_value(factory_id, FactoryCellV1::Column(column), context, work)?
                == FactoryCellReadV1::Value(FactoryValueV1::Signed(value));
        }
        let tag_count = match read_value(
            factory_id,
            FactoryCellV1::Collection(FactoryCollectionV1::SpecTags),
            context,
            work,
        )? {
            FactoryCellReadV1::Value(FactoryValueV1::Unsigned(n)) => n,
            _ => return Err(ValueTreeError::Invalid("industrial factory tag count").into()),
        };
        equal &= tag_count == tags.len() as u64;
        for index in 0..tag_count {
            let expected = usize::try_from(index).ok().and_then(|i| tags.get(i));
            let matches = text_matches(
                factory_id,
                FactoryCellV1::TextItem(FactoryCollectionV1::SpecTags, index),
                expected.map_or("", String::as_str),
                context,
                work,
            )?;
            equal &= expected.is_some() && matches;
        }
        let cost_count = match read_value(
            factory_id,
            FactoryCellV1::Collection(FactoryCollectionV1::SpecBuildCost),
            context,
            work,
        )? {
            FactoryCellReadV1::Value(FactoryValueV1::Unsigned(n)) => n,
            _ => return Err(ValueTreeError::Invalid("industrial factory build cost count").into()),
        };
        equal &= cost_count == build_cost.len() as u64;
        for index in 0..cost_count {
            let expected = usize::try_from(index).ok().and_then(|i| build_cost.get(i));
            let matches = text_matches(
                factory_id,
                FactoryCellV1::StackItem(
                    FactoryCollectionV1::SpecBuildCost,
                    index,
                    FactoryStackColumnV1::Kind,
                ),
                expected.map_or("", |stack| stack.kind.as_str()),
                context,
                work,
            )?;
            let amount = read_value(
                factory_id,
                FactoryCellV1::StackItem(
                    FactoryCollectionV1::SpecBuildCost,
                    index,
                    FactoryStackColumnV1::Amount,
                ),
                context,
                work,
            )?;
            equal &= expected.is_some_and(|stack| {
                matches && amount == FactoryCellReadV1::Value(FactoryValueV1::Signed(stack.amount))
            });
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(equal)
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
fn read_value<M, R, E, C>(
    id: &str,
    cell: FactoryCellV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityReadWorkV1,
) -> Result<FactoryCellReadV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    work.cell()?;
    let value = read_factory_cell_overlay_v1(
        FactoryRecordKeyV1::Factory(id),
        cell,
        ctx,
        work.max_text_bytes,
        &mut io::sink(),
    )?;
    if matches!(value, FactoryCellReadV1::MemberAbsent)
        || (matches!(value, FactoryCellReadV1::RecordAbsent) && cell != FactoryCellV1::Record)
    {
        return Err(ValueTreeError::Invalid("industrial factory missing required cell").into());
    }
    Ok(value)
}
fn text_matches<M, R, E, C>(
    id: &str,
    cell: FactoryCellV1<'_>,
    expected: &str,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityReadWorkV1,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    work.cell()?;
    let mut output = Compare {
        expected: expected.as_bytes(),
        offset: 0,
        equal: true,
        work: &mut work.comparison_bytes_remaining,
    };
    let value = read_factory_cell_overlay_v1(
        FactoryRecordKeyV1::Factory(id),
        cell,
        ctx,
        work.max_text_bytes,
        &mut output,
    )?;
    if !matches!(value, FactoryCellReadV1::Value(FactoryValueV1::Text { .. })) {
        return Err(ValueTreeError::Invalid("industrial factory text cell").into());
    }
    Ok(value
        == FactoryCellReadV1::Value(FactoryValueV1::Text {
            utf8_bytes: expected.len() as u64,
        })
        && output.equal
        && output.offset == expected.len())
}

pub(super) struct CurrentProductionFactoryV1 {
    pub builder_agent_id: String,
    pub site_id: String,
    pub site_location_id: Option<String>,
    pub input_ledger: oasis7_world_state::MaterialLedgerId,
    pub durability_ppm: i64,
    pub recipe_slots: u32,
    pub builtin_spec_available: bool,
    pub spec_matches_builtin: bool,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_production_factory_v1<M, R, E, C>(
    id: &str,
    parent: &oasis7_proto::memory_budget::ReservedValue<
        oasis7_world_state::incremental_factory_records::FactoryFamilyQualificationV1,
    >,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    Option<oasis7_proto::memory_budget::ReservedValue<CurrentProductionFactoryV1>>,
    WorldFieldOverlayErrorV1,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_proto::memory_budget::MemoryClass;
    use oasis7_world_state::{MaterialLedgerId, incremental_ledger_value::LedgerValueKindV1};
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || !ctx.transaction.uses_budget(ctx.values.memory)
        {
            return Err(ValueTreeError::Invalid("industrial factory pool/class").into());
        }
        let cap = usize::try_from(work.max_text_bytes)
            .map_err(|_| ValueTreeError::Invalid("industrial factory text limit"))?;
        let bytes = cap
            .checked_mul(4)
            .and_then(|n| n.checked_add(std::mem::size_of::<CurrentProductionFactoryV1>()))
            .ok_or(ValueTreeError::Invalid("industrial factory retained bytes"))?;
        let fee = ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _phase = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(&ctx.transaction.parent_fence().parent_roots)?;
        match read_value(id, FactoryCellV1::Record, ctx, work)? {
            FactoryCellReadV1::RecordAbsent => return Ok(None),
            FactoryCellReadV1::Value(FactoryValueV1::Unit) => {}
            _ => return Err(ValueTreeError::Invalid("industrial factory record marker").into()),
        }
        let builder_agent_id = read_text(
            id,
            FactoryColumnV1::FactoryBuilderAgentId,
            ctx,
            work,
            cap,
            false,
        )?
        .ok_or(ValueTreeError::Invalid("industrial factory builder"))?;
        let site_id = read_text(id, FactoryColumnV1::FactorySiteId, ctx, work, cap, false)?
            .ok_or(ValueTreeError::Invalid("industrial factory site"))?;
        let site_location_id = read_text(
            id,
            FactoryColumnV1::FactorySiteLocationId,
            ctx,
            work,
            cap,
            true,
        )?;
        let durability_ppm = match read_value(
            id,
            FactoryCellV1::Column(FactoryColumnV1::FactoryDurabilityPpm),
            ctx,
            work,
        )? {
            FactoryCellReadV1::Value(FactoryValueV1::Signed(v)) => v,
            _ => return Err(ValueTreeError::Invalid("industrial factory durability").into()),
        };
        let recipe_slots = match read_value(
            id,
            FactoryCellV1::Column(FactoryColumnV1::SpecRecipeSlots),
            ctx,
            work,
        )? {
            FactoryCellReadV1::Value(FactoryValueV1::Unsigned(v)) => {
                u32::try_from(v).map_err(|_| ValueTreeError::Invalid("industrial factory slots"))?
            }
            _ => return Err(ValueTreeError::Invalid("industrial factory slots type").into()),
        };
        work.cell()?;
        let mut ledger_id = Vec::with_capacity(cap);
        let ledger = read_factory_cell_overlay_v1(
            FactoryRecordKeyV1::Factory(id),
            FactoryCellV1::Column(FactoryColumnV1::FactoryInputLedger),
            ctx,
            work.max_text_bytes,
            &mut ledger_id,
        )?;
        let ledger_id = String::from_utf8(ledger_id)
            .map_err(|_| ValueTreeError::Invalid("industrial input ledger UTF-8"))?;
        let input_ledger = match ledger {
            FactoryCellReadV1::Value(FactoryValueV1::Ledger(info)) => match info.kind {
                LedgerValueKindV1::World => MaterialLedgerId::World,
                LedgerValueKindV1::Agent => MaterialLedgerId::Agent(ledger_id),
                LedgerValueKindV1::Site => MaterialLedgerId::Site(ledger_id),
                LedgerValueKindV1::Factory => MaterialLedgerId::Factory(ledger_id),
            },
            _ => return Err(ValueTreeError::Invalid("industrial input ledger type").into()),
        };
        // The assembler catalog constructs two tag Strings, three MaterialStack
        // Strings and two root Strings. Vec literals have exact length; all
        // string constructors use these fixed literals plus the requested ID.
        // Include the original catalog ID that is replaced before return.
        let catalog_fixed_bytes = std::mem::size_of::<FactoryModuleSpec>()
            + 2 * std::mem::size_of::<String>()
            + 3 * std::mem::size_of::<oasis7_wasm_abi::MaterialStack>()
            + "factory.assembler.mk1".len()
            + "Assembler MK1".len()
            + "assembler".len()
            + "precision".len()
            + "structural_frame".len()
            + "iron_ingot".len()
            + "copper_wire".len();
        let catalog_bytes =
            catalog_fixed_bytes
                .checked_add(id.len())
                .ok_or(ValueTreeError::Invalid(
                    "industrial catalog allocation bound",
                ))?;
        let _catalog_fee = ctx
            .values
            .memory
            .reserve(catalog_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let expected =
            oasis7_agent_api::world_quote::builtin_factory_spec("factory.assembler.mk1", id);
        let spec_matches_builtin = if let Some(expected) = expected.as_ref() {
            let guard = &ctx.transaction.guard();
            current_factory_spec_matches_v1(id, expected, ctx, work, guard, live)?
        } else {
            false
        };
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(fee.retain(CurrentProductionFactoryV1 {
            builder_agent_id,
            site_id,
            site_location_id,
            input_ledger,
            durability_ppm,
            recipe_slots,
            builtin_spec_available: expected.is_some(),
            spec_matches_builtin,
        })))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
fn read_text<M, R, E, C>(
    id: &str,
    column: FactoryColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityReadWorkV1,
    cap: usize,
    optional: bool,
) -> Result<Option<String>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    work.cell()?;
    let mut bytes = Vec::with_capacity(cap);
    let value = read_factory_cell_overlay_v1(
        FactoryRecordKeyV1::Factory(id),
        FactoryCellV1::Column(column),
        ctx,
        work.max_text_bytes,
        &mut bytes,
    )?;
    match value {
        FactoryCellReadV1::Value(FactoryValueV1::Text { .. }) => {
            Ok(Some(String::from_utf8(bytes).map_err(|_| {
                ValueTreeError::Invalid("industrial factory UTF-8")
            })?))
        }
        FactoryCellReadV1::Value(FactoryValueV1::Unit) if optional => Ok(None),
        _ => Err(ValueTreeError::Invalid("industrial factory text type").into()),
    }
}
