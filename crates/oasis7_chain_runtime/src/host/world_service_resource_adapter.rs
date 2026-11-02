//! Compatibility path for the unchanged shared Runtime-to-wire adapter.
pub(crate) use oasis7::world_service::resource_basis_runtime_adapter_v1::resolution;
#[cfg(test)]
pub(crate) use oasis7::world_service::resource_basis_runtime_adapter_v1::{fact, selection};
#[cfg(test)]
use oasis7_client_api::world_service::resource_basis as wire;
#[cfg(test)]
use oasis7_runtime as runtime;
#[cfg(test)]
fn no() -> String {
    "invalid resource basis conversion".into()
}
#[cfg(test)]
pub(crate) fn transfer_native(
    v: wire::ResourceLogisticsTransferQuoteV1,
) -> Result<runtime::LogisticsTransferQuote, String> {
    Ok(runtime::LogisticsTransferQuote {
        requester_agent_id: v.requester_agent_id,
        from_ledger: v.from_ledger.into(),
        to_ledger: v.to_ledger.into(),
        kind: v.kind,
        requested_amount: v.requested_amount.0,
        submission_feasible: v.submission_feasible,
        max_transferable_amount: v.max_transferable_amount.0,
        sent_amount: v.sent_amount.0,
        distance_km: v.distance_km.0,
        loss_bps: v.loss_bps.0,
        expected_loss_amount: v.expected_loss_amount.0,
        expected_received_amount: v.expected_received_amount.0,
        source_amount_before: v.source_amount_before.0,
        source_amount_after: v.source_amount_after.0,
        destination_amount_before: v.destination_amount_before.0,
        destination_expected_amount_after: v.destination_expected_amount_after.0,
        ticks_until_arrival: v.ticks_until_arrival.0,
        ready_at: v.ready_at.0,
        effective_priority: v.effective_priority,
        priority_reason: v.priority_reason,
        inflight_before: usize::try_from(v.inflight_before.0).map_err(|_| no())?,
        inflight_capacity: usize::try_from(v.inflight_capacity.0).map_err(|_| no())?,
        path_id: v.path_id,
        route_ids: v.route_ids,
        tariff_electricity_total: v.tariff_electricity_total.0,
        reroute_count: v.reroute_count,
        recommendation: v.recommendation,
        conditional: v.conditional,
    })
}
