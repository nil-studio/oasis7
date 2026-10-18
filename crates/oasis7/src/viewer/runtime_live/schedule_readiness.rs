use super::{GameplayActionRequest, build_runtime_action_from_gameplay_request};
use crate::runtime::{Action as RuntimeAction, IndustryStage, MaterialLedgerId, WorldState};
use crate::simulator::default_schedule_recipe_readiness;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::MaterialStack;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn schedule_recipe_disabled_reason(
    state: &WorldState,
    agent_id: &str,
    action_id: &str,
) -> Option<String> {
    let RuntimeAction::ScheduleRecipe {
        factory_id,
        recipe_id,
        plan,
        ..
    } = build_runtime_action_from_gameplay_request(&GameplayActionRequest {
        action_id: action_id.to_string(),
        target_agent_id: agent_id.to_string(),
        actor_agent_id: None,
        player_id: String::new(),
        public_key: None,
        auth: None,
    })
    .ok()?
    else {
        return None;
    };
    let Some(factory) = state.factories.get(factory_id.as_str()) else {
        return Some(format!(
            "factory {} is not present in the committed world; wait for the factory snapshot",
            factory_id
        ));
    };
    if factory.builder_agent_id != agent_id {
        return Some(format!(
            "factory {} is owned by {}; only the factory owner may schedule this run",
            factory_id, factory.builder_agent_id
        ));
    }
    let expected_input_ledger = MaterialLedgerId::site(factory.site_id.clone());
    if factory.input_ledger != expected_input_ledger
        || factory.output_ledger != expected_input_ledger
    {
        return Some(format!(
            "factory {} has no authoritative site ledger route; expected input/output ledger {}",
            factory_id, expected_input_ledger
        ));
    }
    let active_jobs = state
        .pending_recipe_jobs
        .values()
        .filter(|job| job.factory_id == *factory_id)
        .count();
    if active_jobs >= usize::from(factory.spec.recipe_slots)
        || usize::from(factory.production.active_jobs) >= usize::from(factory.spec.recipe_slots)
    {
        return Some(format!(
            "factory {} has no free execution slot; wait for a committed recipe completion (active_jobs={} recipe_slots={})",
            factory_id, active_jobs, factory.spec.recipe_slots
        ));
    }
    if !state.agents.contains_key(agent_id) {
        return Some(format!("builder agent unavailable: agent_id={agent_id}"));
    }
    if let Some(profile) = state.recipe_profiles.get(recipe_id.as_str()) {
        if !recipe_stage_gate_allowed(state.industry_progress.stage, profile.stage_gate.as_str()) {
            return Some(format!(
                "recipe stage gate denied: recipe={} required_stage={} current_stage={}",
                recipe_id,
                profile.stage_gate,
                industry_stage_label(state.industry_progress.stage),
            ));
        }
        if !recipe_preferred_tags_compatible(&profile.preferred_factory_tags, &factory.spec.tags) {
            return Some(format!(
                "recipe preferred_factory_tags mismatch: recipe={} preferred={:?} factory_tags={:?}",
                recipe_id, profile.preferred_factory_tags, factory.spec.tags
            ));
        }
    }
    for stack in &plan.produce {
        let Some(profile) = state.product_profiles.get(stack.kind.as_str()) else {
            continue;
        };
        if !product_unlock_stage_allowed(
            state.industry_progress.stage,
            profile.unlock_stage.as_str(),
        ) {
            return Some(format!(
                "product unlock_stage denied: product={} required_stage={} current_stage={}",
                profile.product_id,
                profile.unlock_stage,
                industry_stage_label(state.industry_progress.stage),
            ));
        }
    }
    let consume = recipe_consume_with_maintenance_sinks(state, &plan.consume, &plan.produce);
    let consume_ledger = expected_input_ledger;
    if !state.material_ledgers.contains_key(&consume_ledger) {
        return Some(format!(
            "authoritative site material ledger unavailable: {}; replenish through a committed logistics route before scheduling",
            consume_ledger
        ));
    }
    for stack in &consume {
        let available = ledger_material_balance(state, &consume_ledger, stack.kind.as_str());
        if available < stack.amount {
            return Some(format!(
                "insufficient {} in {} ledger: need {}, have {}; replenish {} before scheduling this run",
                stack.kind, consume_ledger, stack.amount, available, stack.kind
            ));
        }
    }
    let Some(owner) = state.agents.get(factory.builder_agent_id.as_str()) else {
        return Some(format!(
            "factory-owner agent unavailable: agent_id={}",
            factory.builder_agent_id
        ));
    };
    // Simulator recipe costs are advisory only.  The runtime execution plan is
    // the sole authority for the electricity obligation of a live submission.
    let available_owner_electricity = owner.state.resources.get(ResourceKind::Electricity);
    if available_owner_electricity < plan.power_required {
        return Some(format!(
            "insufficient factory-owner electricity: need {}, have {}; replenish electricity before scheduling this run",
            plan.power_required, available_owner_electricity
        ));
    }
    None
}

pub(super) fn factory_build_disabled_reason(
    state: &WorldState,
    agent_id: &str,
    action_id: &str,
) -> Option<String> {
    let RuntimeAction::BuildFactory { site_id, spec, .. } =
        build_runtime_action_from_gameplay_request(&GameplayActionRequest {
            action_id: action_id.to_string(),
            target_agent_id: agent_id.to_string(),
            actor_agent_id: None,
            player_id: String::new(),
            public_key: None,
            auth: None,
        })
        .ok()?
    else {
        return None;
    };
    if state.retired_factory_ids.contains(spec.factory_id.as_str()) {
        return Some(format!(
            "factory {} identity is retired; choose a new factory identity instead of rebuilding",
            spec.factory_id
        ));
    }
    if state.factories.contains_key(spec.factory_id.as_str())
        || state
            .pending_factory_builds
            .values()
            .any(|job| job.spec.factory_id == spec.factory_id)
    {
        return Some(format!(
            "factory {} build is already queued or committed; wait for the committed factory state",
            spec.factory_id
        ));
    }
    let Some(factory_profile) = state.factory_profiles.get(spec.factory_id.as_str()) else {
        return Some(format!(
            "factory profile unknown: factory_id={}",
            spec.factory_id
        ));
    };
    if let Some(reason) = factory_profile_mismatch_reason(factory_profile, &spec) {
        return Some(reason);
    }
    if let Some(reason) = site_location_authority_disabled_reason(state, agent_id, site_id.as_str())
    {
        return Some(reason);
    }
    let Some(power_profile) = state
        .factory_construction_power_profiles
        .get(spec.factory_id.as_str())
    else {
        return Some(format!(
            "construction power profile unavailable: factory_id={}",
            spec.factory_id
        ));
    };
    if power_profile.factory_id != spec.factory_id
        || !power_profile.active
        || power_profile.authority_revision == 0
        || power_profile.electricity_amount < 0
    {
        return Some(format!(
            "construction power profile inactive_or_stale: factory_id={} revision={} active={} kind={}",
            spec.factory_id,
            power_profile.authority_revision,
            power_profile.active,
            power_profile.factory_kind
        ));
    }
    let Some(agent) = state.agents.get(agent_id) else {
        return Some(format!("builder agent unavailable: agent_id={agent_id}"));
    };
    let available_electricity = agent.state.resources.get(ResourceKind::Electricity);
    if available_electricity < power_profile.electricity_amount {
        return Some(format!(
            "insufficient builder electricity: need {}, have {}; replenish electricity before building {}",
            power_profile.electricity_amount, available_electricity, spec.factory_id
        ));
    }
    let builder_ledger_id = MaterialLedgerId::agent(agent_id.to_string());
    let Some(builder_materials) = state.material_ledgers.get(&builder_ledger_id) else {
        return Some(format!(
            "authoritative builder material ledger unavailable: {}; replenish construction materials into this ledger",
            builder_ledger_id
        ));
    };
    let mut required = BTreeMap::new();
    for stack in &spec.build_cost {
        if stack.kind.trim().is_empty() || stack.amount <= 0 {
            return Some(format!(
                "factory {} build cost is not actionable: {}={}",
                spec.factory_id, stack.kind, stack.amount
            ));
        }
        let amount = required.entry(stack.kind.clone()).or_insert(0_i64);
        *amount = amount.checked_add(stack.amount).unwrap_or(i64::MAX);
    }
    for (kind, requested) in required {
        let available = material_balance(builder_materials, kind.as_str());
        if available < requested {
            return Some(format!(
                "insufficient builder material in {}: {} need {}, have {}; replenish {} before building {}",
                builder_ledger_id, kind, requested, available, kind, spec.factory_id
            ));
        }
    }
    None
}

fn material_balance(materials: &BTreeMap<String, i64>, kind: &str) -> i64 {
    materials.get(kind).copied().unwrap_or_default()
}

fn factory_profile_mismatch_reason(
    profile: &crate::runtime::FactoryProfileV1,
    spec: &oasis7_wasm_abi::FactoryModuleSpec,
) -> Option<String> {
    if profile.factory_id != spec.factory_id {
        return Some(format!(
            "factory profile identity mismatch: profile={} spec={}",
            profile.factory_id, spec.factory_id
        ));
    }
    if profile.tier != spec.tier {
        return Some(format!(
            "factory profile tier mismatch: factory_id={} profile={} spec={}",
            spec.factory_id, profile.tier, spec.tier
        ));
    }
    if profile.recipe_slots != spec.recipe_slots {
        return Some(format!(
            "factory profile recipe_slots mismatch: factory_id={} profile={} spec={}",
            spec.factory_id, profile.recipe_slots, spec.recipe_slots
        ));
    }
    let normalized_profile_tags = normalized_factory_tags(&profile.tags);
    let normalized_spec_tags = normalized_factory_tags(&spec.tags);
    if normalized_profile_tags != normalized_spec_tags {
        return Some(format!(
            "factory profile tags mismatch: factory_id={} profile={:?} spec={:?}",
            spec.factory_id, normalized_profile_tags, normalized_spec_tags
        ));
    }
    None
}

fn normalized_factory_tags(tags: &[String]) -> Vec<String> {
    let normalized: BTreeSet<String> = tags
        .iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    normalized.into_iter().collect()
}

fn site_location_authority_disabled_reason(
    state: &WorldState,
    agent_id: &str,
    site_id: &str,
) -> Option<String> {
    let Some(site) = state.factory_site_authorities.get(site_id) else {
        return Some(format!(
            "site authority unavailable: site={} is not registered",
            site_id
        ));
    };
    if site.site_id != site_id || !site.active || !site.chunk_ready || site.authority_revision == 0
    {
        return Some(format!(
            "site authority inactive_or_stale: site={} active={} chunk_ready={} revision={}",
            site_id, site.active, site.chunk_ready, site.authority_revision
        ));
    }
    if site.owner_agent_id != agent_id && !site.authorized_agent_ids.iter().any(|id| id == agent_id)
    {
        return Some(format!(
            "builder is not authorized for site {}: prepare the site owner grant before scheduling",
            site_id
        ));
    }
    let Some(anchor) = state.location_anchors.get(site.location_id.as_str()) else {
        return Some(format!(
            "location anchor unavailable: location_id={}",
            site.location_id
        ));
    };
    if anchor.location_id != site.location_id || !anchor.active || anchor.authority_revision == 0 {
        return Some(format!(
            "location anchor inactive_or_stale: location_id={} revision={} active={}",
            site.location_id, anchor.authority_revision, anchor.active
        ));
    }
    if anchor.effective_at > state.time {
        return Some(format!(
            "location anchor not yet effective: location_id={} effective_at={} now={}",
            site.location_id, anchor.effective_at, state.time
        ));
    }
    let Some(location) = state.agent_location_authorities.get(agent_id) else {
        return Some(format!(
            "builder location authority unavailable: agent_id={}",
            agent_id
        ));
    };
    if location.agent_id != agent_id
        || !location.active
        || location.authority_revision == 0
        || location.location_id != site.location_id
    {
        return Some(format!(
            "builder location authority inactive_or_mismatched: agent_id={} location={} active={} revision={}",
            agent_id, location.location_id, location.active, location.authority_revision
        ));
    }
    if location.effective_at > state.time {
        return Some(format!(
            "builder location authority not yet effective: agent_id={} effective_at={} now={}",
            agent_id, location.effective_at, state.time
        ));
    }
    None
}

/// Returns the simulator's Data estimate as display-only context.
///
/// Runtime factory scheduling has no Data admission rule. Keep the estimate
/// visible so players can compare it with simulator quotes, but label it as
/// non-authoritative and never use it to disable the runtime action.
pub(super) fn schedule_recipe_data_advisory(agent_id: &str, action_id: &str) -> Option<String> {
    let RuntimeAction::ScheduleRecipe {
        recipe_id, plan, ..
    } = build_runtime_action_from_gameplay_request(&GameplayActionRequest {
        action_id: action_id.to_string(),
        target_agent_id: agent_id.to_string(),
        actor_agent_id: None,
        player_id: String::new(),
        public_key: None,
        auth: None,
    })
    .ok()?
    else {
        return None;
    };
    let readiness =
        default_schedule_recipe_readiness(recipe_id.as_str(), i64::from(plan.accepted_batches))?;
    Some(format!(
        "Data estimate only (simulator advisory; non-authoritative): {}",
        readiness.data_cost
    ))
}

fn recipe_consume_with_maintenance_sinks(
    state: &WorldState,
    consume: &[MaterialStack],
    produce: &[MaterialStack],
) -> Vec<MaterialStack> {
    let mut merged = BTreeMap::new();
    let mut order = Vec::new();
    for stack in consume {
        if !merged.contains_key(stack.kind.as_str()) {
            order.push(stack.kind.clone());
        }
        let amount = merged.entry(stack.kind.clone()).or_insert(0_i64);
        *amount = amount.saturating_add(stack.amount);
    }
    for stack in produce.iter().filter(|stack| stack.amount > 0) {
        let Some(profile) = state.product_profiles.get(stack.kind.as_str()) else {
            continue;
        };
        for sink in profile
            .maintenance_sink
            .iter()
            .filter(|sink| sink.amount > 0)
        {
            if !merged.contains_key(sink.kind.as_str()) {
                order.push(sink.kind.clone());
            }
            let amount = merged.entry(sink.kind.clone()).or_insert(0_i64);
            *amount = amount.saturating_add(sink.amount.saturating_mul(stack.amount));
        }
    }
    order
        .into_iter()
        .filter_map(|kind| {
            merged
                .remove(&kind)
                .map(|amount| MaterialStack::new(kind, amount))
        })
        .collect()
}

fn ledger_material_balance(state: &WorldState, ledger_id: &MaterialLedgerId, kind: &str) -> i64 {
    state
        .material_ledgers
        .get(ledger_id)
        .and_then(|materials| materials.get(kind))
        .copied()
        .unwrap_or(0)
}

fn recipe_stage_gate_allowed(current_stage: IndustryStage, stage_gate: &str) -> bool {
    let normalized = stage_gate.trim();
    if normalized.is_empty() {
        return true;
    }
    let Some(required_stage) = parse_industry_stage(normalized) else {
        // Runtime preserves forward compatibility for unknown stage labels by
        // treating them as advisory rather than inventing a new ordering.
        return true;
    };
    current_stage >= required_stage
}

fn product_unlock_stage_allowed(current_stage: IndustryStage, unlock_stage: &str) -> bool {
    recipe_stage_gate_allowed(current_stage, unlock_stage)
}

fn parse_industry_stage(raw: &str) -> Option<IndustryStage> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "bootstrap" => Some(IndustryStage::Bootstrap),
        "scale_out" | "scaleout" | "scale-out" => Some(IndustryStage::ScaleOut),
        "governance" => Some(IndustryStage::Governance),
        _ => None,
    }
}

fn industry_stage_label(stage: IndustryStage) -> &'static str {
    match stage {
        IndustryStage::Bootstrap => "bootstrap",
        IndustryStage::ScaleOut => "scale_out",
        IndustryStage::Governance => "governance",
    }
}

fn recipe_preferred_tags_compatible(preferred_tags: &[String], factory_tags: &[String]) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::FactoryProfileV1;

    fn assembler_profile() -> FactoryProfileV1 {
        FactoryProfileV1 {
            factory_id: "factory.assembler.mk1".to_string(),
            tier: 3,
            recipe_slots: 2,
            tags: vec!["assembler".to_string(), "precision".to_string()],
        }
    }

    #[test]
    fn factory_build_readiness_matches_runtime_profile_admission() {
        let action_id = "build_factory_assembler_mk1";
        let factory_id = "factory.assembler.mk1";

        let missing_reason =
            factory_build_disabled_reason(&WorldState::default(), "starter-agent-0", action_id)
                .expect("missing factory profile must disable the build action");
        assert!(missing_reason.contains("factory profile unknown"));
        assert!(missing_reason.contains(factory_id));

        for (label, expected_reason) in [
            ("tier", "factory profile tier mismatch"),
            ("recipe slots", "factory profile recipe_slots mismatch"),
            ("tags", "factory profile tags mismatch"),
        ] {
            let mut profile = assembler_profile();
            match label {
                "tier" => profile.tier += 1,
                "recipe slots" => profile.recipe_slots += 1,
                "tags" => profile.tags = vec!["wrong".to_string()],
                _ => unreachable!("known factory profile mismatch case"),
            }
            let mut state = WorldState::default();
            state
                .factory_profiles
                .insert(factory_id.to_string(), profile);
            let reason = factory_build_disabled_reason(&state, "starter-agent-0", action_id)
                .unwrap_or_else(|| panic!("{label} profile mismatch must disable build"));
            assert!(reason.contains(expected_reason), "{label} reason={reason}");
        }
    }
}
