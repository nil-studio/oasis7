use super::*;

#[derive(serde::Serialize)]
#[serde(rename = "FactoryBuildStarted")]
struct FactoryBuildStartedPayload<'a> {
    job_id: &'a ActionId,
    builder_agent_id: &'a String,
    site_id: &'a String,
    spec: &'a FactoryModuleSpec,
    consume_ledger: &'a MaterialLedgerId,
    ready_at: &'a WorldTime,
    contract_version: &'a Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    site_authority_revision: &'a Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    site_location_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location_anchor_revision: &'a Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    construction_power_obligation: &'a Option<FactoryBuildPowerObligationV1>,
}

#[derive(serde::Serialize)]
#[serde(rename = "FactoryBuilt")]
struct FactoryBuiltPayload<'a> {
    job_id: &'a ActionId,
    builder_agent_id: &'a String,
    site_id: &'a String,
    spec: &'a FactoryModuleSpec,
}

#[derive(serde::Serialize)]
#[serde(rename = "FactoryDurabilityChanged")]
struct FactoryDurabilityChangedPayload<'a> {
    factory_id: &'a String,
    previous_durability_ppm: &'a i64,
    durability_ppm: &'a i64,
    reason: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "FactoryMaintained")]
struct FactoryMaintainedPayload<'a> {
    operator_agent_id: &'a String,
    factory_id: &'a String,
    consume_ledger: &'a MaterialLedgerId,
    consumed_parts: &'a i64,
    durability_ppm: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "FactoryRecycled")]
struct FactoryRecycledPayload<'a> {
    operator_agent_id: &'a String,
    factory_id: &'a String,
    recycle_ledger: &'a MaterialLedgerId,
    recovered: &'a Vec<MaterialStack>,
    durability_ppm: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "RecipeStarted")]
struct RecipeStartedPayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    emergency_power_obligation: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    committed_recipe_origin: &'a Option<super::CommittedRecipeOrigin>,
    job_id: &'a ActionId,
    requester_agent_id: &'a String,
    factory_id: &'a String,
    recipe_id: &'a String,
    accepted_batches: &'a u32,
    consume: &'a Vec<MaterialStack>,
    produce: &'a Vec<MaterialStack>,
    byproducts: &'a Vec<MaterialStack>,
    power_required: &'a i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    power_owner_agent_id: &'a Option<String>,
    duration_ticks: &'a u32,
    consume_ledger: &'a MaterialLedgerId,
    output_ledger: &'a MaterialLedgerId,
    bottleneck_tags: &'a Vec<String>,
    market_quotes: &'a Vec<MaterialMarketQuote>,
    logistics_route_ids: &'a Vec<String>,
    logistics_path_ids: &'a Vec<String>,
    ready_at: &'a WorldTime,
}

#[derive(serde::Serialize)]
#[serde(rename = "RecipeCompleted")]
struct RecipeCompletedPayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    committed_recipe_origin: &'a Option<super::CommittedRecipeOrigin>,
    job_id: &'a ActionId,
    requester_agent_id: &'a String,
    factory_id: &'a String,
    recipe_id: &'a String,
    accepted_batches: &'a u32,
    produce: &'a Vec<MaterialStack>,
    byproducts: &'a Vec<MaterialStack>,
    output_ledger: &'a MaterialLedgerId,
    bottleneck_tags: &'a Vec<String>,
    logistics_route_ids: &'a Vec<String>,
    logistics_path_ids: &'a Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename = "FactoryProductionBlocked")]
struct FactoryProductionBlockedPayload<'a> {
    action_id: &'a ActionId,
    requester_agent_id: &'a String,
    factory_id: &'a String,
    recipe_id: &'a String,
    blocker_kind: &'a String,
    blocker_detail: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "FactoryProductionResumed")]
struct FactoryProductionResumedPayload<'a> {
    job_id: &'a ActionId,
    requester_agent_id: &'a String,
    factory_id: &'a String,
    recipe_id: &'a String,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_blocked_at: &'a Option<WorldTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_blocker_kind: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_blocker_detail: &'a Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename = "FactoryProductionPaused")]
struct FactoryProductionPausedPayload<'a> {
    action_id: &'a ActionId,
    requester_agent_id: &'a String,
    factory_id: &'a String,
    reason: &'a String,
}

pub(super) fn serialize<S: serde::Serializer>(
    event: &DomainEvent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match event {
        DomainEvent::FactoryBuildStarted {
            job_id,
            builder_agent_id,
            site_id,
            spec,
            consume_ledger,
            ready_at,
            contract_version,
            site_authority_revision,
            site_location_id,
            location_anchor_revision,
            construction_power_obligation,
        } => serialize_adjacent(
            serializer,
            Tag::FactoryBuildStarted,
            &FactoryBuildStartedPayload {
                job_id,
                builder_agent_id,
                site_id,
                spec,
                consume_ledger,
                ready_at,
                contract_version,
                site_authority_revision,
                site_location_id,
                location_anchor_revision,
                construction_power_obligation,
            },
        ),
        DomainEvent::FactoryBuilt {
            job_id,
            builder_agent_id,
            site_id,
            spec,
        } => serialize_adjacent(
            serializer,
            Tag::FactoryBuilt,
            &FactoryBuiltPayload {
                job_id,
                builder_agent_id,
                site_id,
                spec,
            },
        ),
        DomainEvent::FactoryDurabilityChanged {
            factory_id,
            previous_durability_ppm,
            durability_ppm,
            reason,
        } => serialize_adjacent(
            serializer,
            Tag::FactoryDurabilityChanged,
            &FactoryDurabilityChangedPayload {
                factory_id,
                previous_durability_ppm,
                durability_ppm,
                reason,
            },
        ),
        DomainEvent::FactoryMaintained {
            operator_agent_id,
            factory_id,
            consume_ledger,
            consumed_parts,
            durability_ppm,
        } => serialize_adjacent(
            serializer,
            Tag::FactoryMaintained,
            &FactoryMaintainedPayload {
                operator_agent_id,
                factory_id,
                consume_ledger,
                consumed_parts,
                durability_ppm,
            },
        ),
        DomainEvent::FactoryRecycled {
            operator_agent_id,
            factory_id,
            recycle_ledger,
            recovered,
            durability_ppm,
        } => serialize_adjacent(
            serializer,
            Tag::FactoryRecycled,
            &FactoryRecycledPayload {
                operator_agent_id,
                factory_id,
                recycle_ledger,
                recovered,
                durability_ppm,
            },
        ),
        DomainEvent::RecipeStarted {
            emergency_power_obligation,
            committed_recipe_origin,
            job_id,
            requester_agent_id,
            factory_id,
            recipe_id,
            accepted_batches,
            consume,
            produce,
            byproducts,
            power_required,
            power_owner_agent_id,
            duration_ticks,
            consume_ledger,
            output_ledger,
            bottleneck_tags,
            market_quotes,
            logistics_route_ids,
            logistics_path_ids,
            ready_at,
        } => serialize_adjacent(
            serializer,
            Tag::RecipeStarted,
            &RecipeStartedPayload {
                emergency_power_obligation,
                committed_recipe_origin,
                job_id,
                requester_agent_id,
                factory_id,
                recipe_id,
                accepted_batches,
                consume,
                produce,
                byproducts,
                power_required,
                power_owner_agent_id,
                duration_ticks,
                consume_ledger,
                output_ledger,
                bottleneck_tags,
                market_quotes,
                logistics_route_ids,
                logistics_path_ids,
                ready_at,
            },
        ),
        DomainEvent::RecipeCompleted {
            committed_recipe_origin,
            job_id,
            requester_agent_id,
            factory_id,
            recipe_id,
            accepted_batches,
            produce,
            byproducts,
            output_ledger,
            bottleneck_tags,
            logistics_route_ids,
            logistics_path_ids,
        } => serialize_adjacent(
            serializer,
            Tag::RecipeCompleted,
            &RecipeCompletedPayload {
                committed_recipe_origin,
                job_id,
                requester_agent_id,
                factory_id,
                recipe_id,
                accepted_batches,
                produce,
                byproducts,
                output_ledger,
                bottleneck_tags,
                logistics_route_ids,
                logistics_path_ids,
            },
        ),
        DomainEvent::FactoryProductionBlocked {
            action_id,
            requester_agent_id,
            factory_id,
            recipe_id,
            blocker_kind,
            blocker_detail,
        } => serialize_adjacent(
            serializer,
            Tag::FactoryProductionBlocked,
            &FactoryProductionBlockedPayload {
                action_id,
                requester_agent_id,
                factory_id,
                recipe_id,
                blocker_kind,
                blocker_detail,
            },
        ),
        DomainEvent::FactoryProductionResumed {
            job_id,
            requester_agent_id,
            factory_id,
            recipe_id,
            previous_blocked_at,
            previous_blocker_kind,
            previous_blocker_detail,
        } => serialize_adjacent(
            serializer,
            Tag::FactoryProductionResumed,
            &FactoryProductionResumedPayload {
                job_id,
                requester_agent_id,
                factory_id,
                recipe_id,
                previous_blocked_at,
                previous_blocker_kind,
                previous_blocker_detail,
            },
        ),
        DomainEvent::FactoryProductionPaused {
            action_id,
            requester_agent_id,
            factory_id,
            reason,
        } => serialize_adjacent(
            serializer,
            Tag::FactoryProductionPaused,
            &FactoryProductionPausedPayload {
                action_id,
                requester_agent_id,
                factory_id,
                reason,
            },
        ),
        _ => unreachable!("DomainEvent serializer group mismatch"),
    }
}
