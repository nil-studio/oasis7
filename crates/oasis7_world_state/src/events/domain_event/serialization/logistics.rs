use super::*;

#[derive(serde::Serialize)]
#[serde(rename = "MaterialTransferred")]
struct MaterialTransferredPayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    transfer_id: &'a Option<ActionId>,
    requester_agent_id: &'a String,
    from_ledger: &'a MaterialLedgerId,
    to_ledger: &'a MaterialLedgerId,
    kind: &'a String,
    amount: &'a i64,
    distance_km: &'a i64,
    priority: &'a MaterialTransitPriority,
    #[serde(skip_serializing_if = "Option::is_none")]
    route_id: &'a Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename = "LogisticsRouteRegistered")]
struct LogisticsRouteRegisteredPayload<'a> {
    requester_agent_id: &'a String,
    #[serde(skip_serializing_if = "Option::is_none")]
    route_id: &'a Option<String>,
    from_ledger: &'a MaterialLedgerId,
    to_ledger: &'a MaterialLedgerId,
    kind: &'a String,
    distance_km: &'a i64,
    priority: &'a MaterialTransitPriority,
    owner_agent_id: &'a String,
    available: &'a bool,
    capacity_units: &'a i64,
    tariff_electricity_per_unit: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "LogisticsRouteAvailabilityChanged")]
struct LogisticsRouteAvailabilityChangedPayload<'a> {
    requester_agent_id: &'a String,
    route_id: &'a String,
    available: &'a bool,
    owner_agent_id: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "LogisticsPathRerouted")]
struct LogisticsPathReroutedPayload<'a> {
    requester_agent_id: &'a String,
    job_id: &'a ActionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_path_id: &'a Option<String>,
    original_route_ids: &'a Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effective_path_id: &'a Option<String>,
    effective_route_ids: &'a Vec<String>,
    reason: &'a String,
    reroute_count: &'a u32,
    tariff_electricity_total: &'a i64,
    owner_payouts: &'a Vec<LogisticsOwnerPayout>,
    governance_tax_electricity: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "MaterialTransitStarted")]
struct MaterialTransitStartedPayload<'a> {
    job_id: &'a ActionId,
    requester_agent_id: &'a String,
    from_ledger: &'a MaterialLedgerId,
    to_ledger: &'a MaterialLedgerId,
    kind: &'a String,
    amount: &'a i64,
    distance_km: &'a i64,
    loss_bps: &'a i64,
    ready_at: &'a WorldTime,
    priority: &'a MaterialTransitPriority,
    #[serde(skip_serializing_if = "Option::is_none")]
    route_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path_id: &'a Option<String>,
    route_ids: &'a Vec<String>,
    tariff_electricity_total: &'a i64,
    reroute_count: &'a u32,
}

#[derive(serde::Serialize)]
#[serde(rename = "MaterialTransitCompleted")]
struct MaterialTransitCompletedPayload<'a> {
    job_id: &'a ActionId,
    requester_agent_id: &'a String,
    from_ledger: &'a MaterialLedgerId,
    to_ledger: &'a MaterialLedgerId,
    kind: &'a String,
    sent_amount: &'a i64,
    received_amount: &'a i64,
    loss_amount: &'a i64,
    distance_km: &'a i64,
    priority: &'a MaterialTransitPriority,
    #[serde(skip_serializing_if = "Option::is_none")]
    route_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path_id: &'a Option<String>,
    route_ids: &'a Vec<String>,
    tariff_electricity_total: &'a i64,
    reroute_count: &'a u32,
}

pub(super) fn serialize<S: serde::Serializer>(
    event: &DomainEvent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match event {
        DomainEvent::MaterialTransferred {
            transfer_id,
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            amount,
            distance_km,
            priority,
            route_id,
        } => serialize_adjacent(
            serializer,
            Tag::MATERIAL_TRANSFERRED,
            &MaterialTransferredPayload {
                transfer_id,
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                amount,
                distance_km,
                priority,
                route_id,
            },
        ),
        DomainEvent::LogisticsRouteRegistered {
            requester_agent_id,
            route_id,
            from_ledger,
            to_ledger,
            kind,
            distance_km,
            priority,
            owner_agent_id,
            available,
            capacity_units,
            tariff_electricity_per_unit,
        } => serialize_adjacent(
            serializer,
            Tag::LOGISTICS_ROUTE_REGISTERED,
            &LogisticsRouteRegisteredPayload {
                requester_agent_id,
                route_id,
                from_ledger,
                to_ledger,
                kind,
                distance_km,
                priority,
                owner_agent_id,
                available,
                capacity_units,
                tariff_electricity_per_unit,
            },
        ),
        DomainEvent::LogisticsRouteAvailabilityChanged {
            requester_agent_id,
            route_id,
            available,
            owner_agent_id,
        } => serialize_adjacent(
            serializer,
            Tag::LOGISTICS_ROUTE_AVAILABILITY_CHANGED,
            &LogisticsRouteAvailabilityChangedPayload {
                requester_agent_id,
                route_id,
                available,
                owner_agent_id,
            },
        ),
        DomainEvent::LogisticsPathRerouted {
            requester_agent_id,
            job_id,
            original_path_id,
            original_route_ids,
            effective_path_id,
            effective_route_ids,
            reason,
            reroute_count,
            tariff_electricity_total,
            owner_payouts,
            governance_tax_electricity,
        } => serialize_adjacent(
            serializer,
            Tag::LOGISTICS_PATH_REROUTED,
            &LogisticsPathReroutedPayload {
                requester_agent_id,
                job_id,
                original_path_id,
                original_route_ids,
                effective_path_id,
                effective_route_ids,
                reason,
                reroute_count,
                tariff_electricity_total,
                owner_payouts,
                governance_tax_electricity,
            },
        ),
        DomainEvent::MaterialTransitStarted {
            job_id,
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            amount,
            distance_km,
            loss_bps,
            ready_at,
            priority,
            route_id,
            path_id,
            route_ids,
            tariff_electricity_total,
            reroute_count,
        } => serialize_adjacent(
            serializer,
            Tag::MATERIAL_TRANSIT_STARTED,
            &MaterialTransitStartedPayload {
                job_id,
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                amount,
                distance_km,
                loss_bps,
                ready_at,
                priority,
                route_id,
                path_id,
                route_ids,
                tariff_electricity_total,
                reroute_count,
            },
        ),
        DomainEvent::MaterialTransitCompleted {
            job_id,
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            sent_amount,
            received_amount,
            loss_amount,
            distance_km,
            priority,
            route_id,
            path_id,
            route_ids,
            tariff_electricity_total,
            reroute_count,
        } => serialize_adjacent(
            serializer,
            Tag::MATERIAL_TRANSIT_COMPLETED,
            &MaterialTransitCompletedPayload {
                job_id,
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                sent_amount,
                received_amount,
                loss_amount,
                distance_km,
                priority,
                route_id,
                path_id,
                route_ids,
                tariff_electricity_total,
                reroute_count,
            },
        ),
        _ => unreachable!("DomainEvent serializer group mismatch"),
    }
}
