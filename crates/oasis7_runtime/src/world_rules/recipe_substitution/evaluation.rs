use super::*;
pub(super) fn candidate(
    state: &WorldState,
    rules: &SubstitutionStateV1,
    industrial: &IndustrialWorkStateV1,
    request: &SubstitutionPreviewRequestV1,
    reference: &IndustrialRefV1,
    context: SubstitutionContext<'_>,
) -> Result<SubstitutionCandidateV1, WorldError> {
    let SubstitutionContext { actor, world, now } = context;
    qualified(&request.root_ref, world)?;
    qualified(reference, world)?;
    let root = industrial
        .roots
        .get(&request.root_ref.id)
        .filter(|r| r.root_ref == request.root_ref)
        .ok_or_else(|| invalid("substitution_root_missing"))?;
    let binding = super::super::industrial_work_admission::industrial_owner_binding(state, actor)?;
    if root.owner_id != actor || root.owner_binding_digest != binding {
        return Err(invalid("owner_mismatch"));
    }
    let slot = root
        .slots
        .get(&request.slot_id)
        .ok_or_else(|| invalid("substitution_slot_missing"))?;
    if slot.consumed || industrial.dispositions.contains_key(&root.root_ref.id) {
        return Err(invalid("substitution_slot_unavailable"));
    }
    let authority = rules
        .authorities
        .get(&reference.id)
        .filter(|a| &a.authority_ref == reference)
        .ok_or_else(|| invalid("missing_authority"))?;
    if authority.owner_id != actor
        || authority.root_ref != root.root_ref
        || authority.slot_id != request.slot_id
        || authority.expected_binding_revision != slot.binding_revision
        || authority.valid_until < now
        || authority.owner_binding_digest != binding
    {
        return Err(invalid("stale_candidate"));
    }
    let factory = state
        .factories
        .get(&root.factory_id)
        .ok_or_else(|| invalid("substitution_factory_missing"))?;
    if factory.builder_agent_id != actor
        || factory.durability_ppm <= 0
        || state.retired_factory_ids.contains(&root.factory_id)
        || factory.production.status == crate::FactoryProductionStatus::Paused
        || crate::util::hash_json(factory)? != authority.factory_digest
    {
        return Err(invalid("substitution_factory_stale"));
    }
    let profile = state
        .recipe_profiles
        .get(&root.pinned_recipe_id)
        .ok_or_else(|| invalid("substitution_profile_unknown"))?;
    let digest = crate::util::hash_json(profile)?;
    if digest != root.pinned_profile_digest || digest != authority.profile_digest {
        return Err(invalid("substitution_profile_stale"));
    }
    profile
        .validate_substitution_rules()
        .map_err(|e| invalid(e.to_string()))?;
    if crate::util::hash_json(&authority.baseline_impacts)? != root.causal_digest {
        return Err(invalid("substitution_causal_baseline_stale"));
    }
    if authority.valuation.unit.is_empty()
        || authority.valuation.unit.len() > 256
        || authority.valuation.unit.trim() != authority.valuation.unit
        || authority.valuation.consumed != 0
        || authority.risks.is_empty()
        || authority.risks.len() > 32
        || authority
            .risks
            .iter()
            .any(|r| r.is_empty() || r.len() > 256 || r.trim() != r)
        || authority.next_recheck < now
        || authority.next_recheck > authority.valid_until
    {
        return Err(invalid("substitution_risk_evidence_invalid"));
    }
    if authority.duration_ticks != 0 {
        return Err(invalid("unsupported_conversion_duration"));
    }
    let mut result = SubstitutionCandidateV1 {
        candidate_digest: String::new(),
        authority: authority.clone(),
        rule: None,
        kind: SubstitutionCandidateKindV1::SameMaterial,
        plan: None,
        native_source_quantity: None,
        source_material_kind: None,
        candidate_root: None,
        cutover_plan: None,
        cutover_plan_digest: None,
        recipe_plan: None,
        causal_equivalent: false,
        conversion_required: false,
        source_remaining: 0,
        binding_revision: slot.binding_revision,
        resource_balances: BTreeMap::new(),
    };
    match &authority.selection {
        SubstitutionSelectionV1::NewRecipe { candidate_root } => {
            qualified(candidate_root, world)?;
            let target = industrial
                .roots
                .get(&candidate_root.id)
                .filter(|r| r.root_ref == *candidate_root)
                .ok_or_else(|| invalid("substitution_candidate_root_missing"))?;
            let target_profile = state
                .recipe_profiles
                .get(&target.pinned_recipe_id)
                .ok_or_else(|| invalid("substitution_target_profile_missing"))?;
            if target.root_ref == root.root_ref
                || target.owner_id != actor
                || target.owner_binding_digest != binding
                || target.factory_id != root.factory_id
                || target.pinned_recipe_id == root.pinned_recipe_id
                || target.pinned_recipe_id != authority.target_recipe
                || target.work != 0
                || target.first_sink
                || target.started_job_id.is_some()
                || industrial.dispositions.contains_key(&target.root_ref.id)
                || target.pinned_profile_digest != crate::util::hash_json(target_profile)?
                || target.causal_digest != crate::util::hash_json(&authority.candidate_impacts)?
                || !authority.costs.is_empty()
                || authority.power_cost != 0
            {
                return Err(invalid("substitution_target_recipe_not_applicable"));
            }
            let plan =
                crate::builtin_recipe_plan(&target.pinned_recipe_id, target.accepted_batches)
                    .ok_or_else(|| invalid("substitution_target_recipe_unsupported"))?;
            // Reuse the actual native StartRoot validation on a private snapshot.
            // It verifies full canonical slots, qualification, site, power, paths,
            // and current product/factory guards without committing any effects.
            let mut snapshot = state.clone();
            let mut world_rules = snapshot.world_rules.take().unwrap_or_default();
            world_rules.industrial_work = industrial.clone();
            world_rules.substitution = rules.clone();
            snapshot.world_rules = Some(world_rules);
            // A read-only rehearsal must not collide with a real production
            // identity, including completed jobs retained for replay protection.
            let mut simulated_job_id = u64::MAX;
            while snapshot.pending_recipe_jobs.contains_key(&simulated_job_id)
                || snapshot.settled_recipe_job_ids.contains(&simulated_job_id)
            {
                simulated_job_id = simulated_job_id
                    .checked_sub(1)
                    .ok_or_else(|| invalid("substitution_preview_identity_unavailable"))?;
            }
            let ev = crate::DomainEvent::WorldRuleCommitted {
                action_id: simulated_job_id,
                actor_id: actor.into(),
                request_id: crate::util::hash_json(&(reference, "preview-start"))?,
                command: WorldRuleCommandV1::IndustrialWork {
                    command: Box::new(IndustrialWorkCommandV1::StartRoot {
                        root_ref: target.root_ref.clone(),
                        expected_profile_digest: target.pinned_profile_digest.clone(),
                        logistics_route_ids: vec![],
                        logistics_path_ids: vec![],
                    }),
                },
                governance_epoch: 0,
                world_binding: Some(world.into()),
            };
            PreparedWorldRule::prepare(&snapshot, &ev, now)?;
            if !root
                .allowed_dispositions
                .contains(&IndustrialDispositionV1::Terminate)
            {
                return Err(invalid("substitution_cutover_disposition_unsupported"));
            }
            let plan_id = crate::util::hash_json(&(
                world,
                &root.root_ref,
                &target.root_ref,
                "substitution-cutover",
            ))?;
            let disposition = DispositionPlanV1 {
                root_ref: root.root_ref.clone(),
                plan_ref: work::industrial_ref(
                    world,
                    "industrial",
                    "disposition/v1",
                    &plan_id,
                    1,
                    &(
                        &root.root_ref,
                        &target.root_ref,
                        &root.causal_digest,
                        &target.causal_digest,
                        "terminate",
                    ),
                )?,
                expected_causal_digest: root.causal_digest.clone(),
                disposition: IndustrialDispositionV1::Terminate,
            };
            result.cutover_plan_digest = Some(crate::util::hash_json(&(&disposition, target))?);
            result.cutover_plan = Some(disposition);
            result.kind = SubstitutionCandidateKindV1::NewRecipe;
            result.candidate_root = Some(target.clone());
            result.recipe_plan = Some(plan);
        }
        selection => {
            let (source_ref, material) = selection
                .source()
                .ok_or_else(|| invalid("substitution_source_missing"))?;
            qualified(source_ref, world)?;
            let lot = industrial
                .lots
                .get(&source_ref.id)
                .filter(|l| l.lot_ref == *source_ref)
                .ok_or_else(|| invalid("substitution_source_missing"))?;
            let metadata = industrial
                .imported_lot_authorities
                .get(&source_ref.id)
                .ok_or_else(|| invalid("substitution_source_authority_unknown"))?;
            if metadata.expires_at < now
                || metadata.owner_binding_digest != binding
                || metadata.material_profile != *material
                || lot.owner_id != actor
                || lot.source_ledger != factory.input_ledger
            {
                return Err(invalid("substitution_source_not_applicable"));
            }
            if authority.output_material != slot.material
                || authority.output_unit != slot.unit
                || authority.output_spec != slot.spec
                || authority.output_quality != slot.quality
                || authority.output_custody != slot.custody
                || authority.terminal_capacity < slot.quantity
                || authority.target_recipe != root.pinned_recipe_id
            {
                return Err(invalid("substitution_target_not_applicable"));
            }
            result.source_material_kind = Some(lot.material.clone());
            match selection {
                SubstitutionSelectionV1::SameMaterialSource { .. } => {
                    if *material != slot.material_profile
                        || lot.material != slot.material
                        || lot.unit != slot.unit
                        || lot.spec != slot.spec
                        || lot.quality != slot.quality
                        || lot.custody != slot.custody
                        || lot.remaining < slot.quantity
                        || authority.baseline_impacts != authority.candidate_impacts
                        || !authority.costs.is_empty()
                        || authority.power_cost != 0
                    {
                        return Err(invalid("substitution_source_swap_not_applicable"));
                    }
                    result.native_source_quantity = Some(slot.quantity);
                    result.causal_equivalent = true;
                }
                SubstitutionSelectionV1::ProfileRule { rule, .. } => {
                    let rule = profile
                        .substitution_rules
                        .iter()
                        .find(|r| r.rule == *rule && r.input_edge == request.slot_id)
                        .ok_or_else(|| invalid("unsupported_substitution"))?;
                    if rule.required_material != slot.material_profile
                        || lot.material == slot.material
                        || rule.target_unit != slot.unit
                        || !rule.allowed_materials.contains(material)
                        || lot.unit != rule.source_unit
                        || !rule.accepted_specs.contains(&lot.spec)
                        || !rule.accepted_quality.contains(&lot.quality)
                        || !rule.accepted_custody.contains(&lot.custody)
                        || authority.owner_ledger_path_policy != rule.owner_ledger_path_policy
                        || authority.candidate_impacts != rule.impacts
                        || !rule
                            .applicable_input_states
                            .contains(&authority.input_state)
                    {
                        return Err(invalid("not_applicable"));
                    }
                    if !matches!(rule.conversion, SubstitutionConversionV1::Required { .. }) {
                        return Err(invalid("unsupported_direct_substitution"));
                    }
                    let domain_key = crate::util::hash_json(&(
                        &root.root_ref,
                        &request.slot_id,
                        "substitution_conversion",
                    ))?;
                    let plan = rule
                        .quantity_plan(
                            rule.aggregation_domain.clone(),
                            domain_key,
                            slot.quantity,
                            &lot.unit,
                            &slot.unit,
                        )
                        .map_err(|e| invalid(e.to_string()))?;
                    if lot.remaining < plan.source_required {
                        return Err(invalid("resource_unavailable"));
                    }
                    result.kind = SubstitutionCandidateKindV1::ProfileSubstitute;
                    result.plan = Some(plan);
                    result.rule = Some(rule.clone());
                    result.conversion_required = true;
                    result.causal_equivalent =
                        authority.baseline_impacts == authority.candidate_impacts;
                }
                _ => unreachable!(),
            }
            result.source_remaining = lot.remaining;
        }
    }
    let ledger = state.material_ledgers.get(&MaterialLedgerId::agent(actor));
    for (material, amount) in &authority.costs {
        let required = i64::try_from(*amount).map_err(|_| invalid("substitution_cost_overflow"))?;
        let balance = ledger.and_then(|l| l.get(material)).copied().unwrap_or(0);
        if required <= 0 || balance < required {
            return Err(invalid("resource_unavailable"));
        }
        result.resource_balances.insert(material.clone(), balance);
    }
    let power = state
        .agents
        .get(actor)
        .ok_or_else(|| invalid("actor_not_found"))?
        .state
        .resources
        .get(ResourceKind::Electricity);
    if i64::try_from(authority.power_cost).map_err(|_| invalid("substitution_power_overflow"))?
        > power
    {
        return Err(invalid("resource_unavailable"));
    }
    result
        .resource_balances
        .insert("@electricity".into(), power);
    result.candidate_digest = crate::util::hash_json(&result)?;
    Ok(result)
}
