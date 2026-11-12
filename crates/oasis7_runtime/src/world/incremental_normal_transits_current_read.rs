//! Current-backed readers and bounded due-order selection for material transits.

use super::common::*;

struct DynWriteAdapterV1<'a>(&'a mut dyn Write);

impl Write for DynWriteAdapterV1<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

pub(super) struct CurrentLogisticsRouteV1 {
    pub(super) value: LogisticsRouteV1,
    pub(super) _fees: Vec<MemoryReservation>,
}

pub(super) struct CurrentLogisticsPathV1 {
    pub(super) value: LogisticsPathAuthorityV1,
    pub(super) _fees: Vec<MemoryReservation>,
}

fn logistics_read_cell(
    read: &mut impl for<'cell> FnMut(
        LogisticsCellV1,
        &mut dyn Write,
    ) -> Result<LogisticsReadV1, BodyMutationError>,
    cell: LogisticsCellV1,
) -> Result<LogisticsReadV1, BodyMutationError> {
    let mut sink = std::io::sink();
    read(cell, &mut sink)
}

fn logistics_text(
    read: &mut impl for<'cell> FnMut(
        LogisticsCellV1,
        &mut dyn Write,
    ) -> Result<LogisticsReadV1, BodyMutationError>,
    cell: LogisticsCellV1,
    maximum: usize,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
    holds: &mut Vec<MemoryReservation>,
) -> Result<String, BodyMutationError> {
    let bound = maximum
        .checked_add(size_of::<String>() + 32)
        .ok_or(ValueTreeError::ResourceLimited("logistics text layout"))?;
    let fee = charge_reserve(normal, memory, bound, "logistics text")?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(maximum)
        .map_err(|_| ValueTreeError::ResourceLimited("logistics text capacity"))?;
    let LogisticsReadV1::Value(LogisticsValueV1::Text { utf8_bytes }) = read(cell, &mut bytes)?
    else {
        return Err(BodyMutationError::Invalid("normal transit logistics text"));
    };
    if bytes.len() > maximum || usize::try_from(utf8_bytes).ok() != Some(bytes.len()) {
        return Err(BodyMutationError::Invalid(
            "normal transit logistics text length",
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| BodyMutationError::Invalid("normal transit logistics utf8"))?;
    holds.push(fee);
    Ok(text)
}

fn logistics_ledger(
    read: &mut impl for<'cell> FnMut(
        LogisticsCellV1,
        &mut dyn Write,
    ) -> Result<LogisticsReadV1, BodyMutationError>,
    cell: LogisticsCellV1,
    maximum: usize,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
    holds: &mut Vec<MemoryReservation>,
) -> Result<MaterialLedgerId, BodyMutationError> {
    let bound = maximum
        .checked_add(size_of::<String>() + 32)
        .ok_or(ValueTreeError::ResourceLimited("logistics ledger layout"))?;
    let fee = charge_reserve(normal, memory, bound, "logistics ledger")?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(maximum)
        .map_err(|_| ValueTreeError::ResourceLimited("logistics ledger capacity"))?;
    let LogisticsReadV1::Value(LogisticsValueV1::Ledger(info)) = read(cell, &mut bytes)? else {
        return Err(BodyMutationError::Invalid(
            "normal transit logistics ledger",
        ));
    };
    if bytes.len() > maximum || usize::try_from(info.id_bytes).ok() != Some(bytes.len()) {
        return Err(BodyMutationError::Invalid(
            "normal transit logistics ledger length",
        ));
    }
    let id = String::from_utf8(bytes)
        .map_err(|_| BodyMutationError::Invalid("normal transit logistics ledger utf8"))?;
    let ledger = match info.kind {
        LedgerValueKindV1::World if id.is_empty() => MaterialLedgerId::World,
        LedgerValueKindV1::Agent => MaterialLedgerId::Agent(id),
        LedgerValueKindV1::Site => MaterialLedgerId::Site(id),
        LedgerValueKindV1::Factory => MaterialLedgerId::Factory(id),
        _ => {
            return Err(BodyMutationError::Invalid(
                "normal transit logistics ledger kind",
            ));
        }
    };
    holds.push(fee);
    Ok(ledger)
}

/// Decode one parent-authenticated route through exact Current point reads.
/// The private return owner retains every route string fee until its consumer
/// has completed business validation and copied any changed fields.
pub(super) fn read_current_route_v1(
    id: &str,
    maximum: usize,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
    read: &mut impl for<'cell> FnMut(
        LogisticsCellV1,
        &mut dyn Write,
    ) -> Result<LogisticsReadV1, BodyMutationError>,
) -> Result<Option<ReservedValue<CurrentLogisticsRouteV1>>, BodyMutationError> {
    match logistics_read_cell(read, LogisticsCellV1::Record)? {
        LogisticsReadV1::RecordAbsent => return Ok(None),
        LogisticsReadV1::Value(LogisticsValueV1::Unit) => {}
        _ => return Err(BodyMutationError::Invalid("route record marker")),
    }
    let owner_bytes = size_of::<CurrentLogisticsRouteV1>()
        .checked_add(size_of::<LogisticsRouteV1>())
        .and_then(|n| n.checked_add(size_of::<Vec<MemoryReservation>>() + 64))
        .ok_or(ValueTreeError::ResourceLimited("route owner layout"))?;
    let owner_fee = charge_reserve(normal, memory, owner_bytes, "route owner")?;
    let key = LogisticsRecordKeyV1 {
        kind: LogisticsRecordKindV1::Route,
        id,
    };
    let mut holds = Vec::new();
    holds
        .try_reserve_exact(5)
        .map_err(|_| ValueTreeError::ResourceLimited("route fee slots"))?;
    let route_id = logistics_text(
        read,
        LogisticsCellV1::Column(LogisticsColumn::RouteId),
        maximum,
        memory,
        normal,
        &mut holds,
    )?;
    if route_id != key.id {
        return Err(BodyMutationError::Invalid("route outer id mismatch"));
    }
    let from_ledger = logistics_ledger(
        read,
        LogisticsCellV1::Column(LogisticsColumn::FromLedger),
        maximum,
        memory,
        normal,
        &mut holds,
    )?;
    let to_ledger = logistics_ledger(
        read,
        LogisticsCellV1::Column(LogisticsColumn::ToLedger),
        maximum,
        memory,
        normal,
        &mut holds,
    )?;
    let kind = logistics_text(
        read,
        LogisticsCellV1::Column(LogisticsColumn::Kind),
        maximum,
        memory,
        normal,
        &mut holds,
    )?;
    let owner_agent_id = logistics_text(
        read,
        LogisticsCellV1::Column(LogisticsColumn::OwnerAgentId),
        maximum,
        memory,
        normal,
        &mut holds,
    )?;
    let signed = |read: &mut _, cell| -> Result<i64, BodyMutationError> {
        match logistics_read_cell(read, cell)? {
            LogisticsReadV1::Value(LogisticsValueV1::Signed(value)) => Ok(value),
            _ => Err(BodyMutationError::Invalid("route signed field")),
        }
    };
    let distance_km = signed(read, LogisticsCellV1::Column(LogisticsColumn::DistanceKm))?;
    let priority =
        match logistics_read_cell(read, LogisticsCellV1::Column(LogisticsColumn::Priority))? {
            LogisticsReadV1::Value(LogisticsValueV1::Unsigned(0)) => {
                MaterialTransitPriority::Standard
            }
            LogisticsReadV1::Value(LogisticsValueV1::Unsigned(1)) => {
                MaterialTransitPriority::Urgent
            }
            _ => return Err(BodyMutationError::Invalid("route priority")),
        };
    let available =
        match logistics_read_cell(read, LogisticsCellV1::Column(LogisticsColumn::Available))? {
            LogisticsReadV1::Value(LogisticsValueV1::Bool(value)) => value,
            _ => return Err(BodyMutationError::Invalid("route availability")),
        };
    let capacity_units = signed(
        read,
        LogisticsCellV1::Column(LogisticsColumn::CapacityUnits),
    )?;
    let reserved_capacity_units = signed(
        read,
        LogisticsCellV1::Column(LogisticsColumn::ReservedCapacityUnits),
    )?;
    let tariff_electricity_per_unit = signed(
        read,
        LogisticsCellV1::Column(LogisticsColumn::TariffElectricityPerUnit),
    )?;
    let value = LogisticsRouteV1 {
        route_id,
        from_ledger,
        to_ledger,
        kind,
        distance_km,
        priority,
        owner_agent_id,
        available,
        capacity_units,
        reserved_capacity_units,
        tariff_electricity_per_unit,
    };
    Ok(Some(owner_fee.retain(CurrentLogisticsRouteV1 {
        value,
        _fees: holds,
    })))
}

pub(super) fn read_current_path_v1(
    id: &str,
    maximum: usize,
    max_items: usize,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
    read: &mut impl for<'cell> FnMut(
        LogisticsCellV1,
        &mut dyn Write,
    ) -> Result<LogisticsReadV1, BodyMutationError>,
) -> Result<Option<ReservedValue<CurrentLogisticsPathV1>>, BodyMutationError> {
    let marker = logistics_read_cell(read, LogisticsCellV1::Record)?;
    if marker == LogisticsReadV1::RecordAbsent {
        return Ok(None);
    }
    if marker != LogisticsReadV1::Value(LogisticsValueV1::Unit) {
        return Err(BodyMutationError::Invalid("path record marker"));
    }
    let key = LogisticsRecordKeyV1 {
        kind: LogisticsRecordKindV1::Path,
        id,
    };
    let collection_count = match logistics_read_cell(
        read,
        LogisticsCellV1::Collection(LogisticsCollectionV1::RouteIds),
    )? {
        LogisticsReadV1::Value(LogisticsValueV1::Unsigned(count)) => {
            usize::try_from(count).map_err(|_| invalid())?
        }
        _ => return Err(BodyMutationError::Invalid("path route count")),
    };
    if collection_count > max_items {
        return Err(ValueTreeError::ResourceLimited("path route id count").into());
    }
    let route_layout = collection_count
        .checked_mul(size_of::<String>())
        .and_then(|n| n.checked_add(size_of::<Vec<String>>() + 32))
        .ok_or(ValueTreeError::ResourceLimited("path route layout"))?;
    let route_fee = charge_reserve(normal, memory, route_layout, "path route vector")?;
    let mut holds = Vec::new();
    holds
        .try_reserve_exact(6usize.saturating_add(collection_count))
        .map_err(|_| ValueTreeError::ResourceLimited("path fee slots"))?;
    let path_id = logistics_text(
        read,
        LogisticsCellV1::Column(LogisticsColumn::PathId),
        maximum,
        memory,
        normal,
        &mut holds,
    )?;
    if path_id != key.id {
        return Err(BodyMutationError::Invalid("path outer id mismatch"));
    }
    let from_ledger = logistics_ledger(
        read,
        LogisticsCellV1::Column(LogisticsColumn::FromLedger),
        maximum,
        memory,
        normal,
        &mut holds,
    )?;
    let to_ledger = logistics_ledger(
        read,
        LogisticsCellV1::Column(LogisticsColumn::ToLedger),
        maximum,
        memory,
        normal,
        &mut holds,
    )?;
    let kind = logistics_text(
        read,
        LogisticsCellV1::Column(LogisticsColumn::Kind),
        maximum,
        memory,
        normal,
        &mut holds,
    )?;
    let mut route_ids = Vec::new();
    route_ids
        .try_reserve_exact(collection_count)
        .map_err(|_| ValueTreeError::ResourceLimited("path route capacity"))?;
    for index in 0..collection_count {
        route_ids.push(logistics_text(
            read,
            LogisticsCellV1::TextItem(LogisticsCollectionV1::RouteIds, index as u64),
            maximum,
            memory,
            normal,
            &mut holds,
        )?);
    }
    let signed = |read: &mut _, cell| -> Result<i64, BodyMutationError> {
        match logistics_read_cell(read, cell)? {
            LogisticsReadV1::Value(LogisticsValueV1::Signed(value)) => Ok(value),
            _ => Err(BodyMutationError::Invalid("path signed field")),
        }
    };
    let settled_amount = signed(
        read,
        LogisticsCellV1::Column(LogisticsColumn::SettledAmount),
    )?;
    let remaining_recipe_amount = signed(
        read,
        LogisticsCellV1::Column(LogisticsColumn::RemainingRecipeAmount),
    )?;
    holds.push(route_fee);
    let owner_bytes = size_of::<CurrentLogisticsPathV1>()
        .checked_add(size_of::<LogisticsPathAuthorityV1>())
        .and_then(|n| n.checked_add(size_of::<Vec<MemoryReservation>>() + 64))
        .ok_or(ValueTreeError::ResourceLimited("path owner layout"))?;
    let owner_fee = charge_reserve(normal, memory, owner_bytes, "path owner")?;
    Ok(Some(owner_fee.retain(CurrentLogisticsPathV1 {
        value: LogisticsPathAuthorityV1 {
            path_id,
            route_ids,
            from_ledger,
            to_ledger,
            kind,
            settled_amount,
            remaining_recipe_amount,
        },
        _fees: holds,
    })))
}

fn read_cell(
    read: &mut impl for<'cell> FnMut(
        IndustryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<IndustryCellReadV1, BodyMutationError>,
    cell: IndustryCellV1<'_>,
) -> Result<IndustryCellReadV1, BodyMutationError> {
    let mut sink = std::io::sink();
    read(cell, &mut sink)
}

fn required_unsigned(
    read: &mut impl for<'cell> FnMut(
        IndustryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<IndustryCellReadV1, BodyMutationError>,
    column: IndustryColumnV1,
) -> Result<u64, BodyMutationError> {
    match read_cell(read, IndustryCellV1::Column(column))? {
        IndustryCellReadV1::Value(IndustryValueV1::Unsigned(value)) => Ok(value),
        _ => Err(invalid()),
    }
}

fn required_signed(
    read: &mut impl for<'cell> FnMut(
        IndustryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<IndustryCellReadV1, BodyMutationError>,
    column: IndustryColumnV1,
) -> Result<i64, BodyMutationError> {
    match read_cell(read, IndustryCellV1::Column(column))? {
        IndustryCellReadV1::Value(IndustryValueV1::Signed(value)) => Ok(value),
        _ => Err(invalid()),
    }
}

fn text(
    read: &mut impl for<'cell> FnMut(
        IndustryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<IndustryCellReadV1, BodyMutationError>,
    cell: IndustryCellV1<'_>,
    max_bytes: usize,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
    holds: &mut Vec<MemoryReservation>,
) -> Result<String, BodyMutationError> {
    let bound = max_bytes
        .checked_add(size_of::<String>() + 32)
        .ok_or(ValueTreeError::ResourceLimited("transit text layout"))?;
    let fee = charge_reserve(normal, memory, bound, "transit text allocation")?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(max_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("transit text capacity"))?;
    let IndustryCellReadV1::Value(IndustryValueV1::Text { utf8_bytes }) = read(cell, &mut bytes)?
    else {
        return Err(invalid());
    };
    if bytes.len() > max_bytes || usize::try_from(utf8_bytes).ok() != Some(bytes.len()) {
        return Err(invalid());
    }
    let value = String::from_utf8(bytes).map_err(|_| invalid())?;
    holds.push(fee);
    Ok(value)
}

fn optional_text(
    read: &mut impl for<'cell> FnMut(
        IndustryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<IndustryCellReadV1, BodyMutationError>,
    column: IndustryColumnV1,
    max_bytes: usize,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
    holds: &mut Vec<MemoryReservation>,
) -> Result<Option<String>, BodyMutationError> {
    let bound =
        max_bytes
            .checked_add(size_of::<String>() + 32)
            .ok_or(ValueTreeError::ResourceLimited(
                "transit optional text layout",
            ))?;
    let fee = charge_reserve(normal, memory, bound, "transit optional text allocation")?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(max_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("transit optional text capacity"))?;
    let value = read(IndustryCellV1::Column(column), &mut bytes)?;
    let result = match value {
        IndustryCellReadV1::Value(IndustryValueV1::Unit) if bytes.is_empty() => None,
        IndustryCellReadV1::Value(IndustryValueV1::Text { utf8_bytes })
            if bytes.len() <= max_bytes
                && usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            Some(String::from_utf8(bytes).map_err(|_| invalid())?)
        }
        _ => return Err(invalid()),
    };
    // The worst-case capacity was allocated even when the value is Unit.
    holds.push(fee);
    Ok(result)
}

fn ledger(
    read: &mut impl for<'cell> FnMut(
        IndustryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<IndustryCellReadV1, BodyMutationError>,
    column: IndustryColumnV1,
    max_bytes: usize,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
    holds: &mut Vec<MemoryReservation>,
) -> Result<MaterialLedgerId, BodyMutationError> {
    let bound = max_bytes
        .checked_add(size_of::<String>() + 32)
        .ok_or(ValueTreeError::ResourceLimited("transit ledger layout"))?;
    let fee = charge_reserve(normal, memory, bound, "transit ledger allocation")?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(max_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("transit ledger capacity"))?;
    let IndustryCellReadV1::Value(IndustryValueV1::Ledger(info)) =
        read(IndustryCellV1::Column(column), &mut bytes)?
    else {
        return Err(invalid());
    };
    if bytes.len() > max_bytes || usize::try_from(info.id_bytes).ok() != Some(bytes.len()) {
        return Err(invalid());
    }
    let id = String::from_utf8(bytes).map_err(|_| invalid())?;
    let value = match info.kind {
        LedgerValueKindV1::World if id.is_empty() => MaterialLedgerId::World,
        LedgerValueKindV1::Agent => MaterialLedgerId::Agent(id),
        LedgerValueKindV1::Site => MaterialLedgerId::Site(id),
        LedgerValueKindV1::Factory => MaterialLedgerId::Factory(id),
        _ => return Err(invalid()),
    };
    holds.push(fee);
    Ok(value)
}

/// Current decoded job. Its private fee owner covers every retained String,
/// route-id Vec and the record itself. It is not a settlement receipt.
pub(super) struct DecodedCurrentTransitV1 {
    pub(super) job: MaterialTransitJobState,
    _holds: Vec<MemoryReservation>,
}

/// Decode one exact Transit child from the caller's transaction overlay.
/// `read` must use `read_industry_cell_overlay_v1` for this same id, values,
/// writer pool, and cancellation source.
pub(super) fn decode_current_transit_v1(
    outer_id: u64,
    max_scalar_bytes: usize,
    max_items: usize,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
    read: &mut impl for<'cell> FnMut(
        IndustryCellV1<'cell>,
        &mut dyn Write,
    ) -> Result<IndustryCellReadV1, BodyMutationError>,
) -> Result<Option<ReservedValue<DecodedCurrentTransitV1>>, BodyMutationError> {
    match read_cell(read, IndustryCellV1::Record)? {
        IndustryCellReadV1::RecordAbsent => return Ok(None),
        IndustryCellReadV1::Value(IndustryValueV1::Unit) => {}
        _ => return Err(invalid()),
    }
    let hold_slots = 24usize
        .checked_add(
            max_items
                .checked_mul(2)
                .ok_or(ValueTreeError::ResourceLimited("transit fee slots"))?,
        )
        .ok_or(ValueTreeError::ResourceLimited("transit fee slots"))?;
    let hold_bytes = hold_slots
        .checked_mul(size_of::<MemoryReservation>())
        .and_then(|n| n.checked_add(size_of::<Vec<MemoryReservation>>() + 64))
        .ok_or(ValueTreeError::ResourceLimited("transit retained fees"))?;
    let layout = size_of::<DecodedCurrentTransitV1>()
        .checked_add(size_of::<MaterialTransitJobState>())
        .and_then(|n| n.checked_add(hold_bytes))
        .ok_or(ValueTreeError::ResourceLimited("transit decoded record"))?;
    let fee = charge_reserve(normal, memory, layout, "transit decoded record")?;
    let mut holds = Vec::new();
    holds
        .try_reserve_exact(hold_slots)
        .map_err(|_| ValueTreeError::ResourceLimited("transit fee capacity"))?;

    let job_id = required_unsigned(read, IndustryColumnV1::RecordJobId)?;
    if job_id != outer_id {
        return Err(invalid());
    }
    let requester_agent_id = text(
        read,
        IndustryCellV1::Column(IndustryColumnV1::RecordRequesterAgentId),
        max_scalar_bytes,
        memory,
        normal,
        &mut holds,
    )?;
    let from_ledger = ledger(
        read,
        IndustryColumnV1::RecordFromLedger,
        max_scalar_bytes,
        memory,
        normal,
        &mut holds,
    )?;
    let to_ledger = ledger(
        read,
        IndustryColumnV1::RecordToLedger,
        max_scalar_bytes,
        memory,
        normal,
        &mut holds,
    )?;
    let kind = text(
        read,
        IndustryCellV1::Column(IndustryColumnV1::RecordKind),
        max_scalar_bytes,
        memory,
        normal,
        &mut holds,
    )?;
    let amount = required_signed(read, IndustryColumnV1::RecordAmount)?;
    let distance_km = required_signed(read, IndustryColumnV1::RecordDistanceKm)?;
    let loss_bps = required_signed(read, IndustryColumnV1::RecordLossBps)?;
    let priority = match required_unsigned(read, IndustryColumnV1::RecordPriority)? {
        0 => MaterialTransitPriority::Standard,
        1 => MaterialTransitPriority::Urgent,
        _ => return Err(invalid()),
    };
    let route_id = optional_text(
        read,
        IndustryColumnV1::RecordRouteId,
        max_scalar_bytes,
        memory,
        normal,
        &mut holds,
    )?;
    let path_id = optional_text(
        read,
        IndustryColumnV1::RecordPathId,
        max_scalar_bytes,
        memory,
        normal,
        &mut holds,
    )?;
    let route_count = match read_cell(
        read,
        IndustryCellV1::Collection(IndustryCollectionV1::RouteIds),
    )? {
        IndustryCellReadV1::Value(IndustryValueV1::Unsigned(count)) => {
            usize::try_from(count).map_err(|_| invalid())?
        }
        _ => return Err(invalid()),
    };
    if route_count > max_items {
        return Err(ValueTreeError::ResourceLimited("transit route id count").into());
    }
    let route_vec_bytes = route_count
        .checked_mul(size_of::<String>())
        .and_then(|n| n.checked_add(size_of::<Vec<String>>() + 32))
        .ok_or(ValueTreeError::ResourceLimited("transit route id vector"))?;
    let route_fee = charge_reserve(normal, memory, route_vec_bytes, "transit route id vector")?;
    let mut route_ids = Vec::new();
    route_ids
        .try_reserve_exact(route_count)
        .map_err(|_| ValueTreeError::ResourceLimited("transit route id vector"))?;
    for index in 0..route_count {
        route_ids.push(text(
            read,
            IndustryCellV1::TextItem(IndustryCollectionV1::RouteIds, index as u64),
            max_scalar_bytes,
            memory,
            normal,
            &mut holds,
        )?);
    }
    holds.push(route_fee);
    let tariff_electricity_total =
        required_signed(read, IndustryColumnV1::RecordTariffElectricityTotal)?;
    let reroute_count = u32::try_from(required_unsigned(
        read,
        IndustryColumnV1::RecordRerouteCount,
    )?)
    .map_err(|_| invalid())?;
    let ready_at = required_unsigned(read, IndustryColumnV1::RecordReadyAt)?;

    let job = MaterialTransitJobState {
        job_id,
        requester_agent_id,
        from_ledger,
        to_ledger,
        kind,
        amount,
        distance_km,
        loss_bps,
        priority,
        route_id,
        path_id,
        route_ids,
        tariff_electricity_total,
        reroute_count,
        ready_at,
    };
    Ok(Some(
        fee.retain(DecodedCurrentTransitV1 { job, _holds: holds }),
    ))
}

/// Reopen all commitment fields from transaction Current. This is the only
/// decoder used for both the due-order pass and the final prewrite pass.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_transit_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: u64,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    current_read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &BoundNormalTickParentV1<'_>,
    roots: &RootSetV1,
    normal: &mut NormalTailWorkV1<'_, '_>,
    max_scalar_bytes: usize,
    max_items: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<DecodedCurrentTransitV1>>, BodyMutationError> {
    let result = (|| {
        step.transaction.validate_guard(guard, live)?;
        normal.validate_parent(parent).map_err(|_| invalid())?;
        let memory = step.mutation.memory;
        if !step.transaction.uses_budget(memory)
            || !normal.memory().same_pool(memory)
            || current_read.values.class != MemoryClass::Ordinary
            || !current_read.values.memory.same_pool(memory)
        {
            return Err(BodyMutationError::Invalid("transit Current read binding"));
        }
        roots.validate().map_err(|_| invalid())?;
        let _root_digest_fee = charge_reserve(normal, &memory, 4096, "transit root digest")?;
        if roots.digest().map_err(|_| invalid())? != step.transaction.parent_fence().parent_roots {
            return Err(BodyMutationError::Invalid("transit Current read binding"));
        }
        let source = current_read.values.source;
        let stop = RefCell::new(&mut current_read.cancelled);
        let mutation_cancel = RefCell::new(&mut step.mutation.cancelled);
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut *step.transaction,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut *current_read.values.io,
            },
            key_work: &mut *current_read.key_work,
            semantic_bytes: current_read.semantic_bytes,
            record_bytes: current_read.record_bytes,
            take_state_error: &mut current_read.take_state_error,
            cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
        };
        let decoded = decode_current_transit_v1(
            id,
            max_scalar_bytes,
            max_items,
            memory,
            normal,
            &mut |cell, output| {
                let mut sized_output = DynWriteAdapterV1(output);
                read_industry_cell_overlay_v1(
                    IndustryRecordKeyV1 {
                        kind: IndustryRecordKindV1::Transit,
                        id,
                    },
                    cell,
                    &mut overlay,
                    max_scalar_bytes as u64,
                    &mut sized_output,
                )
                .map_err(overlay_error)
            },
        )?;
        if decoded.as_ref().is_some_and(|value| value.job.job_id != id) {
            return Err(invalid());
        }
        if (stop.borrow_mut())() || (mutation_cancel.borrow_mut())() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let fresh = overlay.transaction.guard();
        overlay.transaction.validate_guard(&fresh, live)?;
        Ok(decoded)
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            step.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

/// The legacy due order is `(ready_at, priority, job_id)`. This copy contains
/// no authority; selected records are decoded again before reduction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::world) struct DueTransitOrderV1 {
    pub(in crate::world) ready_at: u64,
    pub(in crate::world) priority: MaterialTransitPriority,
    pub(in crate::world) job_id: u64,
}

pub(in crate::world) struct DueTransitOrderListV1 {
    entries: Vec<DueTransitOrderV1>,
    _extra_fee: Option<MemoryReservation>,
}

impl DueTransitOrderListV1 {
    pub(in crate::world) fn entries(&self) -> &[DueTransitOrderV1] {
        &self.entries
    }
}

/// A completed Current material-transit reducer. The private event is only
/// publishable after its business writes have been staged under this guard.
pub(in crate::world) fn collect_due_transit_order_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    roots: &RootSetV1,
    industry: &ReservedValue<IndustryFamilyQualificationV1>,
    context: &mut IndustryFamilyReadContextV1<'_, M, R, E, C>,
    industry_work: &mut IndustryWorkBudgetV1,
    parent: &BoundNormalTickParentV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    current_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    max_scalar_bytes: usize,
    max_items: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<DueTransitOrderListV1>, BodyMutationError> {
    let parent_maximum = usize::try_from(industry.record_counts()[1])
        .map_err(|_| ValueTreeError::ResourceLimited("transit parent count"))?;
    let maximum = parent_maximum
        .checked_add(oasis7_formal_protocol::incremental_execution::EXECUTION_MAX_WRITES)
        .ok_or(ValueTreeError::ResourceLimited(
            "transit due candidate count",
        ))?;
    let layout = maximum
        .checked_mul(size_of::<DueTransitOrderV1>())
        .and_then(|n| n.checked_add(size_of::<Vec<DueTransitOrderV1>>() + 64))
        .ok_or(ValueTreeError::ResourceLimited("transit due order layout"))?;
    let fee = charge_reserve(normal, memory, layout, "transit due order layout")?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(maximum)
        .map_err(|_| ValueTreeError::ResourceLimited("transit due order capacity"))?;
    let mut extra_fee = None;
    if entries.capacity() > maximum {
        let extra = entries
            .capacity()
            .checked_sub(maximum)
            .and_then(|n| n.checked_mul(size_of::<DueTransitOrderV1>()))
            .ok_or(ValueTreeError::ResourceLimited(
                "transit due order capacity",
            ))?;
        extra_fee = Some(charge_reserve(
            normal,
            memory,
            extra,
            "transit due order capacity",
        )?);
    }
    let scan_result = visit_qualified_transit_parent_identities_v1(
        roots,
        industry,
        context,
        industry_work,
        |identity| {
            let IndustryQualifiedOuterIdentityV1::Record { id, .. } = &*identity else {
                return Ok(());
            };
            if entries.len() == entries.capacity() {
                return Err(ValueTreeError::ResourceLimited(
                    "transit due candidate capacity",
                ));
            }
            entries.push(DueTransitOrderV1 {
                ready_at: 0,
                priority: MaterialTransitPriority::Standard,
                job_id: *id,
            });
            Ok(())
        },
    );
    scan_result.map_err(BodyMutationError::Value)?;

    // Include same-transaction additions and replacements. The key visitor
    // validates the registered Scheduler field and decodes original record
    // roots; its ids remain observations and are all reopened from Current.
    let current_read_cancelled = RefCell::new(&mut current_read.cancelled);
    let industry_read_cancelled = RefCell::new(&mut context.cancelled);
    let mut cancelled =
        || (current_read_cancelled.borrow_mut())() || (industry_read_cancelled.borrow_mut())();
    let mut transition =
        oasis7_world_state::incremental_industry_jobs::IndustryTransitionReadContextV1 {
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: current_read.values.source,
                memory: current_read.values.memory,
                class: current_read.values.class,
                io: &mut *current_read.values.io,
            },
            key_work: &mut *current_read.key_work,
            semantic_bytes: current_read.semantic_bytes,
            record_bytes: current_read.record_bytes,
            max_payload_bytes: context.max_payload_bytes,
            cancelled: &mut cancelled,
        };
    let pending_result = visit_transit_current_outer_changes_v1(
        step.transaction,
        industry,
        &mut transition,
        industry_work,
        |change| {
            let IndustryCurrentOuterChangeV1::Record {
                id, after: Some(_), ..
            } = change
            else {
                return Ok(());
            };
            if entries.len() == entries.capacity() {
                return Err(
                    oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
                        ValueTreeError::ResourceLimited("transit due candidate capacity"),
                    ),
                );
            }
            entries.push(DueTransitOrderV1 {
                ready_at: 0,
                priority: MaterialTransitPriority::Standard,
                job_id: id,
            });
            Ok(())
        },
    );
    pending_result.map_err(overlay_error)?;
    drop(transition);
    drop(cancelled);
    drop(current_read_cancelled);
    drop(industry_read_cancelled);

    // Deduplicate parent and pending candidates by the original outer id, then
    // reopen every candidate from Current so replacements, removals and
    // same-tick saturated ready times use the actual transaction result.
    let sort_work = maximum
        .checked_mul(usize::BITS as usize)
        .and_then(|n| n.checked_mul(size_of::<DueTransitOrderV1>()))
        .ok_or(ValueTreeError::ResourceLimited("transit due sort work"))?;
    normal.charge(
        sort_work
            .checked_mul(2)
            .ok_or(ValueTreeError::ResourceLimited("transit due sort work"))?,
    )?;
    entries.sort_unstable_by_key(|entry| entry.job_id);
    entries.dedup_by_key(|entry| entry.job_id);
    let candidates = entries.len();
    let mut due_count = 0usize;
    for index in 0..candidates {
        let id = entries[index].job_id;
        let fresh = step.transaction.guard();
        let current = read_current_transit_v1(
            id,
            step,
            current_read,
            parent,
            roots,
            normal,
            max_scalar_bytes,
            max_items,
            &fresh,
            live,
        )?;
        let Some(current) = current else {
            continue;
        };
        let job = &current.job;
        if job.job_id != id {
            return Err(ValueTreeError::Invalid("transit Current order id").into());
        }
        if job.ready_at <= now {
            entries[due_count] = DueTransitOrderV1 {
                ready_at: job.ready_at,
                priority: job.priority,
                job_id: job.job_id,
            };
            due_count += 1;
        }
    }
    entries.truncate(due_count);
    entries.sort_unstable();
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(fee.retain(DueTransitOrderListV1 {
        entries,
        _extra_fee: extra_fee,
    }))
}
