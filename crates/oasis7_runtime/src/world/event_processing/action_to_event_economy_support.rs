use super::*;
use oasis7_wasm_abi::FactoryModuleSpec;

impl World {
    pub(crate) fn schedule_recipe_local_scarcity_delay_for_quote(
        &self,
        factory_id: &str,
        recipe_id: &str,
        consume: &[MaterialStack],
        produce: &[MaterialStack],
    ) -> Result<(i64, String), String> {
        let factory = self
            .state
            .factories
            .get(factory_id)
            .ok_or_else(|| format!("factory not found: {factory_id}"))?;
        let effective_consume = merge_recipe_consume_with_maintenance_sink(self, consume, produce);
        let preferred_consume_ledger = factory.input_ledger.clone();
        // Quote the same site-bound ledger that ScheduleRecipe will use. The
        // BuildFactory compatibility fallback must not make a recipe quote
        // appear affordable from global stock.
        let consume_ledger = preferred_consume_ledger.clone();
        for stack in &effective_consume {
            if stack.amount <= 0 {
                return Err(format!(
                    "recipe consume must be > 0: {}={}",
                    stack.kind, stack.amount
                ));
            }
            let available = self.ledger_material_balance(&consume_ledger, stack.kind.as_str());
            if available < stack.amount {
                return Err(format!(
                    "insufficient material {}: requested {}, available {}",
                    stack.kind, stack.amount, available
                ));
            }
        }
        let bottleneck_tags =
            resolve_recipe_bottleneck_tags(self.recipe_profile(recipe_id), &effective_consume);
        let delay = compute_local_scarcity_delay_ticks(
            self,
            &preferred_consume_ledger,
            &consume_ledger,
            &effective_consume,
            &bottleneck_tags,
        ) as i64;
        let reason = match delay {
            0 => "none",
            1 => "local_bottleneck_deficit_moderate",
            _ => "local_bottleneck_deficit_severe",
        };
        Ok((delay, reason.to_string()))
    }
}

pub(super) fn aggregate_material_stacks_for_admission(
    label: &str,
    stacks: &[MaterialStack],
) -> Result<BTreeMap<String, i64>, String> {
    let mut aggregated = BTreeMap::<String, i64>::new();
    for stack in stacks {
        if stack.kind.trim().is_empty() || stack.amount <= 0 {
            return Err(format!(
                "{label} must be positive and named: {}={}",
                stack.kind, stack.amount
            ));
        }
        let total = aggregated.entry(stack.kind.clone()).or_insert(0);
        *total = (*total)
            .checked_add(stack.amount)
            .ok_or_else(|| format!("{label} amount overflow for material kind {}", stack.kind))?;
    }
    Ok(aggregated)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Collect-data authentication fields are a stable signed request schema."
)]
pub(super) fn authenticated_collect_data_to_event(
    world: &World,
    action_id: ActionId,
    collector_agent_id: &str,
    electricity_cost: i64,
    data_amount: i64,
    player_id: &str,
    public_key: &str,
    nonce: u64,
    signature: &str,
) -> Result<WorldEventBody, WorldError> {
    if let Err(error) = oasis7_proto::collect_data_auth::verify_authorization(
        oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
        electricity_cost,
        data_amount,
        player_id,
        public_key,
        nonce,
        signature,
    ) {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!(
                    "authenticated collect_data signature invalid: {error}"
                )],
            },
        }));
    }
    let matching_claims = world
        .state
        .starter_oc_claims
        .values()
        .filter(|claim| {
            claim.player_id == player_id && claim.public_key.as_deref() == Some(public_key)
        })
        .collect::<Vec<_>>();
    if matching_claims.len() != 1 || matching_claims[0].agent_id != collector_agent_id {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!(
                    "authenticated collect_data requires exactly one starter OC player/key binding for collector {collector_agent_id}; found {}",
                    matching_claims.len()
                )],
            },
        }));
    }
    let last_nonce = world
        .state
        .authenticated_collect_data_last_nonces
        .get(player_id)
        .and_then(|by_key| by_key.get(public_key))
        .copied()
        .unwrap_or(0);
    if nonce == 0 || nonce <= last_nonce {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!(
                    "authenticated collect_data nonce replay: expected nonce > {last_nonce}, received {nonce}"
                )],
            },
        }));
    }
    if !world.state.agents.contains_key(collector_agent_id) {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::AgentNotFound {
                agent_id: collector_agent_id.to_string(),
            },
        }));
    }
    if electricity_cost <= 0 || data_amount <= 0 {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::InvalidAmount {
                amount: if electricity_cost <= 0 {
                    electricity_cost
                } else {
                    data_amount
                },
            },
        }));
    }
    let available = world
        .state
        .agents
        .get(collector_agent_id)
        .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
        .unwrap_or(0);
    if available < electricity_cost {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::InsufficientResource {
                agent_id: collector_agent_id.to_string(),
                kind: ResourceKind::Electricity,
                requested: electricity_cost,
                available,
            },
        }));
    }
    Ok(WorldEventBody::Domain(
        DomainEvent::DataCollectedAuthenticated {
            collector_agent_id: collector_agent_id.to_string(),
            electricity_cost,
            data_amount,
            player_id: player_id.to_string(),
            public_key: public_key.to_string(),
            nonce,
            signature: signature.to_string(),
        },
    ))
}

pub(crate) fn ensure_profile_field_whitelist<T: serde::Serialize>(
    profile: &T,
    allowed_fields: &[&str],
    profile_label: &str,
) -> Result<(), String> {
    let value = serde_json::to_value(profile)
        .map_err(|err| format!("{profile_label} whitelist encode failed: {err}"))?;
    let serde_json::Value::Object(fields) = value else {
        return Err(format!(
            "{profile_label} whitelist rejected: payload is not an object"
        ));
    };
    let allowed: BTreeSet<&str> = allowed_fields.iter().copied().collect();
    for key in fields.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(format!(
                "{profile_label} whitelist rejected: field `{key}` is not allowed"
            ));
        }
    }
    Ok(())
}

pub(super) fn merge_recipe_consume_with_maintenance_sink(
    world: &World,
    consume: &[MaterialStack],
    produce: &[MaterialStack],
) -> Vec<MaterialStack> {
    let mut merged: BTreeMap<String, i64> = BTreeMap::new();
    for stack in consume {
        let entry = merged.entry(stack.kind.clone()).or_insert(0);
        *entry = entry.saturating_add(stack.amount);
    }
    for stack in produce {
        if stack.amount <= 0 {
            continue;
        }
        let Some(profile) = world.product_profile(stack.kind.as_str()) else {
            continue;
        };
        for sink in &profile.maintenance_sink {
            if sink.amount <= 0 {
                continue;
            }
            let required_amount = sink.amount.saturating_mul(stack.amount);
            if required_amount <= 0 {
                continue;
            }
            let entry = merged.entry(sink.kind.clone()).or_insert(0);
            *entry = entry.saturating_add(required_amount);
        }
    }
    merged
        .into_iter()
        .map(|(kind, amount)| MaterialStack::new(kind, amount))
        .collect()
}

fn infer_bottleneck_tags(consume: &[MaterialStack]) -> Vec<String> {
    let tags: BTreeSet<String> = consume
        .iter()
        .filter_map(|stack| {
            let normalized = stack.kind.to_ascii_lowercase();
            BOTTLENECK_TAG_KINDS
                .iter()
                .find(|kind| normalized == **kind)
                .map(|kind| (*kind).to_string())
        })
        .collect();
    tags.into_iter().collect()
}

pub(super) fn compute_local_scarcity_delay_ticks(
    world: &World,
    preferred_consume_ledger: &MaterialLedgerId,
    consume_ledger: &MaterialLedgerId,
    consume: &[MaterialStack],
    bottleneck_tags: &[String],
) -> u32 {
    if *preferred_consume_ledger == MaterialLedgerId::world()
        || *consume_ledger != MaterialLedgerId::world()
        || bottleneck_tags.is_empty()
    {
        return 0;
    }

    let bottleneck_set: BTreeSet<&str> = bottleneck_tags.iter().map(String::as_str).collect();
    let mut requested_total: i128 = 0;
    let mut deficit_total: i128 = 0;
    for stack in consume {
        if stack.amount <= 0 {
            continue;
        }
        let normalized_kind = stack.kind.to_ascii_lowercase();
        if !bottleneck_set.contains(normalized_kind.as_str()) {
            continue;
        }
        let requested = stack.amount as i128;
        let available = world
            .ledger_material_balance(preferred_consume_ledger, stack.kind.as_str())
            .max(0) as i128;
        requested_total = requested_total.saturating_add(requested);
        deficit_total = deficit_total.saturating_add(requested.saturating_sub(available).max(0));
    }

    if requested_total <= 0 || deficit_total <= 0 {
        return 0;
    }
    let deficit_ratio_bps = deficit_total
        .saturating_mul(10_000)
        .saturating_div(requested_total);
    if deficit_ratio_bps >= 7_000 { 2 } else { 1 }
}

fn governance_tax_bps_for_material_quotes(world: &World) -> u16 {
    world
        .state
        .gameplay_policy
        .electricity_tax_bps
        .saturating_add(world.state.gameplay_policy.data_tax_bps)
        .min(10_000)
}

pub(crate) fn build_material_market_quotes(
    world: &World,
    preferred_consume_ledger: &MaterialLedgerId,
    consume: &[MaterialStack],
) -> Vec<MaterialMarketQuote> {
    let mut requested_by_kind: BTreeMap<String, i64> = BTreeMap::new();
    for stack in consume {
        if stack.amount <= 0 {
            continue;
        }
        let entry = requested_by_kind.entry(stack.kind.clone()).or_insert(0);
        *entry = entry.saturating_add(stack.amount);
    }
    if requested_by_kind.is_empty() {
        return Vec::new();
    }

    let governance_tax_bps = governance_tax_bps_for_material_quotes(world);
    let mut quotes = Vec::with_capacity(requested_by_kind.len());
    for (kind, requested_amount) in requested_by_kind {
        let transit_loss_bps = material_transit_loss_bps_for_kind(world, kind.as_str());
        let local_available_amount = world
            .ledger_material_balance(preferred_consume_ledger, kind.as_str())
            .max(0);
        let world_available_amount = world
            .ledger_material_balance(&MaterialLedgerId::world(), kind.as_str())
            .max(0);
        let local_deficit_amount = requested_amount
            .saturating_sub(local_available_amount)
            .max(0);
        let deficit_ratio_bps = if requested_amount > 0 {
            ((local_deficit_amount as i128)
                .saturating_mul(10_000)
                .saturating_div(requested_amount as i128)) as i64
        } else {
            0
        };
        let effective_cost_index_ppm = 1_000_000_i64
            .saturating_add(deficit_ratio_bps.saturating_mul(100))
            .saturating_add(transit_loss_bps.saturating_mul(100))
            .saturating_add(i64::from(governance_tax_bps).saturating_mul(100));
        quotes.push(MaterialMarketQuote {
            kind,
            requested_amount,
            local_available_amount,
            world_available_amount,
            local_deficit_amount,
            transit_loss_bps,
            governance_tax_bps,
            effective_cost_index_ppm,
        });
    }
    quotes
}

pub(super) fn recipe_stage_gate_allowed(
    current_stage: crate::IndustryStage,
    stage_gate: &str,
) -> bool {
    let normalized = stage_gate.trim();
    if normalized.is_empty() {
        return true;
    }
    let Some(required_stage) = parse_industry_stage(normalized) else {
        return true;
    };
    current_stage >= required_stage
}

pub(super) fn product_unlock_stage_allowed(
    current_stage: crate::IndustryStage,
    unlock_stage: &str,
) -> bool {
    recipe_stage_gate_allowed(current_stage, unlock_stage)
}

fn parse_industry_stage(raw: &str) -> Option<crate::IndustryStage> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "bootstrap" => Some(crate::IndustryStage::Bootstrap),
        "scale_out" | "scaleout" | "scale-out" => Some(crate::IndustryStage::ScaleOut),
        "governance" => Some(crate::IndustryStage::Governance),
        _ => None,
    }
}

pub(super) fn industry_stage_label(stage: crate::IndustryStage) -> &'static str {
    match stage {
        crate::IndustryStage::Bootstrap => "bootstrap",
        crate::IndustryStage::ScaleOut => "scale_out",
        crate::IndustryStage::Governance => "governance",
    }
}

pub(super) fn recipe_preferred_tags_compatible(
    preferred_tags: &[String],
    factory_tags: &[String],
) -> bool {
    if preferred_tags.is_empty() {
        return true;
    }
    let normalized_factory: BTreeSet<String> = factory_tags
        .iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    preferred_tags.iter().any(|tag| {
        let normalized = tag.trim().to_ascii_lowercase();
        !normalized.is_empty() && normalized_factory.contains(normalized.as_str())
    })
}

pub(super) fn normalized_factory_tags(tags: &[String]) -> Vec<String> {
    let normalized: BTreeSet<String> = tags
        .iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    normalized.into_iter().collect()
}

pub(super) fn factory_profile_matches_spec(
    profile: &crate::FactoryProfileV1,
    spec: &FactoryModuleSpec,
) -> Result<(), String> {
    if profile.factory_id != spec.factory_id {
        return Err(format!(
            "factory profile identity mismatch: profile={} spec={}",
            profile.factory_id, spec.factory_id
        ));
    }
    if profile.tier != spec.tier {
        return Err(format!(
            "factory profile tier mismatch: factory_id={} profile={} spec={}",
            spec.factory_id, profile.tier, spec.tier
        ));
    }
    if profile.recipe_slots != spec.recipe_slots {
        return Err(format!(
            "factory profile recipe_slots mismatch: factory_id={} profile={} spec={}",
            spec.factory_id, profile.recipe_slots, spec.recipe_slots
        ));
    }
    if normalized_factory_tags(&profile.tags) != normalized_factory_tags(&spec.tags) {
        return Err(format!(
            "factory profile tags mismatch: factory_id={} profile={:?} spec={:?}",
            spec.factory_id,
            normalized_factory_tags(&profile.tags),
            normalized_factory_tags(&spec.tags)
        ));
    }
    Ok(())
}

pub(super) fn resolve_recipe_bottleneck_tags(
    recipe_profile: Option<&crate::RecipeProfileV1>,
    consume: &[MaterialStack],
) -> Vec<String> {
    let from_profile: BTreeSet<String> = recipe_profile
        .map(|profile| {
            profile
                .bottleneck_tags
                .iter()
                .map(|tag| tag.trim().to_ascii_lowercase())
                .filter(|tag| !tag.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if !from_profile.is_empty() {
        return from_profile.into_iter().collect();
    }
    infer_bottleneck_tags(consume)
}
