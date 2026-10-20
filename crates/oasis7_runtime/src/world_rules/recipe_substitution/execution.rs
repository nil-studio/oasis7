use super::*;
impl PreparedWorldRule {
    pub(super) fn convert_substitution(
        &mut self,
        state: &WorldState,
        actor: &str,
        reference: &IndustrialRefV1,
        now: u64,
        world: &str,
    ) -> Result<String, WorldError> {
        let decision = self.decision(actor, reference, world)?;
        if let Some(done) = self.rules.substitution.settlements.get(&reference.id) {
            return Ok(done.receipt_ref.id.clone());
        }
        if decision.intent.scope != SubstitutionScopeV1::DecisionAndExactConversion
            || !decision.candidate.conversion_required
        {
            return Err(invalid("conversion_not_authorized"));
        }
        if !decision.candidate.causal_equivalent {
            return Err(invalid("causal_change_requires_cutover"));
        }
        let request = &decision.intent.preview.request;
        let fresh = candidate(
            state,
            &self.rules.substitution,
            &self.rules.industrial_work,
            actor,
            request,
            &decision.candidate.authority.authority_ref,
            world,
            now,
        )?;
        if fresh != decision.candidate {
            return Err(invalid("stale_candidate"));
        }
        let a = &fresh.authority;
        let plan = fresh
            .plan
            .as_ref()
            .ok_or_else(|| invalid("conversion_plan_missing"))?;
        let (source_ref, _) = a
            .selection
            .source()
            .ok_or_else(|| invalid("conversion_source_missing"))?;
        let rule = fresh
            .rule
            .as_ref()
            .ok_or_else(|| invalid("conversion_rule_missing"))?;
        if self
            .rules
            .substitution
            .conversion_domains
            .contains(&plan.domain_key)
        {
            return Err(invalid("substitution_domain_already_consumed"));
        }
        let used = self
            .rules
            .substitution
            .terminal_used
            .get(&a.authority_ref.id)
            .copied()
            .unwrap_or(0);
        let next_used = used
            .checked_add(plan.target_executable)
            .filter(|v| *v <= a.terminal_capacity)
            .ok_or_else(|| invalid("substitution_terminal_capacity_unavailable"))?;
        let conversion_id =
            crate::util::hash_json(&(world, &request.root_ref, reference, "conversion"))?;
        let conversion_ref = work::industrial_ref(
            world,
            "industrial",
            "recipe-conversion/v1",
            &conversion_id,
            1,
            &(&request.root_ref, reference, &fresh),
        )?;
        let id = crate::util::hash_json(&(reference, "conversion_settlement"))?;
        let receipt_ref = work::industrial_ref(
            world,
            "industrial",
            "recipe-substitution/v1",
            &id,
            1,
            &fresh,
        )?;
        let output_ref = work::industrial_ref(
            world,
            "industrial",
            "industrial-lot/v1",
            &format!("{id}:output"),
            1,
            &fresh,
        )?;
        let source = self
            .rules
            .industrial_work
            .lots
            .get(&source_ref.id)
            .expect("validated source")
            .clone();
        let costs = a
            .costs
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.clone(),
                    i64::try_from(*v).map_err(|_| invalid("substitution_cost_overflow"))?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, WorldError>>()?;
        // PreparedWorldRule is discarded on any failure; no public state is touched.
        self.debit(state, &MaterialLedgerId::agent(actor), &costs)?;
        self.agents
            .get_mut(actor)
            .ok_or_else(|| invalid("actor_not_found"))?
            .state
            .resources
            .remove(
                ResourceKind::Electricity,
                i64::try_from(a.power_cost).map_err(|_| invalid("substitution_power_overflow"))?,
            )
            .map_err(|e| invalid(format!("substitution_power:{e:?}")))?;
        self.rules
            .industrial_work
            .lots
            .get_mut(&source_ref.id)
            .expect("validated source")
            .remaining = source
            .remaining
            .checked_sub(plan.source_required)
            .ok_or_else(|| invalid("substitution_source_underflow"))?;
        // Converted outputs stay in industrial escrow. Aggregate ledger credit would
        // expose the same units both to ordinary scheduling and the industrial join.
        let output = IndustrialLotV1 {
            lot_ref: output_ref.clone(),
            owner_id: actor.into(),
            source_ledger: source.source_ledger.clone(),
            material: a.output_material.clone(),
            unit: a.output_unit.clone(),
            spec: a.output_spec.clone(),
            quality: a.output_quality.clone(),
            custody: a.output_custody.clone(),
            provenance_ref: receipt_ref.clone(),
            quantity: plan.target_executable,
            remaining: plan.target_executable,
        };
        work::import_lot(&mut self.rules.industrial_work, output, world)?;
        self.rules.industrial_work.imported_lot_authorities.insert(
            output_ref.id.clone(),
            IndustrialLotAuthorityV1 {
                owner_binding_digest: a.owner_binding_digest.clone(),
                expires_at: a.valid_until.min(
                    self.rules.industrial_work.imported_lot_authorities[&source_ref.id].expires_at,
                ),
                escrow_capacity: plan.target_executable,
                material_profile: rule.required_material.clone(),
            },
        );

        work::record_receipt(
            &mut self.rules.industrial_work,
            IndustrialReceiptV1 {
                receipt_ref: receipt_ref.clone(),
                root_ref: request.root_ref.clone(),
                child_ref: conversion_ref.clone(),
                effect_role: "conversion_settlement".into(),
                payload_digest: crate::util::hash_json(&fresh)?,
                input: vec![IndustrialParentV1 {
                    lot_ref: source.lot_ref,
                    settlement_ref: source.provenance_ref,
                    quantity: plan.source_required,
                }],
                output: vec![output_ref.clone()],
            },
            world,
        )?;
        if plan.residual_numerator > 0 {
            self.rules.substitution.retained_residuals.insert(
                plan.domain_key.clone(),
                SubstitutionResidualBalanceV1 {
                    owner_id: actor.into(),
                    domain_key: plan.domain_key.clone(),
                    source_lot: source_ref.clone(),
                    settlement_ref: receipt_ref.clone(),
                    unit: plan.source_unit.clone(),
                    numerator: plan.residual_numerator,
                    denominator: plan.residual_denominator,
                    next_action: "hold_fractional_residual_pending_supported_disposition".into(),
                },
            );
        }
        self.rules
            .substitution
            .conversion_domains
            .insert(plan.domain_key.clone());
        self.rules
            .substitution
            .terminal_used
            .insert(a.authority_ref.id.clone(), next_used);
        self.rules.substitution.settlements.insert(
            reference.id.clone(),
            ConversionSettlementV1 {
                decision_ref: reference.clone(),
                conversion_ref,
                receipt_ref: receipt_ref.clone(),
                output_lot: output_ref,
                source_consumed: plan.source_required,
                source_loss: plan.source_loss,
                residual_numerator: plan.residual_numerator,
                residual_denominator: plan.residual_denominator,
                committed_at: now,
            },
        );
        Ok(id)
    }
    pub(super) fn bind_substitution(
        &mut self,
        state: &WorldState,
        actor: &str,
        reference: &IndustrialRefV1,
        now: u64,
        world: &str,
    ) -> Result<String, WorldError> {
        let decision = self.decision(actor, reference, world)?;
        if !decision.candidate.causal_equivalent {
            return Err(invalid("causal_change_requires_cutover"));
        }
        let request = &decision.intent.preview.request;
        let authority = &decision.candidate.authority;
        let root = self
            .rules
            .industrial_work
            .roots
            .get(&request.root_ref.id)
            .filter(|r| r.root_ref == request.root_ref)
            .ok_or_else(|| invalid("substitution_root_missing"))?;
        if root.owner_binding_digest != authority.owner_binding_digest
            || authority.valid_until < now
            || authority.owner_binding_digest
                != super::super::industrial_work_admission::industrial_owner_binding(state, actor)?
            || state
                .factories
                .get(&root.factory_id)
                .map(crate::util::hash_json)
                .transpose()?
                .as_ref()
                != Some(&authority.factory_digest)
            || state
                .recipe_profiles
                .get(&root.pinned_recipe_id)
                .map(crate::util::hash_json)
                .transpose()?
                .as_ref()
                != Some(&authority.profile_digest)
        {
            return Err(invalid("substitution_binding_authority_stale"));
        }
        let parent = if decision.candidate.conversion_required {
            let settlement = self
                .rules
                .substitution
                .settlements
                .get(&reference.id)
                .ok_or_else(|| invalid("substitution_conversion_not_settled"))?;
            let output = self
                .rules
                .industrial_work
                .lots
                .get(&settlement.output_lot.id)
                .filter(|lot| lot.lot_ref == settlement.output_lot && lot.owner_id == actor)
                .ok_or_else(|| invalid("substitution_output_missing"))?;
            let output_authority = self
                .rules
                .industrial_work
                .imported_lot_authorities
                .get(&output.lot_ref.id)
                .ok_or_else(|| invalid("substitution_output_authority_missing"))?;
            if output_authority.expires_at < now
                || output_authority.owner_binding_digest != authority.owner_binding_digest
            {
                return Err(invalid("substitution_output_authority_stale"));
            }
            IndustrialParentV1 {
                lot_ref: settlement.output_lot.clone(),
                settlement_ref: settlement.receipt_ref.clone(),
                quantity: decision.candidate.target_executable(),
            }
        } else {
            let fresh = candidate(
                state,
                &self.rules.substitution,
                &self.rules.industrial_work,
                actor,
                request,
                &decision.candidate.authority.authority_ref,
                world,
                now,
            )?;
            if fresh != decision.candidate {
                return Err(invalid("stale_candidate"));
            }
            let lot = self
                .rules
                .industrial_work
                .lots
                .get(
                    &fresh
                        .authority
                        .selection
                        .source()
                        .ok_or_else(|| invalid("substitution_source_missing"))?
                        .0
                        .id,
                )
                .expect("validated source");
            IndustrialParentV1 {
                lot_ref: lot.lot_ref.clone(),
                settlement_ref: lot.provenance_ref.clone(),
                quantity: fresh.target_executable(),
            }
        };
        let plan = ParentBindingRevisionV1 {
            root_ref: request.root_ref.clone(),
            slot_id: request.slot_id.clone(),
            expected_revision: decision.candidate.binding_revision,
            decision_ref: reference.clone(),
            parents: vec![parent],
        };
        if self.rules.industrial_work.bindings.contains(&plan) {
            return Ok(reference.id.clone());
        }
        work::apply_parent_binding(&mut self.rules.industrial_work, plan, world)?;
        Ok(reference.id.clone())
    }
    pub(super) fn cutover_substitution(
        &mut self,
        state: &WorldState,
        actor: &str,
        reference: &IndustrialRefV1,
        disposition: &DispositionPlanV1,
        root: &RootOperationV1,
        now: u64,
        world: &str,
    ) -> Result<String, WorldError> {
        let decision = self.decision(actor, reference, world)?;
        if decision.candidate.cutover_plan.as_ref() != Some(disposition)
            || decision.candidate.candidate_root.as_ref() != Some(root)
        {
            return Err(invalid("substitution_cutover_plan_stale"));
        }
        let receipt_id = crate::util::hash_json(&(reference, "cutover"))?;
        if self
            .rules
            .industrial_work
            .receipts
            .get(&receipt_id)
            .is_some_and(|r| {
                r.payload_digest == crate::util::hash_json(&(disposition, root)).unwrap_or_default()
            })
        {
            return Ok(root.root_ref.id.clone());
        }
        let fresh = candidate(
            state,
            &self.rules.substitution,
            &self.rules.industrial_work,
            actor,
            &decision.intent.preview.request,
            &decision.candidate.authority.authority_ref,
            world,
            now,
        )?;
        if fresh != decision.candidate {
            return Err(invalid("stale_candidate"));
        }
        if !self
            .rules
            .industrial_work
            .roots
            .get(&root.root_ref.id)
            .is_some_and(|persisted| persisted == root)
            || decision.intent.scope != SubstitutionScopeV1::DecisionAndExactConversion
        {
            return Err(invalid("substitution_cutover_candidate_not_admitted"));
        }
        if decision.candidate.candidate_root.as_ref() != Some(root)
            || decision.candidate.causal_equivalent
            || decision.intent.cutover_plan_digest.as_ref()
                != Some(&crate::util::hash_json(&(disposition, root))?)
            || disposition.root_ref != decision.intent.preview.request.root_ref
            || root.owner_id != actor
            || root.root_ref == disposition.root_ref
            || root.pinned_recipe_id != decision.candidate.authority.target_recipe
            || root.causal_digest
                != crate::util::hash_json(&decision.candidate.authority.candidate_impacts)?
            || root.work != 0
            || root.first_sink
        {
            return Err(invalid("substitution_cutover_not_authorized"));
        }
        // Explicit plan holds/terminates the original identity; no receipts or W move.
        work::apply_disposition(&mut self.rules.industrial_work, disposition.clone(), world)?;
        work::accept_root(&mut self.rules.industrial_work, root.clone(), world)?;
        let id = crate::util::hash_json(&(reference, "cutover"))?;
        let receipt = work::industrial_ref(
            world,
            "industrial",
            "recipe-substitution/v1",
            &id,
            1,
            &(disposition, root),
        )?;
        work::record_receipt(
            &mut self.rules.industrial_work,
            IndustrialReceiptV1 {
                receipt_ref: receipt,
                root_ref: disposition.root_ref.clone(),
                child_ref: reference.clone(),
                effect_role: "substitution_cutover".into(),
                payload_digest: crate::util::hash_json(&(disposition, root))?,
                input: vec![],
                output: vec![root.root_ref.clone()],
            },
            world,
        )?;
        Ok(root.root_ref.id.clone())
    }
}
