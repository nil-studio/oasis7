use super::*;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinRecipeStartPreview {
    pub consume_ledger: MaterialLedgerId,
    pub output_ledger: MaterialLedgerId,
    pub consume: Vec<MaterialStack>,
    pub produce: Vec<MaterialStack>,
    pub byproducts: Vec<MaterialStack>,
    pub power_required: i64,
    pub duration_ticks: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBuiltinRecipeQuote {
    pub action: Action,
    pub quoted_at_tick: u64,
    pub authorization: AgentDelegationAuthorizationV1,
    pub electricity_before: i64,
    pub electricity_after: Option<i64>,
    pub rejection: Option<RejectReason>,
    pub preview: Option<BuiltinRecipeStartPreview>,
}

impl NativeBuiltinRecipeQuote {
    pub fn validate_selection(
        &self,
        agent: &str,
        factory: &str,
        recipe: &str,
        batches: u32,
    ) -> Result<(), String> {
        match self.validate_selection_with_work_v1(
            agent,
            factory,
            recipe,
            batches,
            &mut |_, _, _| Ok::<_, std::convert::Infallible>(()),
        ) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }
    /// Original quote predicates, with resource observations paid before the
    /// canonical plan, expected action and complete comparison phase.
    pub fn validate_selection_with_work_v1<E>(
        &self,
        agent: &str,
        factory: &str,
        recipe: &str,
        batches: u32,
        work: &mut impl FnMut(u64, usize, usize) -> Result<(), E>,
    ) -> Result<Result<(), String>, E> {
        work(1, 0, std::mem::size_of::<String>() + 128)?;
        let Some(plan) = super::builtin_recipe_plan_with_work_v1(recipe, batches, work)? else {
            return Ok(Err("unsupported builtin recipe selection".into()));
        };
        let mut heap = agent
            .len()
            .saturating_add(factory.len())
            .saturating_add(recipe.len());
        let mut visits = 16u64;
        let mut compare_bytes = heap;
        for stacks in [&plan.consume, &plan.produce, &plan.byproducts] {
            heap = heap.saturating_add(
                stacks
                    .len()
                    .saturating_mul(std::mem::size_of::<MaterialStack>()),
            );
            for stack in stacks {
                work(1, stack.kind.len(), 0)?;
                heap = heap.saturating_add(stack.kind.len());
                visits = visits.saturating_add(1);
                compare_bytes = compare_bytes.saturating_add(stack.kind.len());
            }
        }
        heap = heap.saturating_add(plan.reject_reason.as_ref().map_or(0, String::len));
        if let Action::ScheduleRecipe {
            requester_agent_id,
            factory_id,
            recipe_id,
            plan,
            logistics_route_ids,
            logistics_path_ids,
        } = &self.action
        {
            compare_bytes = compare_bytes
                .saturating_add(requester_agent_id.len())
                .saturating_add(factory_id.len())
                .saturating_add(recipe_id.len());
            for stack in plan
                .consume
                .iter()
                .chain(&plan.produce)
                .chain(&plan.byproducts)
            {
                work(1, stack.kind.len(), 0)?;
                visits = visits.saturating_add(1);
                compare_bytes = compare_bytes.saturating_add(stack.kind.len());
            }
            for text in logistics_route_ids.iter().chain(logistics_path_ids) {
                work(1, text.len(), 0)?;
                visits = visits.saturating_add(1);
                compare_bytes = compare_bytes.saturating_add(text.len());
            }
        }
        for kind in &self.authorization.grant.action_kinds {
            work(1, kind.len(), 0)?;
            visits = visits.saturating_add(1);
            compare_bytes = compare_bytes.saturating_add(kind.len());
        }
        if let Some(preview) = &self.preview {
            for stack in preview.produce.iter().chain(&preview.byproducts) {
                work(1, stack.kind.len(), 0)?;
                visits = visits.saturating_add(1);
                compare_bytes = compare_bytes.saturating_add(stack.kind.len());
            }
        }
        compare_bytes = compare_bytes
            .saturating_add(self.authorization.grant.agent_id.len())
            .saturating_add(self.authorization.grant.object_id.len());
        work(visits, compare_bytes, heap)?;
        let result = (|| -> Result<(), String> {
            let expected = Action::ScheduleRecipe {
                requester_agent_id: agent.into(),
                factory_id: factory.into(),
                recipe_id: recipe.into(),
                plan: plan.clone(),
                logistics_route_ids: vec![],
                logistics_path_ids: vec![],
            };
            let a = &self.authorization;
            if self.action != expected
                || a.grant.agent_id != agent
                || a.grant.object_id != factory
                || !a.grant.action_kinds.iter().any(|k| k == "ScheduleRecipe")
                || a.grant.revoked
                || self.quoted_at_tick < a.grant.valid_from_tick
                || self.quoted_at_tick > a.grant.valid_until_tick
                || a.cost_units
                    != u64::try_from(plan.power_required).map_err(|_| "invalid canonical power")?
                || a.spent_units.checked_add(a.remaining_units) != Some(a.grant.limit_units)
                || a.remaining_units < a.cost_units
                || self.electricity_before < 0
            {
                return Err("native recipe quote selection or authority mismatch".into());
            }
            match (&self.rejection, &self.preview, self.electricity_after) {
                (None, Some(preview), Some(after))
                    if after >= 0
                        && self.electricity_before.checked_sub(plan.power_required)
                            == Some(after)
                        && preview.produce == plan.produce
                        && preview.byproducts == plan.byproducts
                        && preview.power_required == plan.power_required
                        && preview.duration_ticks >= plan.duration_ticks.max(1) =>
                {
                    Ok(())
                }
                (Some(_), None, None) => Ok(()),
                _ => Err("native recipe quote projection mismatch".into()),
            }
        })();
        work(1, 0, 0)?;
        Ok(result)
    }
}

#[cfg(test)]
mod paid_selection_tests {
    use super::*;
    fn quote() -> NativeBuiltinRecipeQuote {
        let plan = builtin_recipe_plan("recipe.smelter.iron_ingot", 1).unwrap();
        NativeBuiltinRecipeQuote {
            action: Action::ScheduleRecipe {
                requester_agent_id: "a".into(),
                factory_id: "f".into(),
                recipe_id: "recipe.smelter.iron_ingot".into(),
                plan: plan.clone(),
                logistics_route_ids: vec![],
                logistics_path_ids: vec![],
            },
            quoted_at_tick: 5,
            authorization: AgentDelegationAuthorizationV1 {
                grant: AgentDelegationGrantV1 {
                    grant_id: "g".into(),
                    source_id: "s".into(),
                    issuer_id: "p".into(),
                    owner_id: "p".into(),
                    organization_id: None,
                    agent_id: "a".into(),
                    object_id: "f".into(),
                    action_kinds: vec!["ScheduleRecipe".into()],
                    revision: 1,
                    period_id: "q".into(),
                    valid_from_tick: 0,
                    valid_until_tick: 10,
                    limit_units: 100,
                    resource_kind: "electricity".into(),
                    revoked: false,
                },
                cost_units: 8,
                spent_units: 0,
                remaining_units: 100,
            },
            electricity_before: 100,
            electricity_after: Some(92),
            rejection: None,
            preview: Some(BuiltinRecipeStartPreview {
                consume_ledger: MaterialLedgerId::agent("a"),
                output_ledger: MaterialLedgerId::agent("a"),
                consume: plan.consume,
                produce: plan.produce,
                byproducts: plan.byproducts,
                power_required: 8,
                duration_ticks: plan.duration_ticks.max(1),
            }),
        }
    }
    #[test]
    fn original_selection_and_refusal_preserved() {
        let mut value = quote();
        assert_eq!(
            value.validate_selection("a", "f", "recipe.smelter.iron_ingot", 1),
            Ok(())
        );
        let mut paid = 0u64;
        assert_eq!(
            value.validate_selection_with_work_v1(
                "a",
                "f",
                "recipe.smelter.iron_ingot",
                1,
                &mut |s, _, _| {
                    paid += s;
                    Ok::<_, ()>(())
                }
            ),
            Ok(Ok(()))
        );
        assert!(paid > 0);
        value.authorization.cost_units = 9;
        assert_eq!(
            value.validate_selection("a", "f", "recipe.smelter.iron_ingot", 1),
            Err("native recipe quote selection or authority mismatch".into())
        );
    }
    #[test]
    fn resource_failure_keeps_its_outer_type() {
        let value = quote();
        let mut remaining = 1u64;
        let result = value.validate_selection_with_work_v1(
            "a",
            "f",
            "recipe.smelter.iron_ingot",
            1,
            &mut |steps, _, _| {
                remaining = remaining.checked_sub(steps).ok_or("steps")?;
                Ok(())
            },
        );
        assert_eq!(result, Err("steps"));
        assert_eq!(remaining, 0);
    }
}
