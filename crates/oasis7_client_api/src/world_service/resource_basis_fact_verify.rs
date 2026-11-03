use super::quote_economy::stacks;
use super::*;
use std::collections::BTreeSet;
impl ResourceBasisFactV1 {
    pub fn validate(
        &self,
        selection: &ResourceBasisSelectionV1,
        owner: &ResourceOwnerStateV1,
    ) -> Result<(), String> {
        self.selection.validate()?;
        if self.selection != *selection
            || self.agent_id != owner.owner.agent_id
            || self.quoted_at_tick != owner.logical_tick
            || self.actor_owner.player_id != owner.owner.player_id
            || self.actor_owner.public_key.as_ref() != Some(&owner.owner.public_key)
            || self.actor_owner.generation != owner.owner.agent_identity_generation
            || !id(&self.actor_owner.source_id)
            || self.actor_electricity_before.0 < 0
        {
            return Err(no());
        }
        let mut refs = BTreeSet::new();
        let mut ledgers = BTreeSet::new();
        for l in &self.ledgers {
            l.ledger.validate()?;
            if !ledgers.insert(l.ledger.reference())
                || l.materials
                    .windows(2)
                    .any(|m| m[0].material >= m[1].material)
                || l.materials.iter().any(|m| {
                    !id(&m.material)
                        || m.amount.is_some_and(|a| a.0 < 0)
                        || !l.exists && m.amount.is_some()
                })
            {
                return Err(no());
            }
            refs.insert(l.ledger.reference());
            refs.extend(l.materials.iter().map(|m| m.material.clone()));
            match (&l.ledger, &l.owner) {
                (ResourceLedgerV1::World, ResourceLedgerOwnerV1::World) => {}
                (
                    ResourceLedgerV1::Agent { agent_id },
                    ResourceLedgerOwnerV1::Agent {
                        agent_id: other,
                        owner: actor,
                    },
                ) if agent_id == other
                    && id(&actor.player_id)
                    && id(&actor.source_id)
                    && actor.generation.0 > 0 =>
                {
                    if agent_id == &self.agent_id && actor != &self.actor_owner {
                        return Err(no());
                    }
                }
                (
                    ResourceLedgerV1::Site { site_id },
                    ResourceLedgerOwnerV1::Site {
                        site_id: other,
                        owner_agent_id,
                        authority_revision,
                    },
                ) if site_id == other && id(owner_agent_id) && authority_revision.0 > 0 => {}
                (
                    ResourceLedgerV1::Factory { factory_id },
                    ResourceLedgerOwnerV1::Factory {
                        factory_id: other,
                        builder_agent_id,
                    },
                ) if factory_id == other && id(builder_agent_id) => {}
                _ => return Err(no()),
            }
        }
        let (authorization, action_kind, object) = match (&self.selection, &self.quote) {
            (
                ResourceBasisSelectionV1::Transfer {
                    from,
                    to,
                    material,
                    amount,
                    distance_km,
                    priority,
                    routing,
                },
                ResourceBasisQuoteV1::Transfer(v),
            ) => {
                let routes = match routing {
                    ResourceRoutingV1::Direct => vec![],
                    ResourceRoutingV1::Routes { route_ids } => {
                        refs.extend(route_ids.iter().cloned());
                        route_ids.clone()
                    }
                };
                let expected = ResourceQuoteActionV1::TransferMaterial {
                    requester_agent_id: self.agent_id.clone(),
                    from_ledger: from.clone(),
                    to_ledger: to.clone(),
                    kind: material.clone(),
                    amount: *amount,
                    distance_km: *distance_km,
                    priority: Some(*priority),
                    route_id: None,
                    route_ids: routes.clone(),
                    auto_reroute: false,
                };
                let q = &v.quote;
                if v.action != expected
                    || !q.submission_feasible
                    || q.requester_agent_id != self.agent_id
                    || q.from_ledger != *from
                    || q.to_ledger != *to
                    || q.kind != *material
                    || q.requested_amount != *amount
                    || q.route_ids != routes
                    || self.ledgers.len() != 2
                    || !self.objects.is_empty()
                    || !ledgers.contains(&from.reference())
                    || !ledgers.contains(&to.reference())
                {
                    return Err(no());
                }
                let source = self.ledger(from)?;
                let destination = self.ledger(to)?;
                if source.materials.len() != 1
                    || destination.materials.len() != 1
                    || source.materials[0].material != *material
                    || destination.materials[0].material != *material
                    || source.materials[0].amount != Some(q.source_amount_before)
                    || destination.materials[0].amount.map_or(0, |n| n.0)
                        != q.destination_amount_before.0
                    || q.sent_amount != *amount
                    || q.sent_amount.0.checked_sub(q.expected_loss_amount.0)
                        != Some(q.expected_received_amount.0)
                    || q.source_amount_before.0.checked_sub(q.sent_amount.0)
                        != Some(q.source_amount_after.0)
                    || q.destination_amount_before
                        .0
                        .checked_add(q.expected_received_amount.0)
                        != Some(q.destination_expected_amount_after.0)
                    || q.loss_bps.0 < 0
                    || q.loss_bps.0 > 10000
                    || q.expected_loss_amount.0 < 0
                    || q.expected_received_amount.0 < 0
                    || q.tariff_electricity_total.0 < 0
                    || self
                        .quoted_at_tick
                        .0
                        .saturating_add(1)
                        .saturating_add(q.ticks_until_arrival.0)
                        != q.ready_at.0
                {
                    return Err(no());
                }
                (
                    &v.authorization,
                    "TransferMaterial",
                    format!("{}->{}:{material}", from.reference(), to.reference()),
                )
            }
            (
                ResourceBasisSelectionV1::Build {
                    site,
                    factory,
                    factory_kind,
                },
                ResourceBasisQuoteV1::Build(v),
            ) => {
                let native: oasis7_agent_api::world_quote::NativeBuiltinFactoryQuote =
                    (**v).clone().try_into()?;
                native.validate_selection(&self.agent_id, site, factory_kind, factory)?;
                if v.rejection.is_some()
                    || v.preview.is_none()
                    || v.quoted_at_tick != self.quoted_at_tick
                    || v.electricity_before != self.actor_electricity_before
                    || self.objects.len() != 1
                    || self.ledgers.len() != 1
                {
                    return Err(no());
                }
                let preview = v.preview.as_ref().ok_or_else(no)?;
                let l = self.ledger(&preview.consume_ledger)?;
                let before = preview
                    .construction_power_obligation
                    .material_balances_before
                    .as_ref()
                    .ok_or_else(no)?;
                if l.materials.len() != before.len()
                    || l.materials
                        .iter()
                        .any(|m| m.amount.as_ref() != before.get(&m.material))
                {
                    return Err(no());
                }
                match &self.objects[0] {
                    ResourceObjectFactV1::Site {
                        site_id,
                        owner_agent_id,
                        authorized_agent_ids,
                        authority_revision,
                        location_id,
                    } if site_id == site
                        && id(owner_agent_id)
                        && (*owner_agent_id == self.agent_id
                            || authorized_agent_ids.contains(&self.agent_id))
                        && authorized_agent_ids.iter().all(|v| id(v))
                        && *authority_revision == preview.site_authority_revision
                        && *location_id == preview.site_location_id => {}
                    _ => return Err(no()),
                }
                refs.extend([site.clone(), factory.clone()]);
                (&v.authorization, "BuildFactory", factory.clone())
            }
            (
                ResourceBasisSelectionV1::Schedule {
                    factory,
                    recipe,
                    batches,
                },
                ResourceBasisQuoteV1::Schedule(v),
            ) => {
                let native: oasis7_agent_api::world_quote::NativeBuiltinRecipeQuote =
                    (**v).clone().try_into()?;
                native.validate_selection(&self.agent_id, factory, recipe, *batches)?;
                if v.rejection.is_some()
                    || v.preview.is_none()
                    || v.quoted_at_tick != self.quoted_at_tick
                    || v.electricity_before != self.actor_electricity_before
                    || self.objects.len() != 1
                {
                    return Err(no());
                }
                let p = v.preview.as_ref().ok_or_else(no)?;
                let plan = oasis7_agent_api::world_quote::builtin_recipe_plan(recipe, *batches)
                    .ok_or_else(no)?;
                if p.consume != stacks(&plan.consume)
                    || self.ledgers.len()
                        != if p.consume_ledger == p.output_ledger {
                            1
                        } else {
                            2
                        }
                {
                    return Err(no());
                }
                match &self.objects[0] {
                    ResourceObjectFactV1::Factory {
                        factory_id,
                        builder_agent_id,
                        input_ledger,
                        output_ledger,
                    } if factory_id == factory
                        && id(builder_agent_id)
                        && *input_ledger == p.consume_ledger
                        && *output_ledger == p.output_ledger => {}
                    _ => return Err(no()),
                }
                let input = self.ledger(&p.consume_ledger)?;
                let output = self.ledger(&p.output_ledger)?;
                for m in &p.consume {
                    if input
                        .materials
                        .iter()
                        .find(|v| v.material == m.kind)
                        .and_then(|v| v.amount)
                        .is_none_or(|a| a.0 < m.amount.0)
                    {
                        return Err(no());
                    }
                }
                let incoming: BTreeSet<_> = p.consume.iter().map(|v| v.kind.clone()).collect();
                let outgoing: BTreeSet<_> = p
                    .produce
                    .iter()
                    .chain(&p.byproducts)
                    .map(|v| v.kind.clone())
                    .collect();
                let actual_input: BTreeSet<_> =
                    input.materials.iter().map(|v| v.material.clone()).collect();
                let actual_output: BTreeSet<_> = output
                    .materials
                    .iter()
                    .map(|v| v.material.clone())
                    .collect();
                if p.consume_ledger == p.output_ledger {
                    if actual_input != incoming.union(&outgoing).cloned().collect() {
                        return Err(no());
                    }
                } else if actual_input != incoming || actual_output != outgoing {
                    return Err(no());
                }
                refs.extend([factory.clone(), recipe.clone()]);
                (&v.authorization, "ScheduleRecipe", factory.clone())
            }
            _ => return Err(no()),
        };
        let a = authorization;
        let g = &a.grant;
        if g.revision.0 == 0
            || g.resource_kind != "electricity"
            || !id(&g.grant_id)
            || !id(&g.issuer_id)
            || !id(&g.period_id)
            || g.owner_id != self.actor_owner.player_id
            || g.source_id != self.actor_owner.source_id
            || g.agent_id != self.agent_id
            || g.object_id != object
            || g.revoked
            || !g.action_kinds.iter().any(|k| k == action_kind)
            || g.valid_from_tick.0 > self.quoted_at_tick.0
            || g.valid_until_tick.0 < self.quoted_at_tick.0
            || a.spent_units.0.checked_add(a.remaining_units.0) != Some(g.limit_units.0)
            || a.remaining_units.0 < a.cost_units.0
        {
            return Err(no());
        }
        if refs.into_iter().collect::<Vec<_>>() != self.derived_resource_refs {
            return Err(no());
        }
        Ok(())
    }
    fn ledger(&self, wanted: &ResourceLedgerV1) -> Result<&ResourceLedgerFactV1, String> {
        self.ledgers
            .iter()
            .find(|v| &v.ledger == wanted)
            .ok_or_else(no)
    }
}
