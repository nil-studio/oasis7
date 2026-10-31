//! Lossless Runtime-to-wire adapter. No Runtime/AgentAPI DTO or serde is moved.
use oasis7_client_api::world_service::resource_basis as wire;
use oasis7_runtime as runtime;
use wire::{DecimalU64, ResourceDecimalI64V1 as I};
fn no() -> String {
    "invalid resource basis conversion".into()
}
pub(crate) fn selection(
    v: &wire::ResourceBasisSelectionV1,
) -> Result<runtime::ResourceBasisSelectionV1, String> {
    v.validate()?;
    Ok(match v {
        wire::ResourceBasisSelectionV1::Transfer {
            from,
            to,
            material,
            amount,
            distance_km,
            priority,
            routing,
        } => runtime::ResourceBasisSelectionV1::Transfer {
            from: from.clone().into(),
            to: to.clone().into(),
            material: material.clone(),
            amount: amount.0,
            distance_km: distance_km.0,
            priority: *priority,
            routing: match routing {
                wire::ResourceRoutingV1::Direct => runtime::ResourceBasisRoutingV1::Direct,
                wire::ResourceRoutingV1::Routes { route_ids } => {
                    runtime::ResourceBasisRoutingV1::Routes {
                        route_ids: route_ids.clone(),
                    }
                }
            },
        },
        wire::ResourceBasisSelectionV1::Build {
            site,
            factory,
            factory_kind,
        } => runtime::ResourceBasisSelectionV1::Build {
            site: site.clone(),
            factory: factory.clone(),
            factory_kind: factory_kind.clone(),
        },
        wire::ResourceBasisSelectionV1::Schedule {
            factory,
            recipe,
            batches,
        } => runtime::ResourceBasisSelectionV1::Schedule {
            factory: factory.clone(),
            recipe: recipe.clone(),
            batches: *batches,
        },
    })
}
fn actor(v: &runtime::ResourceActorOwnerV1) -> wire::ResourceActorOwnerV1 {
    let runtime::ResourceActorOwnerV1 {
        player_id,
        source_id,
        public_key,
        generation,
    } = v;
    wire::ResourceActorOwnerV1 {
        player_id: player_id.clone(),
        source_id: source_id.clone(),
        public_key: public_key.clone(),
        generation: DecimalU64(*generation),
    }
}
fn owner(v: &runtime::ResourceLedgerOwnerV1) -> wire::ResourceLedgerOwnerV1 {
    match v {
        runtime::ResourceLedgerOwnerV1::World => wire::ResourceLedgerOwnerV1::World,
        runtime::ResourceLedgerOwnerV1::Agent { agent_id, owner } => {
            wire::ResourceLedgerOwnerV1::Agent {
                agent_id: agent_id.clone(),
                owner: actor(owner),
            }
        }
        runtime::ResourceLedgerOwnerV1::Site {
            site_id,
            owner_agent_id,
            authority_revision,
        } => wire::ResourceLedgerOwnerV1::Site {
            site_id: site_id.clone(),
            owner_agent_id: owner_agent_id.clone(),
            authority_revision: DecimalU64(*authority_revision),
        },
        runtime::ResourceLedgerOwnerV1::Factory {
            factory_id,
            builder_agent_id,
        } => wire::ResourceLedgerOwnerV1::Factory {
            factory_id: factory_id.clone(),
            builder_agent_id: builder_agent_id.clone(),
        },
    }
}
fn object(v: &runtime::ResourceObjectFactV1) -> wire::ResourceObjectFactV1 {
    match v {
        runtime::ResourceObjectFactV1::Site {
            site_id,
            owner_agent_id,
            authorized_agent_ids,
            authority_revision,
            location_id,
        } => wire::ResourceObjectFactV1::Site {
            site_id: site_id.clone(),
            owner_agent_id: owner_agent_id.clone(),
            authorized_agent_ids: authorized_agent_ids.clone(),
            authority_revision: DecimalU64(*authority_revision),
            location_id: location_id.clone(),
        },
        runtime::ResourceObjectFactV1::Factory {
            factory_id,
            builder_agent_id,
            input_ledger,
            output_ledger,
        } => wire::ResourceObjectFactV1::Factory {
            factory_id: factory_id.clone(),
            builder_agent_id: builder_agent_id.clone(),
            input_ledger: input_ledger.into(),
            output_ledger: output_ledger.into(),
        },
    }
}
pub(crate) fn resolution(
    world: &runtime::World,
    agent: &str,
    v: &wire::ResourceBasisSelectionV1,
) -> Result<wire::ResourceBasisResolutionV1, String> {
    let s = selection(v)?;
    match world.native_resource_basis(agent, &s) {
        Ok(f) => Ok(wire::ResourceBasisResolutionV1::Ready {
            fact: Box::new(fact(&f, v)?),
        }),
        Err(e) => Ok(wire::ResourceBasisResolutionV1::Blocked {
            reason: match e {
                runtime::ResourceBasisBlockerV1::InvalidSelection => {
                    wire::ResourceBasisBlockerV1::InvalidSelection
                }
                runtime::ResourceBasisBlockerV1::AgentOwnerUnavailable => {
                    wire::ResourceBasisBlockerV1::AgentOwnerUnavailable
                }
                runtime::ResourceBasisBlockerV1::AdmissionUnavailable => {
                    wire::ResourceBasisBlockerV1::AdmissionUnavailable
                }
                runtime::ResourceBasisBlockerV1::LedgerOrObjectUnavailable => {
                    wire::ResourceBasisBlockerV1::LedgerOrObjectUnavailable
                }
                runtime::ResourceBasisBlockerV1::DomainUnavailable => {
                    wire::ResourceBasisBlockerV1::DomainUnavailable
                }
            },
        }),
    }
}
pub(crate) fn fact(
    v: &runtime::NativeResourceBasisFactV1,
    s: &wire::ResourceBasisSelectionV1,
) -> Result<wire::ResourceBasisFactV1, String> {
    let runtime::NativeResourceBasisFactV1 {
        agent_id,
        actor_owner,
        quoted_at_tick,
        actor_electricity_before,
        selection: original,
        quote,
        ledgers,
        objects,
        derived_resource_refs,
    } = v;
    if selection(s)? != *original {
        return Err(no());
    }
    let quote = match quote {
        runtime::ResourceBasisQuoteV1::Transfer(v) => {
            let runtime::ResourceTransferQuoteV1 {
                action,
                authorization,
                quote,
            } = v.as_ref();
            wire::ResourceBasisQuoteV1::Transfer(Box::new(wire::ResourceTransferQuoteV1 {
                action: action.try_into()?,
                authorization: authorization.into(),
                quote: transfer(quote)?,
            }))
        }
        runtime::ResourceBasisQuoteV1::Build(v) => {
            wire::ResourceBasisQuoteV1::Build(Box::new(v.as_ref().try_into()?))
        }
        runtime::ResourceBasisQuoteV1::Schedule(v) => {
            wire::ResourceBasisQuoteV1::Schedule(Box::new(v.as_ref().try_into()?))
        }
    };
    Ok(wire::ResourceBasisFactV1 {
        agent_id: agent_id.clone(),
        actor_owner: actor(actor_owner),
        quoted_at_tick: DecimalU64(*quoted_at_tick),
        actor_electricity_before: I(*actor_electricity_before),
        selection: s.clone(),
        quote,
        ledgers: ledgers
            .iter()
            .map(|v| {
                let runtime::ResourceLedgerFactV1 {
                    ledger,
                    owner: owned,
                    exists,
                    materials,
                } = v;
                wire::ResourceLedgerFactV1 {
                    ledger: ledger.into(),
                    owner: owner(owned),
                    exists: *exists,
                    materials: materials
                        .iter()
                        .map(|v| {
                            let runtime::ResourceMaterialBalanceV1 { material, amount } = v;
                            wire::ResourceMaterialBalanceV1 {
                                material: material.clone(),
                                amount: amount.map(I),
                            }
                        })
                        .collect(),
                }
            })
            .collect(),
        objects: objects.iter().map(object).collect(),
        derived_resource_refs: derived_resource_refs.clone(),
    })
}
pub(super) fn transfer(
    v: &runtime::LogisticsTransferQuote,
) -> Result<wire::ResourceLogisticsTransferQuoteV1, String> {
    let runtime::LogisticsTransferQuote {
        requester_agent_id,
        from_ledger,
        to_ledger,
        kind,
        requested_amount,
        submission_feasible,
        max_transferable_amount,
        sent_amount,
        distance_km,
        loss_bps,
        expected_loss_amount,
        expected_received_amount,
        source_amount_before,
        source_amount_after,
        destination_amount_before,
        destination_expected_amount_after,
        ticks_until_arrival,
        ready_at,
        effective_priority,
        priority_reason,
        inflight_before,
        inflight_capacity,
        path_id,
        route_ids,
        tariff_electricity_total,
        reroute_count,
        recommendation,
        conditional,
    } = v;
    Ok(wire::ResourceLogisticsTransferQuoteV1 {
        requester_agent_id: requester_agent_id.clone(),
        from_ledger: from_ledger.into(),
        to_ledger: to_ledger.into(),
        kind: kind.clone(),
        requested_amount: I(*requested_amount),
        submission_feasible: *submission_feasible,
        max_transferable_amount: I(*max_transferable_amount),
        sent_amount: I(*sent_amount),
        distance_km: I(*distance_km),
        loss_bps: I(*loss_bps),
        expected_loss_amount: I(*expected_loss_amount),
        expected_received_amount: I(*expected_received_amount),
        source_amount_before: I(*source_amount_before),
        source_amount_after: I(*source_amount_after),
        destination_amount_before: I(*destination_amount_before),
        destination_expected_amount_after: I(*destination_expected_amount_after),
        ticks_until_arrival: DecimalU64(*ticks_until_arrival),
        ready_at: DecimalU64(*ready_at),
        effective_priority: *effective_priority,
        priority_reason: priority_reason.clone(),
        inflight_before: DecimalU64(u64::try_from(*inflight_before).map_err(|_| no())?),
        inflight_capacity: DecimalU64(u64::try_from(*inflight_capacity).map_err(|_| no())?),
        path_id: path_id.clone(),
        route_ids: route_ids.clone(),
        tariff_electricity_total: I(*tariff_electricity_total),
        reroute_count: *reroute_count,
        recommendation: recommendation.clone(),
        conditional: *conditional,
    })
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
