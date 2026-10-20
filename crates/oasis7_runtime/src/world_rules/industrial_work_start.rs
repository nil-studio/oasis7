//! Atomic bridge from qualified escrow parents to ordinary recipe production.
use super::industrial_work::{consume_bound_slot, industrial_ref, record_receipt};
use super::industrial_work_admission::{
    industrial_effective_consume, industrial_ledger_access, industrial_owner_binding,
};
use super::{PreparedWorldRule, invalid};
use crate::{DomainEvent, WorldError, WorldState, WorldTime};
use oasis7_agent_api::world_action::*;

/// Checks each material independently in its native integer unit. Pending
/// outputs on the exact destination ledger already occupy this finite runway.
pub(crate) fn validate_industrial_output_capacity(
    state: &WorldState,
    root: &RootOperationV1,
    plan: &oasis7_wasm_abi::RecipeExecutionPlan,
    exclude_job: Option<u64>,
) -> Result<(), WorldError> {
    let mut output = std::collections::BTreeMap::<String, u64>::new();
    for stack in plan.produce.iter().chain(&plan.byproducts) {
        let amount = u64::try_from(stack.amount)
            .ok()
            .filter(|q| *q > 0)
            .ok_or_else(|| invalid("industrial_terminal_quantity_invalid"))?;
        let entry = output.entry(stack.kind.clone()).or_default();
        *entry = entry
            .checked_add(amount)
            .ok_or_else(|| invalid("industrial_terminal_quantity_overflow"))?;
    }
    if output.len() != root.terminal_capacities.len()
        || output
            .keys()
            .any(|kind| !root.terminal_capacities.contains_key(kind))
    {
        return Err(invalid("industrial_terminal_capacity_scope_mismatch"));
    }
    for (kind, incoming) in output {
        let capacity = root.terminal_capacities[&kind];
        if capacity == 0 {
            return Err(invalid("industrial_terminal_capacity_invalid"));
        }
        let balance = state
            .material_ledgers
            .get(&root.output_ledger)
            .and_then(|ledger| ledger.get(&kind))
            .copied()
            .unwrap_or(0);
        let mut committed =
            u64::try_from(balance).map_err(|_| invalid("industrial_terminal_balance_invalid"))?;
        for job in state.pending_recipe_jobs.values().filter(|job| {
            job.output_ledger == root.output_ledger && Some(job.job_id) != exclude_job
        }) {
            for stack in job
                .produce
                .iter()
                .chain(&job.byproducts)
                .filter(|stack| stack.kind == kind)
            {
                let amount = u64::try_from(stack.amount)
                    .ok()
                    .filter(|q| *q > 0)
                    .ok_or_else(|| invalid("industrial_terminal_quantity_invalid"))?;
                committed = committed
                    .checked_add(amount)
                    .ok_or_else(|| invalid("industrial_terminal_quantity_overflow"))?;
            }
        }
        if committed
            .checked_add(incoming)
            .is_none_or(|total| total > capacity)
        {
            return Err(invalid("industrial_terminal_capacity_unavailable"));
        }
    }
    Ok(())
}

impl PreparedWorldRule {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn industrial_start_root(
        &mut self,
        state: &WorldState,
        actor: &str,
        root_ref: &IndustrialRefV1,
        expected_profile_digest: &str,
        route_ids: &[String],
        path_ids: &[String],
        now: WorldTime,
        world: &str,
    ) -> Result<String, WorldError> {
        let root = self
            .rules
            .industrial_work
            .roots
            .get(&root_ref.id)
            .filter(|r| {
                r.root_ref == *root_ref
                    && r.owner_id == actor
                    && root_ref.world_binding == world
                    && !r.first_sink
            })
            .ok_or_else(|| invalid("industrial_start_root_unavailable"))?
            .clone();
        let profile = state
            .recipe_profiles
            .get(&root.pinned_recipe_id)
            .ok_or_else(|| invalid("industrial_recipe_profile_missing"))?;
        if root.pinned_profile_digest != expected_profile_digest
            || root.pinned_profile_digest != crate::util::hash_json(profile)?
        {
            return Err(invalid("industrial_start_profile_stale"));
        }
        let factory = state
            .factories
            .get(&root.factory_id)
            .filter(|f| {
                f.builder_agent_id == actor
                    && f.durability_ppm > 0
                    && !state.retired_factory_ids.contains(&root.factory_id)
                    && f.production.status != crate::FactoryProductionStatus::Paused
            })
            .ok_or_else(|| invalid("industrial_start_factory_unavailable"))?;
        if root.output_ledger != factory.output_ledger {
            return Err(invalid("industrial_start_output_destination_drifted"));
        }
        industrial_ledger_access(state, actor, &factory.input_ledger, now)?;
        industrial_ledger_access(state, actor, &factory.output_ledger, now)?;
        if !crate::world::recipe_stage_gate_allowed(
            state.industry_progress.stage,
            &profile.stage_gate,
        ) || !crate::world::recipe_preferred_tags_compatible(
            &profile.preferred_factory_tags,
            &factory.spec.tags,
        ) {
            return Err(invalid("industrial_start_recipe_ineligible"));
        }
        let plan = crate::builtin_recipe_plan(&root.pinned_recipe_id, root.accepted_batches)
            .ok_or_else(|| invalid("industrial_start_recipe_unsupported"))?;
        for output in plan.produce.iter().chain(&plan.byproducts) {
            if state
                .product_profiles
                .get(&output.kind)
                .is_some_and(|product| {
                    !crate::world::recipe_stage_gate_allowed(
                        state.industry_progress.stage,
                        &product.unlock_stage,
                    )
                })
            {
                return Err(invalid("industrial_start_product_locked"));
            }
        }
        validate_industrial_output_capacity(state, &root, &plan, None)?;
        let consume = industrial_effective_consume(state, &plan)?;
        if consume.len() != root.slots.len() {
            return Err(invalid("industrial_start_inputs_drifted"));
        }
        let owner_binding = industrial_owner_binding(state, actor)?;
        if root.owner_binding_digest != owner_binding {
            return Err(invalid("industrial_start_root_owner_stale"));
        }
        let mut parents = Vec::new();
        for input in &consume {
            let slot = root
                .slots
                .get(&input.kind)
                .filter(|slot| {
                    slot.material == input.kind
                        && i64::try_from(slot.quantity).ok() == Some(input.amount)
                        && !slot.consumed
                        && !slot.parents.is_empty()
                })
                .ok_or_else(|| invalid("industrial_start_inputs_incomplete"))?;
            for parent in &slot.parents {
                let lot = self
                    .rules
                    .industrial_work
                    .lots
                    .get(&parent.lot_ref.id)
                    .filter(|lot| {
                        lot.lot_ref == parent.lot_ref && lot.source_ledger == factory.input_ledger
                    })
                    .ok_or_else(|| invalid("industrial_start_source_not_arrived"))?;
                let authority = self
                    .rules
                    .industrial_work
                    .imported_lot_authorities
                    .get(&lot.lot_ref.id)
                    .ok_or_else(|| invalid("industrial_start_source_authority_missing"))?;
                if authority.owner_binding_digest != owner_binding
                    || authority.expires_at < now
                    || authority.material_profile != slot.material_profile
                {
                    return Err(invalid("industrial_start_lot_authority_stale"));
                }
            }
            parents.extend(consume_bound_slot(
                &mut self.rules.industrial_work,
                root_ref,
                &input.kind,
                world,
            )?);
        }
        let DomainEvent::WorldRuleCommitted { action_id, .. } = self.event else {
            return Err(invalid("industrial_start_event_required"));
        };
        self.rules
            .industrial_work
            .roots
            .get_mut(&root_ref.id)
            .expect("validated root")
            .started_job_id = Some(action_id);
        // Credit exists only on this private preparation clone. The ordinary
        // reducer immediately consumes exactly these certified escrow inputs.
        let mut temporary = state.clone();
        temporary.world_rules = Some(self.rules.clone());
        let ledger = temporary
            .material_ledgers
            .entry(factory.input_ledger.clone())
            .or_default();
        for input in &consume {
            let old = ledger.get(&input.kind).copied().unwrap_or(0);
            ledger.insert(
                input.kind.clone(),
                old.checked_add(input.amount)
                    .ok_or_else(|| invalid("industrial_start_quantity_overflow"))?,
            );
        }
        let ready_at = now
            .checked_add(u64::from(plan.duration_ticks))
            .ok_or_else(|| invalid("industrial_start_time_overflow"))?;
        let event = DomainEvent::RecipeStarted {
            job_id: action_id,
            requester_agent_id: actor.into(),
            factory_id: root.factory_id.clone(),
            recipe_id: root.pinned_recipe_id.clone(),
            accepted_batches: plan.accepted_batches,
            consume,
            produce: plan.produce,
            byproducts: plan.byproducts,
            power_required: plan.power_required,
            power_owner_agent_id: Some(actor.into()),
            duration_ticks: plan.duration_ticks,
            consume_ledger: factory.input_ledger.clone(),
            output_ledger: factory.output_ledger.clone(),
            bottleneck_tags: profile.bottleneck_tags.clone(),
            market_quotes: vec![],
            logistics_route_ids: route_ids.to_vec(),
            logistics_path_ids: path_ids.to_vec(),
            ready_at,
            emergency_power_obligation: None,
            committed_recipe_origin: None,
        };
        let mut prepared = crate::state::industry_transition::PreparedIndustryEvent::prepare(
            &temporary, &event, now,
        )?;
        prepared.bind_industrial_root(root_ref)?;
        if let Some(updated) = prepared.world_rules_projection() {
            self.rules = updated.clone();
        }
        let child = industrial_ref(
            world,
            "industrial",
            "industrial-start/v1",
            &format!("start:{}", root_ref.id),
            1,
            &(root_ref, &event),
        )?;
        let receipt_ref = industrial_ref(
            world,
            "industrial",
            "industrial-receipt/v1",
            &format!("production-start:{}", root_ref.id),
            1,
            &event,
        )?;
        record_receipt(
            &mut self.rules.industrial_work,
            IndustrialReceiptV1 {
                receipt_ref,
                root_ref: root_ref.clone(),
                child_ref: child,
                effect_role: "production_start".into(),
                payload_digest: crate::util::hash_json(&event)?,
                input: parents,
                output: vec![],
            },
            world,
        )?;
        self.industry = Some(prepared);
        Ok(root_ref.id.clone())
    }
}
