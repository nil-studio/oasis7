//! Bounded typed Current decoding for the due-economy tail.
//!
//! This file deliberately starts with the only due-economy input that can be
//! selected without consulting a module: a committed factory build job. The
//! returned DTO is only a paid observation. The caller still binds it to the
//! qualified parent identity, Current outer delta, and the private publisher.

use super::*;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_wasm_abi::FactoryModuleSpec;
use oasis7_world_state::{
    FactoryBuildJobState, FactoryBuildPowerObligationV1, FactoryConstructionPowerMode,
    MaterialLedgerId, MaterialStack,
    incremental_factory_records::{
        FactoryCellReadV1, FactoryCellV1, FactoryCollectionV1, FactoryColumnV1, FactoryRecordKeyV1,
        FactoryStackColumnV1, FactoryValueV1,
    },
    incremental_ledger_value::LedgerValueKindV1,
};
use std::{collections::BTreeMap, io::Write};

fn invalid_build() -> BodyMutationError {
    BodyMutationError::Invalid("normal factory build Current record")
}

fn reserve_work(
    work: &mut NormalTailWorkV1<'_, '_>,
    memory: &MemoryBudget,
    bytes: usize,
) -> Result<MemoryReservation, BodyMutationError> {
    work.charge(bytes)?;
    memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(Into::into)
}

fn read_cell(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    cell: FactoryCellV1<'_>,
) -> Result<FactoryCellReadV1, BodyMutationError> {
    let mut sink = std::io::sink();
    read(cell, &mut sink)
}

fn required_unsigned(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    column: FactoryColumnV1,
) -> Result<u64, BodyMutationError> {
    match read_cell(read, FactoryCellV1::Column(column))? {
        FactoryCellReadV1::Value(FactoryValueV1::Unsigned(value)) => Ok(value),
        _ => Err(invalid_build()),
    }
}

fn required_signed(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    column: FactoryColumnV1,
) -> Result<i64, BodyMutationError> {
    match read_cell(read, FactoryCellV1::Column(column))? {
        FactoryCellReadV1::Value(FactoryValueV1::Signed(value)) => Ok(value),
        _ => Err(invalid_build()),
    }
}

fn optional_unsigned(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    column: FactoryColumnV1,
) -> Result<Option<u64>, BodyMutationError> {
    match read_cell(read, FactoryCellV1::Column(column))? {
        FactoryCellReadV1::Value(FactoryValueV1::Unsigned(value)) => Ok(Some(value)),
        FactoryCellReadV1::Value(FactoryValueV1::Unit) => Ok(None),
        _ => Err(invalid_build()),
    }
}

fn optional_signed(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    column: FactoryColumnV1,
) -> Result<Option<i64>, BodyMutationError> {
    match read_cell(read, FactoryCellV1::Column(column))? {
        FactoryCellReadV1::Value(FactoryValueV1::Signed(value)) => Ok(Some(value)),
        FactoryCellReadV1::Value(FactoryValueV1::Unit) => Ok(None),
        _ => Err(invalid_build()),
    }
}

fn required_text(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    cell: FactoryCellV1<'_>,
    max_bytes: usize,
    memory: &MemoryBudget,
    work: &mut NormalTailWorkV1<'_, '_>,
    holds: &mut Vec<MemoryReservation>,
) -> Result<String, BodyMutationError> {
    let allocation = max_bytes
        .checked_add(std::mem::size_of::<String>() + 32)
        .ok_or(ValueTreeError::ResourceLimited(
            "factory build text allocation",
        ))?;
    let fee = reserve_work(work, memory, allocation)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(max_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("factory build text capacity"))?;
    let value = read(cell, &mut bytes)?;
    let FactoryCellReadV1::Value(FactoryValueV1::Text { utf8_bytes }) = value else {
        return Err(invalid_build());
    };
    if bytes.len() != usize::try_from(utf8_bytes).map_err(|_| invalid_build())?
        || bytes.len() > max_bytes
    {
        return Err(invalid_build());
    }
    let text = String::from_utf8(bytes).map_err(|_| invalid_build())?;
    holds.push(fee);
    Ok(text)
}

fn optional_text(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    cell: FactoryCellV1<'_>,
    max_bytes: usize,
    memory: &MemoryBudget,
    work: &mut NormalTailWorkV1<'_, '_>,
    holds: &mut Vec<MemoryReservation>,
) -> Result<Option<String>, BodyMutationError> {
    let allocation = max_bytes
        .checked_add(std::mem::size_of::<String>() + 32)
        .ok_or(ValueTreeError::ResourceLimited(
            "factory build optional text allocation",
        ))?;
    let fee = reserve_work(work, memory, allocation)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(max_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("factory build optional text capacity"))?;
    match read(cell, &mut bytes)? {
        FactoryCellReadV1::Value(FactoryValueV1::Unit) if bytes.is_empty() => Ok(None),
        FactoryCellReadV1::Value(FactoryValueV1::Text { utf8_bytes })
            if bytes.len() <= max_bytes
                && usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            let text = String::from_utf8(bytes).map_err(|_| invalid_build())?;
            holds.push(fee);
            Ok(Some(text))
        }
        _ => Err(invalid_build()),
    }
}

fn required_ledger(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    column: FactoryColumnV1,
    max_bytes: usize,
    memory: &MemoryBudget,
    work: &mut NormalTailWorkV1<'_, '_>,
    holds: &mut Vec<MemoryReservation>,
) -> Result<MaterialLedgerId, BodyMutationError> {
    let allocation = max_bytes
        .checked_add(std::mem::size_of::<String>() + 32)
        .ok_or(ValueTreeError::ResourceLimited(
            "factory build ledger allocation",
        ))?;
    let fee = reserve_work(work, memory, allocation)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(max_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("factory build ledger capacity"))?;
    let value = read(FactoryCellV1::Column(column), &mut bytes)?;
    let FactoryCellReadV1::Value(FactoryValueV1::Ledger(info)) = value else {
        return Err(invalid_build());
    };
    if bytes.len() != usize::try_from(info.id_bytes).map_err(|_| invalid_build())?
        || bytes.len() > max_bytes
    {
        return Err(invalid_build());
    }
    let id = String::from_utf8(bytes).map_err(|_| invalid_build())?;
    let ledger = match info.kind {
        LedgerValueKindV1::World if id.is_empty() => MaterialLedgerId::World,
        LedgerValueKindV1::Agent => MaterialLedgerId::Agent(id),
        LedgerValueKindV1::Site => MaterialLedgerId::Site(id),
        LedgerValueKindV1::Factory => MaterialLedgerId::Factory(id),
        _ => return Err(invalid_build()),
    };
    holds.push(fee);
    Ok(ledger)
}

fn optional_ledger(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    cell: FactoryCellV1<'_>,
    max_bytes: usize,
    memory: &MemoryBudget,
    work: &mut NormalTailWorkV1<'_, '_>,
    holds: &mut Vec<MemoryReservation>,
) -> Result<Option<MaterialLedgerId>, BodyMutationError> {
    let allocation = max_bytes
        .checked_add(std::mem::size_of::<String>() + 32)
        .ok_or(ValueTreeError::ResourceLimited(
            "factory build optional ledger allocation",
        ))?;
    let fee = reserve_work(work, memory, allocation)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(max_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("factory build optional ledger capacity"))?;
    match read(cell, &mut bytes)? {
        FactoryCellReadV1::Value(FactoryValueV1::Unit) if bytes.is_empty() => Ok(None),
        FactoryCellReadV1::Value(FactoryValueV1::Ledger(info))
            if bytes.len() <= max_bytes
                && usize::try_from(info.id_bytes).ok() == Some(bytes.len()) =>
        {
            let id = String::from_utf8(bytes).map_err(|_| invalid_build())?;
            let ledger = match info.kind {
                LedgerValueKindV1::World if id.is_empty() => MaterialLedgerId::World,
                LedgerValueKindV1::Agent => MaterialLedgerId::Agent(id),
                LedgerValueKindV1::Site => MaterialLedgerId::Site(id),
                LedgerValueKindV1::Factory => MaterialLedgerId::Factory(id),
                _ => return Err(invalid_build()),
            };
            holds.push(fee);
            Ok(Some(ledger))
        }
        _ => Err(invalid_build()),
    }
}

fn collection_len(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    collection: FactoryCollectionV1,
    allow_none: bool,
    max_items: usize,
) -> Result<Option<usize>, BodyMutationError> {
    match read_cell(read, FactoryCellV1::Collection(collection))? {
        FactoryCellReadV1::Value(FactoryValueV1::Unsigned(n)) => {
            let n = usize::try_from(n).map_err(|_| invalid_build())?;
            if n > max_items {
                return Err(
                    ValueTreeError::ResourceLimited("factory build collection count").into(),
                );
            }
            Ok(Some(n))
        }
        FactoryCellReadV1::Value(FactoryValueV1::Unit) if allow_none => Ok(None),
        _ => Err(invalid_build()),
    }
}

fn distinct_cost_kinds(
    stacks: &[MaterialStack],
    work: &mut NormalTailWorkV1<'_, '_>,
) -> Result<usize, BodyMutationError> {
    let mut count = 0usize;
    for (index, stack) in stacks.iter().enumerate() {
        let mut duplicate = false;
        for previous in &stacks[..index] {
            work.charge(stack.kind.len().saturating_add(previous.kind.len()))?;
            if stack.kind == previous.kind {
                duplicate = true;
                break;
            }
        }
        if !duplicate {
            count = count.checked_add(1).ok_or(ValueTreeError::ResourceLimited(
                "factory build balance count",
            ))?;
        }
    }
    Ok(count)
}

fn optional_balance_map(
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
    collection: FactoryCollectionV1,
    stacks: &[MaterialStack],
    max_items: usize,
    memory: &MemoryBudget,
    work: &mut NormalTailWorkV1<'_, '_>,
    holds: &mut Vec<MemoryReservation>,
) -> Result<Option<BTreeMap<String, i64>>, BodyMutationError> {
    let Some(len) = collection_len(read, collection, true, max_items)? else {
        return Ok(None);
    };
    let distinct = distinct_cost_kinds(stacks, work)?;
    if len != distinct {
        return Err(invalid_build());
    }
    let map_bytes = len
        .checked_mul(256 + std::mem::size_of::<String>() + std::mem::size_of::<i64>())
        .ok_or(ValueTreeError::ResourceLimited("factory build balance map"))?;
    let map_fee = reserve_work(work, memory, map_bytes)?;
    let mut balances = BTreeMap::new();
    for (index, stack) in stacks.iter().enumerate() {
        if stacks[..index]
            .iter()
            .any(|previous| previous.kind == stack.kind)
        {
            continue;
        }
        let FactoryCellReadV1::Value(FactoryValueV1::Signed(value)) =
            read_cell(read, FactoryCellV1::Balance(collection, &stack.kind))?
        else {
            return Err(invalid_build());
        };
        let key_fee = reserve_work(
            work,
            memory,
            stack
                .kind
                .len()
                .checked_add(std::mem::size_of::<String>() + 32)
                .ok_or(ValueTreeError::ResourceLimited("factory build balance key"))?,
        )?;
        let mut key = String::new();
        key.try_reserve_exact(stack.kind.len())
            .map_err(|_| ValueTreeError::ResourceLimited("factory build balance key capacity"))?;
        key.push_str(&stack.kind);
        if balances.insert(key, value).is_some() {
            return Err(invalid_build());
        }
        holds.push(key_fee);
    }
    holds.push(map_fee);
    Ok(Some(balances))
}

/// A complete typed child observation with every retained allocation held.
/// It is not a FactoryBuild completion permit; the system reducer must still
/// verify its exact qualified-parent identity and all current business rules.
pub(in crate::world) struct DecodedFactoryBuildJobV1 {
    pub(super) job: FactoryBuildJobState,
    _holds: Vec<MemoryReservation>,
}

impl DecodedFactoryBuildJobV1 {
    pub(in crate::world) fn job(&self) -> &FactoryBuildJobState {
        &self.job
    }
}

/// Decode the original typed factory-build child without materializing any
/// sibling state. Call only after the caller's Factory family and outer-key
/// visitor authenticated this exact id; all cells come from the transaction's
/// Current overlay and share the caller's key/value/IO/cancel work.
pub(in crate::world) fn decode_current_factory_build_job_v1(
    outer_id: u64,
    max_scalar_bytes: usize,
    max_items: usize,
    memory: &MemoryBudget,
    work: &mut NormalTailWorkV1,
    read: &mut impl for<'cell> FnMut(
        FactoryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<FactoryCellReadV1, BodyMutationError>,
) -> Result<ReservedValue<DecodedFactoryBuildJobV1>, BodyMutationError> {
    let fee_slots = 32usize
        .checked_add(
            max_items
                .checked_mul(4)
                .ok_or(ValueTreeError::ResourceLimited("factory build fee slots"))?,
        )
        .ok_or(ValueTreeError::ResourceLimited("factory build fee slots"))?;
    let holds_bytes = fee_slots
        .checked_mul(std::mem::size_of::<MemoryReservation>())
        .and_then(|n| n.checked_add(std::mem::size_of::<Vec<MemoryReservation>>() + 64))
        .ok_or(ValueTreeError::ResourceLimited(
            "factory build retained fees",
        ))?;
    let base_bytes = std::mem::size_of::<DecodedFactoryBuildJobV1>()
        .checked_add(std::mem::size_of::<FactoryBuildJobState>())
        .and_then(|n| n.checked_add(holds_bytes))
        .ok_or(ValueTreeError::ResourceLimited("factory build state"))?;
    let base_fee = reserve_work(work, memory, base_bytes)?;
    let mut holds = Vec::new();
    holds
        .try_reserve_exact(fee_slots)
        .map_err(|_| ValueTreeError::ResourceLimited("factory build retained fee capacity"))?;

    let job_id = required_unsigned(read, FactoryColumnV1::BuildJobId)?;
    if job_id != outer_id {
        return Err(invalid_build());
    }
    let builder_agent_id = required_text(
        read,
        FactoryCellV1::Column(FactoryColumnV1::BuildBuilderAgentId),
        max_scalar_bytes,
        memory,
        work,
        &mut holds,
    )?;
    let site_id = required_text(
        read,
        FactoryCellV1::Column(FactoryColumnV1::BuildSiteId),
        max_scalar_bytes,
        memory,
        work,
        &mut holds,
    )?;
    let consume_ledger = required_ledger(
        read,
        FactoryColumnV1::BuildConsumeLedger,
        max_scalar_bytes,
        memory,
        work,
        &mut holds,
    )?;
    let ready_at = required_unsigned(read, FactoryColumnV1::BuildReadyAt)?;
    let contract_version = u8::try_from(required_unsigned(
        read,
        FactoryColumnV1::BuildContractVersion,
    )?)
    .map_err(|_| invalid_build())?;
    let site_authority_revision =
        optional_unsigned(read, FactoryColumnV1::BuildSiteAuthorityRevision)?;
    let site_location_id = optional_text(
        read,
        FactoryCellV1::Column(FactoryColumnV1::BuildSiteLocationId),
        max_scalar_bytes,
        memory,
        work,
        &mut holds,
    )?;
    let location_anchor_revision =
        optional_unsigned(read, FactoryColumnV1::BuildLocationAnchorRevision)?;

    let factory_id = required_text(
        read,
        FactoryCellV1::Column(FactoryColumnV1::SpecFactoryId),
        max_scalar_bytes,
        memory,
        work,
        &mut holds,
    )?;
    let display_name = required_text(
        read,
        FactoryCellV1::Column(FactoryColumnV1::SpecDisplayName),
        max_scalar_bytes,
        memory,
        work,
        &mut holds,
    )?;
    let tier = u8::try_from(required_unsigned(read, FactoryColumnV1::SpecTier)?)
        .map_err(|_| invalid_build())?;
    let build_time_ticks = u32::try_from(required_unsigned(
        read,
        FactoryColumnV1::SpecBuildTimeTicks,
    )?)
    .map_err(|_| invalid_build())?;
    let base_power_draw = required_signed(read, FactoryColumnV1::SpecBasePowerDraw)?;
    let recipe_slots = u16::try_from(required_unsigned(read, FactoryColumnV1::SpecRecipeSlots)?)
        .map_err(|_| invalid_build())?;
    let throughput_bps =
        u32::try_from(required_unsigned(read, FactoryColumnV1::SpecThroughputBps)?)
            .map_err(|_| invalid_build())?;
    let maintenance_per_tick = required_signed(read, FactoryColumnV1::SpecMaintenancePerTick)?;

    let tag_count = collection_len(read, FactoryCollectionV1::SpecTags, false, max_items)?
        .ok_or_else(invalid_build)?;
    let tag_capacity_bytes = tag_count
        .checked_mul(std::mem::size_of::<String>())
        .and_then(|n| n.checked_add(2 * std::mem::size_of::<usize>()))
        .ok_or(ValueTreeError::ResourceLimited("factory build tags"))?;
    let tags_fee = reserve_work(work, memory, tag_capacity_bytes)?;
    let mut tags = Vec::new();
    tags.try_reserve_exact(tag_count)
        .map_err(|_| ValueTreeError::ResourceLimited("factory build tag capacity"))?;
    for index in 0..tag_count {
        let tag = required_text(
            read,
            FactoryCellV1::TextItem(FactoryCollectionV1::SpecTags, index as u64),
            max_scalar_bytes,
            memory,
            work,
            &mut holds,
        )?;
        tags.push(tag);
    }
    holds.push(tags_fee);

    let cost_count = collection_len(read, FactoryCollectionV1::SpecBuildCost, false, max_items)?
        .ok_or_else(invalid_build)?;
    let cost_capacity_bytes = cost_count
        .checked_mul(std::mem::size_of::<MaterialStack>())
        .and_then(|n| n.checked_add(2 * std::mem::size_of::<usize>()))
        .ok_or(ValueTreeError::ResourceLimited("factory build cost"))?;
    let cost_fee = reserve_work(work, memory, cost_capacity_bytes)?;
    let mut build_cost = Vec::new();
    build_cost
        .try_reserve_exact(cost_count)
        .map_err(|_| ValueTreeError::ResourceLimited("factory build cost capacity"))?;
    for index in 0..cost_count {
        let kind = required_text(
            read,
            FactoryCellV1::StackItem(
                FactoryCollectionV1::SpecBuildCost,
                index as u64,
                FactoryStackColumnV1::Kind,
            ),
            max_scalar_bytes,
            memory,
            work,
            &mut holds,
        )?;
        let amount = match read_cell(
            read,
            FactoryCellV1::StackItem(
                FactoryCollectionV1::SpecBuildCost,
                index as u64,
                FactoryStackColumnV1::Amount,
            ),
        )? {
            FactoryCellReadV1::Value(FactoryValueV1::Signed(value)) => value,
            _ => return Err(invalid_build()),
        };
        build_cost.push(MaterialStack { kind, amount });
    }
    holds.push(cost_fee);

    let obligation_present = match read_cell(
        read,
        FactoryCellV1::Column(FactoryColumnV1::ObligationPresent),
    )? {
        FactoryCellReadV1::Value(FactoryValueV1::Bool(value)) => value,
        _ => return Err(invalid_build()),
    };
    let construction_power_obligation = if obligation_present {
        let payer_agent_id = required_text(
            read,
            FactoryCellV1::Column(FactoryColumnV1::ObligationPayerAgentId),
            max_scalar_bytes,
            memory,
            work,
            &mut holds,
        )?;
        let profile_key = required_text(
            read,
            FactoryCellV1::Column(FactoryColumnV1::ObligationProfileKey),
            max_scalar_bytes,
            memory,
            work,
            &mut holds,
        )?;
        let profile_revision = required_unsigned(read, FactoryColumnV1::ObligationProfileRevision)?;
        let electricity_amount =
            required_signed(read, FactoryColumnV1::ObligationElectricityAmount)?;
        let mode = match required_unsigned(read, FactoryColumnV1::ObligationMode)? {
            0 => FactoryConstructionPowerMode::StartOnlySink,
            _ => return Err(invalid_build()),
        };
        let material_ledger = optional_ledger(
            read,
            FactoryCellV1::Column(FactoryColumnV1::ObligationMaterialLedger),
            max_scalar_bytes,
            memory,
            work,
            &mut holds,
        )?;
        let electricity_before =
            optional_signed(read, FactoryColumnV1::ObligationElectricityBefore)?;
        let electricity_after = optional_signed(read, FactoryColumnV1::ObligationElectricityAfter)?;
        let material_balances_before = optional_balance_map(
            read,
            FactoryCollectionV1::ObligationBalancesBefore,
            &build_cost,
            max_items,
            memory,
            work,
            &mut holds,
        )?;
        let material_balances_after = optional_balance_map(
            read,
            FactoryCollectionV1::ObligationBalancesAfter,
            &build_cost,
            max_items,
            memory,
            work,
            &mut holds,
        )?;
        Some(FactoryBuildPowerObligationV1 {
            payer_agent_id,
            profile_key,
            profile_revision,
            electricity_amount,
            mode,
            material_ledger,
            electricity_before,
            electricity_after,
            material_balances_before,
            material_balances_after,
        })
    } else {
        // The original typed closure proves every obligation child is absent
        // behind this false marker. Reading both optional maps preserves the
        // absent-versus-empty contract rather than fabricating Some(empty).
        for cell in [
            FactoryCellV1::Column(FactoryColumnV1::ObligationPayerAgentId),
            FactoryCellV1::Column(FactoryColumnV1::ObligationProfileKey),
            FactoryCellV1::Column(FactoryColumnV1::ObligationProfileRevision),
            FactoryCellV1::Column(FactoryColumnV1::ObligationElectricityAmount),
            FactoryCellV1::Column(FactoryColumnV1::ObligationMode),
            FactoryCellV1::Column(FactoryColumnV1::ObligationMaterialLedger),
            FactoryCellV1::Column(FactoryColumnV1::ObligationElectricityBefore),
            FactoryCellV1::Column(FactoryColumnV1::ObligationElectricityAfter),
        ] {
            if read_cell(read, cell)? != FactoryCellReadV1::Value(FactoryValueV1::Unit) {
                return Err(invalid_build());
            }
        }
        for collection in [
            FactoryCollectionV1::ObligationBalancesBefore,
            FactoryCollectionV1::ObligationBalancesAfter,
        ] {
            if collection_len(read, collection, true, max_items)?.is_some() {
                return Err(invalid_build());
            }
        }
        None
    };

    let job = FactoryBuildJobState {
        job_id,
        builder_agent_id,
        site_id,
        spec: FactoryModuleSpec {
            factory_id,
            display_name,
            tier,
            tags,
            build_cost,
            build_time_ticks,
            base_power_draw,
            recipe_slots,
            throughput_bps,
            maintenance_per_tick,
        },
        consume_ledger,
        ready_at,
        contract_version,
        site_authority_revision,
        site_location_id,
        location_anchor_revision,
        construction_power_obligation,
    };
    Ok(base_fee.retain(DecodedFactoryBuildJobV1 { job, _holds: holds }))
}
