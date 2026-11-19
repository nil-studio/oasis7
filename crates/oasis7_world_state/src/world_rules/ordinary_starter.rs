//! Finite starter publication, atomic claims, and settlement-derived progression.
use super::*;
use oasis7_agent_api::world_action::*;
use oasis7_agent_api::world_quote::{builtin_factory_spec, builtin_recipe_plan};
use std::collections::BTreeSet;

#[cfg(test)]
mod tests;

pub fn ordinary_starter_owner_key(world: &str, owner: &str) -> Result<String, WorldError> {
    crate::util::hash_json(&("ordinary-starter-owner-v1", world, owner))
}

pub fn ordinary_starter_offer_digest(offer: &OrdinaryStarterOfferV1) -> Result<String, WorldError> {
    crate::util::hash_json(&("ordinary-starter-offer-v1", offer))
}

fn package(state: &WorldState) -> Result<(BTreeMap<String, i64>, i64, String), WorldError> {
    let spec = builtin_factory_spec(ORDINARY_STARTER_FACTORY_KIND, ORDINARY_STARTER_FACTORY_KIND)
        .ok_or_else(|| invalid("ordinary_starter_factory_unavailable"))?;
    let plan = builtin_recipe_plan(ORDINARY_STARTER_RECIPE_ID, 1)
        .ok_or_else(|| invalid("ordinary_starter_recipe_unavailable"))?;
    let consume =
        crate::state::apply_domain_event_industry_helpers::recipe_consume_with_maintenance(
            state,
            &plan.consume,
            &plan.produce,
        );
    let mut materials = BTreeMap::<String, i64>::new();
    for (stack, multiplier) in spec
        .build_cost
        .iter()
        .map(|stack| (stack, 1))
        .chain(consume.iter().map(|stack| (stack, 2)))
    {
        if stack.amount <= 0 || stack.kind.is_empty() {
            return Err(invalid("ordinary_starter_material_invalid"));
        }
        let amount = stack
            .amount
            .checked_mul(multiplier)
            .ok_or_else(|| invalid("ordinary_starter_material_overflow"))?;
        let value = materials.entry(stack.kind.clone()).or_default();
        *value = value
            .checked_add(amount)
            .ok_or_else(|| invalid("ordinary_starter_material_overflow"))?;
    }
    let electricity = plan
        .power_required
        .checked_mul(2)
        .filter(|amount| *amount > 0)
        .ok_or_else(|| invalid("ordinary_starter_power_invalid"))?;
    let product_profiles = plan
        .produce
        .iter()
        .map(|stack| (&stack.kind, state.product_profiles.get(&stack.kind)))
        .collect::<Vec<_>>();
    let digest = crate::util::hash_json(&(
        &plan,
        state.recipe_profiles.get(ORDINARY_STARTER_RECIPE_ID),
        product_profiles,
        &materials,
    ))?;
    Ok((materials, electricity, digest))
}

impl WorldState {
    /// Published public inventory is permanently reserved from personal Agent acquisition.
    pub fn is_ordinary_starter_supplier(&self, agent_id: &str) -> bool {
        self.world_rules.as_ref().is_some_and(|rules| {
            rules
                .ordinary_starter
                .offers
                .values()
                .any(|offer| offer.supplier_agent_id == agent_id)
        })
    }

    /// Read-only obligations for explicit finite supplier provisioning.
    pub fn ordinary_starter_package(
        &self,
    ) -> Result<(BTreeMap<String, i64>, i64, String), WorldError> {
        package(self)
    }
    /// Trusted initialization constructs an offer from already registered, finite authorities.
    /// This never funds the supplier or creates a site, profile, participant, or factory.
    pub fn ordinary_starter_offer(
        &self,
        offer_id: &str,
        supplier: &str,
        world_binding: &str,
        slots: &[OrdinaryStarterSlotInputV1],
        now: WorldTime,
    ) -> Result<OrdinaryStarterOfferV1, WorldError> {
        validate_id(offer_id)?;
        validate_id(supplier)?;
        if world_binding.is_empty() || slots.is_empty() || slots.len() > 32 {
            return Err(invalid("ordinary_starter_configuration_invalid"));
        }
        let (materials, recipe_electricity, dependency_digest) = package(self)?;
        let mut entries = BTreeMap::new();
        for input in slots {
            validate_id(&input.slot_id)?;
            validate_id(&input.site_id)?;
            validate_id(&input.factory_id)?;
            let site = self
                .factory_site_authorities
                .get(&input.site_id)
                .ok_or_else(|| invalid("ordinary_starter_site_missing"))?;
            let anchor_revision = self.active_location_anchor_revision(&site.location_id, now)?;
            let profile = self
                .factory_profiles
                .get(&input.factory_id)
                .ok_or_else(|| invalid("ordinary_starter_factory_profile_missing"))?;
            let power = self
                .factory_construction_power_profiles
                .get(&input.factory_id)
                .ok_or_else(|| invalid("ordinary_starter_construction_power_missing"))?;
            let slot = OrdinaryStarterSlotV1 {
                slot_id: input.slot_id.clone(),
                site_id: input.site_id.clone(),
                factory_id: input.factory_id.clone(),
                location_id: site.location_id.clone(),
                location_anchor_revision: anchor_revision,
                site_authority_revision: site.authority_revision,
                factory_profile: profile.clone(),
                construction_power_revision: power.authority_revision,
                construction_electricity: power.electricity_amount,
            };
            if entries.insert(input.slot_id.clone(), slot).is_some() {
                return Err(invalid("ordinary_starter_duplicate_slot"));
            }
        }
        let offer = OrdinaryStarterOfferV1 {
            offer_id: offer_id.into(),
            revision: 1,
            world_binding: world_binding.into(),
            supplier_agent_id: supplier.into(),
            profile_id: ORDINARY_STARTER_PROFILE_ID.into(),
            profile_revision: ORDINARY_STARTER_PROFILE_REVISION,
            factory_kind: ORDINARY_STARTER_FACTORY_KIND.into(),
            recipe_id: ORDINARY_STARTER_RECIPE_ID.into(),
            input_batches: 2,
            materials,
            recipe_electricity,
            dependency_digest,
            slots: entries,
            published_at: now,
        };
        validate_offer(self, &offer, now)?;
        Ok(offer)
    }
}

fn validate_slot(
    state: &WorldState,
    offer: &OrdinaryStarterOfferV1,
    slot: &OrdinaryStarterSlotV1,
    now: WorldTime,
) -> Result<(), WorldError> {
    let site = state
        .factory_site_authorities
        .get(&slot.site_id)
        .ok_or_else(|| invalid("ordinary_starter_site_missing"))?;
    if site.site_id != slot.site_id
        || site.location_id != slot.location_id
        || !site.active
        || !site.chunk_ready
        || site.authority_revision != slot.site_authority_revision
        || site.owner_agent_id != offer.supplier_agent_id
        || site.registered_at > now
        || site.authorized_agent_ids != [offer.supplier_agent_id.clone()]
        || state.active_location_anchor_revision(&slot.location_id, now)?
            != slot.location_anchor_revision
    {
        return Err(invalid("ordinary_starter_site_authority_changed"));
    }
    if state
        .factories
        .values()
        .any(|factory| factory.factory_id == slot.factory_id || factory.site_id == slot.site_id)
        || state
            .pending_factory_builds
            .values()
            .any(|build| build.spec.factory_id == slot.factory_id || build.site_id == slot.site_id)
    {
        return Err(invalid("ordinary_starter_slot_occupied"));
    }
    let canonical = builtin_factory_spec(&offer.factory_kind, &slot.factory_id)
        .ok_or_else(|| invalid("ordinary_starter_factory_unavailable"))?;
    if state.factory_profiles.get(&slot.factory_id) != Some(&slot.factory_profile)
        || crate::state::canonical_factory_profile_matches_spec(&slot.factory_profile, &canonical)
            .is_err()
    {
        return Err(invalid("ordinary_starter_factory_profile_changed"));
    }
    let power = state
        .factory_construction_power_profiles
        .get(&slot.factory_id)
        .ok_or_else(|| invalid("ordinary_starter_construction_power_missing"))?;
    if !power.active
        || power.authority_revision == 0
        || power.factory_id != slot.factory_id
        || power.factory_kind != offer.factory_kind
        || power.authority_revision != slot.construction_power_revision
        || power.electricity_amount != slot.construction_electricity
        || power.electricity_amount < 0
    {
        return Err(invalid("ordinary_starter_construction_power_changed"));
    }
    Ok(())
}

fn validate_offer(
    state: &WorldState,
    offer: &OrdinaryStarterOfferV1,
    now: WorldTime,
) -> Result<(), WorldError> {
    validate_id(&offer.offer_id)?;
    validate_id(&offer.supplier_agent_id)?;
    if offer.revision != 1
        || offer.world_binding.is_empty()
        || offer.slots.is_empty()
        || offer.slots.len() > 32
        || offer.profile_id != ORDINARY_STARTER_PROFILE_ID
        || offer.profile_revision != ORDINARY_STARTER_PROFILE_REVISION
        || offer.factory_kind != ORDINARY_STARTER_FACTORY_KIND
        || offer.recipe_id != ORDINARY_STARTER_RECIPE_ID
        || offer.input_batches != 2
        || offer.published_at > now
        || state.is_manufactured_agent(&offer.supplier_agent_id)
        || state
            .starter_oc_claims
            .contains_key(&offer.supplier_agent_id)
        || state.agent_claims.contains_key(&offer.supplier_agent_id)
    {
        return Err(invalid("ordinary_starter_offer_invalid"));
    }
    let (materials, electricity, digest) = package(state)?;
    if offer.materials != materials
        || offer.recipe_electricity != electricity
        || offer.dependency_digest != digest
    {
        return Err(invalid("ordinary_starter_recipe_obligation_changed"));
    }
    let mut sites = BTreeSet::new();
    let mut factories = BTreeSet::new();
    let mut total_power = 0_i64;
    for (id, slot) in &offer.slots {
        validate_id(id)?;
        if id != &slot.slot_id
            || !sites.insert(&slot.site_id)
            || !factories.insert(&slot.factory_id)
        {
            return Err(invalid("ordinary_starter_duplicate_slot"));
        }
        validate_slot(state, offer, slot, now)?;
        total_power = total_power
            .checked_add(slot.construction_electricity)
            .and_then(|value| value.checked_add(electricity))
            .ok_or_else(|| invalid("ordinary_starter_power_overflow"))?;
    }
    let supplier = state
        .agents
        .get(&offer.supplier_agent_id)
        .ok_or_else(|| invalid("ordinary_starter_supplier_missing"))?;
    if supplier.state.resources.get(ResourceKind::Electricity) < total_power {
        return Err(invalid("ordinary_starter_supplier_power_insufficient"));
    }
    let stock = state
        .material_ledgers
        .get(&MaterialLedgerId::agent(&offer.supplier_agent_id));
    for (kind, amount) in &offer.materials {
        let total = amount
            .checked_mul(offer.slots.len() as i64)
            .ok_or_else(|| invalid("ordinary_starter_material_overflow"))?;
        if stock
            .and_then(|stock| stock.get(kind))
            .copied()
            .unwrap_or(0)
            < total
        {
            return Err(invalid(format!(
                "ordinary_starter_supplier_material_insufficient:{kind}"
            )));
        }
    }
    Ok(())
}

pub(super) struct OrdinaryStarterCommandContext<'a> {
    pub actor: &'a str,
    pub request: &'a str,
    pub key: &'a str,
    pub now: WorldTime,
    pub world_binding: Option<&'a str>,
    pub system: bool,
}

impl PreparedWorldRule {
    pub(super) fn ordinary_starter_command(
        &mut self,
        state: &WorldState,
        command: &OrdinaryStarterCommandV1,
        context: OrdinaryStarterCommandContext<'_>,
    ) -> Result<String, WorldError> {
        let OrdinaryStarterCommandContext {
            actor,
            request,
            key,
            now,
            world_binding,
            system,
        } = context;
        match command {
            OrdinaryStarterCommandV1::Publish { offer } => {
                if !system
                    || actor != offer.supplier_agent_id
                    || world_binding != Some(offer.world_binding.as_str())
                {
                    return Err(invalid("ordinary_starter_publication_authority_required"));
                }
                if self
                    .rules
                    .ordinary_starter
                    .offers
                    .contains_key(&offer.offer_id)
                {
                    return Err(invalid("ordinary_starter_offer_already_published"));
                }
                if self.rules.ordinary_starter.offers.values().any(|existing| {
                    existing.slots.values().any(|left| {
                        offer.slots.values().any(|right| {
                            left.site_id == right.site_id || left.factory_id == right.factory_id
                        })
                    })
                }) {
                    return Err(invalid("ordinary_starter_slot_already_published"));
                }
                validate_offer(state, offer, now)?;
                self.rules
                    .ordinary_starter
                    .offers
                    .insert(offer.offer_id.clone(), offer.clone());
                Ok(offer.offer_id.clone())
            }
            OrdinaryStarterCommandV1::Claim {
                offer_id,
                offer_revision,
                slot_id,
                offer_digest,
                owner_binding,
                owner_generation,
            } => {
                if system
                    || owner_binding.is_empty()
                    || *owner_generation == 0
                    || state.is_manufactured_agent(actor)
                {
                    return Err(invalid("ordinary_starter_participant_required"));
                }
                let offer = self
                    .rules
                    .ordinary_starter
                    .offers
                    .get(offer_id)
                    .cloned()
                    .ok_or_else(|| invalid("ordinary_starter_offer_missing"))?;
                if world_binding != Some(offer.world_binding.as_str())
                    || offer.revision != *offer_revision
                    || ordinary_starter_offer_digest(&offer)? != *offer_digest
                {
                    return Err(invalid("ordinary_starter_offer_binding_changed"));
                }
                if actor == offer.supplier_agent_id {
                    return Err(invalid("ordinary_starter_supplier_cannot_claim"));
                }
                let owner_key = ordinary_starter_owner_key(&offer.world_binding, owner_binding)?;
                if self.rules.ordinary_starter.claims.contains_key(&owner_key) {
                    return Err(invalid("ordinary_starter_owner_already_claimed"));
                }
                let slot = offer
                    .slots
                    .get(slot_id)
                    .ok_or_else(|| invalid("ordinary_starter_slot_missing"))?;
                if self.rules.ordinary_starter.claims.values().any(|claim| {
                    claim.site_id == slot.site_id || claim.factory_id == slot.factory_id
                }) {
                    return Err(invalid("ordinary_starter_slot_claimed"));
                }
                let (materials, electricity, dependency_digest) = package(state)?;
                if offer.materials != materials
                    || offer.recipe_electricity != electricity
                    || offer.dependency_digest != dependency_digest
                {
                    return Err(invalid("ordinary_starter_recipe_obligation_changed"));
                }
                validate_slot(state, &offer, slot, now)?;
                let electricity = electricity
                    .checked_add(slot.construction_electricity)
                    .ok_or_else(|| invalid("ordinary_starter_power_overflow"))?;
                self.debit(
                    state,
                    &MaterialLedgerId::agent(&offer.supplier_agent_id),
                    &materials,
                )?;
                for (kind, amount) in &materials {
                    self.credit(state, &MaterialLedgerId::agent(actor), kind, *amount)?;
                }
                let mut supplier = state
                    .agents
                    .get(&offer.supplier_agent_id)
                    .cloned()
                    .ok_or_else(|| invalid("ordinary_starter_supplier_missing"))?;
                supplier
                    .state
                    .resources
                    .remove(ResourceKind::Electricity, electricity)
                    .map_err(|_| invalid("ordinary_starter_supplier_power_insufficient"))?;
                let recipient = self
                    .agents
                    .get_mut(actor)
                    .ok_or_else(|| invalid("ordinary_starter_participant_missing"))?;
                recipient
                    .state
                    .resources
                    .add(ResourceKind::Electricity, electricity)
                    .map_err(|_| invalid("ordinary_starter_recipient_power_overflow"))?;
                self.agents
                    .insert(offer.supplier_agent_id.clone(), supplier);
                let location_revision = state
                    .agent_location_authorities
                    .get(actor)
                    .map_or(Some(1), |authority| {
                        authority.authority_revision.checked_add(1)
                    })
                    .ok_or_else(|| invalid("ordinary_starter_location_revision_overflow"))?;
                self.locations.insert(
                    actor.into(),
                    crate::AgentLocationAuthorityV1 {
                        agent_id: actor.into(),
                        location_id: slot.location_id.clone(),
                        active: true,
                        authority_revision: location_revision,
                        effective_at: now,
                    },
                );
                let mut site = state.factory_site_authorities[&slot.site_id].clone();
                site.owner_agent_id = actor.into();
                site.authorized_agent_ids = vec![actor.into()];
                site.authority_revision = site
                    .authority_revision
                    .checked_add(1)
                    .ok_or_else(|| invalid("ordinary_starter_site_revision_overflow"))?;
                site.registered_at = now;
                self.sites.insert(slot.site_id.clone(), site);
                self.rules.ordinary_starter.claims.insert(
                    owner_key.clone(),
                    OrdinaryStarterClaimV1 {
                        world_binding: offer.world_binding.clone(),
                        owner_binding: owner_binding.clone(),
                        owner_generation: *owner_generation,
                        agent_id: actor.into(),
                        offer_id: offer.offer_id.clone(),
                        offer_revision: offer.revision,
                        offer_digest: offer_digest.clone(),
                        slot_id: slot_id.clone(),
                        site_id: slot.site_id.clone(),
                        factory_id: slot.factory_id.clone(),
                        location_id: slot.location_id.clone(),
                        profile_id: offer.profile_id.clone(),
                        profile_revision: offer.profile_revision,
                        recipe_id: offer.recipe_id.clone(),
                        materials,
                        electricity,
                        receipt: OrdinaryStarterClaimReceiptV1 {
                            receipt_key: key.into(),
                            request_id: request.into(),
                            actor_id: actor.into(),
                            command: command.clone(),
                            committed_at: now,
                        },
                        first_production: None,
                    },
                );
                Ok(owner_key)
            }
        }
    }
}

/// Public stock stays separate from an owner's claim and private inventory.
pub fn ordinary_starter_public_offers(
    state: &WorldState,
    world_binding: Option<&str>,
    now: WorldTime,
) -> BTreeMap<String, OrdinaryStarterOfferStatusV1> {
    let Some(rules) = &state.world_rules else {
        return BTreeMap::new();
    };
    rules
        .ordinary_starter
        .offers
        .iter()
        .map(|(id, offer)| {
            let stock = state
                .material_ledgers
                .get(&MaterialLedgerId::agent(&offer.supplier_agent_id))
                .cloned()
                .unwrap_or_default();
            let electricity = state
                .agents
                .get(&offer.supplier_agent_id)
                .map(|agent| agent.state.resources.get(ResourceKind::Electricity))
                .unwrap_or(0);
            let mut blocker = (world_binding != Some(offer.world_binding.as_str()))
                .then(|| "ordinary_starter_world_binding_mismatch".to_string());
            if blocker.is_none() {
                blocker = package(state)
                    .ok()
                    .filter(|(materials, power, digest)| {
                        materials == &offer.materials
                            && *power == offer.recipe_electricity
                            && digest == &offer.dependency_digest
                    })
                    .is_none()
                    .then(|| "ordinary_starter_recipe_obligation_changed".into());
            }
            let available_slots = offer
                .slots
                .iter()
                .filter(|(_, slot)| {
                    blocker.is_none()
                        && validate_slot(state, offer, slot, now).is_ok()
                        && !rules
                            .ordinary_starter
                            .claims
                            .values()
                            .any(|claim| claim.site_id == slot.site_id)
                        && offer
                            .recipe_electricity
                            .checked_add(slot.construction_electricity)
                            .is_some_and(|amount| electricity >= amount)
                        && offer
                            .materials
                            .iter()
                            .all(|(kind, amount)| stock.get(kind).copied().unwrap_or(0) >= *amount)
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            if blocker.is_none() && available_slots.is_empty() {
                blocker = Some("ordinary_starter_supply_unavailable".into());
            }
            (
                id.clone(),
                OrdinaryStarterOfferStatusV1 {
                    offer: offer.clone(),
                    offer_digest: ordinary_starter_offer_digest(offer).unwrap_or_default(),
                    available_slots,
                    supplier_materials: stock,
                    supplier_electricity: electricity,
                    blocker,
                },
            )
        })
        .collect()
}

/// Called only after the completion matched its paid pending job and output preflight.
/// The milestone and actual product credit are installed by one industry transaction.
pub fn ordinary_starter_completion(
    state: &WorldState,
    pending: &crate::RecipeJobState,
    now: WorldTime,
) -> Option<WorldRulesStateV1> {
    let rules = state.world_rules.as_ref()?;
    let (key, claim) = rules.ordinary_starter.claims.iter().find(|(_, claim)| {
        claim.agent_id == pending.requester_agent_id
            && claim.factory_id == pending.factory_id
            && claim.first_production.is_none()
            && claim.recipe_id == pending.recipe_id
    })?;
    let offer = rules.ordinary_starter.offers.get(&claim.offer_id)?;
    let slot = offer.slots.get(&claim.slot_id)?;
    let factory = state.factories.get(&pending.factory_id)?;
    let plan = builtin_recipe_plan(&claim.recipe_id, pending.accepted_batches)?;
    if claim.profile_id != ORDINARY_STARTER_PROFILE_ID
        || claim.profile_revision != ORDINARY_STARTER_PROFILE_REVISION
        || claim.offer_revision != offer.revision
        || claim.world_binding != offer.world_binding
        || factory.builder_agent_id != claim.agent_id
        || factory.site_id != claim.site_id
        || factory.factory_id != slot.factory_id
        || factory.output_ledger != pending.output_ledger
        || factory.input_ledger != pending.consume_ledger
        || pending.power_owner_agent_id.as_deref() != Some(claim.agent_id.as_str())
        || pending.produce != plan.produce
        || pending.byproducts != plan.byproducts
        || pending.power_required != plan.power_required
    {
        return None;
    }
    // Recover the pinned single-batch obligations from the published package, not today's profiles.
    let canonical = builtin_factory_spec(&offer.factory_kind, &slot.factory_id)?;
    let mut expected = offer.materials.clone();
    for stack in canonical.build_cost {
        let value = expected.get_mut(&stack.kind)?;
        *value = value.checked_sub(stack.amount)?;
    }
    expected.retain(|_, amount| *amount != 0);
    for amount in expected.values_mut() {
        if *amount < 0 || *amount % 2 != 0 {
            return None;
        }
        *amount = (*amount / 2).checked_mul(i64::from(pending.accepted_batches))?;
    }
    let mut consumed = BTreeMap::<String, i64>::new();
    for stack in &pending.consume {
        if stack.amount <= 0 {
            return None;
        }
        let amount = consumed.entry(stack.kind.clone()).or_default();
        *amount = amount.checked_add(stack.amount)?;
    }
    if expected != consumed {
        return None;
    }
    let milestone = OrdinaryStarterFirstProductionV1 {
        owner_binding: claim.owner_binding.clone(),
        agent_id: claim.agent_id.clone(),
        site_id: claim.site_id.clone(),
        factory_id: claim.factory_id.clone(),
        profile_id: claim.profile_id.clone(),
        profile_revision: claim.profile_revision,
        recipe_id: pending.recipe_id.clone(),
        job_id: pending.job_id,
        settled_at: now,
        output_ledger: pending.output_ledger.clone(),
        accepted_batches: pending.accepted_batches,
        consume: pending.consume.clone(),
        power_required: pending.power_required,
        produce: pending.produce.clone(),
        byproducts: pending.byproducts.clone(),
    };
    let mut next = rules.clone();
    next.ordinary_starter.claims.get_mut(key)?.first_production = Some(milestone);
    Some(next)
}
