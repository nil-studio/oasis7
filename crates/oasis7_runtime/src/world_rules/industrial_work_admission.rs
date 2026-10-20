//! Certified aggregate-to-escrow admission. Prepared mutations commit together.
use super::industrial_work::{accept_root, import_lot, industrial_ref};
use super::{PreparedWorldRule, invalid, verify_world_rule_authority};
use crate::{MaterialLedgerId, WorldError, WorldState, WorldTime};
use oasis7_agent_api::world_action::*;
use std::collections::BTreeMap;

const MAX_INDUSTRIAL_RECORDS: usize = 4096;

pub fn industrial_owner_binding(state: &WorldState, actor: &str) -> Result<String, WorldError> {
    let owner = state
        .resolved_agent_owner(actor)
        .filter(|o| {
            !o.player_id.is_empty()
                && !o.source_id.is_empty()
                && o.public_key.as_ref().is_some_and(|k| !k.is_empty())
        })
        .ok_or_else(|| invalid("industrial_owner_authority_missing"))?;
    crate::util::hash_json(&(
        owner.primary_agent_id,
        owner.player_id,
        owner.public_key,
        owner.source_id,
        owner.generation,
    ))
}
pub fn industrial_import_manifest(
    world: &str,
    actor: &str,
    key: &str,
    facts: &QualifiedIndustrialLotFactsV1,
) -> Result<String, WorldError> {
    crate::util::hash_json(&("industrial-qualified-import/v1", world, actor, key, facts))
}
pub fn industrial_root_manifest(
    world: &str,
    actor: &str,
    key: &str,
    command: &IndustrialWorkCommandV1,
    state: &WorldState,
) -> Result<String, WorldError> {
    let IndustrialWorkCommandV1::AcceptRoot {
        factory_id,
        recipe_id,
        batches,
        input_baselines,
        authority,
        ..
    } = command
    else {
        return Err(invalid("industrial_root_command_required"));
    };
    let factory = state
        .factories
        .get(factory_id)
        .ok_or_else(|| invalid("industrial_factory_missing"))?;
    let profile = state
        .recipe_profiles
        .get(recipe_id)
        .ok_or_else(|| invalid("industrial_recipe_profile_missing"))?;
    let plan = crate::builtin_recipe_plan(recipe_id, *batches)
        .ok_or_else(|| invalid("industrial_recipe_unsupported"))?;
    crate::util::hash_json(&(
        "industrial-root-admission/v1",
        world,
        actor,
        key,
        factory,
        profile,
        &plan,
        industrial_effective_consume(state, &plan)?,
        input_baselines,
        authority,
    ))
}
pub(super) fn industrial_ledger_access(
    state: &WorldState,
    actor: &str,
    ledger: &MaterialLedgerId,
    now: WorldTime,
) -> Result<(), WorldError> {
    let allowed = match ledger {
        MaterialLedgerId::Agent(owner) => owner == actor,
        MaterialLedgerId::Site(site) => state.factory_site_authorities.get(site).is_some_and(|a| {
            a.active
                && a.chunk_ready
                && a.registered_at <= now
                && (a.owner_agent_id == actor
                    || a.authorized_agent_ids.iter().any(|id| id == actor))
        }),
        MaterialLedgerId::Factory(id) => state.factories.get(id).is_some_and(|f| {
            f.builder_agent_id == actor
                && !state.retired_factory_ids.contains(id)
                && f.durability_ppm > 0
                && state
                    .factory_site_authorities
                    .get(&f.site_id)
                    .is_some_and(|a| {
                        a.active
                            && a.chunk_ready
                            && a.registered_at <= now
                            && (a.owner_agent_id == actor
                                || a.authorized_agent_ids.iter().any(|id| id == actor))
                    })
        }),
        MaterialLedgerId::World => false,
    };
    if !allowed {
        return Err(invalid("industrial_ledger_access_denied"));
    }
    Ok(())
}
pub(super) fn industrial_effective_consume(
    state: &WorldState,
    plan: &oasis7_wasm_abi::RecipeExecutionPlan,
) -> Result<Vec<crate::MaterialStack>, WorldError> {
    let mut quantities = BTreeMap::<String, i64>::new();
    for stack in &plan.consume {
        if stack.amount <= 0 {
            return Err(invalid("industrial_plan_quantity_invalid"));
        }
        let entry = quantities.entry(stack.kind.clone()).or_default();
        *entry = entry
            .checked_add(stack.amount)
            .ok_or_else(|| invalid("industrial_plan_overflow"))?;
    }
    for output in &plan.produce {
        if output.amount <= 0 {
            return Err(invalid("industrial_plan_quantity_invalid"));
        }
        if let Some(profile) = state.product_profiles.get(&output.kind) {
            for sink in &profile.maintenance_sink {
                if sink.amount < 0 {
                    return Err(invalid("industrial_maintenance_invalid"));
                }
                let amount = sink
                    .amount
                    .checked_mul(output.amount)
                    .ok_or_else(|| invalid("industrial_plan_overflow"))?;
                let entry = quantities.entry(sink.kind.clone()).or_default();
                *entry = entry
                    .checked_add(amount)
                    .ok_or_else(|| invalid("industrial_plan_overflow"))?;
            }
        }
    }
    Ok(quantities
        .into_iter()
        .filter(|(_, q)| *q > 0)
        .map(|(kind, amount)| crate::MaterialStack::new(kind, amount))
        .collect())
}
fn metadata(value: &str) -> Result<(), WorldError> {
    if value.is_empty() || value.len() > 256 || value.trim() != value {
        return Err(invalid("industrial_metadata_invalid"));
    }
    Ok(())
}
fn material_profile(profile: &oasis7_wasm_abi::SubstitutionProfileRefV1) -> Result<(), WorldError> {
    metadata(&profile.id)?;
    metadata(&profile.version)?;
    if profile.digest.len() != 64
        || !profile
            .digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("industrial_profile_digest_invalid"));
    }
    Ok(())
}
impl PreparedWorldRule {
    pub(crate) fn industrial_work_command(
        &mut self,
        state: &WorldState,
        actor: &str,
        key: &str,
        command: &IndustrialWorkCommandV1,
        now: WorldTime,
        world_binding: Option<&str>,
    ) -> Result<String, WorldError> {
        let world = world_binding
            .filter(|v| !v.is_empty())
            .ok_or_else(|| invalid("industrial_world_binding_missing"))?;
        let binding = industrial_owner_binding(state, actor)?;
        match command {
            IndustrialWorkCommandV1::ImportQualifiedLot { facts, certificate } => {
                if facts.world_binding != world
                    || facts.owner_id != actor
                    || facts.owner_binding_digest != binding
                    || facts.quantity == 0
                    || facts.quantity > facts.escrow_capacity
                    || facts.expires_at < now
                    || facts.consensus_height == 0
                {
                    return Err(invalid("industrial_import_authority_invalid"));
                }
                industrial_ledger_access(state, actor, &facts.source_ledger, now)?;
                material_profile(&facts.material_profile)?;
                for item in [
                    &facts.material,
                    &facts.unit,
                    &facts.spec,
                    &facts.quality,
                    &facts.custody,
                ] {
                    metadata(item)?;
                }
                verify_world_rule_authority(
                    state,
                    &industrial_import_manifest(world, actor, key, facts)?,
                    facts.consensus_height,
                    certificate,
                )?;
                if self.rules.industrial_work.lots.len() >= MAX_INDUSTRIAL_RECORDS {
                    return Err(invalid("industrial_escrow_record_capacity"));
                }
                let existing = self
                    .rules
                    .industrial_work
                    .lots
                    .values()
                    .filter(|l| {
                        l.owner_id == actor && l.material == facts.material && l.unit == facts.unit
                    })
                    .try_fold(0u64, |sum, l| sum.checked_add(l.remaining))
                    .ok_or_else(|| invalid("industrial_escrow_quantity_overflow"))?;
                if existing
                    .checked_add(facts.quantity)
                    .is_none_or(|q| q > facts.escrow_capacity)
                {
                    return Err(invalid("industrial_escrow_capacity_exceeded"));
                }
                let lot_ref = industrial_ref(
                    world,
                    "industrial",
                    "industrial-lot/v1",
                    &format!("lot:{}", crate::util::hash_json(&(key, facts))?),
                    1,
                    facts,
                )?;
                let lot = IndustrialLotV1 {
                    lot_ref: lot_ref.clone(),
                    owner_id: actor.into(),
                    source_ledger: facts.source_ledger.clone(),
                    material: facts.material.clone(),
                    unit: facts.unit.clone(),
                    spec: facts.spec.clone(),
                    quality: facts.quality.clone(),
                    custody: facts.custody.clone(),
                    provenance_ref: facts.provenance_ref.clone(),
                    quantity: facts.quantity,
                    remaining: facts.quantity,
                };
                // Core rejects reuse of the same provenance even under a new request key.
                import_lot(&mut self.rules.industrial_work, lot, world)?;
                self.debit(
                    state,
                    &facts.source_ledger,
                    &BTreeMap::from([(
                        facts.material.clone(),
                        i64::try_from(facts.quantity)
                            .map_err(|_| invalid("industrial_quantity_overflow"))?,
                    )]),
                )?;
                self.rules.industrial_work.imported_lot_authorities.insert(
                    lot_ref.id.clone(),
                    IndustrialLotAuthorityV1 {
                        owner_binding_digest: binding,
                        expires_at: facts.expires_at,
                        escrow_capacity: facts.escrow_capacity,
                        material_profile: facts.material_profile.clone(),
                    },
                );
                Ok(lot_ref.id)
            }
            IndustrialWorkCommandV1::AcceptRoot {
                factory_id,
                recipe_id,
                batches,
                input_baselines,
                authority,
                certificate,
            } => {
                let factory = state
                    .factories
                    .get(factory_id)
                    .ok_or_else(|| invalid("industrial_factory_missing"))?;
                if factory.builder_agent_id != actor
                    || state.retired_factory_ids.contains(factory_id)
                    || factory.durability_ppm <= 0
                    || factory.production.status == crate::FactoryProductionStatus::Paused
                {
                    return Err(invalid("industrial_factory_unavailable"));
                }
                let profile = state
                    .recipe_profiles
                    .get(recipe_id)
                    .ok_or_else(|| invalid("industrial_recipe_profile_missing"))?;
                if !crate::world::recipe_stage_gate_allowed(
                    state.industry_progress.stage,
                    &profile.stage_gate,
                ) || !crate::world::recipe_preferred_tags_compatible(
                    &profile.preferred_factory_tags,
                    &factory.spec.tags,
                ) {
                    return Err(invalid("industrial_recipe_admission_denied"));
                }
                let plan = crate::builtin_recipe_plan(recipe_id, *batches)
                    .ok_or_else(|| invalid("industrial_recipe_unsupported"))?;
                if authority.world_binding != world
                    || authority.owner_binding_digest != binding
                    || authority.quoted_at != now
                    || authority.expires_at < now
                    || authority.state_digest != crate::util::hash_json(state)?
                    || authority.profile_digest != crate::util::hash_json(profile)?
                    || authority.plan_digest != crate::util::hash_json(&plan)?
                {
                    return Err(invalid("industrial_root_authority_stale"));
                }
                if authority.allowed_dispositions.len() > 1
                    || authority
                        .allowed_dispositions
                        .iter()
                        .any(|d| *d != IndustrialDispositionV1::Terminate)
                {
                    return Err(invalid("industrial_disposition_unsupported"));
                }
                for impact in [
                    &authority.baseline_impacts.outputs,
                    &authority.baseline_impacts.quality,
                    &authority.baseline_impacts.power,
                    &authority.baseline_impacts.time,
                    &authority.baseline_impacts.byproducts,
                    &authority.baseline_impacts.terminal,
                    &authority.baseline_impacts.value_class,
                    &authority.baseline_impacts.logistics,
                    &authority.baseline_impacts.buffer,
                ] {
                    material_profile(impact)?;
                }
                verify_world_rule_authority(
                    state,
                    &industrial_root_manifest(world, actor, key, command, state)?,
                    authority.consensus_height,
                    certificate,
                )?;
                let effective_consume = industrial_effective_consume(state, &plan)?;
                if self.rules.industrial_work.roots.len() >= MAX_INDUSTRIAL_RECORDS
                    || effective_consume.len() != input_baselines.len()
                {
                    return Err(invalid("industrial_root_baseline_incomplete"));
                }
                let mut slots = BTreeMap::new();
                for input in &effective_consume {
                    let baseline = input_baselines
                        .get(&input.kind)
                        .ok_or_else(|| invalid("industrial_input_baseline_missing"))?;
                    material_profile(&baseline.material_profile)?;
                    for item in [
                        &baseline.unit,
                        &baseline.spec,
                        &baseline.quality,
                        &baseline.custody,
                    ] {
                        metadata(item)?;
                    }
                    let slot = IndustrialInputSlotV1 {
                        material_profile: baseline.material_profile.clone(),
                        material: input.kind.clone(),
                        unit: baseline.unit.clone(),
                        spec: baseline.spec.clone(),
                        quality: baseline.quality.clone(),
                        custody: baseline.custody.clone(),
                        quantity: u64::try_from(input.amount)
                            .map_err(|_| invalid("industrial_quantity_invalid"))?,
                        binding_revision: 1,
                        parents: vec![],
                        consumed: false,
                    };
                    if slots.insert(input.kind.clone(), slot).is_some() {
                        return Err(invalid("industrial_duplicate_input"));
                    }
                }
                let output_kinds: std::collections::BTreeSet<_> = plan
                    .produce
                    .iter()
                    .chain(&plan.byproducts)
                    .map(|stack| stack.kind.clone())
                    .collect();
                if authority
                    .terminal_capacities
                    .keys()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>()
                    != output_kinds
                    || authority.terminal_capacities.values().any(|cap| *cap == 0)
                {
                    return Err(invalid("industrial_terminal_capacities_incomplete"));
                }
                let root_ref = industrial_ref(
                    world,
                    "industrial",
                    "industrial-root/v1",
                    &format!("root:{}", crate::util::hash_json(&(key, command))?),
                    1,
                    &(actor, factory, &plan, input_baselines, authority),
                )?;
                let root = RootOperationV1 {
                    owner_binding_digest: binding,
                    allowed_dispositions: authority.allowed_dispositions.clone(),
                    output_ledger: factory.output_ledger.clone(),
                    terminal_capacities: authority.terminal_capacities.clone(),
                    started_job_id: None,
                    root_ref: root_ref.clone(),
                    owner_id: actor.into(),
                    factory_id: factory_id.clone(),
                    pinned_recipe_id: recipe_id.clone(),
                    accepted_batches: plan.accepted_batches,
                    pinned_profile_digest: authority.profile_digest.clone(),
                    causal_digest: crate::util::hash_json(&authority.baseline_impacts)?,
                    work: 0,
                    first_sink: false,
                    slots,
                };
                accept_root(&mut self.rules.industrial_work, root, world)?;
                Ok(root_ref.id)
            }
            IndustrialWorkCommandV1::StartRoot {
                root_ref,
                expected_profile_digest,
                logistics_route_ids,
                logistics_path_ids,
            } => self.industrial_start_root(
                state,
                actor,
                root_ref,
                expected_profile_digest,
                logistics_route_ids,
                logistics_path_ids,
                now,
                world,
            ),
            IndustrialWorkCommandV1::TerminateRoot { root_ref } => {
                self.industrial_terminate_root(state, actor, root_ref, now, world)
            }
            IndustrialWorkCommandV1::ReleaseLot { lot_ref } => {
                let lot = self
                    .rules
                    .industrial_work
                    .lots
                    .get(&lot_ref.id)
                    .filter(|l| {
                        l.lot_ref == *lot_ref
                            && l.owner_id == actor
                            && lot_ref.world_binding == world
                    })
                    .ok_or_else(|| invalid("industrial_release_lot_invalid"))?
                    .clone();
                industrial_ledger_access(state, actor, &lot.source_ledger, now)?;
                let authority = self
                    .rules
                    .industrial_work
                    .imported_lot_authorities
                    .get(&lot_ref.id)
                    .ok_or_else(|| invalid("industrial_release_authority_missing"))?;
                if authority.owner_binding_digest != binding || lot.remaining == 0 {
                    return Err(invalid("industrial_release_unavailable"));
                }
                if self.rules.industrial_work.roots.values().any(|root| {
                    if self
                        .rules
                        .industrial_work
                        .dispositions
                        .get(&root.root_ref.id)
                        .is_some_and(|plan| plan.disposition == IndustrialDispositionV1::Terminate)
                    {
                        return false;
                    }
                    root.slots.values().any(|slot| {
                        !slot.consumed && slot.parents.iter().any(|p| p.lot_ref == *lot_ref)
                    })
                }) {
                    return Err(invalid("industrial_release_bound_lot"));
                }
                let amount = i64::try_from(lot.remaining)
                    .map_err(|_| invalid("industrial_quantity_overflow"))?;
                let ledger = self
                    .ledgers
                    .entry(lot.source_ledger.clone())
                    .or_insert_with(|| {
                        state
                            .material_ledgers
                            .get(&lot.source_ledger)
                            .cloned()
                            .unwrap_or_default()
                    });
                let balance = ledger
                    .get(&lot.material)
                    .copied()
                    .unwrap_or(0)
                    .checked_add(amount)
                    .ok_or_else(|| invalid("industrial_release_balance_overflow"))?;
                if u64::try_from(balance)
                    .ok()
                    .is_none_or(|q| q > authority.escrow_capacity)
                {
                    return Err(invalid("industrial_release_capacity_exceeded"));
                }
                ledger.insert(lot.material.clone(), balance);
                self.rules
                    .industrial_work
                    .lots
                    .get_mut(&lot_ref.id)
                    .expect("validated lot")
                    .remaining = 0;
                Ok(lot_ref.id.clone())
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
