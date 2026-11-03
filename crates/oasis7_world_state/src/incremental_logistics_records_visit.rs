//! Borrowed, exhaustive logistics authority values; IDs retain their original bytes.
use super::*;
use crate::{LogisticsPathAuthorityV1, LogisticsRouteV1, MaterialTransitPriority};
pub(super) fn route<'a, E: From<MemberRecordError>>(
    value: &'a LogisticsRouteV1,
    emit: &mut impl FnMut(LogisticsCellV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let LogisticsRouteV1 {
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
    } = value;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::RouteId),
        MemberPayloadRefV1::Text(route_id.as_str()),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::FromLedger),
        MemberPayloadRefV1::Ledger(from_ledger),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::ToLedger),
        MemberPayloadRefV1::Ledger(to_ledger),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::Kind),
        MemberPayloadRefV1::Text(kind.as_str()),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::DistanceKm),
        MemberPayloadRefV1::I64(*distance_km),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::Priority),
        MemberPayloadRefV1::U64(match priority {
            MaterialTransitPriority::Standard => 0,
            MaterialTransitPriority::Urgent => 1,
        }),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::OwnerAgentId),
        MemberPayloadRefV1::Text(owner_agent_id.as_str()),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::Available),
        MemberPayloadRefV1::Bool(*available),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::CapacityUnits),
        MemberPayloadRefV1::I64(*capacity_units),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::ReservedCapacityUnits),
        MemberPayloadRefV1::I64(*reserved_capacity_units),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::TariffElectricityPerUnit),
        MemberPayloadRefV1::I64(*tariff_electricity_per_unit),
    )?;
    Ok(())
}
pub(super) fn path<'a, E: From<MemberRecordError>>(
    value: &'a LogisticsPathAuthorityV1,
    emit: &mut impl FnMut(LogisticsCellV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let LogisticsPathAuthorityV1 {
        path_id,
        route_ids,
        from_ledger,
        to_ledger,
        kind,
        settled_amount,
        remaining_recipe_amount,
    } = value;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::PathId),
        MemberPayloadRefV1::Text(path_id.as_str()),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::FromLedger),
        MemberPayloadRefV1::Ledger(from_ledger),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::ToLedger),
        MemberPayloadRefV1::Ledger(to_ledger),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::Kind),
        MemberPayloadRefV1::Text(kind.as_str()),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::SettledAmount),
        MemberPayloadRefV1::I64(*settled_amount),
    )?;
    emit(
        LogisticsCellV1::Column(LogisticsColumnV1::RemainingRecipeAmount),
        MemberPayloadRefV1::I64(*remaining_recipe_amount),
    )?;
    let count = u64::try_from(route_ids.len())
        .map_err(|_| MemberRecordError::Invalid("logistics route vector length"))?;
    emit(
        LogisticsCellV1::Collection(LogisticsCollectionV1::RouteIds),
        MemberPayloadRefV1::U64(count),
    )?;
    for (index, id) in route_ids.iter().enumerate() {
        emit(
            LogisticsCellV1::TextItem(LogisticsCollectionV1::RouteIds, index as u64),
            MemberPayloadRefV1::Text(id),
        )?;
    }
    Ok(())
}
