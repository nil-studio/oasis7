//! Sole original direct ScheduleRecipe predicates, with closed read sources.
use super::factory_schedule_sources::FactoryRecipeFactsV1;
use super::*;
pub(in crate::world) fn evaluate_schedule_recipe_v1<'source, S: FactoryRecipeFactsV1<'source>>(
    source: &mut S,
    action_id: ActionId,
    requester_agent_id: &String,
    factory_id: &String,
    recipe_id: &String,
    plan: &oasis7_wasm_abi::RecipeExecutionPlan,
    logistics_route_ids: &Vec<String>,
    logistics_path_ids: &Vec<String>,
) -> Result<WorldEventBody, S::Error> {
    source.reserve_recipe_output(
        requester_agent_id,
        factory_id,
        recipe_id,
        plan,
        logistics_route_ids,
        logistics_path_ids,
    )?;
    if !source.agent_exists(requester_agent_id)? {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::AgentNotFound {
                agent_id: requester_agent_id.clone(),
            },
        }));
    }
    let Some(factory) = source.factory(factory_id)? else {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::FactoryNotFound {
                factory_id: factory_id.clone(),
            },
        }));
    };
    if let Some(reason) = (factory.builder()!=requester_agent_id.as_str()).then(||RejectReason::RuleDenied{notes:vec![format!("schedule recipe requester is not factory builder: factory_id={factory_id} requester={requester_agent_id} builder={}",factory.builder())]}) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason,
                    }));
                }
    let active_jobs = source.pending_recipes_for_factory(factory_id)?;
    if active_jobs >= factory.slots() as usize {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::FactoryBusy {
                factory_id: factory_id.clone(),
                active_jobs,
                recipe_slots: factory.slots(),
            },
        }));
    }
    if let Some(reason) = &plan.reject_reason {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!("recipe plan rejected: {reason}")],
            },
        }));
    }
    for (label, stacks) in [
        ("consume", plan.consume.as_slice()),
        ("produce", plan.produce.as_slice()),
        ("byproduct", plan.byproducts.as_slice()),
    ] {
        if let Some(note) = invalid_recipe_plan_stack_note(label, stacks) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied { notes: vec![note] },
            }));
        }
    }
    if plan.accepted_batches == 0 {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec!["recipe accepted_batches must be > 0".to_string()],
            },
        }));
    }
    if !plan
        .produce
        .iter()
        .any(|stack| !stack.kind.trim().is_empty() && stack.amount > 0)
    {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![
                    "recipe plan must produce at least one positive material output".to_string(),
                ],
            },
        }));
    }
    if let Some(reason) = source.output_stack_rejection(plan)? {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason,
        }));
    }
    let recipe_profile = source.recipe_profile(recipe_id)?;
    if let Some(profile) = recipe_profile.as_ref() {
        if !recipe_stage_gate_allowed(source.stage()?, profile.stage_gate.as_str()) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "recipe stage gate denied: recipe={} required_stage={} current_stage={}",
                        recipe_id,
                        profile.stage_gate,
                        industry_stage_label(source.stage()?),
                    )],
                },
            }));
        }
        if !recipe_preferred_tags_compatible(&profile.preferred_factory_tags, factory.tags()) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "recipe preferred_factory_tags mismatch: recipe={} preferred={:?} factory_tags={:?}",
                        recipe_id,
                        profile.preferred_factory_tags,
                        factory.tags()
                    )],
                },
            }));
        }
    }
    for stack in &plan.produce {
        let Some(product_profile) = source.product_profile(stack.kind.as_str())? else {
            continue;
        };
        if !product_unlock_stage_allowed(source.stage()?, product_profile.unlock_stage.as_str()) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "product unlock_stage denied: product={} required_stage={} current_stage={}",
                        product_profile.product_id,
                        product_profile.unlock_stage,
                        industry_stage_label(source.stage()?),
                    )],
                },
            }));
        }
    }
    let effective_consume = source.effective_consume(&plan.consume, &plan.produce)?;
    let preferred_consume_ledger = factory.input().clone();
    // Both direct and module-backed new submissions are
    // site-bound. Historical RecipeStarted events remain
    // reducer-compatible through their persisted ledgers.
    let consume_ledger = preferred_consume_ledger.clone();
    let output_ledger = factory.output().clone();
    let mut resolved_logistics_route_ids = Vec::new();
    let mut resolved_logistics_path_ids = logistics_path_ids.clone();
    resolved_logistics_path_ids.sort();
    if !logistics_route_ids.is_empty() {
        let mut seen_route_ids = BTreeSet::new();
        for route_id in logistics_route_ids {
            if !seen_route_ids.insert(route_id) {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "duplicate logistics route binding: route_id={route_id}"
                        )],
                    },
                }));
            }
        }
        let consumed_kinds: BTreeSet<String> = effective_consume
            .iter()
            .filter_map(|stack| normalize_logistics_route_kind(stack.kind.as_str()))
            .collect();
        if consumed_kinds.len() != logistics_route_ids.len() {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "logistics route binding count must equal distinct consumed material kinds: routes={} kinds={}",
                        logistics_route_ids.len(),
                        consumed_kinds.len()
                    )],
                },
            }));
        }
        let mut bound_kinds = BTreeSet::new();
        for route_id in logistics_route_ids {
            let Some(route) = source.route(route_id)? else {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("logistics route not found: route_id={route_id}")],
                    },
                }));
            };
            if !source.route_completed(route_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "logistics route is not completed: route_id={route_id}"
                        )],
                    },
                }));
            }
            if route.to_ledger != consume_ledger {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "logistics route destination mismatch: route_id={route_id}"
                        )],
                    },
                }));
            }
            let Some(route_kind) = normalize_logistics_route_kind(route.kind.as_str()) else {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "logistics route material kind is invalid: route_id={route_id}"
                        )],
                    },
                }));
            };
            if !consumed_kinds.contains(&route_kind) || !bound_kinds.insert(route_kind.clone()) {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "logistics route material mismatch: route_id={route_id}"
                        )],
                    },
                }));
            }
        }
        resolved_logistics_route_ids = logistics_route_ids.clone();
        resolved_logistics_route_ids.sort();
    }
    if resolved_logistics_path_ids.is_empty() {
        let mut derived = Vec::with_capacity(resolved_logistics_route_ids.len());
        for route_id in &resolved_logistics_route_ids {
            if let Some(path_id) = source.path_id(std::slice::from_ref(route_id))? {
                derived.push(path_id);
            }
        }
        resolved_logistics_path_ids = derived;
    }
    if !resolved_logistics_path_ids.is_empty()
        && let Err(reason) = source.allocate_paths(
            &consume_ledger,
            &resolved_logistics_route_ids,
            &resolved_logistics_path_ids,
            &effective_consume,
        )?
    {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![reason],
            },
        }));
    }
    let required_materials =
        match action_to_event_economy_support::aggregate_material_stacks_for_admission(
            "recipe consume",
            &effective_consume,
        ) {
            Ok(required_materials) => required_materials,
            Err(reason) => {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![reason],
                    },
                }));
            }
        };
    for (material_kind, requested) in required_materials {
        let available = source.material(&consume_ledger, material_kind.as_str())?;
        if available < requested {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::InsufficientMaterial {
                    material_kind,
                    requested,
                    available,
                },
            }));
        }
    }
    if plan.power_required < 0 {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec!["recipe power_required must be >= 0".to_string()],
            },
        }));
    }
    let power_owner_agent_id = factory.builder().to_owned();
    let available_power = source.electricity(&power_owner_agent_id)?;
    if available_power < plan.power_required {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::InsufficientResource {
                agent_id: power_owner_agent_id,
                kind: ResourceKind::Electricity,
                requested: plan.power_required,
                available: available_power,
            },
        }));
    }
    let market_quotes = source.market_quotes(&preferred_consume_ledger, &effective_consume)?;
    let bottleneck_tags = source.bottleneck_tags(recipe_profile.as_deref(), &effective_consume)?;
    let scarcity_delay_ticks = source.scarcity_delay(
        &preferred_consume_ledger,
        &consume_ledger,
        &effective_consume,
        &bottleneck_tags,
    )?;
    let duration_ticks = plan
        .duration_ticks
        .max(1)
        .saturating_add(scarcity_delay_ticks);
    let ready_at = source.now().saturating_add(duration_ticks as u64);
    Ok(WorldEventBody::Domain(DomainEvent::RecipeStarted {
        emergency_power_obligation: None,
        committed_recipe_origin: None,
        job_id: action_id,
        requester_agent_id: requester_agent_id.clone(),
        factory_id: factory_id.clone(),
        recipe_id: recipe_id.clone(),
        accepted_batches: plan.accepted_batches,
        consume: effective_consume,
        produce: plan.produce.clone(),
        byproducts: plan.byproducts.clone(),
        power_required: plan.power_required,
        power_owner_agent_id: Some(factory.builder().to_owned()),
        duration_ticks,
        consume_ledger,
        output_ledger,
        bottleneck_tags,
        market_quotes,
        logistics_route_ids: resolved_logistics_route_ids,
        logistics_path_ids: resolved_logistics_path_ids,
        ready_at,
    }))
}
